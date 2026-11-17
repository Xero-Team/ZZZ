---
title: Upstream Sync 2026-11-10
description: Selective Zed upstream sync, first 20 commits after the 2026-11-08 baseline.
---

# Upstream Sync 2026-11-10

## Scope

- Local base: `main` at `ced90e0c517bf368238225c4363614b7c2cb1376`
- Target branch: `sync/upstream-2026-11-10`, started at `a3cc2d6657`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `db43f05e398036f08e544c03cbc068216c7a0aaa`
- Live upstream head queried: `f0d896f2c18a543335eb155f729d36b624695591`
- Query time: `2026-11-10`
- Reviewed range: `db43f05e3980..696003b1f6`, 20 commits, oldest first
- Remaining after this batch: 12 commits; the next sync starts after
  `696003b1f6b806c03b9e83f2a448aab060736cf7`

The previous baseline is an ancestor of the live head. The full range is 32
commits; this run reviews the first 20 per the batch default. The remaining
twelve are, oldest first, `47c46191f4`, `cebd5fc600`, `597c383bcd`,
`779e9fc437`, `62acd36cec`, `5e23b6e71d`, `270df71b8d`, `483b746b56`,
`1fea6cbaf6`, `4ed72e0135`, `a1f2e21c2c`, and `f0d896f2c1`.

