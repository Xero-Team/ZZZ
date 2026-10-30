---
title: Upstream Sync 2026-10-29
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-10-29

## Batch 1 {#batch-1}

### Scope

- Local base: `main` at `61c947bee9`
- Target branch: `sync/upstream-2026-10-29-sync`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `37faef81f74fc1bb08be9939c4cffc8ede6ced2f`
- Live upstream head queried: `e20d19d32f25734616243f15b7bfecefc633b8d7`
- Query time: `2026-10-29`
- Reviewed range: the first 20 commits after `37faef81`, oldest first
- Remaining after this batch: 18 commits through the queried live head

The previous reviewed baseline is an ancestor of the queried head. The
range contains 38 commits; this batch covers the first 20, up to and
including `f0210f1621`. The next batch starts at `2c2f737ab2`.

### Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                                                            |
| ---------- | ----- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 5e8998a2a7 | C     | --           | Moves ZZZ to agent-client-protocol 3.6.0 with schema 1.13 and rewrites the `acp` bridge around the v2 schema. ZZZ's `acp_thread` and `agent_servers` use only `schema::v1`; there is no `acp_v2` module.                                                               |
| 66b6bbb6c8 | C     | --           | Release metadata: bumps the app version and rewrites `crates/zed` release tooling, which ZZZ does not carry.                                                                                                                                                           |
| 2580a00264 | B     | 8b5f601787   | Applies frame limits consistently and stabilizes frame pacing through `FramePacer`. Replayed on ZZZ's `complete_frame` scheduling.                                                                                                                                     |
| 9521840152 | C     | --           | Reimplements `PathStyle::join` on a new `crates/path` component iterator with `styled_push`. ZZZ's `PathStyle` lives in `util`, has no `Components`, and every local caller passes a relative user name.                                                               |
| fae4b2ea4f | B     | 18083ad35d   | Opens local files in a local workspace even while a remote workspace is active, via the new `ProjectHost`. Ported with the local `WorkspaceOpenTarget` shape.                                                                                                          |
| 44ec5691b5 | B     | c20e2e91f5   | Starts the macOS ligature-breaking font-size alternation at the requested size. Replayed into `gpui_macos`.                                                                                                                                                            |
| 2daea4d488 | C     | --           | Fixes native-agent auto-compaction. `crates/agent/src/thread.rs` was deleted with the native agent tools (`dcb03e8894`), and no compaction state remains locally.                                                                                                      |
| 8767f1530c | C     | --           | Repairs interrupted npm installs in the shared registry-agent install directory. ZZZ runs registry agents through `npm exec`, which installs into npm's `_npx` cache and never creates `<install_dir>/node_modules`, so the directory this fix repairs does not exist. |
| c693e6a893 | C     | --           | Fixes a crash while expanding an ACP subagent card; depends on `acp_v2` sessions and the rejected `subagent_card.rs` layout.                                                                                                                                           |
| d697904d0e | C     | --           | Links the two views of an ACP session message through `acp_v2` message ids and `subagent_card.rs`; both are absent locally.                                                                                                                                            |
| 33d111eac4 | C     | --           | Renders each linked ACP subagent message as its own card, extending the same rejected card layout.                                                                                                                                                                     |
| 7de7bfdba2 | C     | --           | Opens a stopped subagent card's part in full screen; there is no subagent card locally.                                                                                                                                                                                |
| 2e794a1ea7 | C     | --           | Reports native subagent state like ACP subagents; native threads and the subagent card layout are both absent.                                                                                                                                                         |
| 4d847859f5 | B     | 3614b8c8f9   | Gives each Wayland activation token its own pending action and records the focused surface with each press. Adapted to ZZZ's `u32` serial tracker.                                                                                                                     |
| 2e57653724 | B     | 00bad249e6   | Removes the local-only `PathStyle` guard for repository identity paths and threads `PathStyle` through `Worktree::directory_name`. Adds the missing `PathStyle::parent` helper.                                                                                        |
| e5bdd01cd8 | C     | --           | Truncates long lines in the native agent `read_file` tool; ZZZ has no `read_file_tool.rs` because the native tools were removed.                                                                                                                                       |
| 02d47bf64f | B     | 0d707a8d92   | Unifies asset downloading and compares SHA-256 digests case-insensitively. The crate split and executor-spawn API were omitted; ZZZ's module already has the staging/cleanup behavior.                                                                                 |
| 7f9a39a9c0 | C     | --           | Bumps `livekit`/`libwebrtc`/`webrtc-sys` to pick up `zip` 9.1 and clear three advisories. ZZZ carries no livekit or WebRTC dependency.                                                                                                                                 |
| 1663af412c | C     | --           | Splits LSP data refresh into per-buffer tasks and cleans up stale multibuffer data. `editor.rs` differs from the upstream pre-image by roughly 17k changed lines and every hunk conflicts; a faithful port needs a dedicated re-base of the LSP data pipeline.         |
| f0210f1621 | B     | 2fd0bfd2f1   | Wraps text with UAX #14 line breaking from `icu_segmenter`, replacing the hand-maintained `is_word_char` table.                                                                                                                                                        |

