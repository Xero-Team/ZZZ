---
title: Upstream Sync 2026-09-18
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-09-18

## Scope

- Target branch: `sync/upstream-2026-09-18` from `sync/upstream-2026-09-17`
  (originally `main` at `e6adb70968552e53dae959f107df4f8ac03470d9`)
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previously reviewed baseline: `87a1ea30e819e193f8e3fcb517c6884f91ee7a9c`
- Reviewed upstream head: `45ff33717b6e539d0d2f2123b50c4224b84bd961`
- Live upstream head queried: `72b060af2e901406f9cf4a050c4f30bb479103fe`
- Query time: `2026-09-18T22:15:18+02:00`
- Reviewed range: `87a1ea30..45ff3371`

The first batch on this branch reviewed `d7f28899..25185402` (3 A, 5 B, 12
C). The second covered `25185402..87a1ea30` (5 A, 5 B, 10 C). This third
batch covered the next 20 commits. Counts: 3 A, 3 B, and 14 C. The
reviewed baseline is now `45ff33717b6e539d0d2f2123b50c4224b84bd961`; 86
commits remain through the queried live head.

## Decisions

| Upstream | Class | Local commit | Disposition                                                                                       |
| -------- | ----- | ------------ | ------------------------------------------------------------------------------------------------- |
| 77226930 | C     | --           | Trial copy for the $5 GPT Luna offer; onboarding, plan chips, and hosted-docs billing.            |
| 284c7240 | C     | --           | Document-highlight dynamic registration needs upstream `dynamic_registration.rs`.                 |
| e9d2934e | A     | 24ae4df8     | Cherry-picked with `-x -s`.                                                                       |
| a3e93fff | B     | eabe3cdc     | wasi-sdk 34 with VERSION invalidation; omit bail-to-ensure log churn.                             |
| 52b2927a | A     | 1db3e67a     | Cherry-picked with `-x -s`.                                                                       |
| f9a1fc89 | C     | --           | New `watch::snapshot` public API with no current ZZZ caller.                                      |
| 26b6a267 | C     | --           | Upstream GitHub Actions / xtask Miri pin.                                                         |
| d3865b09 | B     | 8ea5c20a     | macOS path prompts on objc2; `MainThreadMarker::new()` instead of a platform tuple field.         |
| 290cbcb9 | B     | 02a1fd0e     | Remote `path_to_buffer_id` update on file move; omit conflicting telemetry test.                  |
| 6ad3c7f2 | C     | --           | gpui_web keyboard-focus rewrite; `ime_mirror.rs` and related GPUI APIs are absent.                |
| 9e636045 | B     | add60ecd     | Inline assistant uses `default_model()` fallback.                                                 |
| 86b2cf96 | C     | --           | Screen-capture `get_sources` needs new `objc2-screen-capture-kit` and `MacPlatform` marker field. |
| 595d6286 | B     | 552f00b3     | `comment_empty_lines` on `editor.rs`; omit deleted `input.rs` and vscode.json keymaps.            |
| 7e0b34ba | C     | --           | Anthropic `ProviderErrorCategory::PaymentRequired` is absent.                                     |
| 72c53bf0 | C     | --           | `start_external_drag` GPUI API is absent.                                                         |
| 3405c42f | C     | --           | PET lockfile-only fork pin; local microsoft rev already diverged.                                 |
| 5a773a40 | C     | --           | gpui_web IME autoscroll; `ime_mirror.rs` is absent.                                               |
| a57ba9b1 | A     | 7620cf28     | Cherry-picked with `-x -s`.                                                                       |
| 1a84d5d9 | C     | --           | Folder-drag highlight fix needs absent external-drag APIs and new `on_file_drop_exit`.            |
| 25185402 | C     | --           | Native Agent Panel terminal-thread renaming.                                                      |
| dfec59fb | C     | --           | Filesystem debug window needs upstream `OsWatcher`; ZZZ still uses `GlobalWatcher`.               |
| d9e1c024 | C     | --           | Long-press tooltips need absent `LongPressEvent` / `GestureTuning` GPUI APIs.                     |
| 6fff327a | C     | --           | rustc 1.98.1 bump; prior 1.97 bump was C, plus collab Docker and flake lock.                      |
| 1c3d902f | B     | 40a7f609     | Continue diagnostics past paths with no worktree; adapt tests to local `Diagnostic.message`.      |
| 3cef3168 | C     | --           | ACP 2.1 / futures 0.3.34; ZZZ is still on agent-client-protocol 0.12.0.                           |
| f0261834 | B     | ef800257     | `editor.code_lens.foreground` on `Option<String>` theme colors.                                   |
| 7ff8e1c2 | A     | --           | Already equivalent: local ashpd is 0.13.10, newer than the 0.13.5 mailto fix.                     |
| d12e456b | B     | 0b35237d     | Raise Unix `RLIMIT_NOFILE` at startup; omit diverged `prevent_root_execution` copy.               |
| fffb52c6 | C     | --           | Zed Pro upgrade prompts and payment-error telemetry.                                              |
| 5b4a2153 | B     | 69ef4cd7     | `all_font_names` lists platform families only, without fallback / `.SystemUIFont`.                |
| a936ce01 | C     | --           | Upstream `.zed/settings.json` inherit syntax for native-agent eval fixtures.                      |
| fb38178d | B     | d8ae5c81     | Headless Metal `render_scene_to_image` runs in an autorelease pool.                               |
| d2074f4e | C     | --           | `PowerRequest` UAF fix; ZZZ `gpui_windows` has no power-request path.                             |
| a9cdfc99 | A     | --           | Already equivalent: migrator tests already use raw strings / `unindent`, not `indoc`.             |
| 1a89a92e | C     | --           | Unpaced renderer sessions rewrite a much larger upstream `bench_context`.                         |
| 3db02c2c | C     | --           | New unused `Window` visibility / system-sleep APIs intended for hang telemetry.                   |
| a7c7219d | A     | 948e47eb     | Cherry-picked with `-x -s`.                                                                       |
| 9d272b03 | C     | --           | GitHub Actions / xtask macOS SDK printing for release bundling.                                   |
| a2651e3b | A     | 22627461     | Cherry-picked with `-x -s`.                                                                       |
| 87a1ea30 | A     | d9e431df     | Cherry-picked with `-x -s`.                                                                       |
| d27fa556 | C     | --           | Outline-panel deleted-file tree rewrite; 13 conflicts plus local i18n.                            |
| 7960b2a7 | C     | --           | gpui_web Canvas font fallback; `font_weight_and_style` has no ZZZ caller.                         |
| 250b6581 | A     | a6d0420e     | Cherry-picked with `-x -s`.                                                                       |
| bf921d03 | B     | 69a2938f     | Commit-editor Cut/Copy/Paste menu; omit test that needs `simulate_next_frame`.                    |
| 3e442f25 | C     | --           | First-class ACP tool-call names need ACP 2.1; ZZZ is on 0.12.0.                                   |
| d89e9c21 | C     | --           | Native agent elicitation tool-call IDs.                                                           |
| d1dae815 | C     | --           | ACP compaction capability and native-agent compaction rewrite.                                    |
| 25b5569d | B     | 211a9225     | `project_panel::OpenContextMenu`; omit absent external-drag GPUI imports.                         |
| f50ebf29 | C     | --           | Tool-name fallbacks depend on unabsorbed first-class ACP names.                                   |
| 7cda6f05 | A     | c07e4bd2     | Cherry-picked with `-x -s`.                                                                       |
| f792c2d7 | C     | --           | New unused `ShapedLineCursor` public API.                                                         |
| 47b8ea58 | C     | --           | Reveal-in-panel needs `Item::active_project_path` and missing DiffMultibuffer.                    |
| d62802d4 | C     | --           | Trunk README link exists only on the unabsorbed gpui_web gallery rewrite.                         |
| 59d996d8 | C     | --           | Per-request output-limit scaffolding; Copilot, cloud, native agent, no ZZZ setter.                |
| cbffa0f5 | C     | --           | `snapshot_with_edits` / `EditedBufferSnapshot` are absent.                                        |
| 9e6e1416 | C     | --           | Dynamic font APIs plus gpui_web; depends on Canvas fallback.                                      |
| b68add5b | C     | --           | Mutex removal depends on unabsorbed dynamic font loading.                                         |
| 7f00507e | A     | 1aabd210     | Cherry-picked with `-x -s`.                                                                       |
| ba7da93e | B     | 56bd8252     | Drop stale settings keys; keep omitted/`none` provider copy instead of Zeta.                      |
| 45ff3371 | C     | --           | Recent-commands UX follow-up; conflicts across picker, settings, and vscode import.               |

