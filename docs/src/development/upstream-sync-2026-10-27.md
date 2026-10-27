---
title: Upstream Sync 2026-10-27
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-10-27

## Scope

- Local base: `main` at `4d3b091b22`
- Target branch: `sync/upstream-2026-10-27`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `79fe6a645f68f8c9230578f154bf99852d9f716f`
- Reviewed head: `eee7501970fabf711e1bec286ba4cf2a05586b3f`
- Live upstream head queried: `24a2823456f16e36bf32df510fb5c43fee7a2565`
- Query time: `2026-10-27T15:33:43+01:00`
- Reviewed range: the first 20 commits after `79fe6a645f`, oldest first
- Remaining after this batch: 30 commits through the queried live head

The previous reviewed baseline is an ancestor of `FETCH_HEAD`. The range
through the live head contains 50 commits; this batch covers the first 20
as the configured batch size.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                                                         |
| ---------- | ----- | ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 02a04e06e2 | C     | --           | Clears replayed ACP v2 subagent state. The `agent_servers` v1/v2 split and the `acp_v2` module are absent locally, and `AcpThread` has no subagent registry.                                        |
| b0cac499da | C     | --           | Orders ACP v1 load responses and replays subagent history. The whole patch is in the absent `agent_servers/src/acp/v1.rs`.                                                                          |
| 536ec0ea9d | C     | --           | Refuses config changes and reopening for ACP v2 subagents. Adds `AcpSession::is_subagent` fed by `apply_subagent_update`, which is absent locally.                                                  |
| c07b87624c | C     | --           | Applies the subagents RFD to cancellation. ZZZ's `AcpThread` has a diverged request lifecycle with no subagent association or `can_cancel` grant.                                                   |
| a8acb01f21 | C     | --           | Read-only ACP subagent configuration spans `config_options.rs` and the v2 connection; both build on the absent subagent registry.                                                                   |
| d93bf74085 | C     | --           | Renders ACP subagents as inline cards through a new `subagent_card.rs` and v2 session updates; the local conversation view loads subagents as separate threads, so the card layout is unisolatable. |
| af4948a164 | C     | --           | Follow-up to the read-only subagent configuration in `agent_servers/src/acp/v2.rs`, which does not exist locally.                                                                                   |
| 347f171a3f | B     | e8602c82d9   | Parses file positions from the original string so separators survive. The `thread_view` test hunks were omitted; that test does not exist locally.                                                  |
| 18f9d7e731 | B     | 845628ab79   | Redirects the pwsh environment probe through `/dev/fd/0`. The ssh half targets an absent `RemoteEnvironment` probe.                                                                                 |
| 035e6ac2f3 | C     | --           | Buffers ACP v2 subagent traffic sent before `session/new` resolves in `acp/v2.rs` and `session_updates.rs`; both are absent locally.                                                                |
| ad8a62df82 | C     | --           | Treats replayed subagent outcomes as history on the v1/v2 connection pair and the absent subagent registry.                                                                                         |
| c8995a085a | C     | --           | Skips tool-call-referenced session loading where the subagents RFD applies; it reads the v2 association model that is absent locally.                                                               |
| 78f5a535a3 | C     | --           | Keeps inline review on the root thread when inline subagent cards appear, which is the diverged card layout rejected above.                                                                         |
| c5efa10ddd | C     | --           | Derives a subagent titlebar status from its latest run; `AcpThread::subagents` does not exist locally.                                                                                              |
| a5c4ca8bf6 | C     | --           | Offers ACP v1 subagents behind `acp-beta`. The subagent offering path and beta-flag rollout are absent locally.                                                                                     |
| 876c1b0c5f | B     | a0f9127da3   | Removes project group rows that no retained workspace backs, so they stop reappearing in Recent Projects. The regression test was adapted to local helpers.                                         |
| 08cbb88a15 | B     | 76ff13b989   | Sizes the git graph canvas and gutter through layout instead of the prepaint-cached width. Ported into `crates/git_graph`, where ZZZ keeps the graph.                                               |
| 08104dc649 | B     | ce08e0391b   | Adds commit detail tooltips in the same moved file. The commit message disclosure tooltip was omitted; ZZZ has no collapsible message section there.                                                |
| e645f02245 | C     | --           | Skips the GPG askpass wrapper when GPG is missing. ZZZ's `askpass` has no GPG wrapper or `find_gpg_program`, so there is nothing to make quiet.                                                     |
| eee7501970 | C     | --           | Removes Agent checkpoints across `acp_thread`, `agent`, `git`, and `project`. ZZZ keeps them for thread restore and worktree archives; the removal is unisolatable on the diverged thread model.    |

Totals: zero `A`, five `B`, fifteen `C` across twenty commits.

## Applied Work

- `e8602c82d9` is a `B` port of `347f171a3f`.
- `845628ab79` is a `B` port of `18f9d7e731`.
- `a0f9127da3` is a `B` port of `876c1b0c5f`.
- `76ff13b989` is a `B` port of `08cbb88a15`.
- `ce08e0391b` is a `B` port of `08104dc649`.

No commit in this batch was a clean `A`; every candidate needed either a
moved file, a local API shape, or an i18n pass before it fit.

## Per-Commit Narrative

#### 347f171a3f — path separators in file positions (B)