Totals: zero `A`, seven `B`, thirteen `C` across twenty commits.

### Applied Work

- `8b5f601787` is a `B` port of `2580a00264`.
- `18083ad35d` is a `B` port of `fae4b2ea4f`.
- `c20e2e91f5` is a `B` port of `44ec5691b5`.
- `3614b8c8f9` is a `B` port of `4d847859f5`.
- `00bad249e6` is a `B` port of `2e57653724`.
- `0d707a8d92` is a `B` port of `02d47bf64f`.
- `2fd0bfd2f1` is a `B` port of `f0210f1621`.

No commit in this batch was a clean `A`; each candidate either touched a
crate ZZZ does not carry or needed a local API shape before it fit.

### Per-Commit Narrative

#### 2580a00264 — frame limits and pacing (B)

The upstream patch routes frame timing through a `FramePacer` that clamps
the requested interval to the platform's supported range and only
advances the next frame deadline once a frame is actually scheduled. ZZZ
already clamps intervals in `Window::request_frame`, but it computed the
next deadline from the previous request rather than from the delivered
frame, so a slow frame could queue a burst of follow-up frames. The port
adds `FramePacer` in `crates/gpui/src/window.rs`, keeps ZZZ's
`complete_frame()` hook, and drops the upstream assertion on
`frame_scheduled()` because ZZZ's platform backends schedule frames
outside the pacer.

#### fae4b2ea4f — local files in a remote session (B)

`Workspace::open_paths` used to reuse whichever workspace matched the
paths, even when that workspace belonged to a remote host. The port adds
`project::ProjectHost<'a>`, makes `find_existing_workspace` synchronous
over `WorkspaceOpenTarget`, adds `preferred_target_for_files`, and filters
candidate workspaces by `matches_requested_host`. The
`recent_projects/remote_connections.rs` caller and the `zzz` open
listener tests were updated, and the upstream regression test
(`test_local_files_reuse_local_workspace_when_remote_workspace_is_active`)
was ported; it fails on the unpatched tree.

#### 44ec5691b5 — macOS font-size alternation (B)

`MacTextSystemState::layout_line` alternated between the requested size
and `next_up()` starting from the latter, which shaped single-run lines
one step too large. The port starts at the requested size and only
advances for runs that insert text. Upstream keeps this in `gpui_apple`;
ZZZ's macOS text system is `gpui_macos`, and the bundled mono family is
`.ZZZMono`. Both regression tests are macOS-only and cannot be compiled or
run on this Linux host.

#### 4d847859f5 — Wayland activation requests (B)

Each `xdg_activation_v1` token now carries its own pending
window/URI/path action as user-data instead of sharing a single
`pending_activation` slot, so concurrent requests can no longer overwrite
each other. Presses record the surface that had focus,
`WaylandWindow::activate` prefers that context and otherwise still
requests a token for the target surface, and `open_uri`/`reveal_path` use
the input context when available before falling back to the tokenless
open. ZZZ's `SerialTracker` stores plain `u32` serials rather than the
`Serial`/`SelectionSerial` wrappers upstream grew later, so
`ActivationContext` carries a `u32` and non-press pointer events keep
recording their serial as before.

