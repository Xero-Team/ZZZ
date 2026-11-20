---
title: Upstream Sync 2026-11-20 (E)
description: Selective Zed upstream sync, the batch that clears the queue after 4a7d10473a.
---

# Upstream Sync 2026-11-20 (E)

## Scope

- Local base: `main` at `0ad7ea6dbc`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `4a7d10473a3ed32c10026e549381f064dd020114`
- Reviewed range: `4a7d10473a..54fff2ff5b`, 25 commits, oldest first
- New reviewed baseline: `54fff2ff5b2f9fdbe1b99c0c2566e15dcb6cfcb9`
- Live head at query time: `54fff2ff5b2f9fdbe1b99c0c2566e15dcb6cfcb9`
- Remaining after this batch: none

This batch drains the queue that batches B through D left behind: it is
the first pass that reaches the live head since `4a7d10473a`. Two of its
commits were skipped ahead as large features and are classified here for
the first time. `df1ba676f2` is a real port; `0cc1ffb6b3` is rejected
because it builds global skills for the native agent, and that crate is
absent.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                          |
| ---------- | ----- | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| df1ba676f2 | B     | 70289b7bd5   | Toolbar/overflow split for ACP config options, ported onto the local V1 ACP call chain; payload below.                                                               |
| 0cc1ffb6b3 | C     | --           | Local has no `crates/agent_skills`, no native agent tool modules, and no skills settings pages.                                                                      |
| 10c56fa0c6 | B     | 48b7eca8e5   | Ported in batch D; the same row is recorded here only to keep the range contiguous.                                                                                  |
| 72ff4e05ee | C     | --           | Changes only the `folder_indicator: "both"` mode, which is absent locally; local still renders either a folder icon or a chevron.                                    |
| 6ae76a6a0a | C     | --           | Needs `Speed::Ultrafast` plus provider crates that are absent locally.                                                                                               |
| 2cc3c8ec76 | C     | --           | Sandbox grants and proxy events live in `crates/sandbox` and `crates/agent/src/sandboxing.rs`; local has neither.                                                    |
| 3d3bd04fad | C     | --           | Needs `crates/anthropic`; no local provider builds `MessageContent::Thinking`, so the shared-model change has no caller.                                             |
| f693ad0847 | C     | --           | Depends on `crates/git_ui_core` and worktree root-repository discovery APIs that are absent locally.                                                                 |
| 07278eddc4 | C     | --           | Fixes a cache cleanup routine absent locally, and rewrites the `agent_servers/src/acp/{v1,v2,transport}.rs` split that local does not have.                          |
| 925ee76e1b | C     | --           | Sweeping cleanup whose hunks cross live local code (`clock` tests, `project_search`). Removed items still exist unused locally; follow-up list below.                |
| 5a0e9521f6 | C     | --           | Same shape: `sidebar`'s `is_remote` and `ModalHeader::show_back_button` are live locally, so the patch needs a hunk-by-hunk split. Follow-up list below.             |
| d5615304a1 | C     | --           | Dry-run cherry-pick produced 13 conflicts across `project`, `dap` and `extension_host`, pairing unrelated functions. Follow-up list below.                           |
| a246423a3b | A     | c0643158a0   | Clean cherry-pick; `e0bbc9b890` drops the two `.collect()` calls local `highlight_text` does not need.                                                               |
| 4e2c957a6d | C     | --           | 13+ removed items live in provider crates local does not carry; the rest are scattered small hunks across `agent`, `acp_thread` and `action_log`.                    |
| 7279c76ef7 | C     | --           | Upstream deletes `BufferOffsetUtf16` and a row-mapping helper that local still uses in 68 places; the rest needs a hunk-by-hunk split.                               |
| 87da03ad3f | C     | --           | Core of the patch removes `pane_group` center markers and the agent-panel dock trait dispatch, both still live locally.                                              |
| 87e6a02e1e | B     | b51d411ad6   | Ported onto the single-generation `RemovedEntries`; payload below.                                                                                                   |
| ad1058d78d | C     | --           | Rewrites `crates/anthropic`, `crates/copilot_chat`, and the cloud language-model providers; none exist locally.                                                      |
| a65e743ad9 | B     | 9e7d2e5a8d   | Auth-status snapshot, connection hooks and panel account rows, ported onto the local V1 ACP call chain; payload below.                                               |
| 46d9717008 | C     | --           | Touches only `tooling/crates_io_client`, which is absent locally.                                                                                                    |
| 10d1b2a564 | C     | --           | Needs the provider crates `crates/open_ai` and `crates/openai_subscribed`, both absent locally.                                                                      |
| a68d67e058 | C     | --           | The premise that these fields are write-only is false here; `crates/editor/src/inlays/inlay_hints.rs` still reads `column_overshoot_after_line_end`.                 |
| 0a514d7ef5 | B     | c7183557d5   | Landed before this batch without a report row; recorded here so the range is covered.                                                                                |
| 3463950447 | C     | --           | Modifies `script/bundle-mac`, absent locally.                                                                                                                        |
| 54fff2ff5b | C     | --           | Removes no-op builders upstream, but local `render` still reads the fields, so the removal conflicts with live local code and would change component-preview layout. |

