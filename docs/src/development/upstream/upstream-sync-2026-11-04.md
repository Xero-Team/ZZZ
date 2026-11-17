---
title: Upstream Sync 2026-11-04
description: Selective Zed upstream sync through the v1.29.0 release.
---

# Upstream Sync 2026-11-04

## Scope

- Local base: `main` at `be259a36db62a39947e8c61961be5e558a40b788`
- Target branch: `sync/upstream-2026-11-04`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `00b86477f8b81c8d20328b096200f91452a572df`
- Live upstream head queried: `c7bea343838f42b4c14151393e41f0bbe033c95e`
- Query time: `2026-11-04`
- Reviewed range: `00b86477f8b8..c7bea343838f`, 29 commits, oldest first
- Remaining after this batch: zero; the range ends at the live head at
  query time

The previous baseline is an ancestor of the live head. The range
includes the v1.29.0 release bump and the revert of one commit in the
same window. A second query later in the day found one more GPUI commit
(`c7bea34383`) after `e21ff1b368b`; it is recorded in the addendum
below. The next sync starts after `c7bea343838f`.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                                                                                  |
| ---------- | ----- | ------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 504a7f2cf2 | B     | 64e57160d9   | Adds secondary-modifier wheel zoom to standalone terminals and split diffs; the left-hand editor inherits the right-hand editor's zoom setting. Ported onto ZZZ's consolidated `editor/src/element.rs` and `ScrollWheelEvent` shape.                                                         |
| 37568ad7dc | C     | --           | Adds `ContextMenu::embedded_view` and `focus_content` for the git panel's base branch picker. ZZZ has no embedded-view submenu or `ContextMenuSubmenu` type; the feature's only caller is the upstream branch-mode UI ZZZ does not ship. New public API with no local caller.                |
| 504abf602a | C     | --           | Adds `edit_predictions.button` and a Settings Editor toggle for the edit prediction status button. ZZZ deleted the `edit_prediction_ui` crate and the `EditPredictionButton`; no status item or settings row exists to gate.                                                                 |
| 8e8f54c67e | C     | --           | Rewrites upstream `docs/src/reference/all-settings.md` (+1104 lines) around the Zed Settings Editor. ZZZ's copy is a separate 5889-line catalog that does not describe those upstream settings; doc-only churn with no ZZZ behavior change.                                                  |
| 0294cfb58e | B     | d6024a1ff6   | Preserves DirectWrite's native tab advances on Windows, so tabs keep their width and caret positions. Ported onto ZZZ's `ShapedRun`, which carries synthetic italic and bold fields upstream lacks. Windows-target check passed; runtime NOT RUN (Linux host).                               |
| ccb25cbc63 | C     | --           | Adds the `tooling/crates_io_client` crate. ZZZ has no `tooling/crates_io_client` member and no caller for the client; upstream-only release infrastructure.                                                                                                                                  |
| 4ebf3eac14 | C     | --           | Adds list/get/update crates.io endpoints and types to the new client crate. Depends on `ccb25cbc63`, which is absent locally; no ZZZ caller.                                                                                                                                                 |
| 601a7cc8ac | C     | --           | Tracks ACP subagent working directories through `acp_thread/src/subagents.rs`, `agent_servers/src/acp/v1.rs` and `v2.rs`. All three files are absent locally (ACP v2 and the subagent association layer were not absorbed); cherry-pick deleted/modified six paths.                          |
| d36a706614 | C     | --           | Implements ACP v2 background work (paused tool calls, background list, cancel buttons). Depends on `agent_servers/src/acp/v2.rs` and `acp_thread` state updates that are absent locally.                                                                                                     |
| 8a47b35109 | B     | b97eccd46d   | Logs a warning and renders no highlights when a grammar returns an error, instead of panicking. Ported onto ZZZ's existing `parse_text`/`highlight_text` path; upstream's `HighlightSource` trait and `ResolvedHighlights` are absent.                                                       |
| 57dfd54ade | A     | 064141d8b3   | Clean cherry-pick. Fixes the Vim visual-block hang at a wrapped end of file.                                                                                                                                                                                                                 |
| b61f33f8b6 | C     | --           | Rebuilds `ContextMenu` around a new `ContextMenuSubmenu` type and adds `context_menu/preview.rs` for persistent submenu refresh. ZZZ's context menu still uses the plain `Submenu` variant; the 1264-line rewrite is unisolatable from that divergence and has no local caller.              |
| ce6c5413f1 | B     | fa872535f3   | Adds GPUI `Anchored::attach` and makes popovers/dropdowns flip to the side with room. The `context_menu` hunks were omitted because they depend on `ContextMenuSubmenu`; the edit-prediction settings page is absent. Tests adapted to ZZZ's `ScrollWheelEvent` and `debug_bounds` APIs.     |
| b7a6e5cdc0 | C     | --           | Improves call hierarchy name/detail spacing and tooltips in `crates/call_hierarchy`, which ZZZ deleted. Cherry-pick reported the main file deleted by us and two absent dependencies; no local caller.                                                                                       |
| 7e7311ad20 | C     | --           | Loads up to 16 definition/reference results and completion details at once. Reverted in full by `ba36af2050` in this same window, so the net upstream state is unchanged; there is no behavior to absorb.                                                                                    |
| dd34370114 | C     | --           | Adds `OrganizationChanged` and an organization management permission so AI features stop using a lost organization. ZZZ has no accounts, organizations, plan, or `UserStore::organizations`; the account surface is deliberately absent.                                                     |
| ba36af2050 | C     | --           | Reverts `7e7311ad20` before release. Its net effect on ZZZ is nil because the reverted commit was never absorbed; a revert-only commit has no local behavior to retain.                                                                                                                      |
| 8a41a5cc04 | C     | --           | Adds GitHub Trusted Publishing endpoints to `tooling/crates_io_client`. Depends on the absent client crate; upstream-only infrastructure.                                                                                                                                                    |
| ad54befbc6 | B     | 3aa9b3f939   | Activates a panel before opening its dock, so docking does not first activate the previously active panel. Cherry-pick conflicted only in a `#[gpui::test]` placement; the regression test is retained. Follow-up on 2026-11-13 ported the focus-restoration code its import also pulled in. |
| ead5672e16 | C     | --           | Fixes compaction being billed as un-cached input tokens in `crates/agent/src/thread.rs` and provider crates. The native agent thread and the compaction strategy were deleted locally with the native agent tools.                                                                           |
| 9fa6fd3f44 | B     | 0a57dd7fcd   | Lets the worktree picker match checked-out branch names and highlight them. Ported onto ZZZ's consolidated `crates/git_ui/src/worktree_picker.rs`; the placeholder keeps the localized key and updates both locale catalogs.                                                                 |
| 3c4f6d6e90 | C     | --           | Preserves the prompt-cache prefix during compaction for `openai_subscribed`. ZZZ has no ChatGPT/Zed subscription provider and no compaction state; the provider file is absent.                                                                                                              |
| 784374c679 | B     | 2485441938   | Uses `-T` for shell execution and `-t` for terminal launches on remote SSH, and quotes Windows Nushell commands only in PTY mode. Ported with ZZZ's split `build_command`/`build_command_with_options`; test expectations use ZZZ's `-q` SSH flag.                                           |
| b278e79c97 | B     | 650a323090   | Version follow to v1.29.0; the `zzz` package and lockfile version are retained. Upstream's `crates/zed` package identity and release tooling are omitted.                                                                                                                                    |
| 056a44d118 | C     | --           | Allows custom submenu triggers in `ContextMenu` and extends `context_menu/preview.rs`. Both build on the rejected `ContextMenuSubmenu` rewrite; no local caller.                                                                                                                             |
| d71ff73440 | B     | 0be3235160   | Resolves worktree tasks against their own worktree and keeps file variables for single-file and external items. Tests adapted to ZZZ's `ZZZ_` variable prefix and `.ZZZ/tasks.json` location.                                                                                                |
| 1a6de64cf3 | A     | 779097924c   | Clean cherry-pick. Vim `:s` substitute patterns now recognize `\+` and `\{n\}` repetition operators.                                                                                                                                                                                         |
| e21ff1b368 | B     | 2d77a40633   | Moves `parse_path_with_position` into `util` so a running Windows instance parses `path:line:column` arguments. Ported with ZZZ's `zzz://` URL prefix; Windows-target compilation NOT RUN because the Linux host lacks `windows.h`.                                                          |