#### 2e57653724 — repository identity paths (B)

`RepositorySnapshot::linked_worktree_anchor_path` is now public and falls
back to `repo_identity_path`, so a bare repository stored as `foo.git`
keeps its identity name instead of losing the anchor. `Repository`
delegates to the snapshot, `repo_identity_path_if_local` is deleted, and
`Worktree::directory_name` takes a `PathStyle` so the worktree picker,
its create/delete/sort paths, the sidebar worktree menu, and worktree
creation all interpret foreign separators correctly. Upstream calls
`PathStyle::parent` from the `path` crate; ZZZ's `PathStyle` lives in
`util` and had no `parent`, so the port adds a byte-wise
`PathStyle::parent` with its own test. The sidebar and title bar halves of
the upstream diff were already effectively present locally.

#### 02d47bf64f — asset downloads (B)

Upstream splits `http_client::github`/`github_download` into a new
`github_releases` crate, changes both download entry points to spawn on a
`BackgroundExecutor`, and converges every caller on one download path.
ZZZ's `github_download` module already rejects failed HTTP responses,
stages downloads next to the destination, verifies digests while
streaming, cleans up partial work, and renames into place, so the crate
split and the API change were omitted. The one behavior still missing was
the digest comparison: ZZZ required an exact-case match, while release
metadata may spell hex digests in upper case. `verify_sha256` now compares
case-insensitively, with the upstream uppercase-digest regression test
ported to ZZZ's test style.

#### f0210f1621 — UAX #14 line breaking (B)

`LineWrapper::break_indices` now delegates to
`icu_segmenter::LineSegmenter::new_for_non_complex_scripts`, and
`is_word_char` is gone. This fixes breaks that depend on neighbouring
characters: `/` and `?` no longer start a wrapped line, and emoji
sequences such as 👍🏽, flags, and ZWJ families stay intact. The grapheme
handling in `line_layout.rs`, the refreshed wrapping tests, and the
adjusted `test_chunks_with_soft_wrapping` expectation were taken.
ZZZ's `is_word_char` had local tweaks (treating `⋯` and trailing
punctuation as word characters); those are subsumed by UAX #14 instead of
being kept as a second code path. `Cargo.lock` resolves `icu_segmenter`
2.4.0 rather than upstream's 2.3.0; the ICU crates were already in the
graph through `idna`, so only the new package edge appears.

#### 1663af412c — multibuffer LSP data (C)

The patch splits one combined LSP data refresh into per-buffer tasks and
adds explicit cleanup for code lenses, diagnostics, document colors,
links, symbols, and folding ranges when excerpts change or a buffer leaves
the multibuffer. Locally, `editor.rs` differs from the upstream pre-image by roughly
17k changed lines (the LSP data refresh is organized differently), and
every hunk in all eight files conflicts. Reconciling it means re-basing the whole LSP data
pipeline, not porting a hunk, so this is recorded as a `C` with the
behavior still wanted. It is a good candidate for a dedicated change that
starts from the local refresh structure and ports the per-buffer
cancellation and cleanup semantics from this commit.

#### 8767f1530c — npm registry agents (C)

The upstream fix serializes `npm install` per install directory, then
verifies that every package directory under `<install_dir>/node_modules`
has a `package.json` and reinstalls once when one is missing. ZZZ's
registry client was rewritten (`4a5c33c225`) to run `npm exec --yes --
<package>` inside the prefix directory, and `npm exec` installs into npm's
`_npx` cache rather than `<install_dir>/node_modules`; a local check with
npm 11.19.1 confirmed the prefix directory stays empty. There is no
install directory for the guard to repair.

#### Agent and ACP subagent series (C)

`2daea4d488`, `c693e6a893`, `d697904d0e`, `33d111eac4`, `7de7bfdba2`,
`2e794a1ea7`, and `e5bdd01cd8` all operate on code ZZZ intentionally does
not carry. The native agent thread and its tools were removed in
`dcb03e8894`, so there is no `thread.rs`, `read_file_tool.rs`, or
compaction state. The ACP subagent card layout was rejected earlier
(`d93bf74085`) because ZZZ's conversation view opens subagents as separate
threads and has no `acp_v2` sessions; these follow-ups extend that same
layout.

