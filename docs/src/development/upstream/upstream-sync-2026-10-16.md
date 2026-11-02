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
- Reviewed head: `780ac03c02ab79ea616b4e327434df768a8b61d6`
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
`780ac03c02ab79ea616b4e327434df768a8b61d6`.

## Continued Batch 2

### Scope

- Target branch: `sync/upstream-2026-10-16`, continuing the same local base
  `9269d59448`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `780ac03c02ab79ea616b4e327434df768a8b61d6`
- Reviewed head: `32b16ce2ada96ba86c9052e456a2642102b39b7a`
- Live upstream head queried: `d678e7de4d9d2448f67d44b1c19e827257b40223`
- Query time: `2026-10-16T19:03:35+02:00`
- Reviewed range: the next twenty commits after `780ac03c02`, oldest first
- Remaining after this batch: 9 commits through the queried live head

The previous baseline is an ancestor of `FETCH_HEAD`. This batch stays fixed
at twenty commits; the nine later commits remain queued.

### Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                 |
| ---------- | ----- | ------------ | ----------------------------------------------------------------------------------------------------------- |
| 4a8ccb3d44 | A     | 49a4e55c4c   | Installs `@vscode/dwarf-debugging` for the JS debug adapter; direct cherry-pick.                            |
| 5a5fbe7991 | A     | af894e377b   | Right-docked sidebar header keeps window controls at the edge with no projects; direct cherry-pick.         |
| 953ac75529 | B     | 7fa5257c4c   | Adds the builtin Pyrefly LSP; resolves the venv binary via ZZZ's `Toolchain::execution_path`.               |
| 21001b273f | C     | --           | Converts the iOS example to a Cargo example target; ZZZ has no `gpui_ios` backend.                          |
| 402675e805 | C     | --           | Pins `wgpu`/`naga` to a git revision with an API migration across the divergent renderer; dependency churn. |
| e93e6eae8f | C     | --           | Adds inactive ACP v2 initial sessions and config; ZZZ has no `acp/v2.rs`.                                   |
| a598842e88 | C     | --           | Routes v2 reported session state; no local ACP v2 transport.                                                |
| 23772fa6ef | C     | --           | Adds inactive v2 prompt receipt requests; absent ACP v2.                                                    |
| 9be5cb6139 | C     | --           | Settles receipt submissions after disconnect; `acp_thread/src/submission.rs` and ACP v2 are absent.         |
| 7fa2adbe08 | C     | --           | Broad markdown-parsing allocation refactor across divergent and locally deleted modules; not isolatable.    |
| 28d0a3547f | C     | --           | Integrates inactive v2 receipt submissions; absent ACP v2 and native-agent receipts.                        |
| 636924f6d4 | A     | 4a3fba83a5   | Injects HTML into `svg`/`mathml`/`raw` templates and CSS into `css({ ... })`; direct cherry-pick.           |
| 664935cb5e | B     | 4e6e98d556   | Clipboard bitmap takes priority over image files in the agent panel; inline-assistant hunks omitted.        |
| 8a21d136a7 | B     | af85c9eb71   | Queued message rows wrap so text stays readable in narrow panels.                                           |
| 80f1bb45fa | A     | df274ac7c0   | Resolves file icons for stem-based names like `Dockerfile.prod`; direct cherry-pick.                        |
| 7699cb8bb8 | C     | --           | Reworks the touch fling recognizer; ZZZ has no `crates/gpui/src/gestures.rs`.                               |
| 97511be201 | B     | 6523363819   | Defers project-group removal until the save prompt is confirmed.                                            |
| 9535c9d29d | B     | 7e4899351b   | Normalizes line endings in mention links and renders control chars in creases.                              |
| 2be002692f | A     | 0f332f3fd8   | Derives Rust task variables from an ancestor manifest for files outside Cargo projects; direct cherry-pick. |
| 32b16ce2ad | A     | --           | Already equivalent: local `9daa4e7901` corrected the same inverted ordering invariant.                      |

Totals: six `A` (five with code changes, one already equivalent), five `B`,
nine `C` across twenty commits.

### Applied Work

The five direct `A` changes used `git cherry-pick -x -s`. `32b16ce2ad` is
already equivalent in product code. Each `B` commit has `Upstream`, `Retained`,
and `Omitted` trailers.

- `49a4e55c4c` installs `@vscode/dwarf-debugging` alongside `vscode-js-debug`
  so WebAssembly sources can be debugged, factoring the installed-version
  lookup into `installed_version_directory` and logging install failures
  without aborting.
- `af894e377b` adds a `justify_end` spacer when no project is open, so the
  right-docked sidebar header pushes the window controls to the edge.
