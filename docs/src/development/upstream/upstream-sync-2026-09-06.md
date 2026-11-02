---
title: Upstream Sync 2026-09-06
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-09-06

## Scope

- Target branch: `sync/upstream-2026-09-06` from `main` at
  `c32b3602dbdb940eaf523b65f9e7e68aa5c540f7`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previously reviewed baseline: `1057c2cf3d5b4aefd04755e1387c7826a4d7fba6`
- Reviewed upstream head: `9bda6b4e0342f22680bc1e7fcd847f4697c48874`
- Live upstream head queried: `5a9b9558db01a6b906cec2fb70a797affdc58cdd`
- Query time: `2026-09-06T19:16:24+02:00`
- Reviewed range: `1057c2cf..9bda6b4e`

This batch covered the first 20 commits after the previous baseline. Counts:
4 A, 8 B, and 8 C. The reviewed baseline is now
`9bda6b4e0342f22680bc1e7fcd847f4697c48874`; 4 commits remain through the
queried live head.

## Decisions

| Upstream | Class | Local commit       | Disposition                                                                                                       |
| -------- | ----- | ------------------ | ----------------------------------------------------------------------------------------------------------------- |
| 6c365698 | B     | 6fee47e1           | Apply heading-level styles to shaped markdown text; helper ported onto ZZZ's heading path.                        |
| d9141a3f | B     | 36899ce8           | Boost deleted git-gutter marker width; adapt `Option<f32>` instead of `GitGutterWidth`.                           |
| 2988a592 | B     | c9f976aa           | Opt-in cursor movement animation; omit `reduce_motion` and unlocalized Settings UI copy.                          |
| 91c57e81 | A     | edd52c8c           | Cherry-picked with `-x -s`.                                                                                       |
| 80ddc3af | C     | --                 | Organization menu when signed out; account/sign-in surface.                                                       |
| ab7afbff | C     | --                 | `crates/panel` is still used locally via `panel::PanelHeader` in git_ui.                                          |
| 81da416e | A     | a2e38698           | Cherry-picked with `-x -s`.                                                                                       |
| c1eda3e8 | C     | --                 | Markdown preview settings grouping and auto-open rewrite on a already-diverged preview path.                      |
| 9c3e2a07 | B     | c77e95c8           | Project panel path tooltip with `title_tooltip_delay`; Settings UI uses localized `lt()` helpers.                 |
| 52af3dd6 | B     | 2724fbe5, d0175c0e | Continue line-comment prefixes on Vim `o`; drop the rust `/* */` assertion that needs comment overrides.          |
| a7bc7d77 | C     | --                 | GitHub community/guild/triage helper consolidation.                                                               |
| 35695410 | B     | b916f364           | Look up LSP paths before canonicalizing; adapt `RelPathBuf` without `push_component`.                             |
| e2c4075c | C     | --                 | Community PR-board reopen handling.                                                                               |
| 620efaa5 | A     | 774fc801           | Cherry-picked with `-x -s`.                                                                                       |
| ba6b83f8 | C     | --                 | Window-title template rewrite spans collab indicators, MultiWorkspace-divergent workspace.rs, and lockfile churn. |
| 510a624a | A     | bbe67433           | Cherry-picked with `-x -s`.                                                                                       |
| e0ae50d1 | C     | --                 | Unused public `KeyBinding::default_visibility` with no ZZZ caller.                                                |
| c91e24a9 | B     | 9e26873d           | Register Gemini 3.8 Flash on local google_ai APIs; omit thinking-level helpers ZZZ does not have.                 |
| c8c07aea | C     | --                 | Native Agent Panel `disable_ai` menu wiring.                                                                      |
| 9bda6b4e | B     | d2fe7acb           | Disable WebKit canvas touch callouts on ZZZ's per-property gpui_web style path.                                   |

## Applied work

Direct A commits `91c57e81`, `81da416e`, `620efaa5`, and `510a624a` were
absorbed with `git cherry-pick -x -s`.

B ports retain `Upstream`, `Retained`, and `Omitted` trailers:

- `6c365698`: heading-level weight and color now refine the shaped text style
  stack as well as the heading div.
- `d9141a3f`: deleted-line gutter markers keep the default 0.35/0.275 width
  boost when `git_gutter_width` is a small custom value.
- `2988a592`: opt-in `cursor_animation.enabled` springs local bar and block
  cursors. `reduce_motion` gating was omitted because ZZZ removed that API.
- `9c3e2a07`: hovering a project panel entry shows a compact path tooltip.
- `52af3dd6`: Vim `o` continues line-comment prefixes. The rust block-comment
  assertion was dropped because ZZZ's rust language has no comment override
  query.
- `35695410`: worktree lookup uses the LSP path first and only canonicalizes
  after a miss.
- `c91e24a9`: `google_ai` registers `gemini-3.8-flash`.
- `9bda6b4e`: gpui_web canvas sets `-webkit-touch-callout: none`.

## Per-commit notes

### 80ddc3af, c8c07aea

Rejected account and native-agent surfaces. The organization header is gated
on sign-in; the Agent Panel menu item is native-agent UI.

### ab7afbff

Upstream removed `crates/panel` as unused. ZZZ still implements
`panel::PanelHeader` for `GitPanel`, so the crate is not unused locally.

### c1eda3e8

The commit rewrites `markdown_preview_view.rs` (~723 lines), migrator
nesting, theme settings, and workspace restore. Markdown preview has already
diverged; the auto-open path is not isolatable.

### a7bc7d77, e2c4075c

Community/guild/triage GitHub automation. Several workflow files are already
absent locally.

### ba6b83f8

Configurable window titles are philosophy-safe in isolation, but the patch
rewrites `apply_window_title` across a MultiWorkspace-divergent
`workspace.rs`, hard-appends the collab `↗/↙` indicator, and carries
vscode_import, settings_ui, docs, and lockfile hunks. Not isolatable.

### e0ae50d1

Exposes `KeyBinding::default_visibility` with no current ZZZ caller. Unused
public API.

## Verification

```text
PASS git merge-base --is-ancestor 1057c2cf FETCH_HEAD
PASS cargo check --locked -p markdown -p editor -p settings_content -p settings -p settings_ui -p fs -p activity_indicator -p project -p project_panel -p google_ai -p gpui_web
PASS cargo test --locked -p editor cursor_animation --lib
PASS cargo test --locked -p editor test_deleted_marker_base_width
PASS cargo test --locked -p editor test_manipulate_text
PASS cargo test --locked -p fs git_clone_progress
PASS cargo test --locked -p project --test integration test_open_buffer_via_lsp
PASS cargo test --locked -p vim test_o_comment
PASS cargo test --locked -p terminal_view altgr_character_input
PASS cargo test --locked -p settings_json object_replace_escapes
PASS cargo test --locked -p settings_json object_remove_and_rename
PASS cargo test --locked -p google_ai test_gemini_3_8_flash_model_metadata
PASS git diff --check
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (known edition-2024 formatting drift on this host)
```

The reviewed baseline is `9bda6b4e0342f22680bc1e7fcd847f4697c48874`.
Work remains on `sync/upstream-2026-09-06` and has not been merged to `main`.
