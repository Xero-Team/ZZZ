---
title: Upstream Sync 2026-10-24 (b)
description: Selective Zed upstream sync audit, second batch.
---

# Upstream Sync 2026-10-24 (b)

## Scope

- Target branch: `sync/upstream-2026-10-24`, continuing from the first
  batch
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `b7d50b95e8db53607438ef6f5c29d55b7b38f852`
- Reviewed head: `27ea42bf01deeaca66ee778d5eaee1f336708847`
- Live upstream head queried: `79fe6a645f68f8c9230578f154bf99852d9f716f`
- Query time: `2026-10-24T00:52:12+02:00`
- Reviewed range: the next 20 commits after `b7d50b95e8`, oldest first
- Remaining after this batch: 11 commits through the queried live head

The previous reviewed baseline is an ancestor of `FETCH_HEAD`. The range
through the live head contains 31 commits; this batch covers the first 20.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------ |
| 5d47f83542 | B     | 9fcc833c4e   | Adds `on_new_window: readme`. Keeps ZZZ's i18n Settings Editor text and drops duplicate view-menu test helpers.                            |
| 2d6c664dc2 | C     | --           | Caches the macOS main-thread marker on a diverged `gpui_macos`; the Linux host cannot build or verify it.                                  |
| 9b9e7b814a | C     | --           | xtask lint forbidding legacy Objective-C dependencies. Upstream tooling policy with no local behavior or caller.                           |
| 6744a21e2d | C     | --           | Routes ACP subagent associations through v2 subagent registries and thread ownership that ZZZ does not carry.                              |
| 03efd49220 | C     | --           | Button shortcut layout builds on the accessibility aria API and the deleted `project_empty_state` component; not isolatable.               |
| 04470cd9b5 | A     | 85313e6107   | Clean absorption: `remove_worktree` only sends a project update when it removed something.                                                 |
| 389c22e1c7 | C     | --           | Runtime `PlatformInfo` and `Modifiers::secondary(cx)` rewrite across 73 files, including absent `gpui_ios`, `gpui_apple`, and `collab_ui`. |
| 340c703bb2 | B     | 777fb94173   | Duplicate preserves both clipboards; local constructor and `RelPath` adaptation in `74b9a94c34`.                                           |
| 32558ab9d5 | B     | 8f75b5a79a   | Base-side word diffs only for expanded hunks. Regression tests omitted because the upstream test file carries a large formatting rewrite.  |
| fd2aedc471 | C     | --           | Adopts ACP SDK 3.5.0 v2 behavior. ZZZ has a flat v1 `acp.rs` and no v2 integration.                                                        |
| 50304070c2 | A     | 6d4ec0e966   | Clean absorption: pasting in Helix select mode returns to normal mode.                                                                     |
| 566f893ee3 | C     | --           | Handles OpenAI Responses reasoning events in `crates/open_ai`, which ZZZ does not carry.                                                   |
| 574df448ea | C     | --           | Requires `language_models`, `google_ai`, and `opencode` providers that ZZZ does not carry.                                                 |
| f1ccf949d5 | C     | --           | Reuses the iOS cached main-thread marker; `gpui_ios` and `gpui_apple` are absent and the host is Linux.                                    |
| c18e71c782 | B     | e8576e9182   | Fixes the Go test task cwd through the LSP path style; local parent helper in `70a95550ee`.                                                |
| cfb894a5dd | B     | 5a260e2f27   | Normalizes folded watcher keys. Regression tests omitted because ZZZ's watcher test API differs.                                           |
| 9e84b066b5 | C     | --           | Rewrites fs watcher registrations and depends on the absent `fs_watcher/diagnostics.rs`; outside ZZZ's watcher architecture.               |
| 18512de483 | C     | --           | Depends on `reveal_policy` and `should_show_scrollbars` from an unabsorbed scrollbar refactor; the local element has diverged.             |
| fda552edce | B     | 10c19ec7bf   | Indexes agent thread-list metadata; import names adapted to ZZZ.                                                                           |
| 27ea42bf01 | C     | --           | Lockfile-only security refresh with no independent ZZZ behavior.                                                                           |

Totals: two `A`, six `B`, twelve `C` across twenty commits.

## Applied Work

