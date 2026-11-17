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

## Findings

### F1: `7b030b5008` was never classified and its panic guard is still missing

The commit (`project: Fix panic when an inlay hint response outlives its
buffer version (#61747)`, 2026-07-27) sits exactly on the boundary the
2026-08-07 report declares — that report starts its range _after_
`7b030b5008`, so it has no decision row in any table and no ledger entry.
The 2026-10-18 issue survey treated the same boundary as unclassified for
other commits; this one was missed.

The fix is still absent. Upstream returns early when the buffer version
changed during the fetch:

```rust
// crates/project/src/lsp_store.rs (upstream)
if lsp_data.buffer_version != query_version {
    return CacheInlayHints::default();
}
```

Local `crates/project/src/lsp_store.rs:8235` computes the same predicate
as `update_cache` but uses it only to gate writes, while
`crates/project/src/lsp_store.rs:8250` still reads
`lsp_data.inlay_hints.cached_hints(&chunk)` with the pre-await chunk id.
That read goes through `crates/project/src/lsp_store/inlay_hints.rs:129`,
which indexes `self.hints_by_chunks[chunk.id]` directly, so a buffer that
shrank during the refresh still panics — exactly the crash upstream fixed.
`invalidate_for_chunk` (`inlay_hints.rs:251`) and `fetched_hints`
(`inlay_hints.rs:133`) index the rebuilt vectors the same way.

Class: would be `B` once ported; it is currently unrecorded. Tracked by
issue #61.

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

Tracked by issue #62.

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

It stays `B`. No workspace dependency provides D-Bus access today
(`zbus` exists only under `vendor/trash`), so the Linux half needs a new
dependency decision before it can be ported. Tracked by issue #64.

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

Because `A` rows never enter the ledger, this commit is invisible to
re-review until the record is corrected. Tracked by issue #65.

### F6: `7c7b171ff`'s copy fix was lost to a later Markdown rework

`upstream-sync-2026-10-09.md:255` records the blockquote copy fix as `B`
with local commit `db6e989c54`, which added a `current_code_block_text`
accumulator and dropped the raw source slice. That accumulator no longer
exists: `db6e989c54:crates/markdown/src/markdown.rs` contains seven
references, `HEAD` contains none, and the file grew from 6790 to 7129
lines when a later Markdown rework replaced the region. The copy path is
back to the pre-fix shape at `crates/markdown/src/markdown.rs:3335-3341`,
which slices `parsed_markdown.source()[range]` and so copies the `>`
prefixes of an enclosing blockquote.

`parser::extract_code_block_content_range` still exists at
`crates/markdown/src/parser.rs:979`, so the fix has to be re-applied on
top of the current element code. Tracked by issue #66.

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

Because `A`/`B` rows never enter the ledger, none of the twenty-two has a
dedup entry and the next audit re-reads them as absorbed. The isolation
test now fails check 3 for all of them. The rows are listed in issue #67,
which also proposes the report and ledger correction; the classes are left
untouched here for the same reason as F2 and F5.

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

## Corrections applied

| Report | Correction                                                       | Local commit |
| ------ | ---------------------------------------------------------------- | ------------ |
| none   | No row reclassified; F1–F8 need ports, a follow, or report edits | n/a          |

The 2026-11-11 `cd4902bfa2` row should become `B` with a local bump
commit (F2), the 2026-08-18 `9bde578e` row should stop claiming `A` while
its port is reverted (F5), and the twenty-two rows listed in issue #67 should
become `C` now that their crates are gone (F7); none of these
reclassifications is applied while the record still lacks the local
commit or edit that justifies it.

## Ledger

- Rejection rows: 829 before and after regeneration, identical bytes
- Allowlist entries removed: 0; `66b6bbb6c8` still applies
- `cargo xtask check upstream-ledger`: PASS (829 rejection rows)

## Verification

| Check                                                                         | Result  | Notes                                                     |
| ----------------------------------------------------------------------------- | ------- | --------------------------------------------------------- |
| `git status --porcelain` before the audit                                     | PASS    | clean worktree                                            |
| `cargo xtask check upstream-ledger`                                           | PASS    | 829 rejection rows                                        |
| `./script/backfill-upstream-ledger` output vs the ledger                      | PASS    | byte identical                                            |
| Upstream SHA resolution for all 1627 recorded rows                            | PASS    | 0 missing                                                 |
| `git cat-file` / `merge-base --is-ancestor` for every recorded `A`/`B` commit | PASS    | 4 `B` version follows intentionally carry no local commit |
| Upstream revert scan over 40877 commits                                       | PASS    | 2 reverts, both consistent with the record                |
| Review-coverage recomputation                                                 | PASS    | 2 uncovered runs: `7b030b5008` and the 26-commit tail     |
| Local-survival scan of 752 absorbed ports                                     | PASS    | 79 flagged; two lost their behavior (F5, F6)              |
| `script/prettier --write` on this report                                      | PASS    | formatted                                                 |
| Full isolation re-test of all 798 `C` rows                                    | NOT RUN | sampled by reason code and by absent-path claims          |
| Systematic blocker re-check of Oct–Nov `C` rows                               | PASS    | 49 rows name a symbol; none now exists in the named crate |
| Subject-match scan of every `C` row against local commit subjects             | PASS    | 2 matches, both false positives (different PR numbers)    |
| macOS / Windows runtime behaviour                                             | NOT RUN | host is Linux                                             |

The eight findings above are left `C`/`B`/`A` as recorded and tracked as
issues #61, #62, #63, #64, #65, #66, #67, and #68.