## Applied work

Direct A commits `e9d2934e`, `52b2927a`, and `a57ba9b1` were absorbed with
`git cherry-pick -x -s`.

B ports retain `Upstream`, `Retained`, and `Omitted` trailers:

- `a3e93fff`: Extension grammar compilation and `download-wasi-sdk` install
  wasi-sdk 34 and invalidate stale caches via `VERSION`.
- `d3865b09`: `MacPlatform` path open/save panels use objc2 `NSOpenPanel` /
  `NSSavePanel` and block2.
- `290cbcb9`: Remote `UpdateBufferFile` removes the previous path from
  `path_to_buffer_id`.
- `9e636045`: Inline assistant uses `LanguageModelRegistry::default_model()`.
- `595d6286`: `editor::ToggleComments` gains `comment_empty_lines` (default
  true) on the `editor.rs` path.

## Per-commit notes

### 77226930

Updates trial onboarding, plan definitions, Zed cloud provider copy, and
hosted plans-and-pricing docs for a GPT Luna trial offer.

### 284c7240

Advertises and handles `textDocument/documentHighlight` dynamic registration.
ZZZ's `DynamicRegistrations` only tracks watched files and diagnostics. The
selector-aware text-document registration module is absent, so advertising the
capability without handling it would be incorrect.

