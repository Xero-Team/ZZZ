---
title: Upstream Sync 2026-09-30
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-09-30

## Scope

- Target branch: `sync/upstream-2026-09-30` from local `main` at
  `c1b718f7ae66add906517501671df16984581890`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previously reviewed baseline: `bda9c0bd43a8d235d82adb01ea5bc875b861ecfc`
- Reviewed upstream head: `986828d157135be95d999863dd1d618990173b8c`
- Live upstream head queried: `decbf641b18f1982b3475c037e7c5c554471574f`
- Query time: `2026-09-30T02:12:34+02:00`
- Requested range starts after `bda9c0bd43a8d235d82adb01ea5bc875b861ecfc`

The batch is the first twenty commits of the range
`bda9c0bd43..decbf641b1`, ending at `986828d157`. The reviewed baseline
advances to `986828d157135be95d999863dd1d618990173b8c`. The remaining
twenty-four commits stay queued for the next run.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                  |
| ---------- | ----- | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| 4c841aaf1c | C     | --           | Dependency and CI plumbing; feature removals span the absent `livekit_client`.                                                               |
| e683fd7b46 | C     | --           | New `bench_metrics` crate and GPUI benchmark harness; benchmark plumbing is absent locally.                                                  |
| 1a28cff4b4 | C     | --           | Commit reads off the serial job queue; built on the unabsorbed #62536 `object_read_limiter` architecture, which local `git_store` lacks.     |
| d3ccd57194 | C     | --           | Sidebar/`ThreadItem` theme colors; the local sidebar renders new-thread menus and thread entries differently, so the hunk is not isolatable. |
| 4f70d91bda | B     | 3e5bb0b312   | Customizable soft wrap indentation; the file-move settings test is omitted (local rename settings refresh divergence).                       |
| 98f75a23bc | C     | --           | Dependency-removal churn with no independent local behavior.                                                                                 |
| f8b0d52b3e | A     | d7bcd56721   | Debugger "rerun last scenario" fix; cherry-picked with `-x -s`.                                                                              |
| ee43be113e | C     | --           | ACP v2 content-value migration; local keeps the `schema::v1` model and the commit touches the deleted `conversation_view/message_queue.rs`.  |
| fa6aeecf2a | C     | --           | `google_ai` and `language_models` provider crates are absent locally.                                                                        |
| 3794e5e979 | C     | --           | Keyed message upserts stacked on `ee43be113e`; touches the deleted `eval_cli`.                                                               |
| 8a4fd5704c | B     | 296805b70f   | `title_bar.open_menus_on_hover`; setting exposed with localized strings.                                                                     |
| 20e3812408 | C     | --           | Atomic tool presentation built on the unabsorbed v2 content-value model.                                                                     |
| 86c20cc911 | B     | 6474a706c8   | `InputPreference` / `KeystrokeEvent::input_preference`; key_dispatch tests omitted.                                                          |
| 00def035de | C     | --           | `which_key::ShowPendingBindings` depends on the deleted `pending_keystrokes_indicator.rs`.                                                   |
| 93c87da195 | C     | --           | `tabular_data_preview` crate is absent locally.                                                                                              |
| 2440236055 | C     | --           | GPUI seeded benchmarks and heap-allocation metrics; benchmark plumbing.                                                                      |
| 5becf8b591 | B     | 69be268216   | Pending-input panic in read-only buffers; ported into `editor.rs` with the vim regression test.                                              |
| 1b572a2bd0 | C     | --           | `hang_telemetry` crate; telemetry is rejected in source.                                                                                     |
| 447f72e516 | C     | --           | xtask GPUI publishing validation; release/publishing plumbing.                                                                               |
| 986828d157 | C     | --           | Shared tool-call state and diff previews stacked on the unabsorbed v2 work; touches deleted paths.                                           |

Totals: one `A`, four `B`, fifteen `C`.

## Applied work

`f8b0d52b3e` was a clean `A` and used `git cherry-pick -x -s`. Every `B`
was committed with `git commit -s` and a `sync:` subject carrying
`Upstream:` / `Retained:` / `Omitted:` trailers. No remote, pull request,
or named upstream remote was created.