Totals: two `A` (both clean cherry-picks), eleven `B`, sixteen `C` across
29 commits. The decisions table above covers the first 28; the addendum
covers `c7bea34383`.

## Applied Work

- `64e57160d9` is a `B` port of `504a7f2cf2`.
- `d6024a1ff6` is a `B` port of `0294cfb58e`.
- `b97eccd46d` is a `B` port of `8a47b35109`.
- `064141d8b3` is a clean `A` cherry-pick of `57dfd54ade`.
- `fa872535f3` is a `B` port of `ce6c5413f1`.
- `3aa9b3f939` is a `B` port of `ad54befbc6`.
- `0a57dd7fcd` is a `B` port of `9fa6fd3f44`.
- `2485441938` is a `B` port of `784374c679`.
- `650a323090` is a `B` port of `b278e79c97`.
- `0be3235160` is a `B` port of `d71ff73440`.
- `779097924c` is a clean `A` cherry-pick of `1a6de64cf3`.
- `2d77a40633` is a `B` port of `e21ff1b368`.
- `3c3e15e417` is a `B` port of `c7bea34383` (addendum).

Every clean `A` used `git cherry-pick -x -s` and keeps the upstream
patch and message intact. Every `B` was committed with `git commit -s`
and `Upstream:` / `Retained:` / `Omitted:` trailers. No remote, pull
request, or named upstream remote was created.

