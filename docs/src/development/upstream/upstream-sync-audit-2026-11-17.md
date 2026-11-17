---
title: Upstream Sync Audit 2026-11-17
description: Full re-audit of every recorded Zed upstream decision against the live upstream commit graph.
---

# Upstream Sync Audit 2026-11-17

## Scope

- Local branch: `main` at `afe8038860`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Live upstream head compared: `e5be9608ddd4cb0f2f179e1c0014f0a3a0337260` (2026-11-17)
- Previous audited baseline: `00b86477f8b81c8d20328b096200f91452a572df` (2026-11-03 audit)
- Reports enumerated: 49 non-audit `upstream-sync-*.md` reports
- Recorded decision rows: 1653 in report tables
- Unique upstream SHAs after latest-wins deduplication: 1627
  (280 A, 549 B, 798 C)

This audit re-checks the whole recorded history, not only the batches added
after the previous audit. The upstream side was compared against a local
commit-graph clone of `zed-industries/zed` at the head above, so every
recorded SHA was resolved against the real upstream history rather than
re-read from the reports.

Correction (2026-11-17, audit): no decision row was reclassified and no
ledger row changed. Eight findings need work that this audit does not
perform (ports, a version follow, two records whose local port was
reverted, and one record group whose target crates were deleted), so the
recorded classes stand until the corresponding local commits or report
edits land. Each finding has a tracking issue on the repository.

## Method

1. Every decision table in every sync report was parsed with the same
   row shape `./script/backfill-upstream-ledger` uses, and the 1627 unique
   SHAs were resolved with `git rev-parse` inside a
   `--filter=tree:0` clone of upstream. All 1627 resolve; none was
   force-pushed out of upstream history.
2. Every recorded `A`/`B` local commit was checked with `git cat-file -e`
   and `git merge-base --is-ancestor <commit> main`.
3. Upstream reverts were enumerated from `This reverts commit <sha>`
   bodies across all 40877 upstream commits and matched against every
   recorded `A`/`B` row.
4. Upstream follow-ups that name a recorded SHA in their commit message
   were extracted (36 recorded SHAs) and read individually; each one was
   either a benchmark baseline link, a version string, a line-range link,
   or one of the two reverts in step 3.
5. Review coverage was recomputed as "every upstream commit after the
   first reviewed baseline that no report classifies", which is stricter
   than the report-header intervals because report headers are not
   contiguous.
6. Every recorded `C` row whose disposition names an absent local path was
   re-checked against the current tree (12 rows), and the rows the
   rejection justified with `conflict`, `diverged`, or `not isolatable`
   were sampled and re-read against the local call chain.
7. Every absorbed `A`/`B` port was tested for survival: the lines the local
   port added were looked up in the current tree for 752 rows, and the
   flagged rows were read individually instead of being taken as findings.
8. The 127 `C` rows recorded as `diverged` or `unisolatable` were re-tested
   row by row with `git apply --check --verbose` against an export of local
   `HEAD`, and every row with a clean production hunk was read against the
   local call chain. The result is the second pass below.
9. The remaining 710 `C` rows were then put through the same test, with a
   per-hunk split for rows that came within one production file of applying
   (`git apply` reports only the first failing hunk per file) and a post-image
   survival check for every row with a clean product file. That is the third
   pass below.

## Findings

### F1: `7b030b5008` was never classified and its panic guard is still missing

The commit (`project: Fix panic when an inlay hint response outlives its
buffer version (#61747)`, 2026-07-27) sits exactly on the boundary the
2026-08-07 report declares — that report starts its range _after_
`7b030b5008`, so it had no decision row in any table and no ledger entry.
The 2026-10-18 issue survey treated the same boundary as unclassified for
other commits; this one was missed.

The fix was absent until 2026-11-17. Upstream returns early when the buffer
version changed during the fetch:

```rust
// crates/project/src/lsp_store.rs (upstream)
if lsp_data.buffer_version != query_version {
    return CacheInlayHints::default();
}
```

Locally the post-await closure computed the same predicate as `update_cache`
but used it only to gate writes, and kept reading
`lsp_data.inlay_hints.cached_hints(&chunk)` with the pre-await chunk id. That
read goes through `crates/project/src/lsp_store/inlay_hints.rs:129`, which
indexes `self.hints_by_chunks[chunk.id]` directly, so a buffer that shrank
during the refresh panicked — exactly the crash upstream fixed.
`invalidate_for_chunk` (`inlay_hints.rs:251`) and `fetched_hints`
(`inlay_hints.rs:133`) index the rebuilt vectors the same way.

Class: `B`, now recorded. Tracked by issue #61 and corrected on 2026-11-17:
the 2026-08-07 report gained a standalone table for `7b030b5008` (`B`, local
commit `10ad6bee8f`) plus a scope note saying it sits on that report's lower
boundary. The `B` row stays out of the ledger, so the ledger is unaffected.

The guard is applied at `crates/project/src/lsp_store.rs:8239` as upstream
wrote it — the post-await closure returns `other_servers_cached` before
touching `cached_hints` or `invalidate_for_chunk`, which also drops the old
`update_cache` branches — and upstream's regression test
`test_inlay_hint_response_after_buffer_shrinks` is ported to
`crates/editor/src/inlays/inlay_hints.rs`. Removing the guard makes the new
test panic with `index out of bounds: the len is 1 but the index is 9` at
`crates/project/src/lsp_store/inlay_hints.rs:180`, so the test does exercise
the stale chunk id; with the guard, `cargo test -p editor --lib inlay` passes
(40 tests).

### F2: `cd4902bf` recorded `C` breaks the version-follow policy

`cd4902bfa2` (`Bump Zed to v1.30.0 (#66839)`, 2026-11-11) is a two-line
change to `Cargo.lock` and `crates/zed/Cargo.toml`. The
`upstream-sync-2026-11-11.md` row records it `C` with "Upstream release
metadata; ZZZ tracks its own version", and the ledger stores reason
`release-metadata`.

That contradicts the record of every earlier bump: `e24eeb71`, `02c6dd95`,
`0a4a4a95`, `71b60bba`, `b278e79c97` are all `B` version follows, and
`650a323090` (v1.29.0) applies upstream's version to
`crates/zzz/Cargo.toml` plus its lockfile entry. Local
`crates/zzz/Cargo.toml` is `1.29.0` while upstream `crates/zed/Cargo.toml`
is `1.30.0`, so the app version now lags one release. The
`auditing-upstream` reference states the rule directly: a version follow
tracks as `B`.

Tracked by issue #62, and applied on 2026-11-17 the way every earlier bump was:
`crates/zzz/Cargo.toml` and its `Cargo.lock` entry moved to `1.30.0`, and the
row became `B`, exactly like `e24eeb71`, `02c6dd95`, `0a4a4a95`, and
`71b60bba`; it now names the local bump `9e705ee956` where those four still
carry `--`. The ledger drops back to 862 rows because `B` rows stay out of it,
and `cargo xtask check release` still agrees across the manifest, the lockfile,
and the changelog.