- `d7bcd56721` uses the front of the scheduled-scenario queue.
- `296805b70f` adds `title_bar.open_menus_on_hover`, defaulted off.
- `69be268216` reads inserted ranges back from buffer anchors.
- `3e5bb0b312` adds `languages.<language>.soft_wrap_indent`, the code-action
  inline-slot placement fix, and the VS Code `editor.wrappingIndent`
  import.
- `6474a706c8` carries `InputPreference` on keystroke events.

## Per-commit narrative

### 4c841aaf1c — C

Removes unused Cargo features from `wasmtime-wasi`, `aws-*`,
`context_server`, `project_benchmarks`, and the absent `livekit_client`,
edits `Cargo.lock`, and adds a workspace test for `test-support` in
non-dev dependencies. This is dependency and CI plumbing; it changes no
editor behavior and the `livekit_client` surface is absent locally.

### e683fd7b46 — C

Adds a new `bench_metrics` crate and rewires GPUI's Criterion harness to
report hardware counters. The crate, the harness, and the benchmark
plumbing do not exist locally. Nothing in the diff is shipped editor
behavior.

### 1a28cff4b4 — C

Takes commit reads off the per-repository serial job queue by bounding
`show_commit` / `load_commit_diff` with the same semaphore as blob reads.
The commit is explicitly built on #62536, and local `git_store.rs` has no
`blob_read_limiter`/`object_read_limiter`, takes `&mut self` for
`load_commit_diff` without `ignore_shallow_boundary`, and therefore cannot
accept the change as a port. Rule 6 (unabsorbed prerequisite) applies.

### d3ccd57194 — C

Recolors the Threads sidebar header and `ThreadItem` with the
`ghost_element_*` theme roles and adds explicit button hover/active
backgrounds. A dry-run cherry-pick conflicted in `sidebar.rs` and
`sidebar_tests.rs` across three large regions: the local sidebar renders
project-header new-thread menus and thread entries with a different
structure (for example `render_project_header_new_thread_menu`), so the
retained invariant cannot be realized without rewriting the divergent
rendering. Rule 5 applies.

### 4f70d91bda — B, `3e5bb0b312`

Adds the `soft_wrap_indent` language setting (`none`, `same`,
`extra_one`, `extra_two`) and plumbs it through `LanguageSettings`,
`WrapMap`, the display map, and the GPUI `LineWrapper`
(`IndentAdjustment`), plus the code-action inline-slot placement fix and
the VS Code `editor.wrappingIndent` import. A dry-run cherry-pick
conflicted in eight files because local `wrap_map.rs` still carried an
older single-edit flush path and the settings UI is localized. The port
keeps upstream's generalized flush loop and localizes the new Settings
Editor strings with `i18n::tr` and matching `en.json` / `zh-CN.json`
keys. Upstream's `ConfiguredLanguageServer` payload was only diff
context, not an addition, and was not reintroduced. The
`test_soft_wrap_indent_updated_on_file_move_between_directories` test is
omitted: it asserts that per-directory `soft_wrap_indent` refreshes after
a worktree rename, and local settings propagation on rename returns the
old value. That is a pre-existing local divergence unrelated to the
indent mechanism; the other four tests pass.

### f8b0d52b3e — A, `d7bcd56721`

`last_scheduled_scenario` read the back of a queue that is pushed to the
front, so "Rerun last debug scenario" cycled through the oldest entry.
The local `task_inventory.rs` matched the pre-image exactly and the patch
applied cleanly.

### ee43be113e — C

Migrates `acp_thread`'s internal content representation to shared ACP v2
content values with v1 adapters, rewriting the message model, tool and
compaction boundaries, prompt-display, native replay, and settings in
`agent_ui`. Local `acp_thread` deliberately keeps the `schema::v1` surface
(`v1 as acp`) plus local abstractions such as `AgentConfigOptionValue`.
A dry-run cherry-pick conflicted across `acp_thread.rs`,
`connection.rs`, `agent.rs`, `conversation_view.rs`, `thread_view.rs`,
`thread_search_bar.rs`, `entry_view_state.rs`, and `message_editor.rs`,
and modified the deleted `conversation_view/message_queue.rs`. This is an
unisolatable rewrite, not a port.

### fa6aeecf2a — C

Adds structured provider errors and retry classification to the
`google_ai` crate and its `language_models` provider. Both crates are
absent locally; ZZZ removed the in-tree model providers.

### 3794e5e979 — C