- `4a3fba83a5` widens the `html` template injections to `svg`, `mathml`, and
  `raw`, and adds CSS injection for `css({ ... })` object property templates.
- `df274ac7c0` checks a file's stem before its suffixes, so `Dockerfile.prod`
  and `Podfile.lock` resolve their icons.
- `0f332f3fd8` returns no Cargo target information when a Rust file has no
  ancestor `Cargo.toml`, so generic task and debugger variables still resolve.
- `7fa5257c4c` registers the Pyrefly Python language server. The in-venv binary
  is resolved through ZZZ's `Toolchain::execution_path` because the upstream
  `Toolchain::path` accessor is not present locally.
- `6523363819` moves project-group removal out of `remove_project_group` into
  `MultiWorkspace::remove`, gated by a new `remove_project_groups` flag, so the
  group is dropped only after the save prompt is confirmed. `close_workspace`,
  the sidebar archive flow, and the test callers pass `false`.
- `af85c9eb71` lets the queued-message row wrap, gives the editor `flex_1` with
  a 120px floor, and replaces the fixed action-group width with `ml_auto`.
- `7e4899351b` normalizes `\r\n` and `\r` in mention link names and renders
  control characters in mention creases, with acp_thread and message_editor
  regression tests.
- `4e6e98d556` skips image files in `resolve_pasted_context_items` when the
  clipboard also carries a bitmap, so a screenshot is inserted once.

### Rejected Work

- `21001b273f` converts the iOS example to a Cargo example target. ZZZ has no
  `crates/gpui_ios` backend, so the crate and its example are absent.
- `402675e805` pins `wgpu` and `naga` to a `gfx-rs/wgpu` git revision and
  migrates descriptor, color-space, and present APIs. It is dominated by
  dependency and lockfile churn, touches the locally divergent
  `wgpu_renderer.rs`, and its Vulkan surface-teardown fix cannot be exercised
  on this host.
- `e93e6eae8f`, `a598842e88`, `23772fa6ef`, and `28d0a3547f` add and route ACP
  v2 sessions, reported state, and receipt submissions. ZZZ has no
  `crates/agent_servers/src/acp/v2.rs`, imports only `schema::v1 as acp`, and
  keeps its ACP server in a flat `acp.rs`, so there is no v2 transport.
- `9be5cb6139` settles receipt submissions after disconnect through
  `crates/acp_thread/src/submission.rs` and `acp/v1.rs`, neither of which exists
  locally; the receipt lifecycle depends on the absent v2 machinery.
- `7fa2adbe08` is a broad markdown-parsing allocation refactor that changes the
  `language_core` cache types and ripples through editor, languages, and
  markdown. It conflicts with the locally deleted `crates/benchmarks`,
  `call_hierarchy`, and `editor/src/inline_input.rs`, so no coherent slice is
  isolatable.
- `7699cb8bb8` reworks `TouchGestureRecognizer` in
  `crates/gpui/src/gestures.rs`, which ZZZ does not have; its `gpui` ships no
  touch gesture recognizer.

### Verification

| Check                                                                                    | Result  | Notes                                                                          |
| ---------------------------------------------------------------------------------------- | ------- | ------------------------------------------------------------------------------ |
| Live `git ls-remote` and `git fetch --no-tags`                                           | PASS    | `FETCH_HEAD=d678e7de4d9d2448f67d44b1c19e827257b40223`                          |
| Previous baseline ancestry                                                               | PASS    | `780ac03c02` is an ancestor of `FETCH_HEAD`                                    |
| `cargo check --locked -p dap_adapters -p sidebar -p languages -p file_icons`             | PASS    | four `A` cherry-picks compile                                                  |
| `cargo check --locked -p workspace`                                                      | PASS    | project-group removal port                                                     |
| `cargo check --locked -p agent_ui -p acp_thread`                                         | PASS    | agent-panel ports compile                                                      |
| `cargo check --locked --tests -p agent_ui -p acp_thread`                                 | PASS    | ported tests compile                                                           |
| `cargo check --locked --tests -p workspace -p sidebar`                                   | PASS    | updated test callers compile                                                   |
| `cargo test --locked -p acp_thread test_link_normalizes_line_endings_in_name`            | PASS    | 1 test passes                                                                  |
| `cargo test --locked -p agent_ui test_paste_external_file_path_with_line_breaks_in_name` | PASS    | 1 test passes                                                                  |
| `cargo test --locked -p workspace --lib remove`                                          | PASS    | 12 tests pass, including `test_remove_project_group_falls_back_to_neighbor`    |
| `cargo test --locked -p languages --lib rust`                                            | PASS    | 11 tests pass, including the three `target_info_from_abs_path` cases           |
| `cargo test --locked -p file_icons --lib`                                                | PASS    | no test module; icon resolution is not covered by a unit test                  |
| `./script/check-philosophy`                                                              | PASS    | philosophy gate                                                                |
| clippy `--all-targets` on the touched crates                                             | PASS    | clean                                                                          |
| `./script/clippy` full passes on the sidebar/agent_ui set                                | BLOCKED | Pre-existing `recent_projects` feature-unification failure; also fails at base |
| `git diff --check` and `cargo fmt --all -- --check`                                      | PASS    | only pre-existing `crates/project` baseline drift remains                      |
| Windows and macOS runtime checks                                                         | NOT RUN | host is Linux                                                                  |
| `cargo test --workspace`                                                                 | NOT RUN | not requested                                                                  |