### F3: 26 upstream commits after the 2026-11-16 batch are unreviewed

The 2026-11-16 report reviewed 20 commits up to `917559da35` and set the
new reviewed baseline to `596dfdcf32`, noting seventeen commits remained,
starting with `24d37721115` and `021c7d4c4f`. Since then nine more
commits landed upstream through `e5be9608dd` (2026-11-17), so the
unreviewed tail is 26 commits, 2026-11-15 to 2026-11-17.

Five of them touch local surfaces with real callers (`fs` watcher hashing,
`language` buffer splitting, `terminal_view` text runs, `util` natural
sort, `vim` substitute), and `5709f329dc` (Corgi FHS builds) is
upstream-only tooling. None of this is a defect in the record; it is the
next forward batch. Tracked by issue #63.

### F4: `fdcb198110` still carries only the macOS half of `PowerMode`

The 2026-11-03 audit recorded this as F7 and left it `B`. Nothing changed:
`Platform::power_mode` is overridden only in
`crates/gpui_macos/src/platform.rs:1042`; the default at
`crates/gpui/src/platform.rs:291` returns `PowerMode::Unknown`, and
`crates/gpui/src/window.rs:832` applies `low_power_frame_interval` only to
`PowerMode::LowPower`. The 30 fps throttle therefore never engages on
Linux or Windows, where upstream detects power profiles through
`power-profiles-daemon` and Windows power notifications.

It stays `B`. No workspace dependency provides D-Bus access directly
(`zbus` as a workspace member dependency exists only under `vendor/trash`),
but the crates are already built into the Linux binary transitively —
`Cargo.lock` carries `zbus` `5.19.0` and `6.3.0` through `ashpd 0.14.0`,
`accesskit_unix 0.24.1`, `atspi-common 0.13.0`, `atspi-proxies 0.13.0`, and
`oo7 0.7.1`. A direct `zbus` edge in `gpui_linux` would therefore add no crate,
so the remaining choice is between that and a dependency-free sysfs read of
`/sys/class/power_supply/*/status`, which sees AC/battery but not a
user-selected power-saver profile. The Windows half subscribes to platform
power notifications and cannot be built or run on this host; `cargo check -p
gpui_windows --target x86_64-pc-windows-msvc` type-checks it, but the
notification path itself would be untested. Tracked by issue #64, where the
decision is recorded.

### F5: `9bde578e` is recorded `A` but its port was reverted

The `upstream-sync-2026-08-18.md` row records `9bde578e` as `A` with local
commit `dcafe24237`, and the same report's narrative says the commit was
"absorbed later as `dcafe24237` once `spawn_dedicated` landed". Local
history says otherwise: `16d51d6962` (`Revert "acp: Poll the client
connection future on a dedicated thread (#62259)"`) reverted `dcafe24237`
on the same day, and nothing re-ported it since. `git log -S` shows the
helper added by the port and removed by that revert.

The behavior is absent from the current tree:
`crates/gpui/src/executor.rs` has no `spawn_dedicated`, and
`crates/agent_servers/src/acp.rs:982` starts the connection future with
`cx.background_spawn`, which is the call the upstream commit replaced.
The doc comment retained at `acp.rs:710-716` still states the future "must
be polled on a dedicated thread rather than via `background_spawn`", so
the code now contradicts its own explanation and an unoptimized build can
still overflow the 512 KiB GCD worker stack on macOS.

Because `A` rows never enter the ledger, this commit was invisible to
re-review until the record was corrected. Tracked by issue #65, and corrected on
2026-11-17: the row is `C` with the old local commit replaced by a pointer to
the revert, the narrative no longer claims the port survived, and the SHA is in
the ledger. That took the ledger to 863 rows, back to 862 once `cd4902bfa2`
became a `B` version follow (F2) and to 854 once the eight `C` rows that became
ports under R1-R9 did. The absorbed trailer on `dcafe24237` still trips the
ledger gate, so `9bde578e` is listed in `script/upstream-ledger-allowlist` with
the revert as the reason.

Re-porting is still open and needs a decision: upstream's fix requires
`spawn_dedicated` on `gpui`'s executor, and `gpui` is a crate the next upstream
sync overwrites, so the helper would be a recorded divergence rather than a
free fix. Until then an unoptimized macOS build can still overflow the 512 KiB
GCD worker stack.

### F6: `7c7b171ff`'s copy fix was lost to a later Markdown rework

`upstream-sync-2026-10-09.md:255` recorded the blockquote copy fix as `B`
with local commit `db6e989c54`, which added a `current_code_block_text`
accumulator and dropped the raw source slice. That accumulator no longer
existed: `db6e989c54:crates/markdown/src/markdown.rs` contains seven
references, the `HEAD` of that pass contained none, and the file grew from 6790 to 7129
lines when a later Markdown rework replaced the region. The copy path is
back to the pre-fix shape at `crates/markdown/src/markdown.rs:3335-3341`,
which slices `parsed_markdown.source()[range]` and so copies the `>`
prefixes of an enclosing blockquote.

`parser::extract_code_block_content_range` still exists at
`crates/markdown/src/parser.rs:979` (the parser itself uses it), so the fix has
to be re-applied on top of the current element code. Tracked by issue #66, and
re-applied as `4f4562d54f`: the accumulator now lives in `MarkdownEventState` and
is filled from the `Text` and `SubstitutedText` events, with upstream's
`test_copy_code_block_uses_parsed_text` ported alongside, and the row names that
commit instead of the lost `db6e989c54`. Reverting the accumulator locally
makes that test copy `> echo "Zed is awesome!"` for the blockquote cases, so the
test does catch the regression.

### F7: twenty-two `A`/`B` rows absorbed into code the ACP-only cleanup deleted

`75e7e83732` and `91a2911a68` (both 2026-09-27) removed the edit-prediction
stack, `web_search`, `language_models`, and every provider crate
(`anthropic`, `open_ai`, `google_ai`, `open_router`, `opencode`, `mistral`,
`deepseek`, `lmstudio`, `ollama`, `llama_cpp`, `bedrock`, `x_ai`).
Twenty-two `A`/`B` rows still record a local commit whose every touched
file is gone, so every ancestor check passes while the absorbed code is
absent.

The set was computed both ways: a crate-level filter found twenty, and a
file-level check (`git show --name-only` against `git cat-file -e
HEAD:<path>`) added `9e636045` and `53fcf4be`, which survive a crate-only
filter because `crates/language_model` lives on as a three-file shim and
because `53fcf4be` also touched a docs page. Three further rows that a
crate-level filter flags (`08994c41`, `a58fff13`, `b427d4ec`) are the
`crates/zed` → `crates/zzz` rename: their behavior is present at
`crates/zzz/src/main.rs:855`, `crates/zzz/src/zzz/open_listener.rs:344`,
and `crates/zzz/resources/info/Permissions.plist:25`.