`PathWithPosition::parse_str` now rebuilds the parsed path with
`replace_file_name`, which splices the replacement onto the original input
string, instead of calling `Path::with_file_name`, which inserts the host
separator. This keeps `/home/...` and `\\server\...` paths intact when the
editor runs on Windows. The cherry-pick conflicted twice: the local regex
initializer uses `expect` rather than `unwrap`, and the local
`conversation_view/thread_view.rs` has no markdown-link position tests. The
`util/src/paths.rs` behavior and its separator regression test were ported;
the thread-view test updates were dropped as test-only.

#### 18f9d7e731 — pwsh env detection (B)

`capture_unix` now calls a shared `capture_output_redirection` helper, and
the helper redirects both `PowerShell` and `Pwsh` through `> /dev/fd/0`.
The previous `>{}` form produced `>0` for pwsh, which is not a descriptor
redirect, so pwsh startup output leaked into the captured environment. The
pwsh regression test was retained. The `remote/src/transport/ssh.rs` half
was omitted: ZZZ has no `RemoteEnvironment`, `posix_script`, or
stdin-fed remote probe, so `command_output` and
`environment_posix_command` would have had no caller.

#### 876c1b0c5f — unloaded project group rows (B)

`MultiWorkspace::remove_project_group` computed the group's workspaces and
then delegated to `remove`, which returns `Ok(false)` for an empty workspace
list. A group whose workspaces were opened in another window therefore
stayed in `project_groups` and reappeared in Recent Projects. The port adds
the upstream early return: drop the group record, emit
`ProjectGroupsChanged`, serialize, and notify. The regression test was
adapted to ZZZ's `MultiWorkspace::test_new` plus `open_sidebar` setup,
which is what seeds the initial project group locally.

#### 08cbb88a15 — git graph resizing (B)

The graph canvas and ref-label gutter were sized from
`RedistributableColumnsState::cached_container_width`, which is only updated
during prepaint, so closing the commit details panel kept the previous
frame's width until an unrelated re-render. The port deletes
`graph_viewport_width`, drops the gutter's `.min(viewport / 2.)` in favor of
`max_w(relative(0.5))`, and lets the canvas fill the column with
`.w_full().min_w(graph_canvas_content_width())`. Upstream keeps this file in
`crates/git_ui`; ZZZ keeps it in `crates/git_graph`, so the patch was
replayed by hand. The regression test is included.

#### 08104dc649 — git graph tooltips (B)

Two of the three tooltips apply locally: the commit details close button,
using `Tooltip::for_action_in` with the graph's focus handle so the binding
matches what works in the graph, and the "View on" button. Both labels are
localized through `i18n::tr` with new `git_graph.close_details_panel` and
`git_graph.open_in_browser` keys in `en.json` and `zh-CN.json`. The commit
message disclosure tooltip was omitted because ZZZ's commit details panel
has no `toggle-commit-message` disclosure.

#### eee7501970 — remove checkpoints (C)

The commit deletes the Agent checkpoint feature: `git` loses
`checkpoint.gitignore` and most of its repository checkpoint API, `project`
drops `checkpoints_by_work_dir_abs_path`, and `acp_thread` loses over a
thousand lines. ZZZ uses checkpoints for thread restore
(`ThreadView::restore_checkpoint`) and for worktree archives
(`create_archive_checkpoint` / `restore_archive_checkpoint`), so absorbing
this removal would delete a feature ZZZ ships, and the `acp_thread` half
would be unisolatable on top of the diverged v2 thread model rejected above.

#### e645f02245 — GPG askpass wrapper (C)

`generate_gpg_wrapper_script` returns `Ok(None)` when no `gpg` binary is on
`PATH`, so an absent GPG stops producing a warning. ZZZ's `askpass` has no
GPG wrapper at all: there is no `find_gpg_program`, no
`GPG_WRAPPER_SCRIPT_NAME`, and no wrapper script generation, so the quiet
path has no local counterpart to attach to.

## Verification

| Check                                                                       | Result  | Notes                                               |
| --------------------------------------------------------------------------- | ------- | --------------------------------------------------- |
| Live `git ls-remote` and `git fetch --no-tags`                              | PASS    | `FETCH_HEAD=24a2823456`                             |
| Previous baseline ancestry                                                  | PASS    | `79fe6a645f` is an ancestor of `FETCH_HEAD`         |
| `cargo test --locked -p util --lib path_with_position`                      | PASS    | 6 tests                                             |
| `cargo test --locked -p util --lib shell_env`                               | PASS    | 2 tests                                             |
| `cargo test --locked -p workspace --lib test_remove_unloaded_project_group` | PASS    | 1 test                                              |
| `cargo test --locked -p git_graph --lib test_graph_canvas_resizes`          | PASS    | 1 test                                              |
| `cargo check --locked -p git_graph --all-targets`                           | PASS    | tooltips and locale keys                            |
| `cargo check --locked --workspace --all-targets`                            | PASS    | full workspace after all five ports                 |
| `cargo fmt --check` (touched crates)                                        | PASS    | clean                                               |
| `git diff --check`                                                          | PASS    | clean                                               |
| `./script/check-philosophy`                                                 | PASS    | philosophy gate                                     |
| Locale key parity (`en.json` vs `zh-CN.json`)                               | PASS    | 2 new keys in both catalogs                         |
| macOS / Windows runtime checks                                              | NOT RUN | host is Linux                                       |
| `cargo test --workspace`                                                    | NOT RUN | not requested                                       |
| Remaining commits through live head                                         | PENDING | 30 commits; next baseline starts after `eee7501970` |

The reviewed baseline advances to
`eee7501970fabf711e1bec286ba4cf2a05586b3f` for this batch.