- `85313e6107` is a direct `git cherry-pick -x -s` of `04470cd9b5`.
- `6d4ec0e966` is a direct `git cherry-pick -x -s` of `50304070c2`.
- `9fcc833c4e` plus `5568c3b4b1` is a `B` port of `5d47f83542`.
- `777fb94173` plus `74b9a94c34` is a `B` port of `340c703bb2`.
- `8f75b5a79a` is a `B` port of `32558ab9d5`.
- `e8576e9182` plus `70a95550ee` is a `B` port of `c18e71c782`.
- `5a260e2f27` is a `B` port of `cfb894a5dd`.
- `10c19ec7bf` is a `B` port of `fda552edce`.
- `326a2235b7` formats the synced files.

## Per-Commit Narrative

#### 5d47f83542 — open a README by default (B)

Adds `OnNewWindow::Readme` and `Workspace::open_readme`, plus the setting
defaults, Settings Editor text, docs, and workspace tests. Conflicting
`zzz.rs` test helpers that duplicate ZZZ's view-menu/project-window tests were
dropped, and a follow-up fixed the port to use the local `RelPath`
constructor and async workspace handle. The Settings Editor description keeps
the existing i18n key; both catalogs were updated.

#### 2d6c664dc2 — macOS main-thread marker (C)

Changes `gpui_macos` display, display-link, and platform bindings. ZZZ's macOS
platform layer has diverged, and this Linux host cannot compile or run the
change.

#### 9b9e7b814a — xtask Objective-C lint (C)

Extends `tooling/xtask` to reject legacy Objective-C dependencies. This is
upstream build policy without a local behavior change or caller.

#### 6744a21e2d — ACP subagent routing (C)

Adds `acp_thread/src/subagents.rs`, v1/v2 association handling, and child-thread
UI. ZZZ has no ACP v2 subagent registry or receipt-owned thread model, so the
change cannot be isolated onto the current ACP code.

#### 03efd49220 — project shortcut layout (C)

Reworked button rendering around the accessibility aria API and
`wrap_key_binding`, and modifies `project_empty_state.rs`, which ZZZ has
deleted. The button render path and accessibility surface have diverged too
far for a safe partial port.

#### 04470cd9b5 — skip no-op project updates (A)

`WorktreeStore::remove_worktree` now returns early when no worktree was
removed. This stops a remote client from re-creating and releasing handles for
worktrees it already dropped. The remote editing regression test was included.

#### 389c22e1c7 — runtime platform-aware modifiers (C)

Introduces `PlatformInfo`, makes `Keystroke` parsing/unparsing explicit about
the host OS, and threads it through every platform backend and dozens of
callers. The patch touches absent iOS, Apple, and collaboration crates and
rewrites the keybinding display API, so it is a cross-cutting architectural
change rather than an isolatable fix.

#### 340c703bb2 — preserve clipboard on duplicate (B)

Duplicate now goes through `transfer_entries` directly instead of Copy+Paste,
leaving the system clipboard and pending Copy/Cut untouched. The regression
test needed the local four-argument `ProjectPanel::new`, panel registration,
and `smallvec` import.

#### 32558ab9d5 — collapsed-hunk word diffs (B)

`MultiBufferSnapshot` now checks the actual `DiffTransform::DeletedHunk` before
projecting base-side word diffs, so collapsed hunks cannot emit invalid UTF-8
ranges. The upstream test file contains a multi-thousand-line formatting
rewrite, so the regression tests were omitted; existing word-diff tests pass.

#### fd2aedc471 — ACP SDK 3.5.0 (C)

Adopts v2 typed v1 fallback and termination-aware disconnect handling. ZZZ's
ACP implementation is the flat v1 module without v2 session state, so there is
no local place for the change.

#### 50304070c2 — Helix paste mode (A)

Pasting while in `Mode::HelixSelect` now returns to Helix normal mode. The
existing `test_shift_r_paste` was extended and passes.

#### 566f893ee3 — OpenAI Responses reasoning (C)

Changes `crates/open_ai` and its Responses stream mapper. ZZZ does not carry
that crate.

#### 574df448ea — transport errors in model streams (C)

Touches `google_ai`, `language_models`, and `opencode` provider plumbing, none
of which exist in ZZZ.

