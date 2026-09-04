---
title: Upstream Sync 2026-09-04
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-09-04

## Scope

- Target branch: `sync/upstream-2026-09-04` from `main` at
  `f312d0c2941d32da6a5a5c88010c86866a051e96`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previously reviewed baseline: `5f2d7ad735c266854503c1f40e9b490a8fd0e3a0`
- Reviewed upstream head: `1057c2cf3d5b4aefd04755e1387c7826a4d7fba6`
- Live upstream head queried: `1057c2cf3d5b4aefd04755e1387c7826a4d7fba6`
- Query time: `2026-09-04T07:14:12+02:00`
- Reviewed range: `5f2d7ad7..1057c2cf`

This batch covered the remaining 18 commits after the previous baseline.
Counts: 1 A, 8 B, and 9 C. The reviewed baseline is now
`1057c2cf3d5b4aefd04755e1387c7826a4d7fba6`; no upstream commits remain in
the queried range.

## Decisions

| Upstream | Class | Local commit | Disposition                                                                                               |
| -------- | ----- | ------------ | --------------------------------------------------------------------------------------------------------- |
| b1a7ef0c | B     | a061e169     | Snap recomputed padding to the device pixel grid; cherry-pick conflicted on divergent gpui div tests.     |
| d7b9b385 | A     | b86f40aa     | Cherry-picked with `-x -s`.                                                                               |
| f0d8b0b0 | C     | --           | New `HoverListenerMode` API with no current ZZZ caller once the which-key indicator is rejected.          |
| d85eade7 | C     | --           | Pending-keystrokes indicator needs a GPUI timeout rewrite, unused hover API, and localized settings_ui.   |
| 33375c34 | B     | cd3a2c23     | Commit details use `format_timestamp`; upstream `git_ui/src/git_graph.rs` path is absent.                 |
| 55c0cc36 | B     | 18f12069     | Dismiss the SSH-config Open Folder picker; `RemoteServerPickerDelegate` is absent.                        |
| 801c087a | B     | 0cb520d8     | Terminate git revisions with `--`; omit absent `MergeBaseWithWorktree`.                                   |
| e621c4f4 | B     | 363d3e75     | `active_item_as` falls back to `act_as`; omit MultiWorkspace git-graph test.                              |
| ed8d6004 | B     | 024ae938     | Git panel `HistoryList` key context; keep local ChangesList-specific bindings.                            |
| 73ee8fa0 | C     | --           | Native agent `ask_user` elicitation; the file is absent.                                                  |
| 63e31fb7 | C     | --           | Community PR-board GitHub triage automation.                                                              |
| 3ce72bab | C     | --           | Unused `Hitbox::is_hovered_at` public API with no ZZZ caller.                                             |
| 5b055fa7 | C     | --           | Cloud notification websocket plus telemetry `system_id` refresh.                                          |
| 59b0c714 | C     | --           | GitHub young-account / Zed Business sign-in layout.                                                       |
| 49d01146 | C     | --           | OpenAI 404 retry needs `ProviderRejection`/`is_transient` and native agent `thread.rs`.                   |
| 28e52a28 | B     | 434b0ffc     | Always send OpenCode session headers; omit native-agent/sidebar request wiring.                           |
| 206a863a | C     | --           | gpui_web example-only `run_embedded` migration; no product caller and the local example already diverged. |
| 1057c2cf | B     | 5ffd8afb     | Redact docker exec environment secrets and reject invalid env names.                                      |

## Applied work

Direct A commit `d7b9b385` was absorbed with `git cherry-pick -x -s`.

B ports retain `Upstream`, `Retained`, and `Omitted` trailers:

- `b1a7ef0c`: pixel-snapped padding in `clamp_scroll_position` and a local
  regression test using ZZZ's `draw().clear()` API.
- `33375c34`: commit details now call `format_timestamp` in `crates/git_graph`.
- `55c0cc36`: SSH-config Open Folder confirm and click handlers emit
  `DismissEvent`.
- `801c087a`: log, show, load-commit, diff-tree, and merge-base diff commands
  terminate revisions with `--`; untrusted `--no-ext-diff` stays after the
  subcommand.
- `e621c4f4`: `Workspace::active_item_as` falls back to `act_as` so wrapper
  items such as project diffs resolve to an inner editor.
- `ed8d6004`: Git panel dispatch context adds `HistoryList`, and shared
  list-navigation bindings cover that tab.
- `28e52a28`: OpenCode completions always send `x-opencode-session`.
- `1057c2cf`: docker exec logs redact `-e` values and skip invalid names;
  `containerEnv`/`remoteEnv` are validated before spawn.

## Verification

```text
PASS git merge-base --is-ancestor 5f2d7ad7 FETCH_HEAD
PASS cargo check --locked -p gpui -p git -p git_graph -p git_ui -p workspace -p recent_projects -p language_models -p util -p remote -p dev_container
PASS cargo test --locked -p gpui test_fractional_padding_does_not_make_a_fitting_container_scrollable
PASS cargo test --locked -p git -- branch_named_after_a_path
PASS cargo test --locked -p git test_log_source_terminates_revisions
PASS cargo test --locked -p git test_build_command_untrusted_includes_both_safety_args
PASS cargo test --locked -p git_ui test_dispatch_context_with_focus_states
PASS cargo test --locked -p language_models test_opencode_session_header
PASS cargo test --locked -p remote -- redacts
PASS cargo test --locked -p remote -- skips_invalid_env_names
PASS cargo test --locked -p remote -- uses_podman_cli
PASS cargo test --locked -p remote -- preserves_argument_boundaries
PASS cargo test --locked -p dev_container should_reject_invalid_environment_names
PASS cargo test --locked -p util test_redact_string_with_multiple_env_vars
PASS git diff --check
NOT RUN macOS / Windows / wasm32 runtime tests
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (known edition-2024 formatting drift on this host)
```

The reviewed baseline is `1057c2cf3d5b4aefd04755e1387c7826a4d7fba6`.
Work remains on `sync/upstream-2026-09-04` and has not been merged to `main`.