The general reviewed baseline advances to
`32b16ce2ada96ba86c9052e456a2642102b39b7a`.

The full `./script/clippy` run on a package set that pulls in `recent_projects`
fails on a pre-existing non-exhaustive match over
`RemoteConnectionOptions::Mock`; the same failure reproduces on the batch base
`818f06038a`, so it is a baseline defect rather than a regression from this
batch.

## Continued Batch 3

### Scope

- Target branch: `sync/upstream-2026-10-16`, continuing the same local base
  `9269d59448`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `32b16ce2ada96ba86c9052e456a2642102b39b7a`
- Reviewed head: `9f63850f7affbd57772a4f99554d4fd333b50a6f`
- Live upstream head queried: `9f63850f7affbd57772a4f99554d4fd333b50a6f`
- Query time: `2026-10-16T19:10:53+02:00`
- Reviewed range: the ten commits after `32b16ce2ad`, oldest first
- Remaining after this batch: 0 commits through the queried live head

The previous baseline is an ancestor of `FETCH_HEAD`. Upstream advanced by
one commit (`9f63850f7a`) since the last batch, so this run drains the queue.

### Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                        |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------ |
| 5504e87bfd | B     | 19db6a0d9e   | Derives restricted-path parents through `TrustedPath::parent`; upstream's `PathStyle` additions omitted.           |
| df51ce9d4c | C     | --           | Adds ACP v2 session cancellation; `acp/v2.rs` and `acp_thread/src/submission.rs` are absent.                       |
| 9b421ed7fe | C     | --           | Adds v2 `session/list`; absent ACP v2 transport.                                                                   |
| 4f70271800 | C     | --           | Adds v2 session resume and replay (901 lines in `acp/v2.rs`); absent ACP v2.                                       |
| df1ce4fee3 | C     | --           | Preserves organization UI permissions for a GitHub App install surface ZZZ does not have.                          |
| 8c8ada0fbf | C     | --           | Separates explicit session close from view release for the absent v2 backend; new trait API would be dead.         |
| 14119f8d87 | A     | 7b219037cb   | Narrows Rust `sql!` injection to literal string content; direct cherry-pick.                                       |
| 6b218b8412 | C     | --           | Bumps the ACP SDK to 3.4.0 for the v2 migration; dependency and lockfile churn with no independent local behavior. |
| d678e7de4d | C     | --           | Documents Gemini 4 Argon hosted pricing; ZZZ has no `docs/src/account` hosted-model page.                          |
| 9f63850f7a | C     | --           | Effort-specific thinking controls touch the absent `language_models`, `copilot_chat`, and `cloud_llm_client`.      |

Totals: one `A`, one `B`, eight `C` across ten commits.

### Applied Work

- `7b219037cb` is a direct `git cherry-pick -x -s` of `14119f8d87`, with one
  local adaptation: ZZZ's `LanguageConfig::matcher` is a `LanguageMatcher`, not
  an `Arc<LanguageMatcher>`, so the added `sql_lang` test helper drops the
  `Arc::new` wrapper.
- `19db6a0d9e` is a `B` port of `5504e87bfd`.

### Per-Commit Narrative

#### 5504e87bfd — trust scope parents (B)

Upstream switched the security modal from `Path::parent` to
`PathStyle::parent` so a remote Windows worktree opened from Unix shows its
containing folder. ZZZ already routes trust paths through
`TrustedPath`/`NativePath` and its `trusted_worktrees` auto-trust logic already
uses `TrustedPath::starts_with`, so upstream's `PathStyle::parent`,
`starts_with`, and `join_path_preserving_components` additions have no local
caller and were omitted, along with the async home-directory task. Two
remaining raw-`Path` sites were ported: the project-list label in `Render` and
`build_trust_label` now call `TrustedPath::parent`, which resolves Windows
display paths correctly on Unix. The existing
`validate_trust_scope_accepts_remote_windows_paths_cross_platform` test already
asserts `TrustedPath::parent` for a Windows path, so the ported behavior is
covered.