#### f1ccf949d5 — iOS main-thread marker (C)

Moves the cached marker into `gpui_apple` and updates `gpui_ios`. Both crates
are absent locally, and the host cannot verify iOS or macOS behavior.

#### c18e71c782 — Go test cwd on Windows (B)

`LspAdapter::client_command` and the code-lens caller now receive the path
style, and gopls test URIs are converted with `to_file_path_ext`. ZZZ's
`PathStyle` lacks the absent path crate's `parent`, so a local Windows/POSIX
parent helper was added and the test uses `PathStyle::Posix`.

#### cfb894a5dd — folded watcher key identity (B)

Folded watch keys now normalize path components before lowercasing, so
trailing separators, repeated separators, and `.` components share one
registration. ZZZ's watcher test module uses the older two-argument
`add`/`dispatch` API, so the two new tests were omitted.

#### 9e84b066b5 — preserve child watches (C)

Rewrites watcher registration removal and adds
`crates/fs/src/fs_watcher/diagnostics.rs`. ZZZ's watcher has a different
registration model and no diagnostics module, so the patch is an
un-isolatable rewrite.

#### 18512de483 — scrollbar settings on open scrollbars (C)

Routes settings changes through a marker global and reworks
`show_scrollbars`/`should_show_scrollbars` around `reveal_policy`. ZZZ's
scrollbar element and show/hide state have diverged and do not carry
`reveal_policy`.

#### fda552edce — agent thread list index (B)

Adds a covering `threads_list_metadata` index and a shared `LIST_THREADS_QUERY`
so listing threads skips the large `data` blobs. The only conflict was the
environment-variable crate name; ZZZ keeps `zzz_env_vars::ZZZ_STATELESS`.

#### 27ea42bf01 — dependency security refresh (C)

Lockfile-only updates with no independent ZZZ behavior. ZZZ's locked
dependency versions have already diverged.

## Verification

| Check                                                                                                      | Result  | Notes                                                 |
| ---------------------------------------------------------------------------------------------------------- | ------- | ----------------------------------------------------- |
| Live `git ls-remote` and `git fetch --no-tags`                                                             | PASS    | `FETCH_HEAD=79fe6a645f68f8c9230578f154bf99852d9f716f` |
| Previous baseline ancestry                                                                                 | PASS    | `b7d50b95e8` is an ancestor of `FETCH_HEAD`           |
| `cargo check --locked` on languages, project_panel, multi_buffer, fs, agent, vim, workspace, zzz `--tests` | PASS    | all touched crates                                    |
| `cargo test --locked -p remote_server --lib test_removing_missing_worktree_does_not_send_project_update`   | PASS    | 1 test                                                |
| `cargo test --locked -p project_panel --lib test_duplicate_preserves_clipboards`                           | PASS    | 1 test                                                |
| `cargo test --locked -p multi_buffer --lib test_word_diff`                                                 | PASS    | 5 tests                                               |
| `cargo test --locked -p languages --lib test_go_test_task_template_cwd`                                    | PASS    | 1 test                                                |
| `cargo test --locked -p fs --lib watch`                                                                    | PASS    | 16 tests                                              |
| `cargo test --locked -p agent --lib db`                                                                    | PASS    | 12 tests                                              |
| `cargo test --locked -p vim --lib helix::test::test_shift_r_paste`                                         | PASS    | 1 test                                                |
| `cargo fmt --check`                                                                                        | PASS    | clean                                                 |
| `git diff --check main..HEAD`                                                                              | PASS    | clean                                                 |
| `./script/check-philosophy`                                                                                | PASS    | philosophy gate                                       |
| Locale key parity (`en.json` vs `zh-CN.json`)                                                              | PASS    | 3659 keys each; one value updated                     |
| macOS builds and runtime checks                                                                            | NOT RUN | host is Linux                                         |
| Windows builds and runtime checks                                                                          | NOT RUN | host is Linux                                         |
| `cargo test --workspace`                                                                                   | NOT RUN | not requested                                         |
| Remaining commits through live head                                                                        | PENDING | 11 commits; next baseline starts after `27ea42bf01`   |

The reviewed baseline advances to
`27ea42bf01deeaca66ee778d5eaee1f336708847` for this batch.