#### 7f9a39a9c0 — WebRTC zip advisory (C)

The bump exists to move `webrtc-sys-build` from `zip` 0.6 to 9.1 while
extracting prebuilt WebRTC archives. ZZZ has no `livekit`, `libwebrtc`, or
`webrtc-sys` dependency in the workspace manifest, so there is no build
script or advisory path to update.

### Verification

| Check                                                                              | Result  | Notes                                               |
| ---------------------------------------------------------------------------------- | ------- | --------------------------------------------------- |
| `cargo test --locked -p gpui --lib window::tests`                                  | PASS    | 19 tests after `8b5f601787`                         |
| `cargo test --locked -p recent_projects --lib`                                     | PASS    | 36 tests, including the new local/remote reuse test |
| `cargo test --locked -p workspace --lib`                                           | PASS    | 247 tests                                           |
| `cargo check --locked -p zzz --all-targets`                                        | PASS    | open listener caller updated                        |
| `cargo test --locked -p gpui_linux --lib`                                          | PASS    | 26 tests after `3614b8c8f9`                         |
| `cargo test --locked -p util --lib parent_uses_path_style`                         | PASS    | new `PathStyle::parent` test                        |
| `cargo test --locked -p git --lib directory_name`                                  | PASS    | upstream test with `PathStyle::Posix`               |
| `cargo test --locked -p project --lib linked_worktree_anchor`                      | PASS    | upstream anchor test                                |
| `cargo test --locked -p git_ui --lib bare_repository_worktree_uses_generated_name` | PASS    | picker name path                                    |
| `cargo test --locked -p http_client --lib github_download`                         | PASS    | 6 tests, including uppercase digest                 |
| `cargo test --locked -p gpui --lib text_system::line_wrapper`                      | PASS    | 21 tests after UAX #14                              |
| `cargo test --locked -p editor --lib test_chunks_with_soft_wrapping`               | PASS    | updated expectation                                 |
| `cargo check --locked --workspace --all-targets`                                   | PASS    | full workspace after all seven ports                |
| `cargo fmt --all -- --check`                                                       | PASS    | clean                                               |
| macOS / Windows runtime checks                                                     | NOT RUN | host is Linux; `gpui_macos` is macOS-only           |
| `cargo test --workspace`                                                           | NOT RUN | narrowest per-crate checks were used                |
| Remaining commits through the queried head                                         | PENDING | 18 commits; next baseline starts after `f0210f1621` |

The reviewed baseline advances to
`f0210f1621c6be9df4510a4356fa5049680e1c01` for this batch.

## Batch 2 {#batch-2}

### Scope

- Local base: `8eb1dcf53b` on `sync/upstream-2026-10-29-sync`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `f0210f1621c6be9df4510a4356fa5049680e1c01`
- Live upstream head queried: `d4014230735e4265d98066420b9271df2324ccab`
- Query time: `2026-10-30`
- Reviewed range: `f0210f1621..d4014230735e`, nineteen commits, oldest
  first
- Remaining after this batch: zero; the range ends at the queried head

The previous baseline is an ancestor of the queried head, and the whole
remaining range fits in one batch, so this batch closes the `2026-10-29`
sync. The next sync starts after `d401423073`.

### Decisions

