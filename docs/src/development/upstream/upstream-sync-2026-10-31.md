---
title: Upstream Sync 2026-10-31
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-10-31

## Batch 1 {#batch-1}

### Scope

- Local base: `main` at `acc48051d7`
- Target branch: `sync/upstream-2026-10-31`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `d4014230735e4265d98066420b9271df2324ccab`
- Live upstream head queried: `ba63a022c9a22ca66d68ef2a8f0bb9b5b0a9984d`
- Query time: `2026-10-31`
- Reviewed range: `d4014230735e..ba63a022c9a2`, eight commits, oldest
  first
- Remaining after this batch: zero; the range ends at the queried head

The previous baseline is an ancestor of the queried head. The whole
remaining range fits in one batch, so this batch closes the `2026-10-29`
sync. The next sync starts after `ba63a022c9`.

### Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                               |
| ---------- | ----- | ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 8fa085e397 | B     | ceae102bd5   | Repaints cached views when inherited element opacity changes. The cache key carries the opacity, div prepaint applies its opacity to children, and replayed output under a different opacity redraws on the next frame.                   |
| 99e70ee4a0 | B     | 40ba820e45   | Times the animation element with the executor clock so a fake clock steps animations deterministically while production keeps the platform monotonic clock. The spring-animation hunks were omitted; ZZZ has no `SpringAnimationElement`. |
| bfd1dd484b | C     | --           | Re-aligns the author avatar cell in the git graph list. ZZZ's `crates/git_graph/src/git_graph.rs` renders the author column as a plain label and has no table row model to align; avatars only appear in the git panel details pane.      |
| 915290734a | A     | d7b19cff92   | Clean cherry-pick. Adds `build_create_pull_request_url` to the Forgejo provider so Codeberg remotes can create pull requests from the UI.                                                                                                 |
| 422b53cff8 | B     | 7c4b38de3e   | Keeps the commit open in the active tab highlighted in the History tab by sha, clears the highlight when another tab is active, moves the keyboard cursor to the open commit, and only draws the focus border after keyboard input.       |
| 70b1b91b73 | B     | 343c77ff14   | Gives the buffer and project search buttons local click handlers, so a click in an unfocused split no longer resolves the workspace's active pane first.                                                                                  |
| 14b7b0ef37 | B     | 756f865145   | Replaces kde-blur with ext-background-effect, publishes client corner radii to the platform, and keeps kde-blur as a fallback when only the old global is advertised.                                                                     |
| ba63a022c9 | A     | --           | Already equivalent. ZZZ's remote server picker renders its own list with `Scrollbars::always_visible(ScrollAxes::Vertical)`; there is no `Picker::list` call to enable a scrollbar on.                                                    |

Totals: two `A` (one clean, one already equivalent), five `B`, one `C`
across eight commits.

### Applied Work

- `ceae102bd5` is a `B` port of `8fa085e397`.
- `40ba820e45` is a `B` port of `99e70ee4a0`.
- `d7b19cff92` is a clean `A` cherry-pick of `915290734a`.
- `7c4b38de3e` is a `B` port of `422b53cff8`.
- `343c77ff14` is a `B` port of `70b1b91b73`.
- `756f865145` is a `B` port of `14b7b0ef37`.

### Per-Commit Narrative

#### 8fa085e397 — cached view opacity (B)

A `div()` that changes its opacity used to reuse its child's cached
render output, so nested views kept painting at the old alpha until some
other input invalidated them. `ViewCacheKey` now records the
`ElementOpacity` inherited at prepaint time, `div` prepaint wraps the
element in `with_element_opacity`, and painting cached output under a
different opacity requests a fresh frame. ZZZ's `view.rs` keeps
`ViewCacheKey` local, so the port adds the field there and re-requests at
paint time. The upstream test relies on a `simulate_next_frame` helper
and a still-mouse widening case that ZZZ's test platform does not expose;
the ported `cached_view_follows_inherited_opacity` test instead draws
several frames with wrapper and hover opacity changes.

#### 99e70ee4a0 — animation clock (B)

Animation elements now read their start time and elapsed duration from
`cx.background_executor().now()` instead of `Instant::now()`, which makes
`TestAppContext`'s fake clock advance spring and tween progress
deterministically. Local animation tests keep passing. The upstream
spring hunks were omitted because ZZZ has no `SpringAnimationElement` and
no `delayed_frame_pending` state to retarget.

#### bfd1dd484b — git graph author cells (C)

The upstream fix vertically aligns the author avatar inside the git
graph's table row. ZZZ renders the author column as a plain `Label` in
`crates/git_graph/src/git_graph.rs`; there is no author cell, no avatar in
the graph list, and no row model with a configurable cell alignment. The
only avatars in ZZZ's git UI live in the panel's details pane, which this
commit does not touch, so there is no misalignment to correct.

#### 915290734a — Codeberg pull requests (A)

Forgejo remotes had no `build_create_pull_request_url`, so "create pull
request" failed with "Unable to construct pull request URL". The clean
cherry-pick adds the implementation in
`crates/git_hosting_providers/src/providers/forgejo.rs` and its
`test_build_codeberg_create_pull_request_url` test. The patch applied
without conflicts and keeps the upstream author plus the DCO trailer.

