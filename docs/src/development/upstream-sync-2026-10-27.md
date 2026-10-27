---
title: Upstream Sync 2026-10-27
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-10-27

## Scope

- Local base: `main` at `4d3b091b22`
- Target branch: `sync/upstream-2026-10-27`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `79fe6a645f68f8c9230578f154bf99852d9f716f`
- Reviewed head: `eee7501970fabf711e1bec286ba4cf2a05586b3f`
- Live upstream head queried: `24a2823456f16e36bf32df510fb5c43fee7a2565`
- Query time: `2026-10-27T15:33:43+01:00`
- Reviewed range: the first 20 commits after `79fe6a645f`, oldest first
- Remaining after this batch: 30 commits through the queried live head

The previous reviewed baseline is an ancestor of `FETCH_HEAD`. The range
through the live head contains 50 commits; this batch covers the first 20
as the configured batch size. The second batch then covers the next 20
commits; see [Batch 2](#batch-2) below.

Audit correction (2026-10-27): the full re-audit in
[`upstream-sync-audit-2026-10-27.md`](./upstream-sync-audit-2026-10-27.md)
found two defects in the `fdcb198110` port (`db34c508be`). Both are fixed
in `6159bc81e0`, which adds a macOS 12 guard for
`isLowPowerModeEnabled` and defers a forced render across throttling.
`fdcb198110` stays `B` with the corrected local commit; `f14592c4a5`
stays `C`, so the deployment floor remains 10.15.7.

A third correction covers the `bd9aa0ac` port: `53158b01b7` split
displayed paths by the remote style but still resolved the repository
identity with the host `std::path` methods, so a Windows remote `.git`
directory was not recognized as a dot entry. `226e2a45d8` resolves it
with the remote style while keeping local paths on `std::path`. The row
stays `B`, so the totals in each batch are unchanged because no class
moved.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                         |
| ---------- | ----- | ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 02a04e06e2 | C     | --           | Clears replayed ACP v2 subagent state. The `agent_servers` v1/v2 split and the `acp_v2` module are absent locally, and `AcpThread` has no subagent registry.                                        |
| b0cac499da | C     | --           | Orders ACP v1 load responses and replays subagent history. The whole patch is in the absent `agent_servers/src/acp/v1.rs`.                                                                          |
| 536ec0ea9d | C     | --           | Refuses config changes and reopening for ACP v2 subagents. Adds `AcpSession::is_subagent` fed by `apply_subagent_update`, which is absent locally.                                                  |
| c07b87624c | C     | --           | Applies the subagents RFD to cancellation. ZZZ's `AcpThread` has a diverged request lifecycle with no subagent association or `can_cancel` grant.                                                   |
| a8acb01f21 | C     | --           | Read-only ACP subagent configuration spans `config_options.rs` and the v2 connection; both build on the absent subagent registry.                                                                   |
| d93bf74085 | C     | --           | Renders ACP subagents as inline cards through a new `subagent_card.rs` and v2 session updates; the local conversation view loads subagents as separate threads, so the card layout is unisolatable. |
| af4948a164 | C     | --           | Follow-up to the read-only subagent configuration in `agent_servers/src/acp/v2.rs`, which does not exist locally.                                                                                   |
| 347f171a3f | B     | e8602c82d9   | Parses file positions from the original string so separators survive. The `thread_view` test hunks were omitted; that test does not exist locally.                                                  |
| 18f9d7e731 | B     | 845628ab79   | Redirects the pwsh environment probe through `/dev/fd/0`. The ssh half targets an absent `RemoteEnvironment` probe.                                                                                 |
| 035e6ac2f3 | C     | --           | Buffers ACP v2 subagent traffic sent before `session/new` resolves in `acp/v2.rs` and `session_updates.rs`; both are absent locally.                                                                |
| ad8a62df82 | C     | --           | Treats replayed subagent outcomes as history on the v1/v2 connection pair and the absent subagent registry.                                                                                         |
| c8995a085a | C     | --           | Skips tool-call-referenced session loading where the subagents RFD applies; it reads the v2 association model that is absent locally.                                                               |
| 78f5a535a3 | C     | --           | Keeps inline review on the root thread when inline subagent cards appear, which is the diverged card layout rejected above.                                                                         |
| c5efa10ddd | C     | --           | Derives a subagent titlebar status from its latest run; `AcpThread::subagents` does not exist locally.                                                                                              |
| a5c4ca8bf6 | C     | --           | Offers ACP v1 subagents behind `acp-beta`. The subagent offering path and beta-flag rollout are absent locally.                                                                                     |
| 876c1b0c5f | B     | a0f9127da3   | Removes project group rows that no retained workspace backs, so they stop reappearing in Recent Projects. The regression test was adapted to local helpers.                                         |
| 08cbb88a15 | B     | 76ff13b989   | Sizes the git graph canvas and gutter through layout instead of the prepaint-cached width. Ported into `crates/git_graph`, where ZZZ keeps the graph.                                               |
| 08104dc649 | B     | ce08e0391b   | Adds commit detail tooltips in the same moved file. The commit message disclosure tooltip was omitted; ZZZ has no collapsible message section there.                                                |
| e645f02245 | C     | --           | Skips the GPG askpass wrapper when GPG is missing. ZZZ's `askpass` has no GPG wrapper or `find_gpg_program`, so there is nothing to make quiet.                                                     |
| eee7501970 | C     | --           | Removes Agent checkpoints across `acp_thread`, `agent`, `git`, and `project`. ZZZ keeps them for thread restore and worktree archives; the removal is unisolatable on the diverged thread model.    |

Totals: zero `A`, five `B`, fifteen `C` across twenty commits.

## Applied Work

- `e8602c82d9` is a `B` port of `347f171a3f`.
- `845628ab79` is a `B` port of `18f9d7e731`.
- `a0f9127da3` is a `B` port of `876c1b0c5f`.
- `76ff13b989` is a `B` port of `08cbb88a15`.
- `ce08e0391b` is a `B` port of `08104dc649`.

No commit in this batch was a clean `A`; every candidate needed either a
moved file, a local API shape, or an i18n pass before it fit.

## Per-Commit Narrative

#### 347f171a3f — path separators in file positions (B)

`PathWithPosition::parse_str` now rebuilds the parsed path with
`replace_file_name`, which splices the replacement onto the original input
string, instead of calling `Path::with_file_name`, which inserts the host
separator. This keeps `/home/...` and `\\server\...` paths intact when the
editor runs on Windows. The cherry-pick conflicted twice: the local regex
initializer uses `expect` rather than `unwrap`, and the local
`conversation_view/thread_view.rs` has no markdown-link position tests. The
`util/src/paths.rs` behavior and its separator regression test were ported;
the thread-view test updates were dropped as test-only.

#### 18f9d7e731 — pwsh env detection (B)

`capture_unix` now calls a shared `capture_output_redirection` helper, and
the helper redirects both `PowerShell` and `Pwsh` through `> /dev/fd/0`.
The previous `>{}` form produced `>0` for pwsh, which is not a descriptor
redirect, so pwsh startup output leaked into the captured environment. The
pwsh regression test was retained. The `remote/src/transport/ssh.rs` half
was omitted: ZZZ has no `RemoteEnvironment`, `posix_script`, or
stdin-fed remote probe, so `command_output` and
`environment_posix_command` would have had no caller.

#### 876c1b0c5f — unloaded project group rows (B)

`MultiWorkspace::remove_project_group` computed the group's workspaces and
then delegated to `remove`, which returns `Ok(false)` for an empty workspace
list. A group whose workspaces were opened in another window therefore
stayed in `project_groups` and reappeared in Recent Projects. The port adds
the upstream early return: drop the group record, emit
`ProjectGroupsChanged`, serialize, and notify. The regression test was
adapted to ZZZ's `MultiWorkspace::test_new` plus `open_sidebar` setup,
which is what seeds the initial project group locally.

#### 08cbb88a15 — git graph resizing (B)

The graph canvas and ref-label gutter were sized from
`RedistributableColumnsState::cached_container_width`, which is only updated
during prepaint, so closing the commit details panel kept the previous
frame's width until an unrelated re-render. The port deletes
`graph_viewport_width`, drops the gutter's `.min(viewport / 2.)` in favor of
`max_w(relative(0.5))`, and lets the canvas fill the column with
`.w_full().min_w(graph_canvas_content_width())`. Upstream keeps this file in
`crates/git_ui`; ZZZ keeps it in `crates/git_graph`, so the patch was
replayed by hand. The regression test is included.

#### 08104dc649 — git graph tooltips (B)

Two of the three tooltips apply locally: the commit details close button,
using `Tooltip::for_action_in` with the graph's focus handle so the binding
matches what works in the graph, and the "View on" button. Both labels are
localized through `i18n::tr` with new `git_graph.close_details_panel` and
`git_graph.open_in_browser` keys in `en.json` and `zh-CN.json`. The commit
message disclosure tooltip was omitted because ZZZ's commit details panel
has no `toggle-commit-message` disclosure.

#### eee7501970 — remove checkpoints (C)

The commit deletes the Agent checkpoint feature: `git` loses
`checkpoint.gitignore` and most of its repository checkpoint API, `project`
drops `checkpoints_by_work_dir_abs_path`, and `acp_thread` loses over a
thousand lines. ZZZ uses checkpoints for thread restore
(`ThreadView::restore_checkpoint`) and for worktree archives
(`create_archive_checkpoint` / `restore_archive_checkpoint`), so absorbing
this removal would delete a feature ZZZ ships, and the `acp_thread` half
would be unisolatable on top of the diverged v2 thread model rejected above.

#### e645f02245 — GPG askpass wrapper (C)

`generate_gpg_wrapper_script` returns `Ok(None)` when no `gpg` binary is on
`PATH`, so an absent GPG stops producing a warning. ZZZ's `askpass` has no
GPG wrapper at all: there is no `find_gpg_program`, no
`GPG_WRAPPER_SCRIPT_NAME`, and no wrapper script generation, so the quiet
path has no local counterpart to attach to.

## Verification

| Check                                                                       | Result  | Notes                                               |
| --------------------------------------------------------------------------- | ------- | --------------------------------------------------- |
| Live `git ls-remote` and `git fetch --no-tags`                              | PASS    | `FETCH_HEAD=24a2823456`                             |
| Previous baseline ancestry                                                  | PASS    | `79fe6a645f` is an ancestor of `FETCH_HEAD`         |
| `cargo test --locked -p util --lib path_with_position`                      | PASS    | 6 tests                                             |
| `cargo test --locked -p util --lib shell_env`                               | PASS    | 2 tests                                             |
| `cargo test --locked -p workspace --lib test_remove_unloaded_project_group` | PASS    | 1 test                                              |
| `cargo test --locked -p git_graph --lib test_graph_canvas_resizes`          | PASS    | 1 test                                              |
| `cargo check --locked -p git_graph --all-targets`                           | PASS    | tooltips and locale keys                            |
| `cargo check --locked --workspace --all-targets`                            | PASS    | full workspace after all five ports                 |
| `cargo fmt --check` (touched crates)                                        | PASS    | clean                                               |
| `git diff --check`                                                          | PASS    | clean                                               |
| `./script/check-philosophy`                                                 | PASS    | philosophy gate                                     |
| Locale key parity (`en.json` vs `zh-CN.json`)                               | PASS    | 2 new keys in both catalogs                         |
| macOS / Windows runtime checks                                              | NOT RUN | host is Linux                                       |
| `cargo test --workspace`                                                    | NOT RUN | not requested                                       |
| Remaining commits through live head                                         | PENDING | 30 commits; next baseline starts after `eee7501970` |

The reviewed baseline advances to
`eee7501970fabf711e1bec286ba4cf2a05586b3f` for this batch.

## Batch 2

### Scope

- Reviewed baseline: `eee7501970fabf711e1bec286ba4cf2a05586b3f`
- Reviewed head: `57c7b10ece671185a8863515667ee5248c6f0d53`
- Live upstream head queried: `24a2823456f16e36bf32df510fb5c43fee7a2565`
- Query time: `2026-10-27T16:16:03+01:00`
- Reviewed range: the 20 commits after `eee7501970`, oldest first
- Remaining after this batch: 10 commits through the queried live head

### Decisions

| Upstream   | Class | Local commit           | Disposition                                                                                                                                                                                                  |
| ---------- | ----- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 772a8a9100 | B     | 99f7b8d817             | Holds `WindowTextSystem` in an `Rc` and keeps per-window layout caches in a `RefCell`. The font-generation invalidation this patch interleaves with does not exist locally.                                  |
| 2a74c8dff6 | B     | e17c46238f             | Deletes the unused hash-keyed layout cache and its key types. Replayed by hand, and no caller outside `text_system` existed.                                                                                 |
| 1808df02c0 | C     | --                     | Removes `block` by bumping vendored `scap` and rewriting the upstream `gpui_apple` Metal renderer. The upstream `gpui_apple` renderer is absent locally and ZZZ vendors an older `scap`, so nothing applies. |
| f14592c4a5 | C     | --                     | Raises the macOS floor to 12 and replaces a deprecated icon API. The local window has no `iconForFileType` path, and the floor bump is release metadata.                                                     |
| 96b617fa37 | B     | d0217b7d0d             | Drops navigation history and persisted recent paths for deleted entries. The `zed` test hunk was omitted; one test drives the path change directly.                                                          |
| 087340e2d7 | C     | --                     | Only moves the `crash-context` pin to a minidump-writer git SHA in the manifest and lockfile. Dependency churn with no local code path.                                                                      |
| 5e548b8ad9 | B     | 254659a2c9             | Focuses the zoomed panel from its status bar button. The click-through test needs a panel type ZZZ does not carry; a focused test asserts the handle instead.                                                |
| e0f4f28001 | B     | 1f8cbc13e5             | Anchors submenus to their trigger row and stacks nested menus by depth. The accessibility refactor and its focus tests were omitted because ZZZ's context menu keeps cfg-gated roles.                        |
| 3318784374 | B     | 51ccaab565             | Reads `focus_follows_mouse` per dock render instead of caching it at construction. The struct hunk conflicted only over a renamed field.                                                                     |
| 23973bd49f | B     | 33f0f923e4             | Treats ACP tool call location lines as 1-based. The native tool hunks were omitted because ZZZ does not ship the Edit, Grep and Read tools.                                                                  |
| 979cd3fdf0 | B     | 5559e965ef             | Shapes Windows text in segments so U+2028 and U+2029 stay on one line. `FontRun` also carries style and weight locally, so segment runs copy them.                                                           |
| 4d7b2d05ea | A     | aabce0cc14             | Clean cherry-pick: refreshes diagnostic endpoints on every seek.                                                                                                                                             |
| 60919b0248 | B     | 58a2286021             | Leaves focus alone while a single-line editor is focused. Superseded later in the batch by the retained-focus registry.                                                                                      |
| 182d15a629 | C     | --                     | Adds a `SquareR` icon with no local caller. ZZZ has no such usage, so the asset would be dead code.                                                                                                          |
| bd9aa0ac05 | B     | 53158b01b7, 226e2a45d8 | Compares git worktree paths with the remote path style. Local `PathStyle` has no `starts_with`, so containment uses `strip_prefix`; `226e2a45d8` also resolves the repository identity by that style.        |
| bed9563afc | B     | 97190b2bf2             | Stages Metal shaders in `OUT_DIR` and runs the tools there. ZZZ's macOS-only script keeps its older deployment target.                                                                                       |
| 484bcbfea6 | C     | --                     | Gates the Linux backend on `not(target_env = "ohos")`. ZZZ has no local ohos target or backend, so the change would only add dead configuration.                                                             |
| a72cb6a214 | B     | 977cee96c2             | Keeps focus on menus and pickers through a retained-focus registry; supersedes the key-context check. The project-panel test covers both cases.                                                              |
| 2c335b81dc | B     | 7881a03564             | Pins the zed-industries tree-sitter-go fork and highlights method type parameters. The local highlights query is shorter, so the rule was added beside `type_spec`.                                          |
| 57c7b10ece | B     | 222c88091c             | Reads the git graph repository state through `Deref` instead of cloning a snapshot per ref label. Ported into `git_graph`.                                                                                   |

Totals: one `A`, fourteen `B`, five `C` across twenty commits.

### Applied Work

- `aabce0cc14` is a direct `A` cherry-pick of `4d7b2d05ea` with
  `git cherry-pick -x -s`.
- `99f7b8d817` is a `B` port of `772a8a9100`.
- `e17c46238f` is a `B` port of `2a74c8dff6`.
- `d0217b7d0d` is a `B` port of `96b617fa37`.
- `254659a2c9` is a `B` port of `5e548b8ad9`.
- `1f8cbc13e5` is a `B` port of `e0f4f28001`.
- `51ccaab565` is a `B` port of `3318784374`.
- `33f0f923e4` is a `B` port of `23973bd49f`.
- `5559e965ef` is a `B` port of `979cd3fdf0`.
- `58a2286021` is a `B` port of `60919b0248`.
- `53158b01b7` is a `B` port of `bd9aa0ac05`; `226e2a45d8` corrects it
  after the audit.
- `97190b2bf2` is a `B` port of `bed9563afc`.
- `977cee96c2` is a `B` port of `a72cb6a214`.
- `7881a03564` is a `B` port of `2c335b81dc`.
- `222c88091c` is a `B` port of `57c7b10ece`.

### Per-Commit Narrative

#### 772a8a9100 — window text layout synchronization (B)

`WindowTextSystem` no longer needs to be `Send`: `Window` and
`TextLayoutDetails` hold it in an `Rc`, the per-window font-run scratch
buffer moves from a shared `Mutex` pool to a `RefCell`, and
`LineLayoutCache` keeps both frame caches in one
`RefCell<LineLayoutCacheState>` instead of a `Mutex` plus an `RwLock`.
Upstream's patch interleaves a font-generation invalidation path; ZZZ's
cache has no `font_generation` field, so the port keeps the existing
frame-swap semantics and drops `clear_if_font_generation_changed` and its
call sites. `cargo test -p gpui --lib text_system` passes 33 tests.

#### 2a74c8dff6 — unused hash-keyed text layout cache (B)

Deletes `shape_line_by_hash`, `try_layout_line_by_hash`,
`layout_line_by_hash`, the `lines_by_hash` and `wrapped_lines_by_hash`
frame caches, the hash index fields, the reuse, truncate and clear
bookkeeping, and the `HashedCacheKey`/`HashedCacheKeyRef` key types. No
caller outside `crates/gpui/src/text_system*` used them. The patch was
replayed by hand because the files had drifted around the previous
`RefCell` port; the two files shrink by 468 lines.

#### 1808df02c0 — remove the `block` dependency (C)

Bumps the vendored `scap` fork to a revision whose macOS features are
optional and rewrites the upstream `gpui_apple` Metal renderer and atlas
benchmarks to drop `metal` and `block`. ZZZ vendors an older `scap`
revision that still pulls `block`, has no `gpui_apple` or `gpui_ios`
crate, and its `tooling/xtask` has no forbidden-dependency test, so
nothing in the patch has a local target.

#### f14592c4a5 — macOS 12 and SDK 26 (C)

Raises `MACOSX_DEPLOYMENT_TARGET` to 12.0 across cargo config, bundle
metadata and build scripts, and replaces `iconForFileType:` with
`iconForContentType:` behind the new floor. ZZZ's
`gpui_macos/src/window.rs` has no drag-image icon path at all, so the API
replacement has nothing to change, and changing the minimum supported
macOS version is release metadata that the sync does not decide.

#### 96b617fa37 — navigation history for deleted entries (B)

`Workspace` listens to `WorktreeUpdatedEntries`, drops navigation history
and persisted recent paths for entries removed from disk, and
`recent_navigation_history_iter` skips entries whose buffer is gone;
`NavHistory` gains `remove_items` and `retain_entries`. The `crates/zed`
expectation update was omitted because ZZZ has no `zed` crate, and one
test calls `active_item_path_changed` directly because the local
`workspace/item.rs` has no `ItemEvent::UpdateBreadcrumbs` arm. Nine tests
pass.

#### 087340e2d7 — cmd-q panic (C)

Only moves the `crash-context` patch to a minidump-writer git SHA in
`Cargo.toml` and the lockfile. That is dependency churn: it cannot be
validated on this host and carries no code path ZZZ owns.

#### 5e548b8ad9 — panel buttons while zoomed (B)

`Dock::panel_button_focus_handle` now returns the zoomed panel's handle,
which is the panel rendered inside the zoom layer, so its action reaches
the workspace handlers instead of being dispatched from the window root;
`PanelButtons` uses it per button. The upstream click-through test needs
a `SecondTestPanel` that ZZZ does not carry, so a focused test asserts
that the handle switches to the zoomed panel instead.

#### e0f4f28001 — steadier submenus (B)

Submenus measure their trigger row into a per-index
`submenu_trigger_bounds` map and are placed with a local-mode `anchored`
element anchored to that row, flipping only when the viewport leaves less
than the menu minimum width on the right; `submenu_depth` drives
`deferred` priority so nested menus stack above their parents, and the
diagonal-move guard tests the padded submenu bounds instead of a mouse
threshold. The upstream `new_inner` accessibility refactor and its
focus-selection tests were omitted because ZZZ keeps its
`#[cfg(feature = "accessibility")]` roles and has no such split. The
geometry regression test is ported and passes.

#### 3318784374 — docks ignoring focus_follows_mouse (B)

`Dock` reads `WorkspaceSettings::focus_follows_mouse` during render
instead of caching it at construction, so toggling the setting reaches
existing docks without a restart. The struct hunk conflicted only because
ZZZ names the upstream `restoration` field `serialized_dock`.

#### 23973bd49f — ACP tool call location lines (B)

Reads ACP location lines as 1-based in `ToolCall::resolve_location` and
in `ThreadView::open_tool_call_location`'s plain-row fallback, so
external agents' locations no longer open one line low; the regression
test now reports line 2 and asserts row 1. The `crates/agent` hunks that
made the native Edit, Grep and Read tools emit 1-based lines were omitted
because ZZZ does not ship those tools.

#### 979cd3fdf0 — Unicode separators on Windows (B)

DirectWrite treats U+2028 and U+2029 as mandatory breaks even without
wrapping, so `layout_line` now splits after each separator, shapes every
segment with `layout_line_without_separators`, and concatenates glyphs
while rebasing x positions and text indices; both separator tests are
ported. ZZZ's `FontRun` also carries `font_style` and `font_weight`, so
the per-segment runs copy those fields. The Windows code path cannot be
compiled on this host.

#### 4d7b2d05ea — diagnostic endpoints on seek (A)

Clean cherry-pick. The endpoint refresh moves out of the `highlights`
branch so it always runs after a seek.

#### 60919b0248 — single-line inputs under focus_follows_mouse (B)

The debounced handoff skips `window.focus` while the key context reports
a single-line `Editor`, so renaming a file, creating a file or renaming a
terminal tab is no longer cancelled when the pointer rests over another
panel. The project-panel regression test was ported against ZZZ's
`ProjectPanel::new(project, workspace, window, cx)` and fails without the
guard. The next commit replaces the key-context heuristic with a focus
registry.

#### 182d15a629 — SquareR icon (C)

Adds an icon asset and its `IconName` variant with no caller. ZZZ has no
`SquareR` usage, so the asset would be dead code.

#### bd9aa0ac05 — title bar git information on Windows remotes (B)

`TitleBar` asks the active worktree for its `PathStyle` and uses it both
to decide whether a worktree belongs to a repository and to build the
`repo/subfolder` name, so a Windows remote opened from a Unix client
keeps showing the repository, worktree and branch. ZZZ's
`util::paths::PathStyle` has no `starts_with`, so the containment checks
use `strip_prefix` and the display path uses
`RelPath::display(path_style)`.

Superseded: the audit in `upstream-sync-audit-2026-10-27.md` found that
`repo_identity_path` still used host `std::path` methods, so a Windows
remote `.git` directory was not treated as a dot entry. `226e2a45d8`
resolves the identity with the remote style and keeps local non-UTF-8
paths on `std::path`. The class stays `B`.

#### bed9563afc — Metal shaders without the checkout path (B)

`gpui_macos` stages `shaders.metal` in `OUT_DIR` and runs `metal` and
`metallib` with `current_dir` set there, so neither the input path nor
the tools' working directory can embed the checkout in the compiled
library. Upstream keeps this in `gpui_apple` and targets macOS 12 and
iOS; ZZZ's macOS-only script keeps its 10.15.7 target. Apple build
scripts cannot run on this host.

#### 484bcbfea6 — gpui on ohos (C)

Gates the Linux platform functions and the `gpui_linux` dependency on
`not(target_env = "ohos")` and makes `current_platform` panic on ohos.
ZZZ has no ohos target, no `gpui-ohos` backend, and never builds for it,
so the change would only add dead configuration and a panic branch.

#### a72cb6a214 — menus and pickers under focus_follows_mouse (B)

`ui` gains a `RetainedFocusRegistry`; context menus, picker heads and
single-line editors register their handle through
`retained_focus_handle`, and the debounced handoff keeps focus while a
registered handle is focused. This supersedes the key-context check from
60919b02 and also protects right-click menus and popup pickers that use
custom key contexts. The project-panel test now covers both the rename
field and the context menu, and fails without the registry.

#### 2c335b81dc — tree-sitter-go fork (B)

Pins `tree-sitter-go` to the zed-industries fork that parses method type
parameters, highlights `type_parameter_declaration` names as types, and
adds a regression test asserting that the `T` in
`func (r Recv) Do[T any]` is a type. ZZZ's `highlights.scm` is a shorter
local variant, so the query was added beside the existing `type_spec`
rule. All 21 Go language tests pass.

#### 57c7b10ece — git graph snapshot clone (B)

The commit details and ref label render paths read `branch` through
`Repository`'s `Deref<Target = RepositorySnapshot>` instead of calling
`snapshot()`, so no repository state is cloned per ref label. Upstream
keeps this in `git_ui`; the port lands in ZZZ's `git_graph`.

### Verification

| Check                                                                                                    | Result  | Notes                                               |
| -------------------------------------------------------------------------------------------------------- | ------- | --------------------------------------------------- |
| Reviewed baseline ancestry                                                                               | PASS    | `eee7501970` is an ancestor of `FETCH_HEAD`         |
| `cargo check --locked -p ui --all-targets`                                                               | PASS    | submenu geometry                                    |
| `cargo test --locked -p ui --lib context_menu`                                                           | PASS    | 5 tests                                             |
| Navigation history tests in `-p workspace`                                                               | PASS    | 9 tests                                             |
| `cargo test --locked -p project_panel --lib test_focus_follows_mouse_keeps_rename_and_context_menu_open` | PASS    | also fails without the registry                     |
| `cargo check --locked -p agent_ui`                                                                       | PASS    | ACP location change                                 |
| `cargo test --locked -p acp_thread --lib test_tool_call_location_resolves_external_file`                 | PASS    | asserts row 1 for line 2                            |
| `cargo test --locked -p languages --lib go::`                                                            | PASS    | 21 tests                                            |
| `cargo check --locked -p gpui_windows`                                                                   | PASS    | type-checks on Linux; Windows runtime not exercised |
| `cargo test --locked -p gpui --lib text_system`                                                          | PASS    | 33 tests                                            |
| `cargo check --locked --workspace --all-targets`                                                         | PASS    | after all fifteen ports                             |
| `cargo fmt --check` (touched crates)                                                                     | PASS    | clean                                               |
| `git diff --check`                                                                                       | PASS    | clean                                               |
| `./script/check-philosophy`                                                                              | PASS    | philosophy gate                                     |
| Locale key parity (`en.json` vs `zh-CN.json`)                                                            | PASS    | no new keys in this batch                           |
| macOS / Windows runtime checks                                                                           | NOT RUN | host is Linux                                       |
| `cargo test --workspace`                                                                                 | NOT RUN | not requested                                       |
| Remaining commits through live head                                                                      | PENDING | 10 commits; next baseline starts after `57c7b10ece` |

The reviewed baseline advances to
`57c7b10ece671185a8863515667ee5248c6f0d53` for this batch.

## Batch 3

### Scope

- Reviewed baseline: `57c7b10ece671185a8863515667ee5248c6f0d53`
- Reviewed head: `24a2823456f16e36bf32df510fb5c43fee7a2565`
- Live upstream head queried: `24a2823456f16e36bf32df510fb5c43fee7a2565`
- Query time: `2026-10-27T16:16:03+01:00`
- Reviewed range: the 10 commits after `57c7b10ece`, oldest first
- Remaining after this batch: none through the queried live head

### Decisions

| Upstream   | Class | Local commit           | Disposition                                                                                                                                                                                                      |
| ---------- | ----- | ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 6b985a8b26 | B     | 2c3021cad3             | Resolves each `ms` edge from both sides of its pre-edit offset, and sorts surround edits ascending so touching selections stop overlapping. Five regression tests are included.                                  |
| e30095febc | B     | addbece988             | Adds `-Wl,--max-memory=4294967296` to grammar compilation. The Tree-sitter revision bump and the `captures()` migration have no local target because ZZZ uses crates.io `tree-sitter 0.27.1`.                    |
| ec86047d67 | B     | 9b5362fab4             | Reads stash subjects from `%gs` so an external rename shows up, and fixes the failure message. Local helper is named `git_binary`.                                                                               |
| 370591f3d1 | B     | e36451ae2c             | Defers native window close cleanup to a foreground task, so destruction cannot re-enter a borrowed app. The test platform stores the callback.                                                                   |
| dde76a1dca | C     | --                     | A wide internal gpui refactor (`SingleSubscriberSet`, subscriber state machine, lint fixes) with no observable behavior change.                                                                                  |
| 3c2f782514 | B     | 469aa18b3b             | Gives a rejected push a specific toast title. The operation is kept unlocalized for the predicate; the fetch/pull/push button in the PR text is not part of the patch.                                           |
| f2a60bb8b6 | B     | 5eccbc95c5             | Adds path-style-aware `file_name`, `file_stem` and `extension` so `repo.git` bare clones show `repo` on Windows remotes.                                                                                         |
| fdcb198110 | B     | db34c508be, 6159bc81e0 | Caps active windows to 30 fps in low power mode and tolerates 1 ms of display-link jitter. Profiler and notification plumbing were omitted; `6159bc81e0` adds the macOS 12 guard and the deferred forced render. |
| e4a3732b94 | B     | 75b9a36a93             | Pools `CharWidthCache` instead of whole `LineWrapper`s, breaking the `Arc<TextSystem>` cycle. A regression test asserts the text system is freed.                                                                |
| 24a2823456 | C     | --                     | Only advances the wgpu git revision in the manifest and lockfile. ZZZ vendors wgpu v29, so the revision does not apply.                                                                                          |

Totals: zero `A`, eight `B`, two `C` across ten commits.

### Applied Work

- `2c3021cad3` is a `B` port of `6b985a8b26`.
- `addbece988` is a `B` port of `e30095febc`.
- `9b5362fab4` is a `B` port of `ec86047d67`.
- `e36451ae2c` is a `B` port of `370591f3d1`.
- `469aa18b3b` is a `B` port of `3c2f782514`.
- `5eccbc95c5` is a `B` port of `f2a60bb8b6`.
- `db34c508be` is a `B` port of `fdcb198110`; `6159bc81e0` corrects it after the audit.
- `75b9a36a93` is a `B` port of `e4a3732b94`.

### Per-Commit Narrative

#### 6b985a8b26 — Helix surround on adjacent selections (B)

`ms` now records anchors on both sides of each pre-edit edge plus the
length of the text requested at that offset, and resolves an edge forward
by that length without passing the inserted text. Two touching selections
therefore no longer produce overlapping ranges when the closing delimiter
of one and the opening delimiter of the next land on the same offset. The
edit list is applied in ascending stable order, which keeps same-offset
insertions in push order and is the reverse of the local
`Reverse(edit.0.start)` sort. All fifteen `helix::surround` tests pass.

#### e30095febc — Tree-sitter memory imports (B)

Only the extension-builder hunk applies: the clang invocation that builds
a grammar's WebAssembly now passes
`-Wl,--max-memory=4294967296`, so the resulting module declares an
explicit maximum on its memory import and a browser loader can make it
shared without moving bytes. The rest of the patch moves
`tree-sitter`/`tree-sitter-language` to a git revision whose
`QueryMatch::captures` is a method instead of a field; ZZZ depends on
crates.io `tree-sitter 0.27.1`, so both the call-site migration and the
lockfile churn have no local target.

#### ec86047d67 — external stash renames (B)

`stash_entries` asks git for `%gs` (reflog subject) instead of `%s`
(commit subject), which is what `git stash list` shows and what
`git stash store -m` updates, and the adjacent bail message now names
`git stash list`. ZZZ binds the helper as `git_binary`, so the port keeps
that binding; five stash parsing tests pass.

#### 370591f3d1 — native window close cleanup (B)

The shared `on_close` callback no longer touches the app directly. It
detaches a foreground task that removes the window only if the
app-driven closure has not already done so, then removes its system tab,
so a Windows destruction message arriving while GPUI is borrowed cannot
panic or skip cleanup. The test platform stores the callback and exposes
`simulate_close`, and a regression test delivers a close while the app is
borrowed and drains tasks.

#### dde76a1dca — gpui cleanups (C)

Extracts `SingleSubscriberSet`, folds the activation and cancellation
flags into one `Pending | Active | Dropped` state, changes `insert` to be
immediately active with a new `insert_pending`, and rewrites the callers
across `app`, `context`, `window` and the element tree, alongside import
reordering and lint removals. Nothing about it is observable to a user,
and the subscription machinery has already diverged locally, so the
rewrite is not isolatable.

#### 3c2f782514 — rejected push explanation (B)

`is_non_fast_forward_error` matches `[rejected]` output with
`(non-fast-forward)` or `(fetch first)`, and the `Updates were rejected`
message with `behind` or `remote`. When the failed operation is `push` and
the message matches, the toast title becomes
`Push rejected: fetch and integrate remote changes before pushing again`
instead of `git push failed`. ZZZ localizes operations, so
`show_error_toast` keeps the raw operation for the predicate and uses the
localized name in the fallback title; the new string is in both catalogs.
Seven predicate tests are ported. The fetch/pull/push action button that
the PR description mentions is not in the patch, so no button was added.

#### f2a60bb8b6 — bare clone names on Windows remotes (B)

`PathStyle` gains `file_name`, `file_stem` and `extension`, which honor
the style's separators on any host and skip `.` components like std does,
and the title bar uses them to detect and strip a `.git` suffix. A bare
clone whose common directory is `C:\User\test\test_repo.git` opened from a
Unix client now shows `test_repo` rather than the whole path. The
stem/extension table covers seventeen Posix and eleven Windows cases and
passes.

#### fdcb198110 — low power mode frame pacing (B)

`Platform` reports a `PowerMode` (default `Unknown`; macOS reads
`NSProcessInfo.isLowPowerModeEnabled`, which cannot distinguish Automatic
from High Power Mode), `App::power_mode` exposes it, and
`minimum_frame_interval` keeps the previous inactive-window and thermal
behavior while capping an active window to 30 fps in low power mode and
raising an inactive window's interval to at least 30 fps. Frame requests
are accepted within 1 ms of the target interval, so a 16.666 ms callback no
longer waits for the next refresh. ZZZ has no profiler journal, and the
throttle reads the platform mode per frame like `thermal_state`, so the
skip-reason variant and the cached notification observer were omitted.
Three frame-throttle tests pass.

Superseded: the audit in `upstream-sync-audit-2026-10-27.md` found two
defects in the original port and fixed both in `6159bc81e0`. `power_mode`
now returns `Unknown` below macOS 12 instead of messaging a selector that
only exists from Monterey on, and a forced render deferred by the throttle
is remembered and served by the next frame. The class stays `B`.

#### e4a3732b94 — text system reference cycle (B)

The wrapper pool stores `CharWidthCache` instead of `LineWrapper`. A
wrapper owns an `Arc<TextSystem>`, so pooling it kept the text system and
its font caches alive forever once any text had been wrapped. The cache
still carries the ASCII and other-character width tables, so reuse is
preserved, and `LineWrapperHandle::drop` returns only the widths. A
regression test wraps a line, drops every handle and asserts the text
system is freed; all 34 `text_system` tests pass.

#### 24a2823456 — WebGL surface reconfiguration (C)

Only moves the wgpu-family git revision and its `naga` validation
dependency from `3b3506e` to `cddb2f5` in the manifest and lockfile. ZZZ
vendors wgpu v29 rather than tracking the upstream git revision, so there
is no local dependency to advance and no code change to port.

### Verification

| Check                                                                                     | Result  | Notes                                            |
| ----------------------------------------------------------------------------------------- | ------- | ------------------------------------------------ |
| Reviewed baseline ancestry                                                                | PASS    | `57c7b10ece` is an ancestor of `FETCH_HEAD`      |
| `cargo test --locked -p vim --lib helix::surround`                                        | PASS    | 15 tests                                         |
| `cargo check --locked -p extension`                                                       | PASS    | grammar flag                                     |
| `cargo test --locked -p git --lib stash`                                                  | PASS    | 5 tests                                          |
| `cargo test --locked -p gpui --lib test_native_close_defers_app_access`                   | PASS    | defers app access                                |
| `cargo test --locked -p git_ui --lib is_non_fast_forward`                                 | PASS    | 7 predicate tests                                |
| `cargo check --locked -p git_ui --all-targets`                                            | PASS    | toast title                                      |
| `cargo test --locked -p util --lib paths::tests::test_path_style_file_stem_and_extension` | PASS    | 28 cases                                         |
| `cargo check --locked -p title_bar`                                                       | PASS    | display name                                     |
| `cargo test --locked -p gpui --lib window::tests::frame_throttle`                         | PASS    | 2 tests                                          |
| `cargo test --locked -p gpui --lib window::tests::power_modes`                            | PASS    | 1 test                                           |
| `cargo test --locked -p gpui --lib text_system`                                           | PASS    | 34 tests                                         |
| `cargo check --locked --workspace --all-targets`                                          | PASS    | after all eight ports                            |
| `cargo fmt --check`                                                                       | PASS    | clean                                            |
| `git diff --check`                                                                        | PASS    | clean                                            |
| `./script/check-philosophy`                                                               | PASS    | philosophy gate                                  |
| Locale key parity (`en.json` vs `zh-CN.json`)                                             | PASS    | 3662 keys each                                   |
| macOS Low Power Mode runtime                                                              | NOT RUN | host is Linux; `gpui_macos` cannot be built here |
| `cargo test --workspace`                                                                  | NOT RUN | not requested                                    |
| Remaining commits through live head                                                       | NONE    | reviewed head equals the queried live head       |

The reviewed baseline advances to
`24a2823456f16e36bf32df510fb5c43fee7a2565`, the live upstream head at
query time.

## Batch 4

### Scope

- Reviewed baseline: `24a2823456f16e36bf32df510fb5c43fee7a2565`
- Reviewed head: `37faef81f74fc1bb08be9939c4cffc8ede6ced2f`
- Live upstream head queried: `37faef81f74fc1bb08be9939c4cffc8ede6ced2f`
- Query time: `2026-10-27T17:38:29+01:00`
- Reviewed range: the 1 commit after `24a2823456`
- Remaining after this batch: none through the queried live head

### Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                         |
| ---------- | ----- | ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 37faef81f7 | B     | d10420510d   | Drops completion match offsets when the label's current filter text no longer equals the filtered text. Only the render hunk applies; the second hunk targets a label cache that ZZZ does not have. |

Totals: zero `A`, one `B`, zero `C` across one commit.

### Applied Work

- `d10420510d` is a `B` port of `37faef81f7`.

### Per-Commit Narrative

#### 37faef81f7 — solargraph label changes during resolve (B)

Fuzzy match offsets are byte offsets into the candidate string captured
when the menu opened, which is the label's filter slice. Solargraph
returns a different label from `completionItem/resolve` even though the
client does not advertise a changeable label, so applying the recorded
offsets to the new label indexed past its end. The render path now keeps
the offsets only while the candidate string still equals the label's
filter text, and shifts them by the label's current `filter_range.start`;
otherwise the item renders without fuzzy-match emphasis. Three cases are
covered by `test_label_match_ranges_follow_current_filter_text`:
unchanged filter text, filter text at a non-zero offset, and a resolved
label whose filter text changed. The test fails when the guard is
removed.

Upstream's second hunk rewrote the fallback in `sort_string_matches` that
reuses a previously computed match. ZZZ's `sort_string_matches` has
diverged (tiered sorting, no `label_match_state` cache), so there is no
equivalent fallback to guard.

ZZZ also still rejects resolved labels whose filter text changed, in
`regenerate_completion_labels` (the guard came from #22061; upstream
relaxed it in #62433, which predates this sync's baseline). That closes
the panic at its source here, so the upstream LSP-driven regression test
cannot reach the render guard and was replaced by the unit test above.

### Verification

| Check                                                                                    | Result           | Notes                                         |
| ---------------------------------------------------------------------------------------- | ---------------- | --------------------------------------------- |
| Reviewed baseline ancestry                                                               | PASS             | `24a2823456` is an ancestor of `FETCH_HEAD`   |
| `cargo test --locked -p editor --lib test_label_match_ranges_follow_current_filter_text` | PASS             | 1 test, three assertions                      |
| Guard removed (negative control)                                                         | FAIL as expected | stale offsets were reused and the test failed |
| `cargo test --locked -p editor --lib completions_resolve`                                | PASS             | 2 tests                                       |
| `cargo check --locked --workspace --all-targets`                                         | PASS             | after the port                                |
| `cargo fmt --check`                                                                      | PASS             | clean                                         |
| `git diff --check`                                                                       | PASS             | clean                                         |
| `./script/check-philosophy`                                                              | PASS             | philosophy gate                               |
| Locale key parity (`en.json` vs `zh-CN.json`)                                            | PASS             | 3662 keys each                                |
| Upstream LSP-driven regression test                                                      | NOT PORTED       | local resolve rejects filter-text changes     |
| `cargo test --workspace`                                                                 | NOT RUN          | not requested                                 |
| Remaining commits through live head                                                      | NONE             | reviewed head equals the queried live head    |

The reviewed baseline advances to
`37faef81f74fc1bb08be9939c4cffc8ede6ced2f` (2026-10-27 14:32:31 UTC), the
live upstream head at query time.