| Upstream   | Class | Local commit           | Disposition                                                                                                                                                                                                                                                                                                                                                      |
| ---------- | ----- | ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2c2f737ab2 | C     | --                     | Adds a `Send`-able foreground executor so wasm code can dispatch foreground futures from background workers, lifts the web `Application` out of `gpui_platform`'s compile-time wasm branch, and renames the binary in fifteen manifests. ZZZ builds no wasm target and has no web HTTP client; the port would be an unused executor variant plus manifest churn. |
| 7c8dbadcb5 | B     | bbbe9043d4             | Makes Low Power Mode and thermal-state frame intervals configurable per window and takes the longest applicable interval instead of walking a fixed ladder. The profiler journal skip reason is the only omission.                                                                                                                                               |
| 87ce147829 | C     | --                     | Gives the git graph refs their own resizable column. It is built on the unabsorbed `fcd0f76952` table refactor (`TableRow<bool>` visibility, `redistribute_hidden_fractions`, `toggle_column_visibility`); a three-way application against `crates/git_graph` produced twenty conflicts, so it is not isolatable.                                                |
| 9cb5f90e3c | B     | a51905c8c9             | Drops the synthetic `WM_MOUSEMOVE` that Windows emits while repainting under a stationary cursor, so repaints no longer move the editor cursor. Windows-only; reviewed by inspection on this Linux host.                                                                                                                                                         |
| 28917a970d | B     | de725f2903             | Keeps helix registers off the system clipboard while `vim.use_system_clipboard` is `always`, and rebinds `R` to `HelixReplaceWithYanked`.                                                                                                                                                                                                                        |
| 4112666801 | A     | 63bf38b8f8             | Clean cherry-pick; refreshes the macOS Spanish-ISO key-equivalent table in `gpui_macos`.                                                                                                                                                                                                                                                                         |
| 5a534d7b5d | B     | df4bbfd91b             | Adds an off-by-default `git.auto_fetch` setting that periodically fetches in trusted repositories with every interactive prompt disabled. Rebuilt on ZZZ's `GitBinary` askpass plumbing instead of `git_ui_core`.                                                                                                                                                |
| 99cda17e05 | B     | c97e8c49b2             | Falls back to user-level MCP settings when no worktree is visible, so the first ACP thread still receives the user's context servers.                                                                                                                                                                                                                            |
| e8e579d6ea | C     | --                     | Fixes project skill mentions, share links, and deletion in remote workspaces. ZZZ has no project skills: `crates/agent_skills`, `skills_setup.rs`, and the `Project` skill methods are all absent.                                                                                                                                                               |
| 2eb2928c15 | B     | eb4f1d99f5             | Keeps worktree hooks and local task settings unapplied until the worktree is trusted, and replays the pending bundle on trust. The collab trust source is omitted.                                                                                                                                                                                               |
| 3524cb2297 | C     | --                     | Saves new project skills to the remote workspace. Same missing project-skill support as `e8e579d6ea`; there is no `skill_creator.rs` locally.                                                                                                                                                                                                                    |
| bc7041c600 | B     | 08b09579a8             | Flushes a lone modifier keystroke after the pending-input timeout instead of leaving it in the sequence for the rest of the session.                                                                                                                                                                                                                             |
| 33156f0d81 | C     | --                     | Streams language-model background events through `async-channel`. ZZZ's `crates/language_model` is an eight-line shim with no `stream_in_background` event pipeline to reroute.                                                                                                                                                                                  |
| 2529740084 | A     | -- (main `0610da51e9`) | Wraps the GPUI arena in an `Rc` to remove aliasing UB. ZZZ's own equivalent landed on `main` as `0610da51e9`; it is not replayed here because this branch is not an ancestor of `main` and the fix already exists in the merge target.                                                                                                                           |
| 5882c67dc3 | C     | --                     | Deletes unreferenced notification store code and rewrites collab notification tests. `crates/notifications` holds only `notifications.rs` and `status_toast.rs` locally; there is no `notification_store.rs` and no collab crate.                                                                                                                                |
| 025d2a06ee | C     | --                     | Stops ACP v2 prompt submission status rows from flashing. Locally there is no `acp_v2` session model and no `SubmissionId`, so the rows do not exist.                                                                                                                                                                                                            |
| 0373366f43 | C     | --                     | Fixes 30 fps animation on M5 Macs by pacing each window with `CAMetalDisplayLink`. The shared wrapper lives in the absent `gpui_apple` crate and the macOS window rewrite depends on it (`MainThreadMarker`, `MainThreadOnly` delegate), so no hunk applies.                                                                                                     |
| e20d19d32f | B     | 0e4e3c7601             | Configures the wgpu surface once at creation with the format the renderer will use, and skips the renderer's redundant configure. Reuse, recovery, and web paths still configure in the renderer.                                                                                                                                                                |
| d401423073 | C     | --                     | Removes collab extension endpoints now served by Cloud. ZZZ has no `crates/collab` and no Cloud endpoints.                                                                                                                                                                                                                                                       |

