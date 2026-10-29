---
title: Upstream Sync 2026-10-29
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-10-29

## Scope

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

## Decisions

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

## Applied Work

- `8b5f601787` is a `B` port of `2580a00264`.
- `18083ad35d` is a `B` port of `fae4b2ea4f`.
- `c20e2e91f5` is a `B` port of `44ec5691b5`.
- `3614b8c8f9` is a `B` port of `4d847859f5`.
- `00bad249e6` is a `B` port of `2e57653724`.
- `0d707a8d92` is a `B` port of `02d47bf64f`.
- `2fd0bfd2f1` is a `B` port of `f0210f1621`.

No commit in this batch was a clean `A`; each candidate either touched a
crate ZZZ does not carry or needed a local API shape before it fit.

## Per-Commit Narrative

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

## Verification

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
