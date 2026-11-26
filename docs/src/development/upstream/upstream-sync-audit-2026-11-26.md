---
title: Upstream Sync Audit 2026-11-26
description: Re-audit of every recorded Zed upstream rejection and already-equivalent claim up to the 2026-11-24 reviewed baseline.
---

# Upstream Sync Audit 2026-11-26

## Scope

- Local branch: `main` at `b3a1def633`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Re-audited baseline: `f00a94318c059f03892e04f82343e899fbfaa844`, installed by
  `upstream-sync-2026-11-24.md`
- Range start: `7b030b5008`, the first review window
- Excluded: `f00a94318c..` live head, which
  `sync/upstream-2026-11-26` is absorbing concurrently
- Commits in range: 1685, every one already decided as A 276, B 522, C 887
- Rejection ledger rows: 909 class `C`; 607 of them touch a path that still
  exists on `main`
- Already-equivalent claims re-checked: 36 class `A` rows with no local commit

This audit re-checks decisions, not the trees: every recorded `C` whose
touched paths survive locally was compared against the current local code,
and every "already equivalent" claim was compared against the upstream fix
it claimed to match. Nothing here re-absorbs a commit or edits a class.

## Method

The 607 code-touching rejections were partitioned by crate into five
batches (`ai`, `core-editor`, `core-project`, `gpui`, `misc`) and judged
against `git show main:<path>` for the local side and a local clone of
upstream at the baseline for the other. Each row received one verdict:
`STILL-C` (the rejection still holds), `ALREADY-LOCAL` (the fix is present
or moot), or `NOW-WORTH` (a live local defect with no rejected surface).
The `misc` batch overlapped the other four completely and served as an
independent second opinion; eleven rows disagreed and were adjudicated
against the code. Rows whose verdict was `STILL-C` only because a port
would need hand rewriting, while the defect itself is present locally, are
listed below as worth absorbing, not as valid rejections.

## Totals

| Outcome                                     | Rows |
| ------------------------------------------- | ---- |
| Still rejected                              | 536  |
| Already local, claim correctly recorded     | 46   |
| Worth absorbing                             | 25   |
| Already-equivalent claim that does not hold | 6    |

## Stale premises in the ledger

The dominant error is not bad judgement but outdated premises. Rejection
notes cite pins, crates, and APIs that changed after the note was written:

- `agent-client-protocol` is pinned to `=5.6.1` in `Cargo.toml:503` and
  resolves schema 1.22.0, newer than the baseline's `=5.5.1`. Notes that
  say ZZZ pins 2.2.0, 3.5.0, 5.0.0, or 5.3.0 are wrong. The whole ACP v2
  family stays rejected for a different and durable reason: ZZZ does not
  enable the `unstable_protocol_v2` feature, and
  `crates/agent_servers/src/` is a flat v1 `acp.rs`, not the upstream
  `acp/{v1,v2,transport,session_updates}.rs` split.
- `gpui_apple`, `gpui_ios`, `gpui_web`, `gpui_platform`, and `gpui_wgpu`
  are live workspace members. The `MainThreadMarker`/`MainThreadOnly`
  machinery is present in `crates/gpui_apple/src/display_link.rs` and
  `crates/gpui_apple/src/gpui_apple.rs`, and every
  `crates/gpui_macos/src/*.rs` file is already `objc2`-based. Rejections
  justified by an absent Apple layer no longer apply.
- `crates/panel` does not exist locally, so `ab7afbff` was already
  absorbed; the note mistook the `Workspace::panel::<GitPanel>()` method
  for a crate.
- A GPG askpass wrapper does exist (`crates/askpass/src/askpass.rs:538`);
  `e645f02245` is already equivalent because the caller catches the
  missing-binary error and falls back to `(None, None)`.

## Already-equivalent claims that do not hold

Six of the 36 rows classified `A` with no local commit are still pre-image
locally, so the fix was silently dropped:

| Upstream   | Missing local counterpart                                                                                                                                      |
| ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `2ec29977` | `crates/zzz/src/zzz.rs:973` ignores `action.create_new_window`; `open_new_ssh_project_from_project` builds `OpenOptions` without a requesting window.          |
| `65a5c89a` | `Buffer::did_finish_parsing` (`crates/language/src/buffer.rs:1840`) has no `parse_again` parameter and unconditionally calls `request_autoindent`.             |
| `51db7df7` | The markdown table container (`crates/markdown/src/markdown.rs:3262`) still ends in a bare `.overflow_x_scroll()` with no `custom_scrollbars`.                 |
| `9dbc9e76` | `crates/migrator/src/migrations.rs:200` still registers `m_2025_06_16`, and `m_2025_06_27/settings.rs` still requires `source == "custom"`.                    |
| `f56ff65c` | `previous_word_start` (`crates/editor/src/movement.rs:257`) still lacks the `punctuation_is_prefix` guard, so Ctrl+Backspace splits `a-b-c` one character off. |
| `4b407d0f` | The #58882 punctuation-prefix block the revert deletes is still at `crates/editor/src/movement.rs:301-320`, so the revert's premise was never satisfied.       |

`f56ff65c` and `4b407d0f` are a pair. Local code is a hybrid of #62224 and
#58882, so replaying the whole revert would also undo the recorded
`7eee42c9ef` decision; take only the guard from `f56ff65c`.

`f851d82e` is the one row that is genuinely not applicable: ZZZ has no
`crates/edit_prediction`.

## Already local (46 rows)

These keep a `C` in the ledger but the behavior is present on `main`, so
they should leave the backlog instead of being re-read every batch:

```
0373366f43 07df438626 0a9eb81e6c 10cfb45944 17d71d2b 21001b273f 2440236055
2d6c664dc2 37568ad7dc 40180d9c40 47c46191f4 4ec98df500 4fd48c02ac 52894d3f
52b24181 54fff2ff5b 56303a3372 5e8998a2a7 687d662a0b 6b5e15ed46 703b5ac3f4
7759e9f9 86b2cf96 897ba9ad 94f3eb1b72 99f947e5f6 9ab0715969 9bde578e
a47f721832 a60addb9 a8b57a25 ab7afbff b1962f7cd8 c24358d9 c88c83d619
cb1937a8e7 d3bba51492 d86aa6e6b9 e645f02245 e683fd7b46 e83f1c9495
e91b82c106 f1ccf949d5 f37989fbdf f495dd0f77 fd2aedc471
```

Three that matter beyond bookkeeping: `0373366f43` is the M5 Mac frame
pacing (`crates/gpui_apple/src/display_link.rs`), `897ba9ad` is remote
markdown image loading (`markdown_preview_view.rs:29` already routes
through `project::image_store`), and `56303a3372` is audio output
recovery (`audio_pipeline.rs` already has `OutputSink`,
`output_needs_rebuild`, and `with_error_callback`).

## Worth absorbing

Small and mechanical: the patch fits the local pre-image with no rejected
surface.