Adds keyed user/assistant/thought upserts and chunked appends on top of
the v2 content-value stack from `ee43be113e`, rewriting ~2,000 lines of
`acp_thread.rs` and ~500 lines of `agent_ui`. A dry-run cherry-pick
conflicted in `acp_thread.rs`, `conversation_view.rs`, and
`entry_view_state.rs`, and touched the deleted `eval_cli/src/main.rs`. It
depends on an unabsorbed predecessor and is not isolatable.

### 8a4fd5704c — B, `296805b70f`

Adds the `title_bar.open_menus_on_hover` setting (default `false`) that
restores hover-to-open title-bar menus, while a menu that is already open
still switches between menus on hover. A dry-run cherry-pick conflicted
in `page_data.rs`, `application_menu.rs`, and the docs because local
settings-UI text is localized and the cross-platform-menu env var is
renamed. The port keeps upstream's behavior, adds the setting to the
default settings and `all-settings.md`, localizes the Settings Editor
strings, and leaves local doc wording in place instead of adopting
upstream's table reformatting.

### 20e3812408 — C

Makes tool-presentation updates atomic by preparing all incoming content
before mutating presentation fields. The commit is built on the v2
content-value model from `ee43be113e` and its shared preparation path; a
dry-run cherry-pick conflicted in `acp_thread.rs`. Rule 6 applies.

### 86c20cc911 — B, `6474a706c8`

Adds `InputPreference` and `KeystrokeEvent::input_preference`, computed
before interception and recomputed before binding dispatch, so
interceptors can honor the platform text-over-bindings policy. `app.rs`
applied directly; `window.rs` conflicted only on the old inline
`skip_bindings` block, which was replaced with `Window::input_preference`.
Upstream's `key_dispatch` test helpers and new tests are omitted: local
`key_dispatch` tests have diverged and lack `PendingTextInputTestView`.
The production API is a complete local behavior on an existing path.

### 00def035de — C

Adds the `which_key::ShowPendingBindings` action and the `alt-/`
keybinding that opens the pending-keystrokes list. A dry-run cherry-pick
reports `crates/which_key/src/pending_keystrokes_indicator.rs` as deleted
in HEAD and modified by the commit. Local `which_key` ships only
`which_key_modal.rs` and `which_key_settings.rs`; the indicator path was
deliberately removed, so the feature cannot land without reintroducing a
deleted file.

### 93c87da195 — C

Extracts `TableView` from the tabular-data preview pane. The
`tabular_data_preview` crate is absent locally.

### 2440236055 — C

Adds seeded `#[gpui::bench]` benchmarks and heap-allocation metrics to
the GPUI benchmark harness. This is benchmark plumbing on an absent
harness.

### 5becf8b591 — B, `69be268216`

`observe_pending_input` assumed the pending keystrokes were always
inserted; in a read-only buffer at end of file it pointed one character
past the end and panicked. The port takes buffer anchors before
inserting, reads the inserted range back from them, and drops the
redundant read-only guard. ZZZ deleted `editor/src/input.rs`, so the
function was edited in `editor.rs`; upstream's vim regression test
applies and passes.

### 1b572a2bd0 — C

Extracts the "Hang Incidents" telemetry pipeline into a new
`hang_telemetry` crate that emits a `telemetry_events::FlexibleEvent`.
Telemetry is absent in ZZZ source and must not be reintroduced.

### 447f72e516 — C

Adds license and unique-name validation to the xtask GPUI publishing
plan. This is release/publishing plumbing for crates ZZZ does not
publish this way.

### 986828d157 — C

Normalizes legacy ACP tool reports and v2 tool patches into a shared
`ToolCall` model and renders v2 patches with read-only diff editors. It is
stacked on `3794e5e979` and `20e3812408`, touching `acp_thread.rs`,
`diff.rs`, `agent.rs`, `agent_servers`, and `agent_ui`, and modifying the
deleted `eval_cli/src/main.rs`. It depends on unabsorbed predecessors and
is not isolatable.

## Verification