Totals: one `A`, five `B`, nineteen `C`.

## Applied Work

- `a246423a3b` used `git cherry-pick -x -s` against the clean tree. Its
  ported test called `.collect()` on `Language::highlight_text`, which
  returns a `Vec` here, so `e0bbc9b890` removes the two calls.
- `87e6a02e1e` used `git cherry-pick -x -s` and its conflict payload is
  described below.
- `df1ba676f2` keeps upstream's title and body shape but, being a `B`
  port, carries the `Upstream:`/`Retained:`/`Omitted:` payload below.
- `a65e743ad9` is a `B` port and carries the same payload below.

No named upstream remote, push, or pull request was created.

## Per-Commit Narrative

### df1ba676f2 — Agent Options menu in the external-agent composer (B)

Retained: the composer keeps one toolbar control per category, in the
order speed, thinking effort, mode, model, and moves every remaining
config option into an "Agent Options" overflow popover whose pickers keep
their search, groups and favorites. `ConfigOptionsView` gains
`response_controls()`, `toggle_fast_mode()`, `has_overflow` and the
overflow popover handle; `ConfigOptionSelector` gains `is_response_control`
and renders through the shared `build_config_option_picker`, which is what
lets an embedded picker live inside a submenu. `ContextMenu` gains
`embedded_view()`, `focus_content()` and weak parent references so a
picker released while its submenu is open cannot keep the menu alive. A
failed option change now shows in the workspace instead of being dropped,
and four new strings go through `i18n::tr` into both catalogs.

Omitted: `is_read_only` and every read-only branch, because the local
`AgentSessionConfigOptions` trait has no `is_read_only()`; all
`.aria_label(...)` calls, because `aria_label` is gated on the
`accessibility` feature in `crates/ui` and unreachable from `agent_ui`;
upstream's `render_fast_mode_control`/`render_thinking_control` refactor,
because both are native-agent stubs returning `None` here; the two
`VisualTestContext` menu tests and the read-only tests, because the local
tree has no `TestConfigComposer` or menu-action harness. The toolbar order
and the boolean toggle those tests describe are covered by
`composer_uses_the_first_matching_option_for_each_toolbar_control` and
`toggling_fast_mode_uses_the_boolean_speed_option`.

Upstream's `embedded_view` support arrived in an earlier commit
(`37568ad7dc`) that local does not carry, so the port carries a minimal
local equivalent instead of that commit's shape. No local test covers the
submenu focus redirection; the reviewer should exercise the embedded
picker by hand.

### 87e6a02e1e — less work under the scan lock (B)

Retained: `BackgroundScanner::scan_dir` now checks `is_path_excluded`
before it takes the state lock, sorts siblings by file name before the
lock so `populate_dir`'s `SumTree::edit` receives them nearly in order,
reserves one contiguous block of entry ids per directory with
`next_entry_id.fetch_add`, and skips `state.reuse_entry_id` when there is
nothing to reuse.

Omitted: nothing. The only conflicts were the upstream `is_empty`
helpers, which this tree needs on its own `RemovedEntries` rather than on
upstream's two-generation `RemovedEntriesGeneration`: the port adds
`RemovedEntries::is_empty` over the local `by_inode`/`by_path` fields and
keeps the local single-generation structure. `may_reuse_entry_ids` is
expressed with the same two local predicates, `removed_entries.is_empty()`
and `snapshot.child_entries(...)`.

### a65e743ad9 — external agent authentication status (B)

Retained: `ConnectionAuthStatus` holds a `watch` snapshot of the agent's
`auth/status` answer, so a newer notification supersedes an outstanding
query, a probe that fails publishes `Unknown` rather than an error, and a
closed transport clears the snapshot. `AgentConnection` gains
`auth_status()` and `refresh_auth_status()` defaults; the ACP connection
implements both, gated on the local `acp-beta` flag and on the agent
advertising the `auth/status` capability. The panel renders the active
account labels inside the **Toggle Agent Menu** popover above
Reauthenticate and Log Out, and login and logout re-query the agent
because a sign-out notification may never arrive.

Omitted: the new-session sign-in preflight (`preflight_auth`,
`DeferredSessionStart`, `preflight_new_session`) and its two tests, which
need `handle_auth_required` to take a completion sender; both call sites
live in `thread_view.rs`. The stdio negotiation test, because the local
fake harness bypasses the `stdio` transport where the capability gate
lives, so it needs a new mock rather than a port. The upstream
`acp/{v1,v2,runtime,debug_log}.rs` module split, and the v1-to-v2
conversion pair: this tree speaks one protocol, so the notification
handler publishes the v1 response directly. No locale keys: the rows
render strings the agent supplies.

## Follow-Up: Dead Items Still Unused Locally

The six sweep commits above are `C` because they do not apply as units,
not because their targets are alive. These items are still present here
and have no local caller, so a local sweep can remove them without
waiting for another upstream pass:

- `925ee76e1b`: `EditableSettingControl` and its module, `paths::embeddings_dir`,
  `paths::devcontainer_dir`, `util::{path_ends_with, without_path}`,
  `sqlez`'s `scope_alloc_new`/`scope_to_alloc`,
  `migrate_edit_prediction_provider_settings`, plus the unused helpers in
  `zlog`, `http_client` and `migrator`. Keep `FakeSystemClock::set_now`
  (used by local `clock` tests) and `compare_rel_paths`.
- `5a0e9521f6`: `ContextMenu::{header_with_link, custom_entry_with_docs}`,
  `DropdownMenu::no_chevron`, `Modal::show_back`, `Section::new_contained`,
  `Tooltip::new_element`, `Toggleable::from_any_and_all`,
  `FormatDistance::from_now`, `ColorScale::step_alpha`,
  `PerformanceMetrics::get_timing`, `sidebar`'s `project_header_menu_ix`.
  Keep `is_remote` and `show_back_button`.
- `d5615304a1`: `crates/task/src/static_source.rs` (whole file, unused
  module), `dap`'s `DapStatus`/`GithubRepo`/`ScopeId`,
  `LspStore::has_notification_handler`, `Project::{get_workspace_root,
contains_local_settings_file, is_main_worktree, contains_sub_repo}`,
  `terminals::local_terminal_handles`, `OpenedBufferEvent`. Keep
  `set_create_worktree_error`.
- `4e2c957a6d`: `TerminalProviderCommand`, `AgentModelList::is_flat`,
  `TrackedBuffer::diff_base_len`, `expect_terminal`,
  `CompletionTotal::from_options`, `stub_agent_connection`.
- `7279c76ef7`: `EditorSettings::disable_indent_guides_for_buffer`,
  `set_show_horizontal_scrollbar`, `FoldStatus`, `HunkAddedColor`,
  `HunkRemovedColor`, `element::bounding_rect`, `inline_value_inlays`,
  `set_native_display_map_id`, `bottom_relative`, `should_expand_up`,
  `path_for_anchor`. Keep `BufferOffsetUtf16` and `buffer_row_to_tab_row`.
- `87da03ad3f`: `kernel_options::info_text`, three `notebook_ui` constants,
  `BufferSearchBar::enable_search_option`, `ScrollTerminal`,
  `ContentMode::is_limited`, `TestItemEvent`, `Notification::unsuppress`,
  `Pane::{set_close_pane_if_empty, render_menu_overlay, pop_forward}`,
  `PaneGroup::along_edge`, `SerializedWorkspaceLocation::sorted_paths`,
  `StatusBar`'s item-position helpers, `has_active_toast`,
  `set_bottom_dock_layout`. Keep the `pane_group` center markers and the
  `is_agent_panel` dock dispatch.

## Verification

| Check                                                                        | Result  | Notes                                                                                                                                                                                      |
| ---------------------------------------------------------------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `cargo check --locked -p worktree`                                           | PASS    | After the `87e6a02e1e` port.                                                                                                                                                               |
| `cargo test --locked -p worktree --features test-support --lib`              | PASS    | 9 passed.                                                                                                                                                                                  |
| `cargo test --locked -p worktree --features test-support`                    | FAIL    | Baseline: `crates/worktree/tests/integration/worktree_tests.rs` builds `proto::File` without the local `is_private` field. Reproduces at `0ad7ea6dbc`, and that file is outside this diff. |
| `cargo test --locked -p languages --lib typescript`                          | PASS    | 15 passed, including `test_builtin_type_highlighting`.                                                                                                                                     |
| `cargo check --locked -p agent_ui -p ui`                                     | PASS    | After the `df1ba676f2` port.                                                                                                                                                               |
| `cargo test --locked -p agent_ui config_options`                             | PASS    | 9 passed.                                                                                                                                                                                  |
| `cargo check --locked -p agent_servers -p acp_thread -p agent_ui`            | PASS    | After the `a65e743ad9` port.                                                                                                                                                               |
| `cargo test --locked -p agent_servers acp`                                   | PASS    | 25 passed, including the three auth-status tests.                                                                                                                                          |
| `cargo test --locked -p agent_ui auth`                                       | PASS    | 7 passed, including `test_auth_required_on_initial_connect`.                                                                                                                               |
| `cargo check --locked --workspace`                                           | PASS    | Warnings only, all pre-existing manifest warnings.                                                                                                                                         |
| `cargo xtask check locale-keys`, `-locale-placeholders`, `-locale-fallbacks` | PASS    | Both catalogs carry the new keys.                                                                                                                                                          |
| `cargo xtask check upstream-ledger`                                          | PASS    | 889 rejection rows.                                                                                                                                                                        |
| `cargo fmt --all -- --check`                                                 | PASS    |                                                                                                                                                                                            |
| Prettier over this report                                                    | PASS    | `3.9.10`, the version pinned by `script/prettier`.                                                                                                                                         |
| macOS and Windows runtime behavior                                           | NOT RUN | This host is Linux. The `script/bundle-mac` and non-Linux hunks were only inspected.                                                                                                       |