| Upstream     | Fix                                                        | Local anchor                                                      |
| ------------ | ---------------------------------------------------------- | ----------------------------------------------------------------- |
| `6dee3fc7`   | Keep `related_information` for `textDocument/codeAction`   | `crates/language_core/src/diagnostic.rs` has no field             |
| `3e442f25`   | Use the first-class ACP `ToolCall.name`                    | `crates/acp_thread/src/acp_thread.rs:62` meta workaround          |
| `c6e0868c`   | Vim `O` must not re-indent the following line              | `crates/vim/src/normal.rs:728`; add `edit_before_with_autoindent` |
| `0ad5441b`   | `align_selections` by display column                       | `crates/editor/src/editor.rs:9507` uses `point.column`            |
| `fa8d0b77`   | Abort after a Windows panic dump                           | `crates/crashes/src/supported.rs:466` only simulates              |
| `ce6f3af5`   | Drop `-X`/`-Y` before scp/sftp                             | `crates/remote/src/transport/ssh.rs:1720`                         |
| `017c1398fd` | Remote-project list needs no 20rem floor                   | `crates/recent_projects/src/remote_servers.rs:3487`               |
| `849b5b8e07` | Keep the onboarding header out of the scroll container     | `crates/onboarding/src/onboarding.rs:256`                         |
| `66ed3027`   | Fullscreen toggle must show its state                      | `crates/agent_ui/src/agent_panel.rs:4317`                         |
| `93f6b2e5`   | Bump `pet` so macOS stops prompting for Command Line Tools | `Cargo.toml:780` still pins `9e61a22a`                            |
| `e0931d5a`   | Reuse char-scan invisibles detection                       | `crates/editor/src/display_map.rs:1426` still walks graphemes     |
| `389c22e1c7` | Derive the secondary modifier from `PlatformInfo`          | `crates/gpui/src/platform/keystroke.rs:491` is `cfg`-based        |
| `8968bf78`   | Decode non-UTF-8 blobs for project diffs                   | `crates/git/src/repository.rs:1614` returns `Err`                 |

Medium: a real local defect, but the upstream patch cannot be cherry-picked
because the surrounding file diverged. Each needs a hand port.

| Upstream     | Fix                                                     | Local anchor                                                                                                          |
| ------------ | ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| `1416404b88` | Discarding unstaged changes must keep staged content    | `crates/git_ui/src/git_panel.rs:2004` calls `checkout_files("HEAD", …)`; checkpoints already exist in the `git` crate |
| `7ecab315`   | Stop project search holding closed unsaved buffers      | `crates/search/src/project_search.rs:383` still `remove_deleted_buffers`                                              |
| `2240542744` | `file_types` must match in single-file worktrees        | `crates/worktree/src/worktree.rs:1722` `full_path` returns a worktree-relative path                                   |
| `66432e4ca9` | Map canonical LSP paths back into symlinked directories | no `relative_path_for_external_abs_path` locally                                                                      |
| `ab7d21d4`   | Long-poll workspace diagnostics and honour `retrigger`  | `crates/project/src/lsp_store.rs:14829` still times out and re-sends                                                  |
| `1e2e422c`   | Sync extensions on existing-connection remote opens     | `crates/workspace/src/workspace.rs:9755` never touches the extension store                                            |
| `4bdf188c`   | Stash tracked and staged separately                     | `crates/git/src/repository.rs:2380` hardcodes `--include-untracked`                                                   |
| `64d14ea8`   | Fall back to the system pinentry for gpg signing        | `crates/askpass/src/askpass.rs:538` has loopback only                                                                 |
| `45077524`   | Prevent idle sleep while an ACP turn streams            | `crates/gpui/src/platform.rs:410` is a no-op default; needs macOS, Windows, and Linux backends                        |
| `4a9a863c`   | Do not indent from a block-comment closing line         | auto-indent still uses the closing line as its basis                                                                  |
| `a473ea63`   | Drop the previous atlas entry when the image reloads    | `crates/image_viewer/src/image_viewer.rs` never releases; `gpui::Window::drop_image` already exists                   |

Large, and only worth it as its own change: `eb548352` rewrites the
markdown paint path (625 lines in `crates/markdown/src/markdown.rs`) to
clip highlights per line. The overflow is real locally.

One borderline row is left `C`: `200fb85c` fixes incomplete bracket matches
during error recovery, and the bracket cache it needs already exists
inline in `crates/language/src/buffer.rs`, so the "unabsorbed bracket-cache
architecture" note is wrong; the fix itself still needs the error-node
walk ported into that inline code.

## Ledger hygiene

1. `3ab0b5df18` is filed as `philosophy` but the note describes a
   divergence from local #54009, and the commit fixes an ownership cycle
   in ACP session loading, which is live local code in `acp.rs`. It should
   be reclassified and re-reviewed instead of staying in the policy bin.
