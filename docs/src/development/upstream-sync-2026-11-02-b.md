---
title: Upstream Sync 2026-11-02 (b)
description: Selective Zed upstream sync audit, second batch.
---

# Upstream Sync 2026-11-02 (b)

## Scope

- Target branch: `sync/upstream-2026-11-02`, continuing from the first
  batch
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `4ec98df5000b21dca9aa1a064d770fb6ebc63880`
- Live upstream head queried: `00b86477f8b81c8d20328b096200f91452a572df`
- Query time: `2026-11-02`
- Reviewed range: `4ec98df5000b..00b86477f8b8`, three commits, oldest
  first
- Remaining after this batch: zero; the range ends at the queried head

The previous reviewed baseline is an ancestor of the queried head. All
three commits are `B` ports: each carries a safe behavior change but
touches a local surface that has diverged. The next sync starts after
`00b86477f8b8`.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                                                                               |
| ---------- | ----- | ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| f73feed9de | B     | 8bec53fb6b   | Stops `http_client` from re-exporting `anyhow::Result` and `anyhow!`, so consumers import the crate directly. The `language_model` hunks were omitted because that crate has no `http_client` dependency and no `fake_provider` module locally.                                           |
| f4d40a2ccb | B     | e837ac0272   | Makes Git panel change rows draggable with the project panel's `DraggedSelection` payload, so terminals insert the path and panes open the file; marked rows drag together. Ported onto ZZZ's index-based mark model and stateful `on_drag`; the entry-count preview string is localized. |
| 00b86477f8 | B     | c087a7309c   | Adds `debugger.default_width` and `debugger.default_height`, defaulting to 300 pixels, and exposes them in the Settings Editor. Ported with ZZZ's `Option<f32>` pixel convention instead of upstream's `PixelSetting`, and with localized Settings Editor labels.                         |

Totals: zero `A`, three `B`, zero `C` across three commits.

## Applied Work

- `8bec53fb6b` is a `B` port of `f73feed9de`.
- `e837ac0272` is a `B` port of `f4d40a2ccb`.
- `c087a7309c` is a `B` port of `00b86477f8`.

## Per-Commit Narrative

#### f73feed9de — http_client anyhow reexports (B)

`http_client` used to re-export `anyhow::Result` and the `anyhow!` macro,
which made downstream imports depend on it. The port makes the `Result`
import private and drops the macro re-export, and `dev_container` now
imports `anyhow::anyhow` directly in both its library and test modules.

The upstream `language_model` hunks were omitted: ZZZ's crate has no
`http_client` dependency and no `fake_provider` module (that file is
deleted locally), so the hunks only cleaned up imports in code the fork
does not carry. The lockfile hunk's remaining context is also upstream
only. A workspace search finds no other local consumer of the removed
re-exports.

#### f4d40a2ccb — drag Git panel rows (B)

Upstream reuses the project panel's `DraggedSelection` payload so a
dragged change row is accepted by existing drop targets: a terminal
writes the absolute path, and a pane opens the file. A marked row drags
the whole marked set, and rows without a project entry (deleted files)
are not draggable. The port adds `GitPanel::dragged_selection`, the
`DraggedGitEntryView` drag preview, and `on_drag` on the status row.

Local adaptation was required in three places. ZZZ marks rows by index
(`marked_entries: Vec<usize>`) rather than by keyed path, so the marked
set is rebuilt from those indices. `DraggedSelection` still has a private
`resource_ids` field locally, so the payload goes through
`DraggedSelection::new` instead of a struct literal. This GPUI version
only exposes `on_drag` on stateful elements, so the inner name row gains
its own element id. The "N entries" preview label is localized through
the new `git_ui.git_panel.dragged_entry_count` key in both catalogs, and
the regression test runs on ZZZ's `setup_git_panel_with_changes` fixture.

#### 00b86477f8 — debugger panel default size (B)

The debugger panel had a hardcoded 300-pixel default size. Upstream adds
`debugger.default_width` (left/right docks) and `debugger.default_height`
(bottom dock), both defaulting to 300, reads them in
`DebugPanel::default_size`, and exposes both under Panels → Debugger
Panel in the Settings Editor.

Upstream stores them as `PixelSetting`, a content newtype that ZZZ does
not carry. The port follows the local pixel-settings convention instead:
`Option<f32>` in `DebuggerSettingsContent` with
`serialize_optional_f32_with_two_decimal_places`, and `px(...)`
conversion into `gpui::Pixels` in `DebuggerSettings`. That forces
`DebuggerSettingsContent` to drop its `Eq` derive, matching upstream. The
Settings Editor entries use `lt(...)` keys added to both locale catalogs
rather than upstream's hardcoded English strings. The ported test sets
the settings as floats and asserts the per-dock default size.

### Verification

| Check                                                                                                                                                                     | Result  | Notes                                                             |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- | ----------------------------------------------------------------- |
| `cargo check --all-targets -p http_client -p dev_container -p language_model -p git_ui -p dap -p settings_content -p settings_ui -p debugger_ui -p debugger_tools -p zzz` | PASS    | after the final commit                                            |
| `cargo test -p git_ui --lib`                                                                                                                                              | PASS    | 128 tests, including `test_dragged_selection_carries_marked_rows` |
| `cargo test -p debugger_ui --lib`                                                                                                                                         | PASS    | 73 tests, including `test_debug_panel_default_size`               |
| `cargo test -p settings_ui --lib`                                                                                                                                         | PASS    | 21 tests                                                          |
| `cargo test -p dap --lib`                                                                                                                                                 | PASS    | 8 tests                                                           |
| Locale catalog key parity                                                                                                                                                 | PASS    | 3770 keys in each of `en.json` and `zh-CN.json`, no diff          |
| `cargo fmt --all -- --check`                                                                                                                                              | PASS    | clean                                                             |
| `git diff 7133a86c1d..HEAD --check`                                                                                                                                       | PASS    | clean                                                             |
| macOS / Windows runtime checks                                                                                                                                            | NOT RUN | host is Linux; those backends are platform-only                   |
| `cargo test --workspace`                                                                                                                                                  | NOT RUN | narrowest per-crate checks were used                              |
| Remaining commits through the queried head                                                                                                                                | NONE    | `00b86477f8b8` is the live head                                   |

The reviewed baseline advances to
`00b86477f8b81c8d20328b096200f91452a572df` for this batch, which is the
live upstream head at query time.