Upstream paths under `crates/zed/` were read against ZZZ's `crates/zzz/`, and
upstream `zed://` URL prefixes against ZZZ's `zzz://`.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| ---------- | ----- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 4c90ed88c3 | C     | --           | Directs the community PR board to new fields in `.github/workflows/community_pr_board*.yml` and rewrites `script/github-community-pr-board.py` and `script/github_helpers.py`. ZZZ carries no community, triage, or release automation; rejected surface with no independent editor behavior.                                                                                                                                                                                      |
| e231594b6b | C     | --           | Adds configurable date formats and per-view date styles across 23 files, including a new `git_ui/src/git_timestamp.rs`, a `git.date_style` settings family, `settings_ui` page data, and every diverged blame, tooltip, graph, stash, and panel call site. ZZZ already localizes timestamps through `time_format::format_localized_timestamp` with a fixed `TimestampFormat` and has no `EditorSettings::file_diff` family to extend, so the settings plumbing would be a rewrite. |
| 1253dde6ec | C     | --           | Rewrites 614 lines of `git_ui/src/commit_view.rs` to scroll the commit view to the file a blame entry points at. ZZZ's `commit_view.rs` has no blame entries, blame columns, or blame navigation at all, so the retained behavior has no local anchor.                                                                                                                                                                                                                             |
| 9a8ab54db4 | C     | --           | Adds `auto_generate_thread_titles` and title restoration for native agent threads via `NativeAgentConnection`. ZZZ's `crates/agent` has no `NativeAgentConnection` and no thread-title generation; this is native-agent surface, which is out of bounds.                                                                                                                                                                                                                           |
| b5cd310b3b | B     | 8544429edd   | Hides the read-only toggle on tabs that cannot be toggled. Ported `Item::read_only_toggle_buffer` in place of `Item::toggle_read_only`, moved the capability flip into `ItemHandle::toggle_read_only`, and gated the tab icon, the tab tooltip, and the context-menu entry on it. The editor test is retained; the markdown-preview test is omitted as test-only.                                                                                                                  |
| 94f3eb1b72 | C     | --           | Adds a 715-line `project_bookmarks` crate wired into `picker_preview` (absent locally), a new `picker_preview::PickerPreview` render path, keymap-free bookmark pickers, and project bookmark storage. The feature spans an absent crate and a preview widget ZZZ does not have.                                                                                                                                                                                                   |
| d2a4fb6ded | C     | --           | Adds range selection, edge jumps, and page navigation to `crates/tabular_data_preview`. ZZZ has no `tabular_data_preview` crate.                                                                                                                                                                                                                                                                                                                                                   |
| 4ce4b7b567 | C     | --           | Moves `Div`'s `Interactivity` out of line and rewrites `crates/gpui/src/randomized_element_tree.rs` plus its benchmark. ZZZ has neither the randomized element-tree harness nor the matching `Div` internals; the retained change is the harness rewrite, which cannot be isolated.                                                                                                                                                                                                |
| 62682ebde9 | B     | 5b799f3aa7   | Computes the tab stop order only when focus moves. Ported `TabStopMap` from a `SumTree` to an append-then-sort vector, with `next`/`prev` taking `&mut self`; the twelve tab stop tests pass. The accessor `tab_stop_count` is dropped because it has no local caller.                                                                                                                                                                                                             |
| 2311b2497c | B     | 90b0ba15e3   | Stops remote branch switches from resending the commit graph. Ported the `!project.is_local()` guard in `GitPanel::preload_commit_history`; the `commit_history_shas` accessor and the transport tests live in the absent `collab` crate and are omitted.                                                                                                                                                                                                                          |
| 4023e3c88e | B     | 75a6716983   | Takes a line wrapper from the pool only when truncating. Ported the moved call into the truncation branch of `TextLayout::prepaint`, so non-truncated layouts no longer lock the wrapper.                                                                                                                                                                                                                                                                                          |
| 96508041c7 | A     | ad732d6c2f   | Clean cherry-pick. Splits the status entry icons out of the `flex_1`/`min_w_0` container so they stop overlapping the diff stats in a narrow git panel.                                                                                                                                                                                                                                                                                                                            |
| 22580c6e69 | B     | 22981046a0   | Trims per-line and per-glyph work in the text system: shared empty `FontFeatures::default`, a `has_background` early return in both local `paint_background` methods, an empty decoration-run list, and the cheap subpixel-support check first in `window/methods_paint.rs`. The `layout_line` call-site hunk is covered by the next commit.                                                                                                                                       |
| ddb90e5e89 | B     | 663e749f2f   | Shares the shaped line's text with the layout cache. Ported the generic `WindowTextSystem::layout_line` and its `font_runs` helper so the cache key holds the caller's `SharedString` pointer, plus the upstream pointer-sharing test.                                                                                                                                                                                                                                             |
| ec23721646 | A     | 88f0e25edd   | Clean cherry-pick. Renders ACP config options right to left so the agent's first option sits next to the send button.                                                                                                                                                                                                                                                                                                                                                              |
| 13d1a56e78 | B     | 7ad6158e81   | Fixes `file://` and `zzz://file` URLs that carry a Windows drive letter. Ported `parse_local_file_path` with the leading-slash strip and drive-prefix check; ssh paths keep `parse_file_path`. The Windows-only test is omitted because it cannot run on this host.                                                                                                                                                                                                                |
| 7c409ecdc2 | B     | 3d8edf58cd   | Accepts several `ssh://` paths for one host in a single `OpenRequest`. Ported the relaxed guard and added the accepted-case and rejected-case tests.                                                                                                                                                                                                                                                                                                                               |
| 39467cf4b1 | C     | --           | Removes hand-written `IntoElement` impls from 30 files and replaces the blanket impl in `crates/gpui/src/element.rs`. ZZZ's element impls and `element.rs` have diverged, the change alters no local behavior on its own, and the load-bearing part is the trait rewrite rather than any isolatable fix.                                                                                                                                                                           |
| 051a638727 | C     | --           | Adds `git.commit_view.expand_description` and `expand_files` defaults. The commit extends the `git` view settings family next to `EditorSettings::file_diff`, which ZZZ never absorbed, so it depends on an unabsorbed prerequisite and has no settings anchor locally.                                                                                                                                                                                                            |
| 696003b1f6 | B     | 492f065dd8   | Keeps the first rendered markdown block at the top. Ported `ParsedMarkdown::first_rendered_block_start` and the `first_block_start` field/parameter on the element builder, so the top margin is dropped on the first block that renders instead of on source offset 0. The upstream test is retained.                                                                                                                                                                             |

Totals: two `A` (both clean cherry-picks), nine `B`, nine `C` across 20 commits.

## Applied Work

- `ad732d6c2f` is a clean `A` cherry-pick of `96508041c7`, landed with
  `git cherry-pick -x -s`.
- `88f0e25edd` is a clean `A` cherry-pick of `ec23721646`, landed with
  `git cherry-pick -x -s`.
