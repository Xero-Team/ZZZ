---
title: Upstream Sync 2026-08-27
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-08-27

## Scope

- Target branch: `sync/upstream-2026-08-27` from `main` at
  `2761d2445eca441bc5948e8d35bebb01274496cf`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Reviewed upstream head: `4c7244790a075e862eeb4e5ccc12d6c8f5da6f7e`
- Live upstream head queried: `8166e3d7b8b42d8aaf4d4dee7fcd25ab4ec65105`
- Query time: `2026-08-27T23:25:05+02:00`
- Requested range starts after `aa3718614b3ade75524be6f8b2e101bd1166e02c`

`A` is a complete safe absorption or an already-equivalent local change.
`B` needs a local equivalent port or further API review and is deliberately not
claimed as synchronized unless a local commit is listed. `C` is rejected by
ZZZ's local-first, no-account, ACP-only boundary, or failed isolation.

The reviewed baseline is now `4c7244790a075e862eeb4e5ccc12d6c8f5da6f7e`.
Default batch was the first 20 commits after `aa3718614b`. 155 upstream commits
remain after this head. Counts: 3 A, 6 B, 11 C.

## Decisions

| Upstream | Class | Local commit | Disposition                                                     |
| -------- | ----- | ------------ | --------------------------------------------------------------- |
| 83f3a8e3 | C     | --           | ChatGPT Subscription Responses transport; `HttpSend` absent.    |
| 78712609 | A     | 3510b24e     | Cherry-picked with `-x -s`.                                     |
| 4bdf188c | C     | --           | Stash tracked/staged options; git_panel and git.proto conflict. |
| 2893b86b | B     | dcf64063     | macOS simple fullscreen covering the notch.                     |
| 1274a5dc | B     | 4d7ead54     | VS Code npm task `path` property.                               |
| cf08569e | B     | 62714f97     | `file_scan_exclusions` `"..."` splice.                          |
| fd5cd939 | B     | 5c0060f4     | Markdown loose-list task markers.                               |
| fdad9186 | C     | --           | `git_ui_core` askpass files are absent.                         |
| 05473ed8 | C     | --           | `csv_preview` rename; local crate already diverged.             |
| dbc90d18 | C     | --           | Depends on rejected `05473ed8` crate rename.                    |
| a7d74150 | C     | --           | Settings UI Default/Custom needs absent `PixelSetting`.         |
| 03c9c4e7 | C     | --           | `ParsedSvg` / `render_parsed` APIs are absent.                  |
| 87324045 | C     | --           | Lockfile-only `async-tar` fork for Cursor ACP download.         |
| 3624a5bf | C     | --           | Depends on rejected `6dee3fc7` diagnostic proto.                |
| 35f63e40 | B     | 0322d76b     | Markdown code-block `buffer_line_height`.                       |
| 2040e0de | C     | --           | `spawn_dedicated` / scheduler rewrite plus lockfile churn.      |
| 00c0e96e | C     | --           | `LoadedFile` Rope rewrite conflicts on local worktree decode.   |
| aad75630 | A     | 1baf13a1     | Cherry-picked with `-x -s`.                                     |
| 3f660a0a | B     | 3abceceb     | Drop deprecated `std::usize` / `std::u32` / `std::u64` imports. |
| 4c724479 | A     | 17ec7f5a     | Cherry-picked with `-x -s` after `aad75630`.                    |

## Applied Work

The work branch contains the listed A/B local commits. Every direct upstream
commit was created with `git cherry-pick -x -s`; B commits retain their full
`Upstream:` trailer and explain omissions. No remote branch, pull request, or
upstream remote was created.

## Per-commit notes

### B ports

- `2893b86b`: `fullscreen_mode`, GPUI simple-fullscreen APIs, macOS
  borderless implementation, title-bar padding, and `ToggleFullScreen`
  routing. Omitted agent_ui, sidebar, settings_ui page metadata, and
  `all-settings.md`.
- `1274a5dc`: npm `path` as worktree-relative cwd, with `options.cwd`
  still winning. Omitted gulp/shell test-file split; kept
  `typescript.json`.
- `cf08569e`: `SplicingVec` so `"..."` extends inherited
  `file_scan_exclusions`. Omitted agent-tool production hunks and docs.
  Test assignments adapted to `SplicingVec::from`.
- `fd5cd939`: look through a wrapping paragraph for task markers. Kept
  local `ToggleState::Selected` / `Unselected`.
- `35f63e40`: fenced code blocks use relative `buffer_line_height`.
  Omitted render tests that need diverged preview helpers.
- `3f660a0a`: dropped the deprecated integer prelude imports on the
  local files that still had them.

### Representative C

Philosophy: ChatGPT Subscription Responses transport, Cursor ACP tar
fork.

Missing architecture: `RequestError::HttpSend` / `compact_response`,
`git_ui_core`, `ParsedSvg`, `PixelSetting`, `spawn_dedicated`.

Unisolatable conflicts: git_panel + git.proto stash UI, csv_preview
rename, diagnostic related-info proto from `6dee3fc7`, worktree
`LoadedFile` Rope rewrite on a file that already has a local 6GB
workaround for the same peak-memory issue.

## Verification

```text
PASS git merge-base --is-ancestor aa3718614b FETCH_HEAD
PASS cargo check --locked -p anthropic -p language_models
PASS cargo test --locked -p anthropic --lib list_models_preserves_anthropic_api_errors
PASS cargo test --locked -p task -- can_deserialize_npm_tasks
PASS cargo test --locked -p settings_content --lib test_file_scan_exclusions
PASS cargo test --locked -p markdown --lib test_task_list_marker_for_item
PASS cargo check --locked -p settings_content -p settings -p worktree
PASS cargo check --locked -p gpui -p workspace -p markdown -p task
PASS cargo check --locked -p gpui_macos -p platform_title_bar
PASS cargo check --locked --tests -p zzz
PASS cargo check --locked --tests -p worktree -p agent -p project_panel -p anthropic
NOT RUN macOS / Windows / wasm32 gpui_web runtime
NOT RUN cargo test --workspace
NOT RUN cargo fmt --check (edition-2024 let-chain rustfmt errors on this host)
```

The reviewed baseline is `4c7244790a075e862eeb4e5ccc12d6c8f5da6f7e`.
Work remains on `sync/upstream-2026-08-27` and has not been merged to
`main`.