Because `A`/`B` rows never enter the ledger, none of the twenty-two had a
dedup entry and the next audit would have re-read them as absorbed. The
isolation test also fails check 3 for all of them.

Corrected on 2026-11-17 as issue #67 proposed: all twenty-two rows are now `C`
in their reports, each keeping its old local commit in the disposition text and
naming the cleanup commit that deleted the code. The reports' totals moved with
them (three rows in the 09-18 summary, one and two in the 09-24 sections) and
each affected report gained a dated `## Corrections` section. The SHAs now have
ledger rows, which took the ledger from 840 to 862. Because git history still
carries their absorption trailers, all twenty-two are listed in
`script/upstream-ledger-allowlist` with the reason; that allowlist entry is what
keeps `check upstream-ledger` green, and it must stay until the ACP-only cleanup
is reverted.

The 59 rows that lost only some of their files were traced to deliberate
local commits — the `zed` → `zzz` rename (`6c807c7a35`), the agent-runtime
decoupling (`15384c08be`), the native-agent tool removal
(`dcb03e8894`), the Codestral removal (`866c5d816f`), vendoring
(`f97df57029`), and the collab server removal (`54f4ede9a1`) — and were
not counted as findings.

### F8: 230 ledger rows binned `untriaged` or `upstream-infra` touch product code

`/script/backfill-upstream-ledger` derives `reason_code` with `reason_for()`,
which returns the first regex that matches the disposition text. A note that
mentions a benchmark, a workflow, docs, or "upstream-only" is filed as
`upstream-infra` even when the real justification is "no local caller", and a
note that matches nothing is filed as `untriaged`. The two bins hold 355 of
829 rows, and 230 of them touch `crates/**/*.rs` outside `crates/zed`
(checked per row with `git diff-tree --no-commit-id --name-only -r <sha>`).

The dispositions are mostly right; the reason codes are not, and the ledger
header calls the file the dedup index for re-review. Tracked by issue #68.

Corrected on 2026-11-17. `reason_for()` no longer returns the first matching
pattern: it collects every class whose alternatives match and then walks an
explicit priority order that puts `upstream-infra` after the product-behavior
classes, so a note that mentions a benchmark while giving a product reason lands
in that product reason. `absent-api` also gained `no caller` and `has no`, which
is how the "no local caller" rejections were being worded, and `unisolatable`
gained `cannot be isolated`. The fallback is no longer `untriaged` for every
unmatched note: one that names a `crates/<crate>/` path or an inline code span
is a statement about a local surface, so it becomes `absent-api`.

The ledger was regenerated, and the bins moved accordingly:

| reason_code        | before | after |
| ------------------ | ------ | ----- |
| `untriaged`        | 236    | 155   |
| `absent-api`       | 229    | 349   |
| `philosophy`       | 142    | 142   |
| `upstream-infra`   | 123    | 87    |
| `diverged`         | 75     | 76    |
| `unisolatable`     | 51     | 47    |
| `release-metadata` | 6      | 6     |

`script/check-upstream-ledger` also gained the rule the issue asked for: a row
binned `upstream-infra` or `release-metadata` whose note names a
`crates/<crate>/` path or a Rust symbol now fails the gate. It fired once, on
`4ce4b7b567`, whose note called the randomized element-tree harness a
"benchmark" while the actual reason was that the harness cannot be isolated;
the note now says harness and the row bins as `unisolatable`.

### Non-findings re-checked

- Ledger: `./script/backfill-upstream-ledger` reproduces
  `docs/src/development/upstream-rejected.tsv` byte for byte (829 rows).
- Absorbed commits: every recorded `A`/`B` local commit resolves and is an
  ancestor of `main`. The four `B` rows with `--` are version follows
  (`e24eeb71`, `02c6dd95`, `0a4a4a95`, `71b60bba`); each names a
  superseding commit, and `9e40f452`, `8ad9315d1e`, and `c5133374` all
  resolve on `main`.
- Upstream reverts: only two reverted a commit ZZZ recorded. `7e7311ad20`
  and its revert `ba36af2050` are both `C` in the 2026-11-04 report, and
  `91bf967e` (inspector flag) was followed by its revert `01c555b435`,
  absorbed as `3b2097d9`. No `A`/`B` row points at a commit upstream has
  since dropped.
- `cffa8d94a8`: the `has_active_modal` guard added by `770bd7a166` is gone
  from `crates/which_key/src/which_key.rs` after `432955772e` ported
  `48741fd17d`, but the popup now renders above the modal layer
  (`crates/workspace/src/multi_workspace.rs:386-388`), which is upstream's
  own replacement. Not a regression.
- The twelve `C` rows that name an absent local path were re-checked and
  every claim still holds: no `crates/agent_servers/src/acp/` directory,
  `crates/agent/src/tools/` still carries only
  `context_server_registry.rs`, no `open_ai` or `language_models` crate,
  `crates/language_model/src` still has three files, and
  `crates/gpui_web/src/viewport.rs` is still absent.
- `bfd1dd484b` stays `C`: the local graph list renders author, time, and
  sha as plain labels at `crates/git_graph/src/git_graph.rs:2168-2170`,
  with no avatar cell to align.
- The 32 `A` rows that claim equivalence without a local commit were
  re-checked where the claim is falsifiable: `premultiplied_alpha` is in
  `crates/gpui_wgpu/src/wgpu_renderer.rs:1143`, `0610da51e9` shares the
  GPUI arena through an `Rc`, `editor_for_diff` downcasts with
  `and_then(..).ok()` at `crates/agent_ui/src/entry_view_state.rs:380`,
  `Cargo.lock` pins `rustls` 0.23.46 and `ashpd` 0.14.0 (both newer than
  the versions those rows reason about), `baseten`, `indoc`, and
  `.github/CODEOWNERS.hold` are absent, and
  `crates/gpui/src/platform/test/platform.rs` imports no `DisplayId`.
- `4ba7d82f68` (macOS JetBrains `cmd-o`) stays `C`: the local
  `assets/keymaps/macos/jetbrains.json:120-135` binds `cmd-o` to
  `project_symbols::Toggle` inside the `Workspace` context, which beats
  the context-free `workspace::Open` at
  `assets/keymaps/default-macos.json:34`, so the dialog-open bug the
  upstream commit fixes is already absent.

## C-row isolation re-review (second pass)

The first pass sampled the 798 `C` rows. This pass runs the isolation test on
all 127 rows the ledger records as `diverged` (74) or `unisolatable` (53), the
two classes whose justification is a claim about the local tree rather than a
missing product surface.

Method, per row:

1. Resolve the upstream commit in the `zed-industries/zed` clone and take its
   diff (`git show --format=`).
2. Export local `HEAD` to a scratch tree (`git archive HEAD`) and run
   `git apply --check --verbose` against it, recording per file whether the
   hunks apply, which hunk line fails, and which files do not exist. A file that
   reports `Checking patch X...` and no error applies.