## Per-Commit Narrative

#### 504a7f2cf2 — wheel zoom in terminals and diffs (B)

Upstream extracts `adjust_buffer_font_size_for_scroll` into
`theme_settings` and calls it from the editor, split-diff, and terminal
scroll handlers. ZZZ keeps the editor element in `editor/src/element.rs`
rather than `editor/src/element/mouse.rs`, uses `ScrollWheelEvent` rather
than `ScrollEvent`, and has no `adjust_buffer_font_size_for_scroll`.
The port adds the shared helper, replaces the editor's inlined delta
math, stops disabling zoom on the split-diff right-hand editor, and lets
the left-hand editor inherit the right-hand editor's
`enable_mouse_wheel_zoom`. Terminal zoom is gated on
`TerminalMode::Standalone` and `EditorSettings::mouse_wheel_zoom`. Both
upstream regression tests were ported with ZZZ's event and
`Modifiers::secondary_key()` shapes and pass.

#### 37568ad7dc — embedded submenu views (C)

The upstream commit adds `ContextMenuItem::EmbeddedView(AnyView)`, a
`content_focus_handle` on `ContextMenu`, and `focus_content(handle)` so a
submenu can host a focusable view. Its stated caller is the git panel's
base branch picker, part of the upstream branch-mode UI. ZZZ's
`ContextMenuItem` has no embedded-view variant, the context menu has no
`focus_content` target, and the local git panel uses a `PopoverMenu`
with `branch_picker::select_popover` rather than a nested submenu. The
new API would have no caller, so per the isolation test it is C.

#### 504abf602a — edit prediction button toggle (C)

The setting touches `assets/settings/default.json`,
`language_settings.rs`, `settings_content/src/language.rs`,
`settings_ui/src/page_data.rs`, and
`edit_prediction_ui/src/edit_prediction_button.rs`. ZZZ removed the
`edit_prediction_ui` crate and its status button, has no
`EditPredictionSettings::button` field, and no status-bar row for edit
predictions. There is no product surface to gate and no local call site
for the setting, so the whole commit is C.

#### 8e8f54c67e — all-settings rewrite (C)

This is a documentation-only regeneration of upstream's
`docs/src/reference/all-settings.md` (+1104 lines) around the Zed
Settings Editor, including Agent Panel font settings and other options
ZZZ does not ship. ZZZ maintains its own 5889-line catalog with a
different structure; importing the upstream text would document absent
settings. Doc-only churn with no ZZZ behavior change, so C.