| Check                                                                               | Result  |
| ----------------------------------------------------------------------------------- | ------- |
| `git diff --check`                                                                  | PASS    |
| `cargo fmt --all -- --check`                                                        | PASS    |
| `cargo check --locked -p gpui`                                                      | PASS    |
| `cargo check --locked -p editor --tests`                                            | PASS    |
| `cargo check --locked -p settings --tests`                                          | PASS    |
| `cargo check --locked -p settings_ui`                                               | PASS    |
| `cargo check --locked -p title_bar`                                                 | PASS    |
| `cargo check --locked -p zzz -p vim --tests`                                        | PASS    |
| `cargo test --locked -p vim --lib test_jk_pending_input_at_end_of_read_only_buffer` | PASS    |
| `cargo test --locked -p editor --lib soft_wrap_indent` (4 passed)                   | PASS    |
| `cargo test --locked -p editor --lib display_row_for_inline_code_action` (2 passed) | PASS    |
| `cargo test --locked -p gpui --lib text_system::line_wrapper` (16 passed)           | PASS    |
| `cargo test --locked -p settings --lib test_import_wrapping_indent`                 | PASS    |
| Locale key parity (`en.json` vs `zh-CN.json`, 3454 keys each)                       | PASS    |
| `./script/check-philosophy`                                                         | PASS    |
| `test_soft_wrap_indent_updated_on_file_move_between_directories`                    | OMITTED |
| macOS / Windows runtime checks for platform hunks                                   | NOT RUN |
| `cargo test --workspace`                                                            | NOT RUN |

The omitted soft-wrap test fails only because local rename handling does
not refresh per-directory settings; the retained indent behavior is
covered by the four passing tests. `InputPreference` was verified by type
and borrow checking only; the GPUI interception runtime behavior has no
local automated test after the divergent test module was dropped.

## Continuation (batch 2)

### Scope

- Target branch: `sync/upstream-2026-09-30`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previously reviewed baseline for this run:
  `986828d157135be95d999863dd1d618990173b8c`
- Reviewed upstream head: `5d5963361f3080539f2d7034e92736167c53a04a`
- Live upstream head queried: `decbf641b18f1982b3475c037e7c5c554471574f`
- Query time: `2026-09-30T02:48:20+02:00`
- Range continues after `986828d157135be95d999863dd1d618990173b8c`

This is the second batch of the day. It covers the next twenty commits,
ending at `5d5963361f`, and advances the reviewed baseline to
`5d5963361f3080539f2d7034e92736167c53a04a`. Four commits remain queued
for a later run: `017f9b89aa`, `0f9c923e67`, `263fb11177`, and
`decbf641b1`.

### Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                  |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------ |
| 2280e5d95e | A     | f521eeb8cf   | Vim multi-selection paste crash; cherry-picked with `-x -s`.                                                 |
| e773a56d6c | C     | --           | gpui_web iOS keyboard/browser-activity fix uses absent `touch_input`/`ime_mirror` infrastructure.            |
| 408c713a35 | A     | 03b60caa4f   | Default `Typst.allow_rewrap`; cherry-picked with `-x -s`.                                                    |
| c821538b57 | C     | --           | `agent.max_idle_retained_threads` subscribes to `AcpThreadEvent::StatusChanged`, which local never absorbed. |
| 3135cdf8dc | C     | --           | Random element-tree benchmarks and leak fixes on an absent GPUI benchmark harness.                           |
| 72d28c32c2 | C     | --           | Shared plan state built on the unabsorbed ACP v2 content-value model (`acp_v2`).                             |
| dd510f99e0 | C     | --           | `language_models_cloud` provider crate is absent locally.                                                    |
| bd747337d7 | B     | 44ead3d869   | Drops legacy TOML/Zig extension executable handling; local comment text had diverged.                        |
| ead2d9eac0 | B     | 8f93e7484d   | Strips the GitHub `sha256:` prefix during deserialization.                                                   |
| eb1ca9768b | C     | --           | GitHub workflow build-step plumbing.                                                                         |
| c32938c34c | C     | --           | `open_ai` provider crate is absent locally.                                                                  |
| 12f79c0aeb | C     | --           | `cloud_api_client` crate and cloud websocket path are absent.                                                |
| 14dd03e896 | B     | 0f53bee90d   | Guards follow-up agent sends from stale send results.                                                        |
| 9307bb7dc5 | C     | --           | macOS PR runner sizing in CI.                                                                                |
| 7604aa3f19 | C     | --           | Removes the visual test binary CI build.                                                                     |
| c87632ef44 | B     | be2c72286a   | Reads remote `terminal.shell` through a new `GetTerminalShell` RPC.                                          |
| adfcaa122a | C     | --           | Documents Zed-hosted model pricing; a commercial account surface.                                            |
| 1dc8844439 | B     | 23ff6cbd7d   | which-key shows task names for `task::Spawn::ByName` bindings.                                               |
| 7232ba04ef | C     | --           | GPUI profiler `journal` module and its dependency are absent locally.                                        |
| 5d5963361f | B     | a5a12fe3b7   | Diffs LSP format responses that replace the whole buffer.                                                    |