#### 422b53cff8 — history tab highlight (B)

The History tab now highlights the commit that the active tab has open
rather than HEAD. `CommitView` exposes `commit_sha()` and
`repository()`, `GitPanel` tracks `active_history_sha`, and the workspace
forwards `ActiveItemChanged` so switching tabs clears or moves the
highlight. `active_history_entry_index` and `set_commit_history` seed the
cursor when the history tab opens, `focus_in` only takes focus when the
last input was keyboard, and the row renders `element_selected` instead
of a hard-coded HEAD marker. Five upstream tests and the
`HistoryPanelTest` helper were ported onto ZZZ's `VisualTestContext`
signatures; the context-menu hover branch was omitted because ZZZ has no
hover target there.

#### 70b1b91b73 — search button focus context (B)

Search buttons dispatched window actions, so a click in an unfocused
split first resolved the workspace's active pane and toggled the wrong
search bar. `search_bar.rs` gains `render_local_action_button`, which
focuses the button's focus handle and calls a handler directly.
`SearchSource` gains a `BufferSearchBar` variant with its own
discriminant so buffer and project buttons keep distinct element ids, and
`BufferSearchBar` passes its own context. `ProjectSearchView` gains
`toggle_replace`, `toggle_filters_ui`, and `toggle_search_option_ui` so
the landing page and toolbar share one set of handlers, and
`ProjectSearchBar` delegates to them instead of duplicating the logic.
Translated labels and tooltips stay on the existing `app_i18n::tr` keys.
The search-on-type refresh that upstream folds into `toggle_filters_ui`
was omitted because ZZZ's `toggle_filters` deliberately does not start a
search.

#### 14b7b0ef37 — ext-background-effect (B)

KDE removed kde-blur from Plasma 6.7, so the Wayland backend now prefers
`ext-background-effect` and keeps kde-blur as a fallback. The port adds
`Pixels::to_i32`,
`PlatformWindow::set_client_corner_radii`/`Window::set_client_corner_radii`,
publishes the client decoration radii from
`workspace::client_side_decorations` for untiled edges, and builds the
blur region with `add_rounded_rect`, which rasterizes each corner with
scanline insets so the blur follows the rounded window shape. The
`client.rs` globals split into `kde_blur_manager` and
`background_effects_manager`, and `update_window` now computes its region
from the tiled inset area. The four `rounded_rect_tests` and the
`Pixels::to_i32` test came along. The redraw request in
`set_client_corner_radii` was dropped because this fork's
`WaylandWindowStatePtr` has no `request_redraw`; `update_window` still
writes the blur region immediately and the next frame commits it.

#### ba63a022c9 — remote picker scrollbar (A, already equivalent)

Upstream adds `list_measure_all()` and `show_scrollbar(true)` to the
`Picker::list` in `remote_servers.rs`. ZZZ's remote server picker does
not use `Picker::list`: it renders its own list inside the modal and
wraps it in
`custom_scrollbars(Scrollbars::always_visible(ScrollAxes::Vertical))`
with a tracked scroll handle. The scrollbar is already always visible and
measured from the section's layout, so no code change was needed.

### Verification

| Check                                                                     | Result  | Notes                                           |
| ------------------------------------------------------------------------- | ------- | ----------------------------------------------- |
| `cargo check --locked -p search --all-targets`                            | PASS    | after `343c77ff14`                              |
| `cargo test --locked -p search --lib`                                     | PASS    | 58 tests                                        |
| `cargo test --locked -p git_ui --lib`                                     | PASS    | 127 tests after `7c4b38de3e`                    |
| `cargo test --locked -p git_hosting_providers --lib create_pull_request`  | PASS    | 3 tests, including the Codeberg URL             |
| `cargo test --locked -p gpui --lib cached_view_follows_inherited_opacity` | PASS    | new regression test                             |
| `cargo test --locked -p gpui --lib animation`                             | PASS    | 2 tests after the executor clock                |
| `cargo test --locked -p gpui --lib test_pixels_to_i32`                    | PASS    | new truncation and saturation test              |
| `cargo test --locked -p gpui_linux --lib`                                 | PASS    | 33 tests, including 4 `rounded_rect_tests`      |
| `cargo check --locked -p gpui -p gpui_linux -p workspace --all-targets`   | PASS    | after `756f865145`                              |
| `cargo fmt -p search -p gpui -p gpui_linux -p workspace -- --check`       | PASS    | clean                                           |
| macOS / Windows runtime checks                                            | NOT RUN | host is Linux; those backends are platform-only |
| `cargo test --workspace`                                                  | NOT RUN | narrowest per-crate checks were used            |
| Remaining commits through the queried head                                | NONE    | `ba63a022c9` is the live head                   |

The reviewed baseline advances to
`ba63a022c9a22ca66d68ef2a8f0bb9b5b0a9984d` for this batch, which is the
live upstream head at query time.