#### 0294cfb58e — native Windows tab advances (B)

DirectWrite reports tabs as zero-glyph drawing runs. GPUI used to skip
those runs, losing tab spacing. The fix queries each tab's native
hit-test width and emits an invisible space glyph so the caret and byte
indices still map. ZZZ had already diverged: `ShapedRun` carries
`synthetic_italic` and `synthetic_bold`, and the renderer computes them
from `font_run_end_utf16`. The cherry-pick conflicted in the
`DrawGlyphRun` body, so the port keeps ZZZ's synthetic-style computation
and adds the tab branch with those fields on both `ShapedRun` pushes.
`cargo check --locked -p gpui_windows --target x86_64-pc-windows-msvc`
passes; Windows runtime behavior is NOT RUN on this Linux host.

#### ccb25cbc63, 4ebf3eac14, 8a41a5cc04 — crates.io client (C)

These three commits build `tooling/crates_io_client` from scratch: a real
and simulated client, list/get/update crate endpoints, types, tests, and
GitHub Trusted Publishing configuration. ZZZ has no `tooling/crates_io_client`
member, no workspace dependency on it, and no extension-publishing caller
that would use it. The chain is upstream-only release infrastructure
with no independent ZZZ behavior, so all three are C.

#### 601a7cc8ac — subagent working directories (C)

The commit extends `SubagentAssociation` with `cwd` and
`additional_directories` and threads parent work dirs through
`AcpThread::inherit_subagent_work_dirs`. Its files
`crates/acp_thread/src/subagents.rs`,
`crates/agent_servers/src/acp/v1.rs`, and
`crates/agent_servers/src/acp/v2.rs` do not exist locally; the dry-run
cherry-pick reported them deleted by us or unmerged. ZZZ has not
absorbed the ACP v2 compatibility layer this builds on, so the isolation
test fails on absent architecture. C.

#### d36a706614 — ACP v2 background work (C)

Implements the ACP v2 background-work RFD: paused tool calls, a
background-work list, cancel buttons, and terminal cancel. It depends on
`agent_servers/src/acp/v2.rs` and `acp_thread` state-update machinery
that are absent locally, and its UI hunks target the agent panel and
subagent cards ZZZ does not carry. Unisolatable from the rejected ACP v2
surface, so C.

#### 8a47b35109 — grammar parse errors (B)

`HighlightSource::parse` returned a `Tree` and used `expect`/`unwrap`;
a grammar that rejects the text panicked the editor. Upstream makes it
return `Result<Tree>` and logs a warning with empty highlights on error.
ZZZ has no `HighlightSource` trait or `ResolvedHighlights`; the local
`Language::highlight_text` calls `parse_text` directly. The port makes
`parse_text` return `Result<Tree>`, logs the error with the language
name and text length, and returns an empty highlight vector. Two
pre-existing `syntax_map` tests fail on baseline tree-sitter output both
before and after this commit; they are unrelated.

#### b61f33f8b6 — persistent submenu refresh (C)

The commit replaces `ContextMenuItem::Submenu { .. }` with a
`ContextMenuSubmenu` struct carrying stable IDs, rewrites `rebuild`,
and adds `context_menu/preview.rs` (1264 lines across two files). ZZZ's
`ContextMenuItem` still uses the plain `Submenu` variant and the menu
has no persistent-submenu builder. The rewrite is unisolatable from
that divergence, imports a new public type with no current ZZZ caller,
and is a prerequisite for `37568ad7dc` and `056a44d118`, both already
C. C.

#### ce6c5413f1 — popover side flipping (B)

GPUI gains `Anchored::attach(target, corner)`, which places an element
next to a target and flips it when it does not fit; popover and dropdown
menus adopt it so wide menus stop covering their trigger. Dropdowns
default to `Anchor::BottomLeft`. The `context_menu` hunks build on the
rejected `ContextMenuSubmenu` type and were omitted, and the
edit-prediction settings page does not exist locally. The GPUI, popover,
and dropdown changes compile and their new tests pass after adapting
`window.draw(cx).clear(cx)` to `.clear()` and using
`VisualTestContext::debug_bounds` in the anchored test.