3. Classify the failures: test-module hunk, import hunk, or a production hunk.
4. For every row with a clean production hunk, read the local call chain and
   decide whether the behaviour is already present under another shape.

`git apply` is the right instrument because it answers the question the class
asserts: can the upstream change still be applied to this tree, file by file.

### Scope note

`main` advanced to `c00ed3e478` (a merge of `origin/main`) while this pass ran,
which added the 2026-11-17 batch and eight ledger rows. All eight are
`absent-api` or `upstream-infra`, so the `diverged`/`unisolatable` set is still
exactly 127 rows and every row below was re-tested again against `c00ed3e478`;
the five findings keep the same file and hunk verdicts, and the line numbers
quoted here are the post-merge ones.

### Result

| Bucket                                               | Rows |
| ---------------------------------------------------- | ---- |
| Production hunks apply (re-absorb candidates)        | 2    |
| Production hunks apply except a mechanical hand edit | 3    |
| Recorded blocker contradicted by the tree            | 2    |
| Core production hunk conflicts, `C` stands           | 117  |
| Every touched file absent locally, `C` stands        | 3    |

No row applies in full: 0 of 127. Of the 76 rows with at least one clean file,
most are clean only in a test, a markdown page, `Cargo.lock`, or a leaf module
whose consumer conflicts. Seven rows are actionable: five ports and two
record corrections.

### R1: `6721ea2e` applies apart from its test