### f9a1fc89, 26b6a267

Snapshot watch channels are a new unused API. The Miri pin is GitHub Actions
and xtask workflow infrastructure.

### 6ad3c7f2, 5a773a40

gpui_web keyboard focus and IME autoscroll both touch `ime_mirror.rs`, which
ZZZ does not have. The keyboard rewrite also spans GPUI test/window APIs.

### 86b2cf96, 72c53bf0, 1a84d5d9

Screen-capture `get_sources` adds `objc2-screen-capture-kit` and needs a
`MainThreadMarker` field on `MacPlatform`. External drag and the project-panel
highlight follow-up require `start_external_drag` / `on_file_drop_exit`.

### 7e0b34ba, 3405c42f, 25185402

Anthropic credit exhaustion maps to an absent `PaymentRequired` category.
Python PET is a lockfile-only switch to the zed-industries fork at a rev
ZZZ does not share. Terminal-thread renaming is native Agent Panel.

## Verification

```text
PASS git merge-base --is-ancestor d7f28899 FETCH_HEAD
PASS cargo check --locked -p sqlez -p extension -p language_model -p recent_projects
PASS cargo check --locked -p editor -p project -p gpui_macos -p remote_server
PASS cargo test --locked -p extension --lib test_installed_wasi_sdk_version
PASS cargo test --locked -p editor --lib test_toggle_comment
PASS cargo test --locked -p editor --lib test_advance_downward_on_toggle_comment
PASS cargo test --locked -p project --test integration test_completion_label
PASS git diff --check
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (known edition-2024 formatting drift on this host)
```

The first-batch reviewed baseline was
`251854020a9dbea1a388bf392c9bd04fd136a557`.

## Batch 2 applied work

Direct A commits `a7c7219d`, `a2651e3b`, and `87a1ea30` were absorbed with
`git cherry-pick -x -s`. Already-equivalent A commits `7ff8e1c2` and
`a9cdfc99` made no local code change.

B ports retain `Upstream`, `Retained`, and `Omitted` trailers:

- `1c3d902f`: `merge_diagnostic_entries` continues past paths with no
  worktree instead of aborting the batch.
- `f0261834`: CodeLens text uses `editor.code_lens.foreground` and falls
  back to `text_muted`.
- `d12e456b`: `util::increase_open_file_limit` raises `RLIMIT_NOFILE`
  toward 10240 on macOS and 65536 on other Unix.
- `5b4a2153`: `TextSystem::all_font_names` returns sorted unique platform
  families, including `add_fonts`.
- `fb38178d`: headless Metal `render_scene_to_image` runs inside
  `objc::rc::autoreleasepool`.

## Batch 2 per-commit notes

### dfec59fb

Adds `OsWatcher` diagnostics recording and a 929-line debug window. ZZZ's
`fs_watcher` still uses `GlobalWatcher` with separate native/poll backends,
so the snapshot APIs do not isolate.

### d9e1c024

Long-press tooltip activation dispatches `LongPressEvent` and uses
`GestureTuning`. Those GPUI types are absent.

### 6fff327a, 3cef3168, 9d272b03