#### b7a6e5cdc0 — call hierarchy details (C)

Adds spacing between the call hierarchy name and detail, fixes tooltip
width measurement, and appends the separate detail in
`CallSignatureTooltip`. All of it lives in `crates/call_hierarchy`, which
ZZZ deleted; the cherry-pick reported the main file deleted by us and
`crates/edit_prediction_context/src/fake_definition_lsp.rs` absent. No
local caller remains, so C.

#### 7e7311ad20 and ba36af2050 — responsiveness and its revert (C)

`7e7311ad20` loads up to 16 definition/reference results and completion
details concurrently and reads both sides of a file comparison
together. `ba36af2050` reverts it in full before the release so it can
bake. The net upstream state at the reviewed head is identical to the
parent of `7e7311ad20`; the diff between that parent and the revert head
is only the unrelated organization-permission work (`dd34370114`). ZZZ
absorbed neither side, so both commits are C with no behavior to retain.

#### dd34370114 — organization management (C)

Adds `UserStore::OrganizationChanged`, a management permission, and
follow-up after account reload so AI features stop using an organization
the user was removed from. ZZZ has no accounts, organizations, plans, or
`UserStore::organizations`; this is exactly the account surface the fork
keeps absent. C on philosophy grounds.

#### ad54befbc6 — dock panel activation (B)

Handling `PanelEvent::Activate` opened the dock before activating the
emitting panel, so `set_open` activated whichever panel was previously
active and stole focus. The fix swaps the two calls. The cherry-pick
conflicted only because the new regression test landed adjacent to a
local `#[gpui::test]` attribute; the resolved file keeps both. The test
`test_dock_panel_activation` passes.

**Follow-up (2026-11-13).** That conflict resolution also imported the
two tests adjacent to the new one — `test_zoomed_panel_stays_open_when_focus_is_lost`
and `test_focus_restores_to_panel_when_focused_child_is_removed`. They
are context in `ad54befbc6` because earlier upstream commits had already
added them, and the code those commits came with was never absorbed, so
both failed. The gap was `Workspace::on_focus_lost` restoring focus to
`Workspace::focus_handle`, which is the active pane: any focus loss
therefore moved focus to the center pane, and since a pane focus event
calls `dismiss_zoomed_items_to_reveal(None)`, the zoomed dock closed.

The missing pieces are ported from upstream:

- `InteractionOwner::focus_lost_path`, recorded around focus-lost
  listener dispatch in the draw focus phase.
- `Window::focus_lost_restore_target`, returning the nearest focusable
  ancestor of the element that lost focus.
- `Workspace::fallback_focus_handle`, preferring a zoomed dock panel's
  activation handle over the center pane.
- `Workspace::on_focus_lost` now prefers the restore target and falls
  back to `fallback_focus_handle`; `MultiWorkspace::focus_active_workspace`
  reuses the same helper instead of its own copy of the loop.

`crates/gpui/api-snapshot.txt` gained one entry for the new public
window method.

| Check                                    | Result | Notes    |
| ---------------------------------------- | ------ | -------- |
| `cargo test --workspace --no-fail-fast`  | PASS   | 0 failed |
| `cargo test --locked -p workspace --lib` | PASS   | 254      |
| `cargo test --locked -p gpui --lib`      | PASS   | 299      |
| `cargo xtask check`                      | PASS   | 21/21    |

#### ead5672e16 — compaction token billing (C)

Introduces a `CompactionStrategy` enum and `PreserveRequestPrefix` in
`crates/agent/src/thread.rs` and the language-model providers so
compaction is billed as cached input tokens. The native agent thread and
its compaction state were deleted locally with the native agent tools;
`3c4f6d6e90` is its subscription-provider follow-up. No local caller,
so C.

#### 9fa6fd3f44 — worktree branch search (B)

The worktree picker now indexes both the worktree name and the
checked-out branch name, deduplicates rows by worktree, and highlights
the matched branch text. Upstream keeps the picker in
`crates/git_ui_core`; ZZZ consolidates it into
`crates/git_ui/src/worktree_picker.rs`. The port keeps ZZZ's localized
placeholder key, updates the fallback and both locale catalogs to name
branch search, and retains the upstream `docs/src/git.md` line. All nine
`worktree_picker` tests pass, including the new branch-search test.

