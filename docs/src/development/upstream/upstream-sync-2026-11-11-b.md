---
title: Upstream Sync 2026-11-11 (batch B)
description: Selective Zed upstream sync, the remaining commits after the 2026-11-11 batch through 20f87a421d.
---

# Upstream Sync 2026-11-11 (batch B)

## Scope

- Local base: `main` at `1a70d7df88` (previous batch recorded through
  `f87e1a4b06`)
- Target branch: `sync/upstream-2026-11-11`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `20f87a421daafc618242acb267e88928da2ef72b`
- Live upstream head queried: `43bd5d3cacadd6087c237fe2843f260595b933a7`
- Query time: `2026-11-11`
- Reviewed range: `20f87a421d..64ef8026c7`, 4 commits, oldest first
- New reviewed baseline: `64ef8026c7bc3b29b53c57ebcd6e67a0d9a63ef0`
- Remaining after this batch: `43bd5d3cac`

The five commits after `20f87a421d` end at the 2026-11-11 live head. This report
covers four and stops before `43bd5d3cac`, which is deferred rather than
classified (see the end of this report).

ZZZ keeps upstream's `crates/zed/` files under `crates/zzz/`, so
`crates/zed/src/zed.rs` is read as `crates/zzz/src/zzz.rs`.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                                                                                               |
| ---------- | ----- | ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 5255d9c721 | C     | --           | Skips LM Studio and llama.cpp discovery when `disable_ai` is true. ZZZ has no `crates/language_models`, so neither provider exists locally; the crate that holds `restart_fetch_models_task` and the `ai_disabled` helper is absent.                                                                      |
| 5014e5b302 | C     | --           | Updates the ACP SDK to 5.2.0 and adopts schema 1.17. ZZZ pins `agent-client-protocol = "=5.0.0"`, so the schema 1.17 types this commit is built on are absent locally; the 5.1.0 bump it depends on was already rejected, and its new session references and inline config defaults are `acp-beta`-gated. |
| fdb60b8003 | A     | ebfa54df43   | Right-aligns the selected-repository check by taking `flex_1` on the label. Cherry-picked intact; it applies on the `912159c7ad` port from the previous batch.                                                                                                                                            |
| 64ef8026c7 | B     | 7203f3bdc3   | Resolves new-window placement from the active window. Ported onto ZZZ's split `window/methods.rs` module and its `window_bounds` field naming; the payloads are listed below.                                                                                                                             |

Totals: one `A`, one `B`, two `C` across 4 commits.

## Applied Work

- `ebfa54df43` is the intact cherry-pick of `fdb60b8003`
  (`git cherry-pick -x -s`).
- `7203f3bdc3` is a `B` port of `64ef8026c7`.

Both are signed off for DCO. No remote, pull request, or named upstream remote
was created.

## Per-Commit Narrative

### 5255d9c721 — model discovery under `disable_ai` (C)

The change gates `restart_fetch_models_task` in
`crates/language_models/src/provider/{lmstudio,llama_cpp}.rs` on the
`ai_disabled` helper. ZZZ does not carry the `language_models` crate at all —
local `crates/` has `language_model` and `language_model_core` only — so there
is no discovery task, no provider construction, and no `ai_disabled` helper to
gate.

### 5014e5b302 — ACP SDK 5.2.0 (C)

The commit's own features are built on schema 1.17 types: `ToolCallContent::
Session` rendering, `sessionCapabilities.options`, and inline `configOptions`
on `session/new`. None of those exist in the pinned 5.0.0 SDK, so the port would
have to start by taking the 5.1.0 and 5.2.0 dependency bumps that the 2026-11-10
batch rejected as lockfile churn with no independent local behaviour. The
session-reference rendering also indexes `AcpThread`'s multi-session model,
which ZZZ has not absorbed.

### fdb60b8003 — right-aligned repository check (A)

One line: `Label::new(display_name).single_line().truncate()` gains `.flex_1()`
so the trailing check icon sits at the right edge, matching the other pickers.
The pre-image is exactly the line the previous batch's `912159c7ad` port
produced, so the cherry-pick applied with no fuzz.

### 64ef8026c7 — window placement from the active window (B)

Upstream replaces the `TODO, BUG` in `default_bounds` (which fell back to the
primary display whenever the active window was on the update stack) by
recording each window's bounds and display on the update stack:

- `window_update_stack` becomes `Vec<WindowUpdateEntry>` carrying
  `window_id`, `bounds`, and `display_id`.
- `App::active_display` and `App::active_window_placement` expose the placement,
  and `App::sync_window_update_entry` refreshes the entry from
  `Window::bounds_changed`.
- `Window` gains `last_window_bounds` and `display_id` accessors;
  `WindowBounds::centered` takes an optional display id.
- Wayland reports same-size maximize/fullscreen changes through the `moved`
  callback, and X11 does the same for the maximize-state change it can see
  without a `ConfigureNotify`.

Local divergences the port had to respect:

- `Window::new` and `bounds_changed` live in `crates/gpui/src/window/methods.rs`,
  not `window.rs`, and the bounds field is `window_bounds` rather than
  upstream's `bounds`.
- ZZZ had its own `active_window_bounds` helper that walked
  `App::window_stack()` and skipped windows on the update stack — a workaround
  for the same defect. It became unreachable once the stack carries bounds, so
  it was deleted instead of being kept beside `active_window_placement`.
- `Window::new` already resolved the display from `platform_window.display()`, so
  `display_id` is reused after resolution rather than introduced.
- `crates/zed/src/watcher_debug.rs` does not exist locally and its hunk was
  dropped; `crates/settings_ui/src/pages/audio_test_window.rs` is likewise
  absent, so only the settings editor, profiler, and about windows take the new
  `display_id` argument.

## Deferred

`43bd5d3cac` ("Persist workspace PathStyle to database") was not classified.
Its UI fix reads `RecentWorkspace.path_style`, which the same commit introduces
through a `path_style` column on the `workspaces` table and a `PathListColumns`
wrapper for sqlez's tuple limit. The `crates/recent_projects` half of the diff
also deletes `get_recent_projects` and `delete_recent_project`, refactors ZZZ
never absorbed and still carries. Landing the schema change without those
refactors would persist a column with no read path, which is exactly the
scaffolding-for-later the isolation rules forbid, so the commit stays at the
head of the next range instead of being half-landed or marked rejected.

## Verification

| Check                                                                                       | Result  | Notes                                                                                 |
| ------------------------------------------------------------------------------------------- | ------- | ------------------------------------------------------------------------------------- |
| `cargo check --locked --workspace`                                                          | PASS    | After `7203f3bdc3`; whole workspace in 1m02s.                                         |
| `cargo check --locked -p gpui_linux -p miniprofiler_ui`                                     | PASS    | Platform backends and the profiler call site.                                         |
| `cargo check --locked -p settings_ui -p zzz`                                                | PASS    | Both remaining `WindowBounds::centered` call sites.                                   |
| `cargo test --locked -p gpui --lib test_window_opened_from_bounds_observer_uses_new_bounds` | PASS    | The new regression test: a window opened from a bounds observer takes the new bounds. |
| `cargo fmt --all -- --check`                                                                | PASS    |                                                                                       |
| `git diff --check`                                                                          | PASS    |                                                                                       |
| `cargo xtask check upstream-ledger`                                                         | PASS    | Ledger regenerated with `./script/backfill-upstream-ledger`.                          |
| macOS, Windows, Wayland and X11 runtime behaviour                                           | NOT RUN | This host is Linux; the backend hunks were compiled, not exercised at runtime.        |
