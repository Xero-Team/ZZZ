---
title: Upstream Sync 2026-10-16
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-10-16

## Scope

- Target branch: `sync/upstream-2026-10-16`, branched from `main` at
  `9269d59448`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `36682547bea1c3d4fa60c08d341cf60081853941`
- Reviewed head: `5504e87bfdaa90b3b4852a814940ece148f5fe1d`
- Live upstream head queried: `5504e87bfdaa90b3b4852a814940ece148f5fe1d`
- Query time: `2026-10-16T14:38:00+02:00`
- Reviewed range: the twenty oldest commits after `36682547be`, oldest
  first
- Remaining after this batch: 21 commits through the queried live head

The previous reviewed baseline is an ancestor of `FETCH_HEAD`. The
batch size is twenty, so twenty-one commits after the reviewed head
remain for a later run.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                              |
| ---------- | ----- | ------------ | -------------------------------------------------------------------------------------------------------- |
| 18787a9a08 | A     | 5afa8ebc11   | Adds `llvm` to the Arch package list in `script/linux`; direct cherry-pick.                              |
| 76b8ab814f | C     | --           | Adds Gemini 4 Argon to the absent `crates/google_ai` provider enum.                                      |
| 61c8c19fc8 | C     | --           | Drives BYOK Gemini models from Google's catalog; absent `google_ai` and `language_models` crates.        |
| bc98c8f042 | C     | --           | Fixes Gemini tool-result images; `crates/google_ai` is absent.                                           |
| e18de65b75 | B     | 25b6c887bc   | Translucent layers composite with source-over alpha; macOS hunks re-homed into `crates/gpui_macos`.      |
| 7dc397b61b | B     | 308ecf36fd   | Spawns the remote daemon with `CreateProcess` on Windows; the deleted `windows.rs` had a local tweak.    |
| df134bf1ec | A     | --           | Already equivalent: ZZZ's `Sidebar::archive_thread` never reopens a closed workspace, so it cannot loop. |
| c403bc6af6 | C     | --           | Routes ACP v2 permission and elicitation requests; ZZZ has no v2 protocol.                               |
| 72e6f413aa | C     | --           | Removes v1 coupling from ACP logs and fixtures under the v2 migration.                                   |
| 2b190da0f5 | C     | --           | Bumps the `zed` crate to v1.26.0; upstream release metadata for an absent crate.                         |
| b25dc09370 | C     | --           | Rewrites remote reconnection handshake/resync; ZZZ's remote transport has diverged.                      |
| 2dcc4cc04d | B     | db8a14078f   | Restarts stopped language servers; ported onto ZZZ's LSP start/stop model, collab test omitted.          |
| a47f721832 | C     | --           | Adds the `gpui_ios` platform backend crate; absent in ZZZ.                                               |
| e463c5ab74 | B     | 7d6401934c   | Queues dropped focus handles for cleanup; native-agent subagent and benchmark hunks omitted.             |
| a90dedb0e5 | C     | --           | Adds the inactive ACP v2 connection and auth foundation.                                                 |
| 0943bcb76c | C     | --           | Shares ACP connection lifecycle across v1 and v2.                                                        |
| d67bbfda3c | C     | --           | Uses receipts and reported state for native agent turns; ZZZ has no `NativeAgent`.                       |
| 3e145061d8 | C     | --           | Moves the legacy ACP adapter into `acp/v1.rs`; absent v2 architecture.                                   |
| e462f873c6 | B     | 2a51349aa6   | Ignores foreign `WM_USER` messages on Windows; ported onto ZZZ's `Window::interaction` layout.           |
| 780ac03c02 | A     | a3ac67bcc3   | Uses the unquoted label as `filterText` for JSON enum completions; direct cherry-pick.                   |

Totals: three `A` (two with code changes, one already equivalent),
five `B`, twelve `C` across twenty commits.

## Applied Work

- `5afa8ebc11` is a direct `git cherry-pick -x -s` of `18787a9a08`.
- `a3ac67bcc3` is a direct `git cherry-pick -x -s` of `780ac03c02`.
- `25b6c887bc` is a `B` port of `e18de65b75`.
- `308ecf36fd` is a `B` port of `7dc397b61b`.
- `db8a14078f` is a `B` port of `2dcc4cc04d`.
- `7d6401934c` is a `B` port of `e463c5ab74`, followed by
  `ad8495876b`, which only applies rustfmt to the ported test.
- `2a51349aa6` is a `B` port of `e462f873c6`.

## Per-Commit Narrative

### 18787a9a08 — `script/linux` llvm (A)

The Arch package list gained `llvm` for `llvm-objcopy` in
`script/bundle-linux`. ZZZ's `script/linux` is identical around the
`pacman` block, so the pick applied cleanly.