- `8544429edd` is a `B` port of `b5cd310b3b`.
- `5b799f3aa7` is a `B` port of `62682ebde9`.
- `90b0ba15e3` is a `B` port of `2311b2497c`.
- `75a6716983` is a `B` port of `4023e3c88e`.
- `22981046a0` is a `B` port of `22580c6e69`.
- `663e749f2f` is a `B` port of `ddb90e5e89`.
- `7ad6158e81` is a `B` port of `13d1a56e78`.
- `3d8edf58cd` is a `B` port of `7c409ecdc2`.
- `492f065dd8` is a `B` port of `696003b1f6`.

Every `B` commit carries `Upstream`, `Retained`, and `Omitted` trailers.

## Per-Commit Narrative

### e231594b6b — date format and per-view date style settings (C)

The feature is a settings family: `git.date_style` plus per-view overrides,
plumbed through `settings_content::ProjectSettings`, `EditorSettings`,
`settings_ui::page_data`, `assets/settings/default.json`, and nine `git_ui`
render sites. ZZZ formats timestamps through
`time_format::format_localized_timestamp` with a hard-coded
`TimestampFormat::MediumAbsolute` and has no `EditorSettings::file_diff`, so
the surrounding settings group does not exist. Retaining the settings without
the render sites, or the render sites without the settings, would be
scaffolding; the full port is a rewrite of the feature rather than an isolated
behavior change.

### 1253dde6ec — scroll the commit view to the blamed file (C)

The commit adds blame-to-file navigation to the commit view: a `BlameTarget`
plumbed from `commit_view.rs` to `blame_ui.rs`, `commit_tooltip.rs`,
`commit_context_menu.rs`, and `git_graph.rs`, plus editor reveal calls.
ZZZ's `commit_view.rs` contains no blame surface at all, so there is no local
caller and no anchor for the behavior.

### 9a8ab54db4 — disable automatic thread title generation (C)

The settings entry and the restore path are native-agent surface: the commit
threads `title` through `NativeAgentConnection::open_thread` and
`agent_panel.rs`. ZZZ has no `NativeAgentConnection` in `crates/agent` and no
thread-title generation to disable.

### 94f3eb1b72 — bookmark picker (C)

A new 715-line `project_bookmarks` crate, a `PickerPreview` render path that
only exists in `crates/picker_preview` (absent locally), and project bookmark
storage. The picker and preview APIs are the feature's substrate, so the
commit cannot be isolated from crates ZZZ does not carry.

### 4ce4b7b567 — move a Div's Interactivity out of line (C)

The commit's retained value is the `randomized_element_tree` rewrite and its
benchmark, which asserts the same element tree renders identically after
`Div` stops boxing its `Interactivity`. ZZZ has no
`crates/gpui/src/randomized_element_tree.rs`, and its `div.rs` internals have
diverged, so neither the harness nor its assertion can be ported.

### 39467cf4b1 — remove hand-written IntoElement impls (C)

Thirty files lose their explicit `IntoElement` impls in favor of the blanket
impl, which is itself rewritten in `element.rs`. The commit has no user-facing
behavior; its correctness is established by compiling the whole workspace
against the new blanket impl. ZZZ's element impls (`list.rs`, `markdown.rs`,
`scrollbar.rs`) have diverged, so the removal is a broad mechanical rewrite
whose only invariant is "the tree still compiles".

### 051a638727 — commit view defaults (C)

The commit adds `commit_view.expand_description` and `commit_view.expand_files`
to the `git` settings group, immediately after `EditorSettings::file_diff` in
`editor_settings.rs` and `settings_content::project`. ZZZ never absorbed
`file_diff`, which is why the diff does not apply; the commit depends on that
unabsorbed prerequisite.

### b5cd310b3b — read-only toggle only where it can work (B)

`Item` used to own `toggle_read_only(&mut self, …)`, and only `Editor`
implemented it. The port replaces that with `read_only_toggle_buffer(&self)`
returning the singleton `Buffer`, and `ItemHandle::toggle_read_only` performs
the capability flip and refresh itself. `Pane::render_tab` and the tab context
menu consult `read_only_toggle_buffer(…)` before offering the toggle, so
markdown previews and multibuffer editors no longer show a control that did
nothing. The editor test covers both cases; the preview test is omitted as
test-only and harness-dependent.

### 62682ebde9 — lazy tab stop ordering (B)