2. `script/backfill-upstream-ledger` skips `upstream-sync-audit-*.md`, so
   the 86 class `C` rows whose only source is
   `upstream-sync-audit-2026-09-28.md` never reach the ledger. Review
   windows only move forward, so this does not cause duplicate work, but
   the dedup index under-reports.
3. `a4916265` carries both a `B` row with local commit `89c1511729` and a
   later `C` row. That local commit, and nine others recorded by
   `upstream-sync-audit-2026-09-22.md`, no longer exist as objects after a
   history rewrite. Their content was verified present by reading `main`,
   except the `project_search` row, whose local file has since diverged.

## Verification

| Check                                                     | Result  | Evidence                                             |
| --------------------------------------------------------- | ------- | ---------------------------------------------------- |
| Commit coverage of the audited range                      | PASS    | 1685 upstream commits, 1685 decision rows            |
| Rejection rows accounted for                              | PASS    | 909 ledger rows, 607 code-touching, 607 verdicts     |
| Partition deduplication                                   | PASS    | 659 listed rows collapse to 607 unique SHAs          |
| Spot checks of `NOW-WORTH` claims against `main`          | PASS    | 20 rows read directly, all confirmed                 |
| `cargo test -p language --lib`                            | PASS    | 153 passed                                           |
| `cargo test -p editor --lib`                              | PASS    | 953 passed, 1 ignored                                |
| `cargo test -p markdown --lib`                            | PASS    | 236 passed, including the new wide-table test        |
| `cargo test -p languages --lib css`                       | PASS    | 1 passed                                             |
| `cargo check -p zzz`                                      | PASS    | one pre-existing unused-dependency warning, no error |
| `cargo xtask clippy -p markdown -p language -p editor`    | PASS    | exit 0, no error                                     |
| `cargo xtask clippy -p languages -p zzz`                  | PASS    | exit 0, no error                                     |
| `rustfmt --edition 2024 --check` on the six changed files | PASS    | clean                                                |
| macOS and Windows runtime behavior                        | NOT RUN | host is Linux                                        |
| Dependency vulnerability scan                             | NOT RUN | no `cargo-audit` or `osv-scanner` on the host        |

## Landed after this audit

Four of the six broken equivalence claims are fixed on
`audit/upstream-fixes-2026-11-26`, one commit each, every one matching its
upstream counterpart's semantics:

| Upstream                                   | Local commit | Scope                                                                                                      |
| ------------------------------------------ | ------------ | ---------------------------------------------------------------------------------------------------------- |
| `f56ff65c92b6346c16f1fed846fc736b39d9c71f` | `a1c8de670a` | `previous_word_start` punctuation-prefix guard, with the assertions moved onto the `skip_punctuation` path |
| `65a5c89a9ea2a7ed852d4ed2983621124072677b` | `be05af06be` | `did_finish_parsing` defers auto-indent and `ParseStatus::Idle` while a reparse is queued                  |
| `51db7df7501e672fdfc9fa4875f43215329fc0d9` | `1fd719d600` | Horizontal scrollbars on wide markdown tables, plus a regression test                                      |
| `2ec2997789ead364eb53b23972127d7b82b7002f` | `e34deeb8d6` | Existing-connection remote opens honour `create_new_window` and `default_open_behavior`                    |

The guard in `f56ff65c` was proven by removing it: without it
`a-b-ˇcˇ` lands on `DisplayPoint(0, 3)` instead of `DisplayPoint(0, 4)`.

## Not performed

- `9dbc9e76` (drop `m_2025_06_16`, rewrite `m_2025_06_27`) stays open. The
  upstream change edits 354 lines of `migrator.rs` and depends on the order in
  which migration output feeds later migrations, which the local tree has
  diverged from. It needs its own review, not a batch pick.
- `f851d82e` is not applicable: this tree has no `crates/edit_prediction`.
- No ledger row was reclassified by this audit. The 6 broken equivalence
  claims and the 25 worth-absorbing rows are findings; only the four listed
  above are closed.
- `f00a94318c..` live head was not audited; `sync/upstream-2026-11-26`
  owns that range.