### e18de65b75 — source-over alpha compositing (B)

Upstream edited `crates/gpui_apple`, `crates/gpui_wgpu`, and
`crates/gpui_windows`. ZZZ keeps the macOS renderer in
`crates/gpui_macos`, so the two `set_destination_alpha_blend_factor`
hunks in `build_pipeline_state` and `build_path_sprite_pipeline_state`
were re-homed there. The `gpui_wgpu` change replaces the hand-written
`paths_blend` with `wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING`, and
the `gpui_windows` change sets `DestBlendAlpha` to
`D3D11_BLEND_INV_SRC_ALPHA` in `create_blend_state` and
`create_blend_state_for_path_sprite`. No hunks were dropped beyond the
crate rename.

### 7dc397b61b — Windows remote daemon spawn (B)

The cherry-pick conflicted in `Cargo.lock` and on the modify/delete of
`crates/remote_server/src/windows.rs`, because ZZZ's copy replaced an
upstream `unwrap()` with `CoInitializeEx(...).ok().context(...)`. The
conflict was resolved by taking the upstream shape: `spawn_server_windows`
spawns `smol::process::Command` with
`DETACHED_PROCESS | CREATE_BREAKAWAY_FROM_JOB`, the `windows` module and
crate dependency are removed, and the lockfile line is dropped. The
local COM error-context tweak is dropped with the module, which is inert
now that the module is unused. Windows runtime behavior is not exercised
on this host.

### df134bf1ec — sidebar archive loop (A, already equivalent)

Upstream's `archive_thread` opened a closed workspace and re-entered
`archive_thread`, which looped when the linked worktree folder was
deleted. ZZZ's `Sidebar::archive_thread` (`crates/sidebar/src/sidebar.rs`)
has no `should_load_closed_workspace_for_archive` /
`open_workspace_for_archive` path and archives the thread directly, so
the described loop cannot occur. No code change.

### 2dcc4cc04d — restart stopped language servers (B)

The cherry-pick conflicted in `editor_tests.rs`, `lsp_button.rs`,
`lsp_store.rs`, `manifest_tree/server_tree.rs`, and `project.rs`; the
`crates/collab` test was a modify/delete because ZZZ has no collab
crate. Resolutions:

- `Project::restart_language_servers_for_buffers` keeps ZZZ's
  parameter-free signature and forwards to the three-argument
  `LspStore` method; the stray `true` argument is removed.