#### 3c4f6d6e90 — subscription cache prefix (C)

Makes `openai_subscribed` preserve the prompt-cache prefix during
compaction via `tool_choice: none`. ZZZ has no ChatGPT/Zed subscription
provider and no compaction state (see `ead5672e16`); the provider file
is absent. C on philosophy and absent-architecture grounds.

#### 784374c679 — Windows SSH PTY mode (B)

For Windows SSH servers whose default shell is Nushell, upstream
distinguishes non-interactive shell execution (`-T`) from interactive
terminal launches (`-t`) and wraps the PowerShell command in quotes
only for PTY mode. ZZZ splits `RemoteClient::build_command` and
`build_command_with_options`, so `Project::exec_in_shell` now calls the
latter with `Interactive::No` while terminal launch keeps the default
`Interactive::Yes`. The mock transport and both new tests are retained;
the Windows command test was adapted to ZZZ's `-q` SSH flag instead of
upstream's `-o LogLevel=ERROR`. Linux-side tests pass; Windows runtime
is NOT RUN.

#### b278e79c97 — v1.29.0 version follow (B)

Bumps `crates/zed` and its lockfile entry to 1.29.0. Following the
version-follow policy recorded in the 2026-09-22 audit, ZZZ adopts the
app version number in `crates/zzz/Cargo.toml` and `Cargo.lock` while
omitting upstream's `crates/zed` package identity and release tooling.
`cargo check --locked -p zzz` passes. This follows the same pattern as
the 1.28.0 adoption in `819743b0ad`.

#### 056a44d118 — custom submenu triggers (C)

Adds `ContextMenu::custom_submenu_trigger` and extends
`context_menu/preview.rs`. Both build on the `ContextMenuSubmenu` type
and persistent-refresh machinery from `b61f33f8b6`, which is C, and
there is no local caller. C.

#### d71ff73440 — single-file task variables (B)

`TaskContexts::resolve_task` scopes worktree tasks to their own
worktree and falls back through item, worktree, and default contexts;
active worktrees must have a directory root; external items keep file
variables but use the visible project's root, cwd, and environment. The
port resolves a conflict in `Inventory::list_tasks` onto ZZZ's existing
`TaskContexts::resolve_task`, and adapts the new tests to ZZZ's `ZZZ_`
task-variable prefix and `.ZZZ/tasks.json` config location. All 16
`tasks_ui` tests and the `task_inventory` tests pass.

#### e21ff1b368 — Windows path:line arguments (B)

Moves `parse_path_with_position` from the CLI into `util` so the
single-instance Windows handler can parse `path:line:column` arguments
with the same canonicalization instead of dropping them when
`fs::canonicalize` fails. The port keeps ZZZ's `zzz://` URL prefix and
its local `--diff` helper comment, and fixes the doc comment to say
ZZZ. `cargo check --locked -p cli -p util` and the CLI tests pass.
`cargo check --locked -p zzz --target x86_64-pc-windows-msvc` is NOT
RUN: the `stacker` build script fails on this Linux host because
`windows.h` is unavailable.

## Verification