Totals: one `A`, one `A` already equivalent on `main`, eight `B`, nine
`C` across nineteen commits.

### Applied Work

- `bbbe9043d4` is a `B` port of `7c8dbadcb5`.
- `a51905c8c9` is a `B` port of `9cb5f90e3c`.
- `de725f2903` is a `B` port of `28917a970d`.
- `63bf38b8f8` is a clean `A` cherry-pick of `4112666801`.
- `df4bbfd91b` is a `B` port of `5a534d7b5d`.
- `c97e8c49b2` is a `B` port of `99cda17e05`.
- `eb4f1d99f5` is a `B` port of `2eb2928c15`.
- `08b09579a8` is a `B` port of `bc7041c600`.
- `0e4e3c7601` is a `B` port of `e20d19d32f`.
- `7c2a042e95` is a formatting-only follow-up so `cargo fmt --check`
  passes for the frame-interval port.

### Per-Commit Narrative

#### 2c2f737ab2 — web HTTP client execution (C)

The commit adds a `Send`-able `ForegroundExecutor` variant, moves the web
`Application` constructor out of `gpui_platform`'s `target_family =
"wasm"` branch into explicit callers, and fixes binary-name warnings in
fifteen manifests. ZZZ builds no wasm target, its web client is not
dispatched from `cx.background_spawn`, and `gpui_platform::application`
still selects the web backend by `cfg`. Nothing would call the new
executor locally, and the manifest half is dependency-only churn, so the
commit is rejected rather than partly imported.

#### 7c8dbadcb5 — power and thermal frame intervals (B)

`WindowOptions` gains `low_power_frame_interval` and
`thermal_frame_interval` next to the existing `inactive_frame_interval`,
and `minimum_frame_interval` returns the longest interval that currently
applies instead of walking a fixed ladder. A window in Low Power Mode
with a serious thermal state now throttles to 30 fps, and an inactive
window under Low Power Mode no longer ignores a configured interval
longer than 30 fps. The upstream `frame_limits_combine_*` coverage was
ported and the frame tests were rewritten against `WindowOptions::default()`
now that the fixed `THIRTY_FPS_INTERVAL`/`SIXTY_FPS_INTERVAL` constants
are gone. Only the `profiler::journal::record_frame_skipped` call was
dropped, because ZZZ carries no profiler journal.

#### 87ce147829 — git graph refs column (C)

The refs column is not a self-contained layout change. It renders through
the table column-visibility machinery introduced by `fcd0f76952`
(`TableRow<bool>` rows, `redistribute_hidden_fractions`,
`toggle_column_visibility`), none of which exists in ZZZ's
`crates/git_graph/src/git_graph.rs`. A three-way application against the
local tree produced twenty conflicts across the graph, the table, and the
picker, so the behavior cannot be isolated; it needs the table refactor
first.

#### 9cb5f90e3c — synthetic mouse moves (B)

`WindowsWindowState` now remembers the last reported cursor position and
`handle_mouse_move_msg` drops a `WM_MOUSEMOVE` that repeats that position
with no button held. Windows synthesizes those events when a window
repaints under a stationary cursor, which moved the editor cursor and
cleared the selection whenever the command palette opened. The
accessibility field keeps ZZZ's feature-flagged shape; only
`mouse_position` was added. The target cannot be compiled on this host
because of pre-existing `windows`/`windows-core` API drift, so the change
was reviewed by inspection.

#### 28917a970d — helix and the system clipboard (B)

In helix mode the unnamed register no longer mirrors into the system
clipboard when `vim.use_system_clipboard` is `always`, so plain
`y`/`d`/`c`/`p` leave the OS clipboard alone and only `+`/`*` touch it.
`R` is bound to `vim::HelixReplaceWithYanked`, which replaces each
selection with the default register, seeds a bare cursor from the next
character, keeps the replacement selected, and returns to helix normal
mode. The upstream register and paste tests came along; the new shift-`R`
test was merged with ZZZ's local CRLF replace test.

#### 4112666801 — Spanish-ISO mappings (A)

A table-only change in `gpui_macos/src/keyboard.rs`; the cherry-pick
applied without conflicts and keeps the upstream author plus the DCO
trailer.

#### 5a534d7b5d — git auto fetch (B)

An off-by-default `git.auto_fetch` setting periodically fetches the
default branch's remote and the current branch's upstream remote for
trusted repositories. Unattended fetches never prompt: the terminal
prompt, credential helpers, askpass, and ssh batch mode are all disabled
in one place, and git never runs auto-maintenance on the user's behalf.
Fetches run off the repository job queue, refresh the branch list, and
are cancelled when the timer restarts or the repository closes. Upstream
builds the command in the new `git_ui_core` crate and toggles askpass
through `RemoteAskPassOperation`; both are absent locally, so
`run_remote_command` was rebuilt on ZZZ's `GitBinary` askpass insertion,
and the settings UI strings go through `i18n::tr` with four new keys in
both locale catalogs.

#### 99cda17e05 — user MCP servers before the first worktree (B)

`ContextServerStore::resolve_all_context_server_settings` falls back to
`ProjectSettings::get(None, cx)` when no visible worktree exists, marking
the entries with `worktree_id: None`. An ACP thread created before the
project's first folder finishes loading therefore still receives the
user's MCP servers in `session/new`. The regression test asserts that a
user-level stdio server starts with no worktrees.

#### e8e579d6ea — remote project skills (C)

The patch extends `agent_ui` mention sets and `settings_ui` skill setup so
project skills keep working when the project lives on a remote host. ZZZ
removed project skills with the native agent surface: there is no
`crates/agent_skills`, no `skills_setup.rs`, no `skill_creator.rs`, and no
`Project` skill methods. Nothing local would consume the changed code.

#### 2eb2928c15 — restricted mode hooks (B)

`SettingsObserver::update_settings` checks worktree trust before applying
any non-editorconfig local settings, so `.ZZZ/tasks.json`,
`.ZZZ/debug.json`, and their `.vscode` equivalents stay unapplied until
the worktree is trusted, and every pending kind is recorded per folder.
Trusting a worktree replays the pending bundle through
`update_settings`. `Workspace::run_create_worktree_tasks` filters through
`TrustedWorktrees`, so global task hooks no longer fire in a restricted
workspace. Upstream's `is_via_collab` parameter was omitted because trust
is always resolved through `TrustedWorktrees` here. The upstream
regression test and both behaviors were documented in `worktree-trust.md`.

#### 3524cb2297 — project skills to the remote (C)

Extends the same project-skill surface to persist newly created skills on
the remote host. It edits `settings_ui/src/pages/skill_creator.rs`, which
does not exist locally, and the `Project` skill methods that `e8e579d6ea`
also depends on. Same rejection.

#### bc7041c600 — standalone modifier timeout (B)

`Keystroke::is_modifier_key` identifies bare `shift`, `control`, `alt`,
`platform`, and `function` keystrokes, and the pending-input timer now
starts when the only pending keystroke is one of them. A tap of `shift`
to toggle an IME no longer lingers for the rest of the session and no
longer turns a later tap into `shift shift`. Both upstream regression
tests were ported into the local `key_dispatch` test module; the first
fails without the new timeout condition. The upstream change lives in
`Window::dispatch_key_event`, which ZZZ keeps in `interaction.rs`.

#### 33156f0d81 — language-model event streaming (C)

The commit replaces the background-event fan-in of `LanguageModel` with
`async-channel` receivers. ZZZ's `crates/language_model` was reduced to an
eight-line type shim when the native agent tools were removed; it has no
`stream_in_background`, no event types, and no subscribers, so there is no
pipeline to reroute.

#### 2529740084 — GPUI arena aliasing (A, already equivalent)

Upstream wraps the element arena in an `Rc` to stop `&mut` aliasing when
an element holds a reference into the arena across a re-entrant update.
ZZZ fixed the same defect on `main` in `0610da51e9` with its own shape.
This branch predates that commit, and replaying the upstream patch here
would collide with it at merge time, so no local commit was made. The
branch therefore keeps the `RefCell<Arena>`-based code until it merges
`main`.

#### 5882c67dc3 — notification cleanup (C)

Upstream deletes unreferenced notification-store helpers and rewrites the
collab integration tests that used them. ZZZ's `crates/notifications`
contains only `notifications.rs` and `status_toast.rs`; the targeted file
and the collab test crate do not exist, so there is nothing to remove.

#### 025d2a06ee — ACP v2 submission rows (C)

The patch keeps submission-status rows from flashing on ACP v2 prompts by
tracking `SubmissionId`s on `acp_v2` sessions. ZZZ's conversation view
uses the v1 ACP schema and has neither the v2 session type nor submission
ids, so the rows being fixed are not rendered locally.

#### 0373366f43 — M5 animation rate (C)

On macOS 14 and later each window is paced by a `CAMetalDisplayLink`
owned by `gpui_apple::display_link::MetalFrameSource`, and the renderer
draws into the drawable that link hands out instead of calling
`nextDrawable`. ZZZ has no `gpui_apple` crate; the macOS backend is
`gpui_macos`, which has no `MainThreadMarker`/`MainThreadOnly` wrapper and
no shared Apple display-link module. Recreating that module is new
infrastructure rather than a port, and the result could not be built or
measured on this Linux host.

#### e20d19d32f — wgpu surface configuration (B)

Adapter selection now configures the surface with the format, alpha mode,
present mode, and size the renderer will use, and the renderer skips its
own configure when the context was just created against that surface.
Windows that reuse an existing context, device recovery with an existing
context, and the web path still configure in the renderer. ZZZ's
`WgpuRenderer::surface_configuration` reads the window render target
configuration lazily, so the port takes an `Option<&Surface>` and falls
back to the local headless configuration when no surface is attached.

#### d401423073 — collab extension endpoints (C)

Removes extension endpoints that Cloud now serves. ZZZ has no
`crates/collab`, no Cloud endpoints, and no extension-hosting endpoint
code, so the deletion targets nothing local.

### Verification

| Check                                                                                                                        | Result  | Notes                                                            |
| ---------------------------------------------------------------------------------------------------------------------------- | ------- | ---------------------------------------------------------------- |
| `cargo check --locked --workspace --all-targets`                                                                             | PASS    | after all ten commits                                            |
| `cargo fmt --all -- --check`                                                                                                 | PASS    | after `7c2a042e95`                                               |
| `./script/check-philosophy`                                                                                                  | PASS    | no account, telemetry, collab, or native-agent surface added     |
| `cargo test --locked -p fs -p git -p project -p git_ui --lib`                                                                | PASS    | 19 / 64 / 56 (+1 ignored) / 122 tests                            |
| `cargo test --locked -p vim --lib`                                                                                           | PASS    | 576 tests, including the helix register and shift-`R` coverage   |
| `cargo test --locked -p gpui --lib key_dispatch`                                                                             | PASS    | 8 tests, including both standalone-modifier regression tests     |
| `cargo test --locked -p gpui_wgpu --lib`                                                                                     | PASS    | 23 tests after the surface-configuration port                    |
| `cargo test --locked -p agent_ui --lib context_server_configuration`                                                         | PASS    | user-level MCP servers with no worktrees                         |
| `cargo test --locked -p git --lib compose_batch_mode`                                                                        | PASS    | auto-fetch command composition                                   |
| `cargo clippy -p git -p fs -p project -p git_ui -p settings_ui -p gpui_wgpu --all-targets --all-features -- --deny warnings` | PASS    | clean after fixing a `single_match_else`                         |
| `assets/locales/en.json` and `zh-CN.json` key parity                                                                         | PASS    | 3750 keys in each catalog                                        |
| DCO check over the ten batch commits                                                                                         | PASS    | every commit has `Signed-off-by`                                 |
| macOS / Windows runtime checks                                                                                               | NOT RUN | host is Linux; `gpui_macos` and `gpui_windows` are platform-only |
| `cargo test --workspace`                                                                                                     | NOT RUN | per-crate checks were used instead                               |
| Remaining commits through the queried head                                                                                   | NONE    | `d401423073` is the live head                                    |

The reviewed baseline advances to
`d4014230735e4265d98066420b9271df2324ccab` for this batch, which is the
live upstream head at query time.