- `lsp_button.rs` keeps ZZZ's `download_language_server(name, buffers,
cx)` call and drops the `clear_stopped` argument from the restart call.
- `server_tree.rs` drops the removed `rebased_server_ids` bookkeeping and
  uses the new `preserved_server_ids` derivation in `finish`.
- `lsp_store.rs` keeps ZZZ's `.expect("should be a local instance")`
  style while adopting the dedented `observe_release` closure, and
  removes the `clear_stopped` argument from `download_language_server`.

The `crates/collab` test is not restored. ZZZ's `editor_tests.rs` lacks
the surrounding upstream tests, so the block was kept at ZZZ's version
and only `test_restart_language_server_action_after_stop` was added,
along with the `mpsc`/`FutureExt` imports it needs.

### e463c5ab74 — focus cleanup (B)

`FocusMap` becomes a struct holding the handle slot map plus a
`dropped_handles` queue; `FocusHandle::drop` enqueues the id on the last
release and `release_dropped_focus_handles` scans only that queue. ZZZ
nests focus on `Window::interaction`, so the blur pass reads
`window.interaction.focus`. The two `window.rs` regression tests are
kept (the second drops the upstream `focus_generation` assertion, which
ZZZ's `Window` does not expose). Omitted: the `agent_ui`
subagent-restoration fix and its `acp_v2` test, the criterion benchmark
with its `autobenches`/`gpui_platform` plumbing, and the `key_dispatch`
test addition that assumes the upstream test layout.

### e462f873c6 — foreign `WM_USER` messages (B)

The cherry-pick conflicted in `events.rs` and `platform.rs`. Ported:
`handle_gpui_events` validates `wparam` against
`WindowsWindowInner::validation_number` before dispatching
`WM_GPUI_CURSOR_STYLE_CHANGED`, `WM_GPUI_FORCE_UPDATE_WINDOW`, and
`WM_GPUI_GPU_DEVICE_LOST`; `handle_device_lost` ignores a null payload;
`WM_GPUI_KEYDOWN` is removed and `translate_accelerator` becomes an
inner method that calls the target window's `handle_keydown_msg`;
`window_from_hwnd` and `set_cursor_style` are updated accordingly.
ZZZ's `WM_GPUI_DISPLAYS_CHANGED` constant, present only in upstream, was
not introduced. Windows compilation is blocked on this host (the `gpui`
build script needs `x86_64-w64-mingw32-windres`).

### 780ac03c02 — JSON enum completion order (A)

`unquote_filter_texts` now falls back to the unquoted label when a
completion item has no `filterText`, so quoted enum values match as
prefixes. ZZZ's `crates/languages/src/json.rs` was identical around the
function, so the pick applied cleanly with its new unit test.

## Rejected Work

- `76b8ab814f`, `61c8c19fc8`, and `bc98c8f042` change the built-in
  Google AI provider. ZZZ has no `crates/google_ai` and no
  `crates/language_models`; AI is ACP-only, so there is no reachable
  caller for these model registrations or their catalog plumbing.
- `c403bc6af6`, `72e6f413aa`, `a90dedb0e5`, `0943bcb76c`, and
  `3e145061d8` are one continuous migration of the ACP layer to protocol
  v2 (`acp/v2.rs`, `acp/runtime.rs`, `acp/v1.rs`). ZZZ's ACP crates
  still import `agent_client_protocol::schema::v1 as acp` and have no v2
  protocol, and each commit requires the preceding one in the stack.
- `d67bbfda3c` rewrites `crates/agent/src/agent.rs` around native agent
  receipts and reported state. ZZZ has no `NativeAgent`; its agent UI is
  ACP-driven.
- `2b190da0f5` bumps `crates/zed` to v1.26.0. ZZZ renamed that crate to
  `crates/zzz`, and a version bump is upstream release metadata.
- `b25dc09370` rewrites the remote reconnection flow to separate
  handshake and resync timeouts, changing `ChannelClient::reconnect` to
  return a connection-ready receiver and restructuring message handling.
  ZZZ's `crates/remote` transport has diverged (`RemoteClient::reconnect`
  and `ChannelClient::reconnect` have different shapes), so the change
  is not isolatable.
- `a47f721832` adds the `crates/gpui_ios` backend (4315 new lines). ZZZ
  has no iOS platform backend.

## Verification

| Check                                                                                   | Result  | Notes                                                                 |
| --------------------------------------------------------------------------------------- | ------- | --------------------------------------------------------------------- |
| Live `git ls-remote` and `git fetch --no-tags`                                          | PASS    | `FETCH_HEAD=5504e87bfdaa90b3b4852a814940ece148f5fe1d`                 |
| Previous baseline ancestry                                                              | PASS    | `36682547be` is an ancestor of `FETCH_HEAD`                           |
| `cargo check --locked -p gpui_wgpu`                                                     | PASS    | source-over alpha port                                                |
| `cargo check --locked -p remote_server`                                                 | PASS    | Windows spawn port (Linux path)                                       |
| `cargo check --locked -p project -p editor -p language_tools`                           | PASS    | LSP restart port                                                      |
| `cargo check --locked --tests -p project -p editor -p language_tools -p extension_host` | PASS    | ported tests compile                                                  |
| `cargo test --locked -p editor test_restart_language_server_action_after_stop`          | PASS    | 1 test passes                                                         |
| `cargo test --locked -p project --test integration managing_language_servers`           | PASS    | 1 test passes                                                         |
| `cargo test --locked -p project --test integration restart`                             | PASS    | 5 tests pass                                                          |
| `cargo test --locked -p project --test integration removing_invisible_worktree`         | PASS    | 1 test passes                                                         |
| `cargo check --locked -p gpui`                                                          | PASS    | focus cleanup port compiles                                           |
| `cargo test --locked -p gpui focus`                                                     | PASS    | includes both new focus-cleanup tests                                 |
| `cargo test --locked -p languages test_unquote_filter_texts`                            | PASS    | both JSON completion tests pass                                       |
| `./script/clippy -p gpui -p languages -p project`                                       | PASS    | includes the philosophy gate                                          |
| `./script/clippy -p editor -p language_tools -p remote_server -p gpui_wgpu`             | PASS    | clean                                                                 |
| `git diff --check` and `cargo fmt --all -- --check`                                     | PASS    | only pre-existing `crates/project` baseline drift remains             |
| Windows cross-compile `-p gpui_windows --target x86_64-pc-windows-gnu`                  | BLOCKED | `gpui` build script needs `x86_64-w64-mingw32-windres`, not installed |
| macOS and Windows runtime checks                                                        | NOT RUN | host is Linux                                                         |
| `cargo test --workspace`                                                                | NOT RUN | not requested                                                         |

The general reviewed baseline advances to
`5504e87bfdaa90b3b4852a814940ece148f5fe1d`.
