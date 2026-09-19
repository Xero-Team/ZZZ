---
title: Upstream Sync 2026-09-17
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-09-17

## Scope

- Target branch: `sync/upstream-2026-09-17` from `main` at
  `e6adb70968552e53dae959f107df4f8ac03470d9`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previously reviewed baseline: `9bda6b4e0342f22680bc1e7fcd847f4697c48874`
- Reviewed upstream head: `490aad88d5c754e4b0fbbb2bf1e3d6936df27729`
- Live upstream head queried: `74646bf29c1a3d1cdb178930d810ecc5a8a6bece`
- Query time: `2026-09-17T23:13:05+02:00`
- Reviewed range: `9bda6b4e..490aad88`

This batch covered the first 20 commits after the previous baseline. Counts:
4 A, 8 B, and 8 C. The reviewed baseline is now
`490aad88d5c754e4b0fbbb2bf1e3d6936df27729`; 151 commits remain through the
queried live head.

## Decisions

| Upstream | Class | Local commit | Disposition                                                                                          |
| -------- | ----- | ------------ | ---------------------------------------------------------------------------------------------------- |
| 5313741f | C     | --           | Native agent session leak rewrite; `WeakEntry` and native `Thread` storage are absent.               |
| ee3b5558 | C     | --           | ChatGPT subscription usage-limit classification; `ProviderErrorCategory::PaymentRequired` is absent. |
| 22e92168 | C     | --           | Concurrent peer LSP requests through the collab RPC path.                                            |
| 5a9b9558 | A     | 44bc6b09     | Cherry-picked with `-x -s`.                                                                          |
| 1c3aa005 | A     | 2bc9b841     | Cherry-picked with `-x -s`.                                                                          |
| 1870e269 | A     | 91324b04     | Cherry-picked with `-x -s`.                                                                          |
| 69164008 | C     | --           | Community PR-board platform mapping.                                                                 |
| c7801b0c | B     | 6ca31d21     | Honor macOS tiled-window margins for titlebar Fill; omit the extra `is_resizable` guard.             |
| 13e5c99a | B     | 6d2c65e3     | Disable native gpui_web canvas selection on ZZZ's per-property style path.                           |
| 4a217d53 | B     | 70f1e9b7     | Disable Wayland IME when no text input handler is present.                                           |
| 1d25e83f | B     | faab5165     | Honor project panel preview setting for keyboard opens; keep localized Settings UI copy.             |
| b95b188b | B     | 7b3d284c     | Simulate `TestWindow` scale factor; omit nested-window draw tests ZZZ does not have.                 |
| 63d15474 | B     | e121fd30     | Poll worktree root path on a 5s timer; omit the notify fork rev bump.                                |
| 20fa2fa8 | C     | --           | Cross-crate LLVM IR reduction; not one isolatable local behavior.                                    |
| 5bdfa7c8 | B     | f13db0c6     | Use Option instead of Alt in macOS modifier hints; omit absent `git_ui_core` picker.                 |
| e5784305 | C     | --           | `crates/agent_skills` is absent.                                                                     |
| ad51f682 | A     | 181f3bc4     | Cherry-picked with `-x -s`.                                                                          |
| db10a8dd | C     | --           | `GlobalWatcher` testability rewrite spans MultiWorkspace and a later OsWatcher replacement.          |
| 6f73c7d0 | C     | --           | Replaces `GlobalWatcher` with `OsWatcher`; depends on rejected `db10a8dd`.                           |
| 490aad88 | B     | 95a089b4     | Copy full debugger variable values via DAP evaluate clipboard context.                               |

## Applied work

Direct A commits `5a9b9558`, `1c3aa005`, `1870e269`, and `ad51f682` were
absorbed with `git cherry-pick -x -s`.

B ports retain `Upstream`, `Retained`, and `Omitted` trailers:

- `c7801b0c`: titlebar Fill prefers AppKit `_zoomFill:` when the window
  responds.
- `13e5c99a`: gpui_web canvas sets `user-select` and `-webkit-user-select`
  to `none`.
- `4a217d53`: missing Wayland text input handlers no longer keep IME enabled.
- `1d25e83f`: `project_panel::Open` uses
  `preview_tabs.enable_preview_from_project_panel`.
- `b95b188b`: TestWindow stores a scale factor and exposes
  `simulate_scale_factor_change`.
- `63d15474`: the background scanner checks the worktree root every 5s.
  FakeFs file handles look up the current path by inode.
- `5bdfa7c8`: `ui::alt_key_name!` expands to "option" on macOS.
- `490aad88`: Copy Value evaluates `evaluateName` with DAP clipboard
  context, falling back to the trimmed display value.

## Per-commit notes

### 5313741f

Rewrites native agent session storage to hold a weak ACP thread and
mentions `WeakEntry`. ZZZ has no `WeakEntry` API and does not keep the
native `Thread` session table.

### ee3b5558

Classifies ChatGPT subscription `usage_limit_reached` as payment-required
instead of a transient 429. Local `language_model_core` has no
`ProviderErrorCategory::PaymentRequired`.

### 22e92168

Adds concurrent peer LSP request handling on the collab RPC path. Collab
is rejected.

### 69164008

Adds `platform:web` to GitHub community PR-board mapping.

### 20fa2fa8

Binary-size work that adds shared non-generic helpers across gpui, markdown,
rpc, ui, grammars, and other crates. The retained invariant is whole-binary
LLVM IR reduction, not an isolatable local bugfix.

### e5784305

Counts skill description length in `crates/agent_skills`, which is absent.

### db10a8dd, 6f73c7d0

`db10a8dd` refactors `GlobalWatcher` construction across MultiWorkspace,
worktree tests, and fs. `6f73c7d0` then replaces that type with `OsWatcher`.
ZZZ already polls the worktree root on a timer without the notify
`WatchRoot` file-descriptor savings, so this watcher rewrite is not
absorbed.

## Verification

```text
PASS git merge-base --is-ancestor 9bda6b4e FETCH_HEAD
PASS cargo check --locked -p project -p debugger_ui
PASS cargo check --locked -p gpui -p fs -p worktree -p project_panel -p ui -p grammars
PASS cargo test --locked -p debugger_ui --lib test_evaluate_variable_value_uses_clipboard_context
PASS cargo test --locked -p project_panel test_opening_file_with_project_panel_previews_disabled
PASS cargo test --locked -p gpui test_simulate_scale_factor_change
PASS cargo test --locked -p fs test_realfs_executable_metadata
PASS cargo test --locked -p worktree test_root_ancestor_rename_is_detected_without_fs_events
PASS git diff --check
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (known edition-2024 formatting drift on this host)
```

The reviewed baseline is `490aad88d5c754e4b0fbbb2bf1e3d6936df27729`.
Work remains on `sync/upstream-2026-09-17` and has not been merged to `main`.
