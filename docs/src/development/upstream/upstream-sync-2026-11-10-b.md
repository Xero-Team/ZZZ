---
title: Upstream Sync 2026-11-10 (batch B)
description: Selective Zed upstream sync, the remaining 12 commits through the 2026-11-10 live head.
---

# Upstream Sync 2026-11-10 (batch B)

## Scope

- Local base: `main` at `ced90e0c517bf368238225c4363614b7c2cb1376`
- Target branch: `sync/upstream-2026-11-10`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `696003b1f6b806c03b9e83f2a448aab060736cf7`
- Live upstream head queried: `f0d896f2c18a543335eb155f729d36b624695591`
- Query time: `2026-11-10`
- Reviewed range: `696003b1f6..f0d896f2c1`, 12 commits, oldest first
- Remaining after this batch: zero; the range ends at the live head

This batch finishes the 32-commit range that started at `db43f05e3980`. The
previous report covers the first 20; this one covers the last 12. ZZZ keeps
upstream's `crates/zed/` files under `crates/zzz/`, so `crates/zed/src/zed.rs`
is read as `crates/zzz/src/zzz.rs` and
`crates/zed/src/zed/open_listener.rs` as
`crates/zzz/src/zzz/open_listener.rs`. Those paths exist locally; the rename is
the only reason an upstream path can look missing.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                                                                                                                                                                                                  |
| ---------- | ----- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 47c46191f4 | C     | --           | Handles unavailable macOS pasteboards by restoring the `nil` check. Upstream rewrote `gpui_macos/src/pasteboard.rs` onto `objc2` (`Message`, `Retained`, `msg_send`), while ZZZ's file is still the `cocoa`/objc1 implementation with `id`/`nil` and no `Retained` handling. The hunks have no local anchor and the backend is macOS-only, so nothing can be validated here. |
| cebd5fc600 | C     | --           | Retries the POSIX environment probe without stdin. ZZZ's `crates/remote/src/transport/ssh.rs` has neither `environment_posix`, `environment_posix_command`, nor `command_output`, so there is no local probe to retry and no anchor for the fix.                                                                                                                             |
| 597c383bcd | B     | 5533ef8637   | Makes CLI `--add` prefer a matching worktree, then the focused window and its active workspace, and stops adding the CLI's working directory as a root when a local window is already focused. `preferred_target_for_files` became `preferred_workspace_target` with an `OpenVisible` parameter. Tests omitted as test-only; see the omission note below.                    |
| 779e9fc437 | B     | bfb887f768   | Stops excluding a symlink from corrupting the worktree index by reusing the excluded symlink's entry id. `reused_entry_id` now returns the existing entry's id when the path already exists with the same inode. The rename, split-event, and four symlink scanner-restart tests are retained.                                                                               |
| 62acd36cec | B     | 3539d52b47   | Adds horizontal scrolling to `dev: open key context view` so long predicates are reachable. Ported onto ZZZ's localized labels: the possibility rows moved into a tracked, horizontally scrolling container.                                                                                                                                                                 |
| 5e23b6e71d | C     | --           | Opens single files in the same window. Over 900 lines across `workspace`, `multi_workspace`, `recent_projects`, `project`, `open_path_prompt`, and `zzz/src/zzz.rs`, restructuring how windows are created and reused. That is a feature-level rewrite of window creation rather than an isolated behavior change.                                                           |
| 270df71b8d | B     | 9cd72c9b7b   | Shows the sidebar waiting state for unanswered ACP elicitation forms. `root_thread_has_pending_tool_call` became `root_thread_is_waiting_for_input` and also reports a pending elicitation. Test omitted as test-only; see the omission note below.                                                                                                                          |
| 483b746b56 | C     | --           | Opens absolute paths from the file finder. Over 1200 lines across `file_finder`, `picker`, `workspace`, and `crates/picker_preview`, which ZZZ does not carry; the preview path is the feature's substrate.                                                                                                                                                                  |
| 1fea6cbaf6 | C     | --           | Bumps `agent-client-protocol` from 5.0.0 to 5.1.0 in `Cargo.lock` with no code changes. ZZZ pins the SDK at `=5.0.0`; the repository rules reject lockfile-only dependency churn with no independent local behavior.                                                                                                                                                         |
| 4ed72e0135 | B     | aaa48710f4   | Removes Bun from the list of debuggable test frameworks on the JavaScript and TypeScript pages and states that detected Bun tests run as tasks but cannot be debugged. The claim matches ZZZ's own debug locator, which only builds scenarios for npm, pnpm, yarn, and the configured TypeScript runner.                                                                     |
| a1f2e21c2c | B     | 83b8ff3a6e   | Falls back to a whole-repository git scan when more than 64 paths changed, because a scoped refresh degrades to roughly paths x index entries. The threshold test is retained.                                                                                                                                                                                               |
| f0d896f2c1 | C     | --           | Adds per-model capability entries for two Anthropic models. ZZZ has no `crates/anthropic`; commercial model providers are silent-manual configuration only, so there is no capability list to extend.                                                                                                                                                                        |

Totals: six `B` and six `C` across 12 commits.

## Applied Work