#### 14119f8d87 — Rust `sql!` injection (A)

`injections.scm` now captures `string_content` for `string_literal` and
`raw_string_literal` inside `sql!`, and keeps the historical unquoted
token-tree form via a `#not-match?` guard. The commit's `syntax_map_tests.rs`
additions auto-merged onto ZZZ's existing `sql_lang` helper; the only
adaptation is the non-`Arc` matcher field noted above.

#### df51ce9d4c, 9b421ed7fe, 4f70271800 — ACP v2 session lifecycle (C)

These three add cancellation, `session/list`, and resume/replay to the inactive
v2 adapter. ZZZ has no `crates/agent_servers/src/acp/v2.rs` (its ACP server is
a flat `acp.rs`), imports only `schema::v1 as acp`, and has no
`crates/acp_thread/src/submission.rs`; the receipt lifecycle they extend does
not exist locally.

#### df1ce4fee3 — organization UI permissions (C)

Adds `ui_permissions.can_install_github_app` to the `Organization` record. The
field only feeds the organization GitHub App install surface, an account and
commercial path that ZZZ omits.

#### 8c8ada0fbf — session close vs view release (C)

Adds `AgentConnection::should_close_sessions_on_release` so the v2 backend can
opt out of automatic teardown, and rewrites `close_all_sessions` to aggregate
failures around v2 close-lifecycle tests. Without a v2 backend there is no
non-default implementor, so the new trait API would be dead scaffolding and its
error-aggregation rework is coupled to the absent v2 session model.

#### 6b218b8412 — ACP SDK 3.4.0 (C)

A `Cargo.toml`/`Cargo.lock` bump that moves `agent-client-protocol` and its
schema to the versions carrying the v2 protocol. ZZZ pins `=3.3.0` with the
`unstable` feature only, so this is dependency churn with no independent local
behavior.

#### d678e7de4d — Gemini 4 Argon pricing (C)

Edits `docs/src/account/zed-hosted-models.md`, a hosted-model pricing page that
ZZZ does not ship.

#### 9f63850f7a — effort-specific thinking controls (C)

Threads a thinking-disable capability through `language_models` providers,
`copilot_chat`, `cloud_llm_client`, and `language_models_cloud`. ZZZ has none
of those crates; its AI surface is ACP-only.

### Local Fix

`ed08b99257` repairs the pre-existing feature-unification defect recorded at
the end of Batch 2. `remote::RemoteConnectionOptions::Mock` exists whenever
`remote/test-support` is enabled, which can leak into a build through another
crate (`agent_ui/test-support` -> `workspace/test-support`) while
`recent_projects/test-support` stays off. The Mock match arms in
`recent_projects` are gated on that crate's own `test-support`, so a targeted
`./script/clippy -p sidebar -p agent_ui` run failed with a non-exhaustive
match. The fix mirrors the `cfg(not(any(test, feature = "test-support")))`
wildcard already used in `crates/project/src/trusted_worktrees.rs`, keeping
the matches exhaustive in both feature configurations without changing the
production `remote` API.

### Verification

| Check                                                       | Result  | Notes                                                   |
| ----------------------------------------------------------- | ------- | ------------------------------------------------------- |
| Live `git ls-remote` and `git fetch --no-tags`              | PASS    | `FETCH_HEAD=9f63850f7affbd57772a4f99554d4fd333b50a6f`   |
| Previous baseline ancestry                                  | PASS    | `32b16ce2ad` is an ancestor of `FETCH_HEAD`             |
| `cargo check --locked --tests -p language -p grammars`      | PASS    | `sql!` injection cherry-pick                            |
| `cargo test --locked -p language --lib test_rust_sql_macro` | PASS    | 2 tests pass                                            |
| `cargo check --locked -p workspace`                         | PASS    | trust-parent port                                       |
| `cargo test --locked -p workspace --lib security_modal`     | PASS    | 6 tests pass, including the cross-platform Windows case |
| `./script/check-philosophy`                                 | PASS    | philosophy gate                                         |
| `git diff --check`                                          | PASS    | clean                                                   |
| `./script/clippy` (workspace)                               | PASS    | full `--workspace --all-features` run                   |
| `./script/clippy -p sidebar -p agent_ui`                    | PASS    | after the `ed08b99257` local fix                        |
| Windows and macOS runtime checks                            | NOT RUN | host is Linux                                           |
| `cargo test --workspace`                                    | NOT RUN | not requested                                           |

The general reviewed baseline advances to
`9f63850f7affbd57772a4f99554d4fd333b50a6f`.