Tracked by issue #69. `crates/workspace/src/dock.rs` applies in full and the
only failure in `crates/workspace/src/workspace.rs` is at `:14663`, inside
`mod tests`. The dock reset actions at `workspace.rs:7261` and `:7280` still
reset only the active panel, so the reported upstream bug (#57388) was present
locally. Ported as `cfe7c99672`, row `B`.

### R2: `78f5a535a3` applies apart from its test

Tracked by issue #70. `crates/agent_ui/src/agent_diff.rs` applies and the only
failure is the added test at `conversation_view.rs:6796`. The production hunk
is the missing `parent_session_id().is_none()` guard on
`AgentDiff::set_active_thread`, and local `agent_ui` already renders inline
subagent cards and documents the linked action log at
`conversation_view/thread_view.rs:1557`, so the premise holds. Ported as
`ac27e367a5`, row `B`.

### R3: `1ade7854` applies in its core file

Tracked by issue #71. `crates/project/src/search.rs` and its integration test
apply; `crates/project/src/project_search.rs` fails at `:76` because of local
edits around `task_handle` / a clippy allow, and the remainder is a
`LineHint` -> `MatchPositionHint` rename. The local regex path still matches
line by line with the trailing newline in the haystack and never sets
`multi_line(true)`, so `$`-anchored regexes did not match. Ported as
`4676cce620`, row `B`.

### R4: `34241d5f` applies in four of six files

Tracked by issue #72. `fallible_options.rs`, `settings_content.rs`,
`terminal.rs`, both `Cargo.toml` edits, and the new probe test apply. The two
failures are the flattened structs local diverged on, and each needs the same
two-line treatment as the files that apply. `serde_path_to_error` is already a
workspace dependency. Ported as `6ee8fa5e1b`, row `B`; upstream's
`flatten_collision_probe` test is included verbatim.

### R5: `5299a14401` applies apart from an import block

Tracked by issue #73. `markdown_preview_view.rs` applies and the only
`markdown.rs` failure is the import hunk at `:34`. Every prerequisite the
`MarkdownInputHandler` needs is present locally (`InputHandler` at
`crates/gpui/src/platform.rs:1522`, `UTF16Selection` at `:1509`, the
`language`/`text` UTF-16 helpers), and the local element still has the
`RenderedLine`/`RenderedText` shape the patch edits. macOS-only at runtime.
Ported as `6c8aa3849f`, row `B`; the clip region had to change shape, because
local `RenderedText` is shared by clone and upstream's per-line bounds would
never be observed. Runtime behaviour is unverified on this Linux host — only
`cargo check` and the ported hit-test test ran.

### R6: two rows whose recorded blocker is contradicted

Tracked by issue #74.

- `82f2558e17` claims the three path APIs "ZZZ rewrote in the previous batch".
  They were not rewritten: `Project::visibility_for_paths`,
  `Project::visibility_for_subpaths`, and `workspace::find_existing_workspace`
  still take `&[PathBuf]` at `crates/project/src/project.rs:2260`, `:2303` and
  `crates/workspace/src/workspace.rs:9180`, which is why those hunks apply. The
  only behavioural hunk is a `resolve_open_behavior` change in
  `crates/zed/src/zed/open_listener.rs`, and that function has no local
  counterpart.
- `492acd6c` is recorded `unisolatable` but is a revert of `6297c88f`, which ZZZ
  never absorbed: `crates/terminal/src/pty_info.rs:210` still has the
  reverted-to `terminate_child_process` and no `TerminalProcessIds` exists. The
  row is an absence, not an unisolatable rewrite.

### Re-checked and left as `C`

- `4ba7d82f68` (macOS JetBrains `cmd-o`) is not a finding after all. The local
  JetBrains keymap binds `cmd-o` to `project_symbols::Toggle` inside a
  `Workspace` context block (`assets/keymaps/macos/jetbrains.json:120`, binding
  at `:135`), which already outranks the context-free
  `workspace::Open` at `assets/keymaps/default-macos.json:34`. The dialog bug
  the commit fixes is absent locally even though its keymap hunk applies.
- `7ecab315`, `0ad5441b`, `00c0e96e`, `49469000`,
  `e8fbacb6`, `90eb566f`, `b25dc09370`, `897ba9ad`, `b2131e9d`, `674dd790`,
  `92e8e2b8a0`, `4bdf188c`, `6dee3fc7`, `f8c27835`, `33c6212b`, `45ff3371`
  all fail in the file that carries the behaviour, not only in its tests.

### Full row table

Clean / failed / absent counts are production files only (docs, `Cargo.lock`,
and assets are excluded); test files are included in "failed" when their hunk
conflicts.

| Upstream     | Class        | Clean / failed / absent | Verdict               |
| ------------ | ------------ | ----------------------- | --------------------- |
| `00c0e96e`   | diverged     | 4 / 2 / 0               | diverges, `C` stands  |
| `05473ed8`   | diverged     | 0 / 2 / 5               | diverges, `C` stands  |
| `05c2c9ab`   | diverged     | 0 / 1 / 0               | diverges, `C` stands  |
| `0ad5441b`   | diverged     | 3 / 2 / 0               | diverges, `C` stands  |
| `0d08af1e`   | diverged     | 1 / 9 / 0               | diverges, `C` stands  |
| `0dea8c63`   | unisolatable | 0 / 6 / 1               | diverges, `C` stands  |
| `107ee1a6`   | unisolatable | 2 / 3 / 1               | diverges, `C` stands  |
| `10cfb45944` | diverged     | 1 / 6 / 0               | diverges, `C` stands  |
| `12ef40c3`   | diverged     | 7 / 21 / 5              | diverges, `C` stands  |
| `1475887f`   | unisolatable | 8 / 12 / 0              | diverges, `C` stands  |
| `158c16f5`   | unisolatable | 1 / 6 / 2               | diverges, `C` stands  |
| `1663af412c` | diverged     | 0 / 8 / 0               | diverges, `C` stands  |
| `184e124b`   | diverged     | 0 / 3 / 0               | diverges, `C` stands  |
| `18512de483` | diverged     | 0 / 1 / 1               | diverges, `C` stands  |
| `18be72fd`   | diverged     | 3 / 15 / 8              | diverges, `C` stands  |
| `1a89a92e`   | unisolatable | 0 / 5 / 0               | diverges, `C` stands  |
| `1ade7854`   | unisolatable | 2 / 1 / 0               | port, issue #71       |
| `1e2e422c`   | unisolatable | 9 / 12 / 1              | diverges, `C` stands  |
| `206a863a`   | diverged     | 0 / 1 / 0               | diverges, `C` stands  |
| `2240542744` | diverged     | 3 / 3 / 1               | diverges, `C` stands  |
| `224373ab`   | diverged     | 3 / 2 / 0               | diverges, `C` stands  |
| `22945084`   | diverged     | 0 / 16 / 5              | diverges, `C` stands  |
| `2318f45f`   | unisolatable | 0 / 3 / 0               | diverges, `C` stands  |
| `23aca989`   | diverged     | 6 / 10 / 4              | diverges, `C` stands  |
| `279da638`   | diverged     | 9 / 4 / 6               | diverges, `C` stands  |
| `2d6c664dc2` | diverged     | 0 / 4 / 0               | diverges, `C` stands  |
| `318c664e`   | diverged     | 6 / 7 / 0               | diverges, `C` stands  |
| `33c6212b`   | unisolatable | 2 / 3 / 0               | diverges, `C` stands  |
| `34241d5f`   | unisolatable | 5 / 2 / 0               | port, issue #72       |
| `402675e805` | diverged     | 1 / 5 / 0               | diverges, `C` stands  |
| `45ff3371`   | diverged     | 2 / 9 / 0               | diverges, `C` stands  |
| `492acd6c`   | unisolatable | 0 / 2 / 0               | record fix, issue #74 |
| `49469000`   | unisolatable | 8 / 3 / 0               | diverges, `C` stands  |
| `4a6121bb0f` | diverged     | 2 / 3 / 0               | diverges, `C` stands  |
| `4aad57fd`   | unisolatable | 3 / 2 / 0               | diverges, `C` stands  |
| `4ba7d82f68` | diverged     | 1 / 1 / 0               | diverges, `C` stands  |
| `4bdf188c`   | diverged     | 3 / 4 / 0               | diverges, `C` stands  |
| `5299a14401` | diverged     | 1 / 1 / 0               | port, issue #73       |
| `52b24181`   | unisolatable | 4 / 5 / 0               | diverges, `C` stands  |
| `53dbfe40`   | unisolatable | 1 / 0 / 8               | diverges, `C` stands  |
| `582e6a57`   | unisolatable | 2 / 13 / 1              | diverges, `C` stands  |
| `59dd60d4dd` | unisolatable | 0 / 7 / 0               | diverges, `C` stands  |
| `5e23b6e71d` | unisolatable | 1 / 8 / 1               | diverges, `C` stands  |
| `5e28272c`   | unisolatable | 0 / 0 / 1               | no local target       |
| `5e8998a2a7` | unisolatable | 0 / 10 / 10             | diverges, `C` stands  |
| `5f1f7e63`   | diverged     | 0 / 1 / 0               | diverges, `C` stands  |
| `60bf47b9`   | unisolatable | 5 / 14 / 2              | diverges, `C` stands  |
| `64d14ea8`   | diverged     | 0 / 1 / 0               | diverges, `C` stands  |
| `6721ea2e`   | diverged     | 1 / 1 / 0               | port, issue #69       |
| `674dd790`   | unisolatable | 2 / 1 / 0               | diverges, `C` stands  |
| `68b65045`   | diverged     | 1 / 0 / 3               | diverges, `C` stands  |
| `6971ae21e9` | unisolatable | 0 / 1 / 3               | diverges, `C` stands  |
| `6dee3fc7`   | diverged     | 2 / 3 / 0               | diverges, `C` stands  |
| `735bc10fd0` | unisolatable | 0 / 2 / 2               | diverges, `C` stands  |
| `74495e740e` | unisolatable | 0 / 1 / 0               | diverges, `C` stands  |
| `76b1096c`   | diverged     | 0 / 4 / 1               | diverges, `C` stands  |
| `78a8526196` | diverged     | 2 / 10 / 0              | diverges, `C` stands  |
| `78f5a535a3` | diverged     | 1 / 1 / 0               | port, issue #70       |
| `78fde14d12` | diverged     | 1 / 1 / 0               | diverges, `C` stands  |
| `7ca96c2a58` | diverged     | 10 / 12 / 1             | diverges, `C` stands  |
| `7ecab315`   | unisolatable | 3 / 2 / 0               | diverges, `C` stands  |
| `7fecbb2c4b` | unisolatable | 2 / 2 / 1               | diverges, `C` stands  |
| `8166e3d7`   | diverged     | 7 / 12 / 0              | diverges, `C` stands  |
| `82f2558e17` | unisolatable | 3 / 0 / 1               | record fix, issue #74 |
| `87ce147829` | diverged     | 0 / 0 / 1               | no local target       |
| `897ba9ad`   | diverged     | 0 / 6 / 1               | diverges, `C` stands  |
| `8a5cf7e23a` | diverged     | 0 / 2 / 1               | diverges, `C` stands  |
| `8bbcd83968` | diverged     | 4 / 16 / 0              | diverges, `C` stands  |
| `8fc1a8a0`   | diverged     | 0 / 6 / 1               | diverges, `C` stands  |
| `90eb566f`   | diverged     | 2 / 2 / 0               | diverges, `C` stands  |
| `92e8e2b8a0` | unisolatable | 2 / 5 / 0               | diverges, `C` stands  |
| `933d8d9381` | unisolatable | 0 / 1 / 20              | diverges, `C` stands  |
| `956a49e4`   | diverged     | 2 / 6 / 2               | diverges, `C` stands  |
| `975845875b` | unisolatable | 1 / 11 / 45             | diverges, `C` stands  |
| `984bf4d4`   | diverged     | 0 / 1 / 4               | diverges, `C` stands  |
| `9decdcc1`   | diverged     | 0 / 5 / 0               | diverges, `C` stands  |
| `9df33dea42` | unisolatable | 1 / 6 / 0               | diverges, `C` stands  |
| `a21007b7`   | unisolatable | 0 / 5 / 2               | diverges, `C` stands  |
| `a434bb7e`   | unisolatable | 0 / 2 / 0               | diverges, `C` stands  |
| `a4a3a968d1` | unisolatable | 0 / 2 / 3               | diverges, `C` stands  |
| `a60addb9`   | diverged     | 0 / 4 / 0               | diverges, `C` stands  |
| `ab7d21d4`   | diverged     | 0 / 4 / 0               | diverges, `C` stands  |
| `adc8d7918e` | diverged     | 0 / 1 / 0               | diverges, `C` stands  |
| `afff3d2616` | unisolatable | 7 / 7 / 0               | diverges, `C` stands  |
| `b2131e9d`   | diverged     | 2 / 4 / 0               | diverges, `C` stands  |
| `b25dc09370` | diverged     | 0 / 2 / 0               | diverges, `C` stands  |
| `ba490197cf` | diverged     | 0 / 2 / 1               | diverges, `C` stands  |
| `be07e43ea0` | diverged     | 2 / 10 / 3              | diverges, `C` stands  |
| `be8c6f9f`   | unisolatable | 1 / 5 / 0               | diverges, `C` stands  |
| `c07b87624c` | diverged     | 0 / 2 / 0               | diverges, `C` stands  |
| `c1eda3e8`   | diverged     | 1 / 13 / 0              | diverges, `C` stands  |
| `c83adb3d`   | diverged     | 0 / 3 / 1               | diverges, `C` stands  |
| `cb1937a8e7` | diverged     | 1 / 1 / 0               | diverges, `C` stands  |
| `cdc537c6`   | unisolatable | 2 / 3 / 1               | diverges, `C` stands  |
| `ce6f3af5`   | diverged     | 0 / 1 / 0               | diverges, `C` stands  |
| `cef06d35`   | unisolatable | 1 / 2 / 0               | diverges, `C` stands  |
| `d1dae815`   | unisolatable | 0 / 6 / 1               | diverges, `C` stands  |
| `d528c665e1` | unisolatable | 0 / 4 / 7               | diverges, `C` stands  |
| `d62802d4`   | unisolatable | 0 / 1 / 0               | diverges, `C` stands  |
| `d85eade7`   | unisolatable | 3 / 13 / 2              | diverges, `C` stands  |
| `d86aa6e6b9` | diverged     | 1 / 3 / 0               | diverges, `C` stands  |
| `d93bf74085` | unisolatable | 0 / 6 / 1               | diverges, `C` stands  |
| `db10a8dd`   | unisolatable | 7 / 11 / 5              | diverges, `C` stands  |
| `db1de83a2c` | unisolatable | 0 / 1 / 0               | diverges, `C` stands  |
| `dbf7f638`   | diverged     | 0 / 0 / 2               | no local target       |
| `dde76a1dca` | unisolatable | 16 / 15 / 3             | diverges, `C` stands  |
| `ded896cc`   | diverged     | 0 / 4 / 0               | diverges, `C` stands  |
| `df673c1b`   | unisolatable | 0 / 4 / 1               | diverges, `C` stands  |
| `e231594b6b` | diverged     | 4 / 17 / 1              | diverges, `C` stands  |
| `e52ab15eac` | diverged     | 0 / 1 / 0               | diverges, `C` stands  |
| `e83f1c9495` | diverged     | 0 / 1 / 0               | diverges, `C` stands  |
| `e8fbacb6`   | diverged     | 3 / 4 / 0               | diverges, `C` stands  |
| `e91b82c106` | diverged     | 1 / 13 / 0              | diverges, `C` stands  |
| `e96ffb0fd3` | diverged     | 0 / 5 / 3               | diverges, `C` stands  |
| `eb2f43d982` | unisolatable | 2 / 4 / 0               | diverges, `C` stands  |
| `eb354c8d`   | unisolatable | 1 / 7 / 0               | diverges, `C` stands  |
| `eb548352`   | diverged     | 0 / 2 / 0               | diverges, `C` stands  |
| `eee7501970` | diverged     | 1 / 13 / 3              | diverges, `C` stands  |
| `ef075910`   | unisolatable | 7 / 37 / 3              | diverges, `C` stands  |
| `f0685e0a`   | diverged     | 4 / 10 / 3              | diverges, `C` stands  |
| `f37989fbdf` | unisolatable | 6 / 21 / 0              | diverges, `C` stands  |
| `f41ac64812` | diverged     | 0 / 6 / 0               | diverges, `C` stands  |
| `f8c27835`   | diverged     | 3 / 1 / 0               | diverges, `C` stands  |
| `f99c53f4f6` | unisolatable | 1 / 3 / 3               | diverges, `C` stands  |
| `fa00dccc`   | diverged     | 0 / 9 / 2               | diverges, `C` stands  |
| `fc9258b4`   | diverged     | 4 / 8 / 3               | diverges, `C` stands  |
| `ffe0fa29e4` | diverged     | 3 / 12 / 8              | diverges, `C` stands  |

## C-row isolation re-review (third pass)

The second pass covered the 127 `diverged`/`unisolatable` rows. This pass covers
the other 710 `C` rows: 236 `untriaged`, 204 `absent-api`, 141 `philosophy`,
123 `upstream-infra`, and 6 `release-metadata`.

Method, per row:

1. `git apply --check --verbose` against an export of local `HEAD`, recording
   per file whether the hunks apply, the failing hunk line, and missing files.
2. For rows that came within one production file of applying, split the commit
   into single-hunk patches and test each hunk, because `git apply` reports only
   the first failing hunk per file and otherwise hides whether the
   behaviour-carrying hunk applies.
3. For every row with a clean product file, measure how much of the commit's
   added text already exists locally (post-image survival), which is the test
   for a silently absorbed row.

### Result

| Bucket                                        | Rows |
| --------------------------------------------- | ---- |
| Apply in full                                 | 13   |
| Clean product file, no product conflict       | 42   |
| At least one clean product file               | 219  |
| Every touched file absent locally, `C` stands | 159  |
| Already absorbed (post-image present)         | 0    |

No row in this tranche is a stale "already absorbed" record: the one row the
survival metric flagged, `52e0b848` (threads sidebar auto-open), matched only its
test-file hunks while every production hunk conflicts, so it stays `C`.

### R7: `91a6890d` applies in full and is not a policy rejection

Tracked by issue #75. `sidebar: Wrap import onboarding title when the panel is
narrow` is recorded `C` `philosophy` with the tautological note "Sidebar
onboarding title wrap." The `philosophy` bin comes from `reason_for()` matching
the word "onboarding"; the commit applies to the current tree with zero
conflicts and the banner is still rendered from
`crates/sidebar/src/sidebar.rs:5838` (callers at `:5661`, `:5724`) with the
pre-image `justify_between().flex_wrap()` layout at `:5866-5868`. Ported as
`cd15c09f3f`, row `B`.

### R8: `bbd198f5` needs one import hunk, not absent plumbing

Tracked by issue #76. Recorded `absent-api` with "Large semantic-token ordering
rewrite requires absent server-order plumbing", but the plumbing is introduced by
this commit and 14 of its 15 hunks apply; only
`crates/editor/src/semantic_tokens.rs:6` (imports) conflicts. The bug is live
locally: `crates/editor/src/semantic_tokens.rs:315-317` sorts highlights on
`range.start` alone, while
`crates/project/src/lsp_store/semantic_tokens.rs:678` promises that later tokens
override earlier ones, so two servers tokenizing the same range resolve by
`HashMap` iteration order and change between sessions. Ported as
`3a4d60b6a4`, row `B`.

### R9: `a7e23df6` applies in most of its hunks and its recorded port does not exist

Tracked by issue #77. Recorded `untriaged` with "Attempted port failed test
compilation; reverted per B-port rule", but `git log --all --grep=a7e23df6` and
`--grep="fallback language"` return nothing, so the note describes a port that is
not in this repository's history. The feature is still missing: untagged fenced
blocks resolve to `None` at `crates/markdown/src/markdown.rs:2816-2822`, and
`fallback_code_block_language` does not exist. Per hunk, `parser.rs` is 4/4 and
`markdown.rs` is 7/10; the three conflicts are the import line and the two
language-resolution regions, one of which differs only because local added a
`first_word` retry. Ported as `02ceb1939d`, row `B`; the report note claiming a
failed port was corrected in the same pass.

### Rows that apply in full and stay `C`

Every remaining full-apply row is rejected for a reason the patch cannot fix:
ZZZ rejects public API and assets with no caller (and `check gpui-api` would
demand a snapshot bless), the row is upstream CI or nix infrastructure, or the
missing half is in another crate.

| Upstream     | Recorded reason | What applies                                                | Verdict                                              |
| ------------ | --------------- | ----------------------------------------------------------- | ---------------------------------------------------- |
| `182d15a629` | absent-api      | `square_r.svg`, `icons.rs`                                  | unused icon asset, policy-consistent                 |
| `2239fa76b1` | untriaged       | `label.rs`                                                  | unused public API, policy-consistent                 |
| `2890c340`   | untriaged       | `.rules`                                                    | upstream `.rules` HARD RULE, must not absorb         |
| `3384317a`   | absent-api      | `Cargo.toml`                                                | `util/debug-embed` genuinely absent, `C` stands      |
| `3ce72bab`   | absent-api      | `window.rs`                                                 | unused public API, policy-consistent                 |
| `538a4a26`   | upstream-infra  | `.gitignore`, `config.toml`, `editor-doc-comment.patch`, +3 | upstream wezel CI infra                              |
| `5618f439`   | untriaged       | `wezel.yml`                                                 | upstream wezel CI infra                              |
| `7bb5656e63` | philosophy      | `geometry.rs`, `scrollbar.rs`                               | unused public API plus gpui-api snapshot, `C` stands |
| `95106f9c`   | untriaged       | `project-panel.md`                                          | staff-gated behaviour absent                         |
| `a19e19440d` | upstream-infra  | `linux`                                                     | upstream CI infra                                    |
| `cff4edce`   | untriaged       | `build.nix`                                                 | upstream nix infra                                   |
| `e0ae50d1`   | absent-api      | `keybinding.rs`                                             | unused public API, policy-consistent                 |

`2890c340` is worth calling out: it restores an upstream `.rules` entry that
orders agents to prepend a `> [!IMPORTANT]` marker to `README.md` and never
remove it. That is a review-flow instruction aimed at automated contributors, not
a coding rule, and absorbing it would let upstream dictate local file writes.
The recorded rejection is correct.

### Near misses left as `C`

These rows are one hunk away from applying, but the hunk that fails carries the
behaviour or the prerequisite:

| Upstream     | Why it stays `C`                                                                                            |
| ------------ | ----------------------------------------------------------------------------------------------------------- |
| `d0ae2f05`   | `sidebar.rs` 0/1 hunks apply; only the test file applies (selection-flicker fix is in the conflicting hunk) |
| `b61f33f8b6` | `context_menu.rs` 12/30; builds on the rejected submenu rewrite                                             |
| `d9e1c024`   | `div.rs` 14/21; depends on the unabsorbed long-press gesture architecture                                   |
| `2936989f`   | `line.rs` 3/5; new `LineLayout` API with no local caller                                                    |
| `5ccbbbd8`   | `taffy.rs` 0/1; new grid API with no local caller                                                           |
| `745c322dab` | `acp.rs` 1/14; needs the absent ACP v2 shared-thread model                                                  |
| `e8c6c67fad` | `acp.rs` 0/4; needs the absent ACP v2 terminal notifications                                                |
| `86b2cf96`   | `screen_capture.rs` 6/7; macOS objc2 migration, unverifiable on this host                                   |
| `cd8c467418` | `settings_store.rs` 0/1; completes the native-agent tool catalog ZZZ does not ship                          |

### Re-checked and left as `C`

- `3384317a` (`debug-embed = ["dep:rust-embed", "util/debug-embed"]`): the
  local `crates/util/Cargo.toml` has no `debug-embed` feature, so the recorded
  "would not compile" claim holds; the feature has to come from elsewhere first.
- `93f6b2e5` (macOS Python shim): the python hunk needs the newer `pet` revision
  (`find_and_report_envs` gains an argument and `PythonEnvironmentKind::Hatch`
  appears), so the recorded `upstream-infra` dependency-churn rejection stands
  until a pet bump is on the table.
- `e0ae50d1`, `182d15a629`, `3ce72bab`, `2239fa76b1`, `7bb5656e63`, `f9a1fc89`,
  `fe9556a1`: patches apply, but each adds a public API or asset with no local
  caller, which ZZZ rejects on purpose and `check gpui-api` would reject again.
- `10676bad` and `52e0b848`: collab-only and native-agent-only surfaces that
  ZZZ does not ship.

## Tail and platform verification

Two follow-ups after the three isolation passes.

### R10: the three commits after the audited baseline had no decision rows

Found by this audit and tracked by issue #78. `git log e5be9608dd..origin/main`
returned three commits (`849b5b8e07` pinned header on the first-run page,
`3dcdee6d2b` new Linux sandbox, `698368515d` Gemini thinking level) with no
decision row anywhere: `upstream-sync-2026-11-17-b.md` names the gap and issue
#63's range stops at `e5be9608dd`, so neither owned them.

The rows were written during this audit as
`docs/src/development/upstream/upstream-sync-2026-11-17-c.md`, which advances the
reviewed baseline to `698368515d` and adds three `C` rows to the ledger
(`diverged` for the pinned header, `absent-api` for the sandbox, `philosophy` for
the provider crates). Isolation had already decided all three: `849b5b8e07` is
0/2 hunks with the local file 412 diff lines from the pre-image, and the other two
touch paths that do not exist locally.

### Cross-platform checks are possible on this host

`cargo check` against the installed rust-std targets works for both platform
crates, so platform-gated rows can be type-checked even though they cannot be
run:

| Command                                                       | Result       |
| ------------------------------------------------------------- | ------------ |
| `cargo check -p gpui_macos --target aarch64-apple-darwin`     | PASS (1m12s) |
| `cargo check -p gpui_windows --target x86_64-pc-windows-msvc` | PASS (26s)   |

This does not replace runtime verification, and `xtask build`/`dist` still only
produce host artifacts inside the container, but it removes "cannot compile the
platform half" from the list of blockers.

### Ports applied after the isolation passes

Every row below was re-implemented on `c00ed3e478`, compiled, and exercised by
the tests named in its issue. None is committed, so every ledger row stays `C`.

| Upstream     | Issue | Local change                                                       | Test                                               |
| ------------ | ----- | ------------------------------------------------------------------ | -------------------------------------------------- |
| `91a6890d`   | #75   | sidebar onboarding banner keeps the title on its own flexible line | `cargo check -p sidebar`                           |
| `78f5a535a3` | #70   | `AgentDiff::set_active_thread` only for root threads               | `cargo check -p agent_ui`                          |
| `1ade7854`   | #71   | `MatchPositionHint`, `multi_line(true)`, one `find_iter` pass      | `cargo test -p project --test integration search`  |
| `34241d5f`   | #72   | `flattened_deserialize!` for the five settings containers          | `cargo test -p settings_content` and `-p settings` |
| `a7e23df6`   | #77   | `has_untagged_code_block` and a resolved fallback language         | `cargo test -p markdown --lib`                     |
| `bbd198f5`   | #76   | `HighlightKey::SemanticToken(u32)` and per-server precedence       | `cargo test -p editor --lib semantic_tokens`       |
| `6721ea2e`   | #69   | `reset_panel_sizes` and `resize_panel_sizes` on `Dock`             | `cargo test -p workspace --lib`                    |
| `5299a14401` | #73   | `MarkdownInputHandler` plus the visible-bounds hit test            | `cargo test -p markdown --lib`                     |

The `5299a14401` port needed one architectural adaptation. Upstream records each
rendered line's clip region in a per-line element's `prepaint`; ZZZ's markdown
tree has a single `impl Element` for `MarkdownElement`, so the clip region is
recorded once per block on `RenderedText` instead, and
`source_index_for_visible_position()` rejects points outside it. That path is
macOS-only at runtime and is type-checked here rather than run.

## Corrections applied

| Report | Correction                                                       | Local commit |
| ------ | ---------------------------------------------------------------- | ------------ |
| none   | No row reclassified; F1–F8 need ports, a follow, or report edits | n/a          |
| none   | R1–R6 stay `C` until the local commits in issues #69–#74 land    | n/a          |

The second-pass rows keep their class for the same reason: the ledger note is
the record of a decision, so a `C` row only becomes `B` once the port exists.

All of those reclassifications landed on 2026-11-17: `cd4902bfa2` became `B`
with the local bump (F2), `9bde578e` stopped claiming `A` while its port is
reverted (F5), the twenty-two rows in issue #67 became `C` because their crates
are gone (F7), and the nine rows behind R1-R9 became `B` with the local commits
named in those sections. Each port commit carries the `Upstream:` trailer, so
every one of those rows now has both halves of the record: the class says the
work exists locally, and the trailer says which upstream commit it came from.

## Ledger

- Rejection rows: 829 at the first pass, byte identical to the regeneration
- Rejection rows after `c00ed3e478` merged the 2026-11-17 batch: 837
- Rejection rows after the 2026-11-17 batch C report: 840
- Rejection rows after the F7 correction (issue #67): 862
- Rejection rows after the R1-R9 ports landed: 854, the eight rows that became
  `B` leaving the ledger
- Reason codes after the F8 correction (issue #68): see the table above
- Allowlist entries removed: 0; `66b6bbb6c8` still applies
- Allowlist entries added: 23, one per F7 row plus `9bde578e` (F5)
- `cargo xtask check upstream-ledger`: PASS (854 rejection rows)

## Verification

| Check                                                                         | Result  | Notes                                                               |
| ----------------------------------------------------------------------------- | ------- | ------------------------------------------------------------------- |
| `git status --porcelain` before the audit                                     | PASS    | clean worktree                                                      |
| `cargo xtask check upstream-ledger`                                           | PASS    | 854 rejection rows after the F7 correction and the R1-R9 ports      |
| `./script/backfill-upstream-ledger` output vs the ledger                      | PASS    | byte identical                                                      |
| Upstream SHA resolution for all 1627 recorded rows                            | PASS    | 0 missing                                                           |
| `git cat-file` / `merge-base --is-ancestor` for every recorded `A`/`B` commit | PASS    | 4 `B` version follows intentionally carry no local commit           |
| Upstream revert scan over 40877 commits                                       | PASS    | 2 reverts, both consistent with the record                          |
| Review-coverage recomputation                                                 | PASS    | 2 uncovered runs: `7b030b5008` and the 26-commit tail               |
| Local-survival scan of 752 absorbed ports                                     | PASS    | 79 flagged; two lost their behavior (F5, F6)                        |
| `script/prettier --write` on this report                                      | PASS    | formatted                                                           |
| Isolation re-test of the 127 `diverged`/`unisolatable` `C` rows               | PASS    | `git apply --check --verbose` per row; 5 portable, 2 misrecorded    |
| Isolation re-test of the remaining 710 `C` ledger rows                        | PASS    | `git apply --check --verbose` per row; 3 portable, 159 fully absent |
| Per-hunk applicability for the 11 near-miss rows                              | PASS    | single-hunk patches; behaviour hunk conflicts in every case         |
| Post-image survival scan of the 219 rows with a clean product file            | PASS    | 0 silently absorbed rows; 1 false positive (`52e0b848`, test file)  |
| Systematic blocker re-check of Oct–Nov `C` rows                               | PASS    | 49 rows name a symbol; none now exists in the named crate           |
| Subject-match scan of every `C` row against local commit subjects             | PASS    | 2 matches, both false positives (different PR numbers)              |
| Upstream commits after the `e5be9608` baseline                                | 3       | none recorded; tracked by issue #78                                 |
| `cargo check -p gpui_macos --target aarch64-apple-darwin`                     | PASS    | cross-target type-check works on this host                          |
| `cargo check -p gpui_windows --target x86_64-pc-windows-msvc`                 | PASS    | same                                                                |
| Ports applied after the isolation passes (8 rows)                             | PASS    | see the table above; nothing committed                              |
| macOS / Windows runtime behaviour                                             | NOT RUN | host is Linux                                                       |

The eight first-pass findings are left `C`/`B`/`A` as recorded and tracked as
issues #61, #62, #63, #64, #65, #66, #67, and #68. The second pass adds five
re-absorb findings, tracked as issues #69, #70, #71, #72, and #73, and one
record-correction finding for two rows, tracked as issue #74. The third pass adds
three more, tracked as issues #75, #76, and #77, and a coverage gap for the three
commits after the audited baseline, tracked as issue #78 and now recorded in
`upstream-sync-2026-11-17-c.md`. All 862 ledger rows have now been isolated at
least once.