| Check                                                                                     | Result  | Notes                                                |
| ----------------------------------------------------------------------------------------- | ------- | ---------------------------------------------------- |
| `cargo check --locked -p theme_settings -p editor -p terminal_view`                       | PASS    | after `504a7f2cf2`                                   |
| `cargo test --locked -p editor test_lhs_mouse_wheel_zoom_respects_rhs_configuration`      | PASS    | 1 test                                               |
| `cargo test --locked -p terminal_view test_mouse_wheel_zoom`                              | PASS    | 1 test                                               |
| `cargo check --locked -p gpui_windows --target x86_64-pc-windows-msvc`                    | PASS    | after `0294cfb58e`                                   |
| `cargo check --locked -p language`                                                        | PASS    | after `8a47b35109`                                   |
| `cargo test --locked -p vim test_visual_block_end_of_document_wrapped_eof`                | PASS    | 1 test                                               |
| `cargo test --locked -p gpui test_anchored_attach_moves_to_the_side_with_room`            | PASS    | 1 test                                               |
| `cargo test --locked -p ui dropdown_lines_up_with_its_attached_edge`                      | PASS    | 1 test                                               |
| `cargo test --locked -p ui popover_opens_below_its_trigger_or_above_when_it_does_not_fit` | PASS    | 1 test                                               |
| `cargo test --locked -p workspace test_dock_panel_activation`                             | PASS    | 1 test                                               |
| `cargo test --locked -p git_ui worktree_picker`                                           | PASS    | 9 tests                                              |
| `cargo test --locked -p project --lib remote_exec_in_shell_uses_non_interactive_command`  | PASS    | 1 test                                               |
| `cargo test --locked -p project --lib remote_terminal_launch_uses_interactive_command`    | PASS    | 1 test                                               |
| `cargo test --locked -p remote test_build_command_windows`                                | PASS    | 1 test                                               |
| `cargo check --locked -p zzz`                                                             | PASS    | after the v1.29.0 version bump                       |
| `cargo test --locked -p tasks_ui --lib`                                                   | PASS    | 16 tests                                             |
| `cargo test --locked -p vim test_replace_repetition_operators`                            | PASS    | 1 test                                               |
| `cargo check --locked -p cli -p util` and `cargo test --locked -p cli`                    | PASS    | 13 CLI tests                                         |
| `cargo fmt --all -- --check`                                                              | PASS    | clean                                                |
| `git diff --check` on every applied commit                                                | PASS    | clean                                                |
| `cargo check --locked -p zzz --target x86_64-pc-windows-msvc`                             | BLOCKED | `stacker` build script needs `windows.h`; Linux host |
| Windows and macOS runtime checks                                                          | NOT RUN | host is Linux; those backends are platform-only      |
| `cargo test --workspace`                                                                  | NOT RUN | narrowest per-crate checks were used                 |
| Remaining commits through the first queried head                                          | NONE    | `e21ff1b368b` was the live head at first query       |

## Addendum: c7bea34383

A second `git ls-remote` later on `2026-11-04` returned
`c7bea343838f42b4c14151393e41f0bbe033c95e`, one commit past
`e21ff1b368b`. The reviewed baseline advances to this new live head.

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                           |
| ---------- | ----- | ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| c7bea34383 | B     | 3c3e15e417   | Fixes the cursor briefly reverting when a drag starts, before the next paint. Ported onto ZZZ's `interaction.rs` hit-test and `InteractionOwner::dispatch_mouse_listeners` shapes; upstream regression test retained. |

#### c7bea34383 — drag cursor before the next paint (B)

`reset_cursor_style` used to read only the rendered frame's cursor
style, so a drag started by a mouse listener in the current event
briefly showed whatever was under the pointer until the next paint
recorded the drag's cursor. The fix prefers
`cx.active_drag_cursor_style()` and moves the cursor resolution to
after `dispatch_mouse_listeners`, so a drag started in the same event
takes effect immediately. The inspector-picking path also refreshes the
cursor before returning.

ZZZ keeps the mouse hit-test update in `interaction.rs` and the
listener dispatch in `InteractionOwner::dispatch_mouse_listeners`, so a
clean cherry-pick conflicted. The port keeps those shapes: it removes
the early `reset_cursor_style` call gated on `update_mouse_hit_test`,
calls `update_mouse_hit_test` unconditionally, and adds a single
`reset_cursor_style` after the listeners. `TestPlatform::cursor_style()`
and `TestAppContext::test_platform()` are added under `#[cfg(test)]`
because ZZZ had no accessor for the former and the upstream test uses
both.

| Check                                                                    | Result | Notes     |
| ------------------------------------------------------------------------ | ------ | --------- |
| `cargo test --locked -p gpui test_drag_cursor_applies_before_next_paint` | PASS   | 1 test    |
| `cargo test --locked -p gpui --lib`                                      | PASS   | 291 tests |
| `cargo fmt --all -- --check`                                             | PASS   | clean     |

The reviewed baseline advances to
`c7bea343838f42b4c14151393e41f0bbe033c95e`, which is the live upstream
head at the second query time.