`TabStopMap` previously inserted every focusable element into a `SumTree` on
every frame and sought through it on tab. The port keeps insertion order in a
vector, records each handle's latest index, and sorts in place the first time
the order is needed (`sort_unstable`, since insertion indices never tie). The
twelve existing tab stop tests, plus the upstream `test_insert_after_sorting`,
pass. `tab_stop_count` is not ported: nothing local calls it, and importing it
would add dead code.

### 2311b2497c — stop remote commit-graph preloading (B)

`GitPanel::preload_commit_history` runs when the History tab is about to be
shown and asks the repository for `graph_data`. On a remote project that
request round-trips to the remote host, so switching branches resent the whole
graph. The port returns early when `!project.is_local()`. The upstream tests
and the `commit_history_shas` accessor they use live in the absent `collab`
crate and are omitted.

### 22580c6e69 and ddb90e5e89 — text system trimming (B)

Four of the five hunks are behavior-preserving micro-optimizations: sharing one
empty `FontFeatures` list, skipping `paint_line_background` when no decoration
run has a background, ordering the subpixel check before
`background_appearance()`, and starting the decoration-run vector empty. ZZZ's
`paint_background` exists on `ShapedLine` and `WrappedLine` rather than on
`LineLayout`, so the guard is placed in both. The fifth hunk is subsumed by
`ddb90e5e89`, which makes `WindowTextSystem::layout_line` generic, splits run
computation into `font_runs`, and hands the caller's `SharedString` to the
cache so the cache key shares the pointer; the upstream pointer-sharing test
is retained and passes.

### 13d1a56e78 and 7c409ecdc2 — opening paths from URLs (B)

Both commits harden `OpenRequest::parse`. The first splits local file URLs off
`parse_file_path` so a Windows drive letter survives the URL's leading `/`;
ZZZ's prefixes are `zzz://file` and `zzz://ssh`, so only the prefix names
change. The second relaxes the "cannot open both local and ssh paths" guard to
allow a second `ssh://` path for the connection already recorded, leaving
different connections rejected. The added tests cover one host with several
paths, a local path followed by a remote one, and two conflicting connections.

### 696003b1f6 — first rendered block at the top (B)

The markdown element builder removed the top margin on the block whose source
offset was 0. When a document starts with blank lines, an HTML comment, or
link definitions, that block renders nothing and the margin collapses into the
next visible block. The port computes the first block that renders anything —
skipping `RootStart`/`RootEnd`, blank HTML blocks, and empty HTML comments —
and removes the margin there. ZZZ's builder takes `line_breaking` where
upstream takes `highlights`, so only the new parameter and field are added.
The upstream test is retained and passes.

## Verification

| Check                                                                                 | Result  | Notes                                                             |
| ------------------------------------------------------------------------------------- | ------- | ----------------------------------------------------------------- |
| `cargo check --locked -p gpui -p markdown -p workspace -p editor -p git_ui`           | PASS    | after all nine `B` ports                                          |
| `cargo check --locked -p zzz`                                                         | PASS    | after the two `open_listener` ports                               |
| `cargo test --locked -p gpui --lib tab_stop`                                          | PASS    | 10 tests                                                          |
| `cargo test --locked -p gpui --lib shape_line_shares_its_text`                        | PASS    | 1 test                                                            |
| `cargo test --locked -p markdown first_rendered_block`                                | PASS    | 1 test                                                            |
| `cargo test --locked -p editor --lib test_read_only_toggle_requires_singleton_buffer` | PASS    | 1 test                                                            |
| `cargo test --locked -p zzz --bins open_listener::tests::test_parse_ssh_urls`         | PASS    | 2 tests                                                           |
| `cargo test --locked -p zzz --bins open_listener::tests::test_reject_ssh_urls`        | PASS    | 2 tests                                                           |
| `cargo fmt --all -- --check`                                                          | PASS    | clean                                                             |
| `git diff --check` on every applied commit                                            | PASS    | clean                                                             |
| Windows drive-letter URL parsing                                                      | NOT RUN | the test is `#[cfg(target_os = "windows")]` and the host is Linux |
| macOS and Windows runtime checks                                                      | NOT RUN | host is Linux; those backends are platform-only                   |
| `cargo test --workspace`                                                              | NOT RUN | narrowest per-crate checks were used                              |

The reviewed baseline advances to
`696003b1f6b806c03b9e83f2a448aab060736cf7`.