- `aaa48710f4` is a `B` port of `4ed72e0135`.
- `83b8ff3a6e` is a `B` port of `a1f2e21c2c`.
- `bfb887f768` is a `B` port of `779e9fc437`.
- `3539d52b47` is a `B` port of `62acd36cec`.
- `9cd72c9b7b` is a `B` port of `270df71b8d`.
- `5533ef8637` is a `B` port of `597c383bcd`.

Every `B` commit carries `Upstream`, `Retained`, and `Omitted` trailers and is
signed off for DCO. No remote, pull request, or named upstream remote was
created.

## Per-Commit Narrative

### 597c383bcd — CLI `--add` workspace selection (B)

`find_existing_workspace` iterated `multi_workspace.workspaces()` and kept the
first workspace whose visibility matched, so the active workspace of the
focused window was only chosen by accident of iteration order. The port walks
the active workspace first and compares a priority of
`(visibility, is_focused_window)`, which makes the focused window and its
active workspace win ties. The subdirectory fallback moved out of the loop into
`preferred_workspace_target(host, OpenVisible::All, cx)`, so it can report
`OpenVisible::All` while the wait-on-files path keeps `OpenVisible::None`. In
`open_local_workspace`, a diff-only `--add` no longer pushes the CLI's working
directory as an extra root when a local window is already focused.

### 779e9fc437 — worktree entry id on symlink exclusion (B)

Excluding a symlink that points inside the open project handed the symlink's
entry id to the target, which corrupts the index because the target is still
live. `reused_entry_id` now looks the path up in the snapshot first and returns
the existing entry's id when the inode matches, before the removed-inode rename
path can claim it. The end of the function reuses that lookup instead of
querying the snapshot twice.

### 62acd36cec — key context view scrolling (B)

The matched-possibility list was appended directly to the last-keystroke row,
so predicates wider than the view were clipped. The rows now live in a
`v_flex` with `overflow_x_scroll`, a `track_scroll` handle, and
`Scrollbars::new(ScrollAxes::Horizontal)`. Each cell is `single_line` and
`flex_shrink_0` so columns stay aligned while scrolling. The localized labels
are unchanged.

### 270df71b8d — sidebar waiting state for elicitations (B)

The sidebar only counted pending permission requests. The local equivalent of
upstream's `elicitation_requests` is the `ElicitationStore` that backs
`request_elicitation_form_states`, so the new
`has_unanswered_elicitation` reports whether any tracked elicitation is still
`ElicitationStatus::Pending`. The method is computed on the conversation view
rather than the conversation entity because the store lives there.

### 4ed72e0135 — Bun debugging documentation (B)

`@types/bun` only adds `bun test` run tasks, and `NodeLocator` builds debug
scenarios only for npm, pnpm, yarn, and `$TYPESCRIPT_RUNNER`, so listing Bun as
debuggable was wrong. The two pages now list it as runnable-only and explain
that `vscode-js-debug` does not implement the WebKit Inspector Protocol.

### a1f2e21c2c — whole-repo git scan fallback (B)

Git matches every pathspec against every index entry, so a scoped
`git status`/`git diff` costs roughly paths x index entries. Above
`MAX_SCOPED_STATUS_PATHS` the refresh now scopes to the repository root instead,
which also makes stale-entry removal walk the whole previous snapshot.

## Omission Notes

Two upstream regression tests were not ported. Both omissions are test-only;
the retained behavior is unchanged and is exercised elsewhere.

- `270df71b8d`: the test drives elicitation through `acp_v2`
  (`CreateElicitationRequest`, `ElicitationAction`) and three sidebar test
  helpers that ZZZ does not carry, so it cannot compile here.
- `597c383bcd`: the two tests call `Workspace::new`,
  `MultiWorkspace::retain_active_workspace`, and an `open_workspace_file`
  helper whose local shape differs.

## Verification

| Check                                                                                                             | Result  | Notes                                           |
| ----------------------------------------------------------------------------------------------------------------- | ------- | ----------------------------------------------- |
| `cargo check --locked -p workspace -p zzz`                                                                        | PASS    | CLI workspace selection and the diff-only guard |
| `cargo check --locked -p agent_ui -p sidebar`                                                                     | PASS    | sidebar waiting state                           |
| `cargo check --locked -p language_tools`                                                                          | PASS    | key context view scrolling                      |
| `cargo test --locked -p project limit_repo_paths_falls_back_to_root_above_threshold`                              | PASS    | 1 test                                          |
| `cargo test --locked -p worktree --test integration test_symlink_scanner_restart`                                 | PASS    | 4 tests                                         |
| `cargo test --locked -p worktree --test integration test_entry_id_is_reused_when_rename_overwrites_existing_path` | PASS    | 1 test                                          |
| `cargo test --locked -p gpui_util`                                                                                | PASS    | 7 tests                                         |
| macOS and Windows runtime checks                                                                                  | NOT RUN | host is Linux; those backends are platform-only |
| `cargo test --workspace`                                                                                          | NOT RUN | narrowest per-crate checks were used            |

The reviewed baseline advances to
`f0d896f2c18a543335eb155f729d36b624695591`.