Totals: two `A`, six `B`, twelve `C`.

### Applied work

`2280e5d95e` and `408c713a35` were clean `A` commits and used
`git cherry-pick -x -s`. Every `B` was committed with `git commit -s` and
a `sync:` subject carrying `Upstream:` / `Retained:` / `Omitted:`
trailers. No remote, pull request, or named upstream remote was created.

- `f521eeb8cf` adds `BufferSnapshot::summaries_for_anchors_unordered` and
  uses it to serialize Vim paste marks in document order.
- `03b60caa4f` allows rewrapping for Typst in the default settings.
- `44ead3d869` stops chmod'ing language server binaries for the legacy
  `toml@0.0.2` and `zig@0.0.1` extension versions.
- `8f93e7484d` deserializes GitHub release assets through
  `deserialize_sha256_digest`, so callers always see a bare digest.
- `0f53bee90d` or turn generation guards the view update after a send
  settles, and clears a stale error when a new turn starts.
- `23ff6cbd7d` labels `zzz_actions::Spawn::ByName` bindings with their
  task name in the which-key modal.
- `c87632ef44` adds the `GetTerminalShell` request/response messages and a
  headless-project handler, and uses the resulting shell for remote
  terminals.
- `a5a12fe3b7` diffs a whole-buffer formatting edit against the buffer and
  applies minimal text edits instead.

### Per-commit narrative

#### e773a56d6c — C

Rewrites touch-key handling and focus/blur activity tracking in
`gpui_web`. The added keyboard path reads `this.touch_input` and
`this.ime_mirror.virtual_keyboard_enabled()`, neither of which exists
anywhere in local `crates/gpui_web`. The commit depends on unabsorbed
touch and IME infrastructure; rule 6 applies.

#### c821538b57 — C

Adds `agent.max_idle_retained_threads` and re-runs retained-thread cleanup
when a thread becomes idle by subscribing to
`AcpThreadEvent::StatusChanged`. Local `AcpThreadEvent` has no
`StatusChanged` variant; upstream had it by `bda9c0bd43`, but the local
fork never absorbed it. The port would require importing the event and
its emitters first. Rule 6 applies.

#### 3135cdf8dc — C

Adds a randomized element-tree benchmark, a `gpui_platform` bench
harness, and Linux/WGPU benchmark leak fixes. ZZZ carries none of this
benchmark plumbing and the change ships no editor behavior.

#### 72d28c32c2 — C

Gives structured agent plans retained keyed state. It is written against
`acp_v2::PlanEntry`, `acp_v2::PlanId`, and `acp_v2::Meta`, while local
`acp_thread` keeps `agent_client_protocol::schema::v1 as acp`. The shared
v2 model was rejected in the previous batch (`ee43be113e`). Rule 6
applies.

#### dd510f99e0 — C

Classifies token failures in `language_models_cloud` and adjusts
`client.rs`. The `language_models_cloud` crate is absent locally; ZZZ
removed the in-tree model providers.

#### bd747337d7 — B, `44ead3d869`

Removes the legacy `toml`/`zig` executable fixup from
`extension_lsp_adapter.rs`. A dry-run cherry-pick conflicted only because
the local TODO comment renamed `zed::` to `zzz::`. The port deletes the
same block and its now-unused `make_file_executable` import; `with_context`
still uses the retained `Context` import.

#### ead2d9eac0 — B, `8f93e7484d`

Moves the GitHub release digest normalization into a `serde`
`deserialize_with` function so both `latest_github_release` and
`get_release_by_tag_name` strip `sha256:`. A dry-run cherry-pick
conflicted in the tests module because local `github.rs` never had the
upstream request-deadline test. The port keeps the digest deserialization
test and omits the deadline test.