The rustc 1.98.1 bump includes collab Dockerfiles and depends on the
rejected 1.97 bump. ACP 2.1 is not portable onto local 0.12.0. macOS SDK
printing is release-workflow infrastructure.

### fffb52c6, a936ce01, 3db02c2c

Payment errors still drive Zed Pro upgrade UI and telemetry. Project
settings inherit syntax targets native-agent eval fixtures. Window
visibility / `on_system_sleep` have no product caller; the intended
consumer is hang telemetry.

### d2074f4e, 1a89a92e

Windows `PowerRequest` does not exist locally. Unpaced renderer sessions
rewrite `bench_context` far beyond the local bench helper.

## Batch 2 verification

```text
PASS git merge-base --is-ancestor 25185402 FETCH_HEAD
PASS cargo check --locked -p project -p settings_content -p theme
PASS cargo check --locked -p theme_settings -p editor -p util
PASS cargo check --locked -p gpui -p gpui_wgpu -p gpui_linux
PASS cargo test --locked -p project --test integration test_diagnostic_batches_skip_paths_without_worktrees
PASS cargo test --locked -p theme_settings --lib code_lens_foreground
PASS cargo test --locked -p gpui_wgpu --lib all_font_names_tracks_available_families
PASS cargo test --locked -p editor --lib test_gutter_button_tooltip
PASS cargo test --locked -p gpui_linux --lib drag
PASS git diff --check
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (known edition-2024 formatting drift on this host)
NOT RUN cargo check -p gpui_macos (cfg(target_os = "macos") on this Linux host)
```

The reviewed baseline is `87a1ea30e819e193f8e3fcb517c6884f91ee7a9c`.
Work remains on `sync/upstream-2026-09-18` and has not been merged to
`main`.

## Batch 3 applied work

Direct A commits `250b6581`, `7cda6f05`, and `7f00507e` were absorbed with
`git cherry-pick -x -s`.

B ports retain `Upstream`, `Retained`, and `Omitted` trailers:

- `bf921d03`: commit-message editors get Cut/Copy/Paste context menus.
  Local follow-up `eaeeaf09` drops the regression test.
- `25b5569d`: `project_panel::OpenContextMenu` deploys the menu at the
  selected entry, with Menu / Shift-F10 bindings.
- `ba7da93e`: settings reference drops `features`,
  `edit_prediction_provider`, `agent_font_size`, and
  `projects_online_by_default`.

## Batch 3 per-commit notes

### d27fa556

Rewrites `outline_panel.rs` (+4489/-1263) to synthesize `ProjectEntryId`s
for deleted files. Cherry-pick hit 13 conflicts plus local i18n.

### 7960b2a7, 9e6e1416, b68add5b, d62802d4

Canvas fallback, dynamic font installation, and the mutex follow-up are
gpui_web plus unused public APIs. The Trunk README fix only exists on
that unabsorbed gallery rewrite.

### 3e442f25, d89e9c21, d1dae815, f50ebf29

First-class tool-call names and ACP compaction need ACP 2.1
(`tool_call.name`, `CompactionCapabilities`). Elicitation IDs are native
agent. Tool-name fallbacks depend on the names commit.

### 47b8ea58, cbffa0f5, f792c2d7, 59d996d8

Reveal-in-panel needs `Item::active_project_path` and missing
`DiffMultibuffer` / `StagedDiff`. Diff highlighting needs
`snapshot_with_edits`. `ShapedLineCursor` has no caller. Output-limit
plumbing is scaffolding for later Copilot/cloud/native-agent PRs.

### 45ff3371

Follow-up to unabsorbed recent-commands UX. Conflicts in picker,
`settings_content`, vscode import, and settings UI.

## Batch 3 verification

```text
PASS git merge-base --is-ancestor 87a1ea30 FETCH_HEAD
PASS cargo check --locked -p gpui -p project_panel
PASS cargo check --locked -p project -p agent_ui -p agent_servers -p git_ui
PASS cargo test --locked -p project_panel --lib test_context_menu
PASS cargo test --locked -p project_panel --lib test_panel_keeps_focus_highlight
PASS git diff --check
NOT RUN cargo test -p git_ui test_commit_editor_context_menu_clipboard_actions (omitted; needs simulate_next_frame)
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (known edition-2024 formatting drift on this host)
```

The reviewed baseline is `45ff33717b6e539d0d2f2123b50c4224b84bd961`.
Work remains on `sync/upstream-2026-09-18` and has not been merged to
`main`.
