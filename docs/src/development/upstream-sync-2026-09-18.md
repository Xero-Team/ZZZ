---
title: Upstream Sync 2026-09-18
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-09-18

## Scope

- Target branch: `sync/upstream-2026-09-18` from `sync/upstream-2026-09-17`
  (originally `main` at `e6adb70968552e53dae959f107df4f8ac03470d9`)
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previously reviewed baseline: `d7f28899166cead457a2239ad23056d0a980bd7c`
- Reviewed upstream head: `251854020a9dbea1a388bf392c9bd04fd136a557`
- Live upstream head queried: `650a8d1bedaca3f841f1c11f89fa786f574c2aab`
- Query time: `2026-09-18T20:44:51+02:00`
- Reviewed range: `d7f28899..25185402`

This batch covered the first 20 commits after the previous baseline. Counts:
3 A, 5 B, and 12 C. The reviewed baseline is now
`251854020a9dbea1a388bf392c9bd04fd136a557`; 123 commits remain through the
queried live head.

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

The reviewed baseline is `251854020a9dbea1a388bf392c9bd04fd136a557`.
Work remains on `sync/upstream-2026-09-18` and has not been merged to `main`.