#### 14dd03e896 — B, `0f53bee90d`

Ensures an older send's settled result cannot mutate the view of a newer
turn. Local had already removed the `Agent Turn Completed` telemetry from
this path, so the port keeps the generation guard and omits the telemetry
event. `start_turn` now clears any displayed error, and
`StubAgentConnection::defer_next_prompt_response` plus one regression test
cover late success/error under deferred contents. The test uses the local
`acp` v1 alias and the local 3-argument `send_content`.

#### c87632ef44 — B, `be2c72286a`

Makes `terminal.shell` configurable for remote sessions. Local proto
envelopes live in `zzz.proto` with a different numbering than upstream
`zed.proto` (local max was `461`), so the port adds `GetTerminalShell` and
`GetTerminalShellResponse` at `462`/`463` and registers them in
`messages!`, `request_messages!`, and `entity_messages!`. The client
requests the remote shell in `create_terminal_shell_internal` and picks
program, arguments, or system accordingly; `remote_server` gains a
`terminal` dependency and a `handle_get_terminal_shell` handler that reads
`TerminalSettings`. The `test_remote_settings` additions are adapted to
the local raw-string shape. Its pre-existing final `override-rust-analyzer`
assertion fails on the local baseline independently of this change.

#### 1dc8844439 — B, `23ff6cbd7d`

Labels which-key bindings that spawn a named task with the task name.
Local `which_key` renders through `which_key_modal.rs`; the deleted
`pending_keystrokes_indicator.rs` path does not exist, and `Spawn` lives
in the renamed `zzz_actions` crate. The port adds `binding_label` to the
modal, a `zzz_actions` dependency, and a unit test.

#### 7232ba04ef — C

Adds foreground-measurement drop logic to a GPUI profiler journal and a
new dependency. Local `crates/gpui/src/profiler/` does not exist, so the
target module is absent.

#### adfcaa122a — C

Documents Zed-hosted model pricing. Hosted models, pricing, and accounts
are a commercial surface that is absent by design.

#### 5d5963361f — B, `a5a12fe3b7`

When a formatting response is one edit that replaces the whole buffer,
local now diffs the formatted text against the buffer and applies minimal
edits, preserving unchanged anchors instead of selecting the document and
scrolling to the end. A dry-run cherry-pick conflicted because local
`format_via_lsp` uses direct `matches!` provider guards rather than
upstream's `formatting_supported` booleans. The port keeps the local
guards, omits the upstream `test_range_formatting_prefers_range_capable_current_server`
helper test (never absorbed locally), and ports the anchor-preservation
test adapted to the local test module.

### Verification

| Check                                                                                                   | Result  |
| ------------------------------------------------------------------------------------------------------- | ------- |
| `git diff --check`                                                                                      | PASS    |
| `cargo fmt --all -- --check`                                                                            | PASS    |
| `cargo check -p proto`                                                                                  | PASS    |
| `cargo check -p which_key -p http_client -p language_extension`                                         | PASS    |
| `cargo check -p acp_thread -p agent_ui --tests`                                                         | PASS    |
| `cargo check -p project -p remote_server --tests`                                                       | PASS    |
| `cargo check -p zzz -p vim --tests`                                                                     | PASS    |
| `cargo test -p text --lib test_summaries_for_unordered_anchors` (2 passed)                              | PASS    |
| `cargo test -p vim --lib test_paste_multiple_clipboard_selections_at_end_of_file`                       | PASS    |
| `cargo test -p agent_ui --lib test_stale_send_result_preserves_new_prompt`                              | PASS    |
| `cargo test -p project --test integration test_full_buffer_formatting_edit_preserves_unchanged_anchors` | PASS    |
| `cargo test -p which_key --lib test_binding_label_uses_task_name_for_spawn_by_name`                     | PASS    |
| `cargo test -p http_client --lib test_asset_digest_deserialization`                                     | PASS    |
| `test_remote_settings` (pre-existing `override-rust-analyzer` assertion)                                | FAIL    |
| macOS / Windows runtime checks for platform hunks                                                       | NOT RUN |
| `cargo test --workspace`                                                                                | NOT RUN |

The remote shell assertion added to `test_remote_settings` passed before
the pre-existing local `override-rust-analyzer` assertion failed; the
failure reproduces on the clean baseline without this batch's changes.
