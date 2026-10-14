---
title: Upstream Sync 2026-10-13
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-10-13

## Scope

- Target branch: `sync/upstream-2026-10-13`, branched from `main` at
  `f3832cb0ad`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `7eccdcecb5c5fe150a9e94f98b55af30b7d486b6`
- Reviewed head: `36682547bea1c3d4fa60c08d341cf60081853941`
- Live upstream head queried: `36682547bea1c3d4fa60c08d341cf60081853941`
- Query time: `2026-10-13T21:17:42+02:00`
- Reviewed range: the twenty-six commits after `7eccdcecb5`, oldest first
- Remaining after this batch: 0 commits through the queried live head

The previous reviewed baseline is an ancestor of `FETCH_HEAD`. The range is
longer than the twenty-commit batch because upstream advanced by twenty-six
commits since the last query; no commits remain behind the live head.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                         |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------- |
| 6802ba72f0 | C     | --           | Automatic fold unfolding on jumps; touches the deleted `editor/src/navigation.rs` and upstream-only selection APIs. |
| 4905e67333 | C     | --           | Fixes a permission in a first-contribution GitHub workflow; community automation ZZZ does not ship.                 |
| f929b3f7ce | C     | --           | Reimplements `PathStyle::strip_prefix` on the absent `crates/path` `Components` API; unisolatable refactor.         |
| 725573ec14 | C     | --           | Native-agent command lifetime; no ACP-only caller and ZZZ has no `NativeAgent`.                                     |
| c66312b200 | C     | --           | Prepares shared ACP v2 receipt lifecycle for native agents; no local caller.                                        |
| ff52936d53 | C     | --           | Decouples process output from ACP v1; ZZZ is still entirely ACP v1.                                                 |
| dd85cf3677 | C     | --           | Renames shared APIs to ACP v2 metadata spelling; absent protocol variant.                                           |
| 735bc10fd0 | C     | --           | Pairs for local terminal environments under the v2 rewrite.                                                         |
| 10644801ad | C     | --           | Config-driven modes and defaults for ACP v2 agent servers.                                                          |
| 26363aabdb | C     | --           | Relicenses `extension_suggest`; that crate is absent in ZZZ.                                                        |
| 8d541bcc9b | C     | --           | Docs fix for Zed-specific links and release automation prose; describes surfaces ZZZ does not ship.                 |
| 8231a43e45 | A     | 661155ed55   | Tab switcher no longer changes selection on mouse hover; conflicting pick resolved onto ZZZ's mode enum.            |
| ef376006f3 | C     | --           | Adds a `tab_switcher.show_file_preview` setting; depends on the upstream `selected_file_project_path` shape.        |
| 1773013397 | C     | --           | Normalizes ACP v2 protocol errors; no v2 caller in ZZZ.                                                             |
| db1de83a2c | C     | --           | Scopes message IDs by message kind under the v2 rewrite.                                                            |
| 745c322dab | C     | --           | Routes v2 message updates into shared threads; absent protocol and thread architecture.                             |
| b35ff6b252 | C     | --           | Routes v2 tools and plans into shared state.                                                                        |
| 60b0983f18 | C     | --           | Binds v2 terminals to common session ingress.                                                                       |
| cb13b37a6d | B     | c3e64e93d9   | SVG preview context menu item, ported onto ZZZ's `RelPath`-based panel and preview APIs.                            |
| 1de16662ee | C     | --           | Windows-only cpal default-device fix; ZZZ's `resolve_device` has a different shape and no Windows host here.        |
| 3ec36860d2 | C     | --           | Routes v2 session info and command updates.                                                                         |
| d3267a39ff | A     | 231c15fdf7   | Agent terminal scrollbar placement and radius; conflicting pick resolved onto ZZZ's terminal view.                  |
| 05d5e10362 | C     | --           | Documents `agent.subagent_model`; that setting and the native agent panel are absent.                               |
| 077458d949 | C     | --           | Anthropic Sonnet/Haiku 5.5 support; `crates/anthropic` is absent in ZZZ.                                            |
| a63db9fd90 | C     | --           | Routes v2 notices and compaction updates.                                                                           |
| 36682547be | C     | --           | Routes v2 configuration and usage updates.                                                                          |

Totals: two `A`, one `B`, twenty-three `C` across twenty-six commits.

## Applied Work

- `661155ed55` is a direct `git cherry-pick -x -s` of `8231a43e45`. The dry-run
  conflicted only in `TabSwitcher::new` and `Render`, where ZZZ's
  `TabSwitcherMode` enum and its `picker` field naming differ; the resolution
  kept the upstream revert of the popover delay by deleting the local
  `_show_task` and `visible` fields entirely and restoring the single
  `.child(self.picker.clone())` render. The first-row `debug_selector` hunk was
  re-applied against ZZZ's `index` variable instead of the upstream `ix`.
- `231c15fdf7` is a direct `git cherry-pick -x -s` of `d3267a39ff`. The
  conflicts were resolved onto ZZZ's terminal view: the upstream `read_only`,
  `show_workspace_actions`, and `MouseInputMode` hunks were dropped because ZZZ
  has no such terminal fields, and `Rems::ZERO` was replaced with
  `Rems::default()` because ZZZ's `gpui::Rems` has no `ZERO` constant.
- `c3e64e93d9` is a `B` port of `cb13b37a6d`.

## Rejected Work

- `6802ba72f0` unfolds folded sections when jumping to a location. Its
  cherry-pick conflicts on `crates/editor/src/navigation.rs`, which ZZZ deleted
  when it consolidated editor navigation into `editor.rs`; the remaining
  conflicts are in `selection.rs` and `selections_collection.rs`, where the
  upstream adds `SelectionEffects::unfold`, `preserve_buffer_positions`, and a
  `resolve_selections(.., expand_blocks)` split. ZZZ keeps the older
  `resolve_selections_wrapping_blocks` shape with no
  `resolve_selections_without_display_round_trip` parameter, so the folding
  behavior cannot be isolated without importing the whole selection-resolution
  rework and restoring a deleted module.
- `4905e67333` edits `.github/workflows/welcome_first_contribution.yml`;
  community automation ZZZ keeps absent.
- `f929b3f7ce` reimplements `PathStyle::strip_prefix` on `Components`, which
  lives in the absent `crates/path` crate, and updates every caller around the
  new `RelPath` construction chain. ZZZ has `util::rel_path::RelPath` and a
  `&str`-based `PathStyle`, so neither the refactor nor its caller updates can
  be isolated.
- `725573ec14`, `c66312b200`, `ff52936d53`, `dd85cf3677`, `735bc10fd0`,
  `10644801ad`, `1773013397`, `db1de83a2c`, `745c322dab`, `b35ff6b252`,
  `60b0983f18`, `3ec36860d2`, `a63db9fd90`, and `36682547be` are one continuous
  migration of the native agent and ACP layer to protocol v2
  (`acp_v2::TerminalExitStatus`, shared receipt lifecycle, `session_updates`,
  `message_updates`). ZZZ has no `crates/agent/src/agent.rs` `NativeAgent`, no
  `crates/anthropic`, and every ACP crate still imports
  `agent_client_protocol::schema::v1 as acp`, so none of these introduce a
  reachable local caller. Each would also require the preceding commit in the
  stack.
- `26363aabdb` relicenses `crates/extension_suggest` from GPL-3.0 to
  Apache-2.0. ZZZ has no `extension_suggest` crate, so there is nothing to
  relicense.
- `8d541bcc9b` fixes broken links in Zed's docs, including
  `docs/src/development/release-notes.md` prose about Zed's preview release
  script and links to `zed-industries` repositories and the Zed extension
  registry. The links describe upstream infrastructure rather than behavior ZZZ
  ships.
- `ef376006f3` adds a `tab_switcher.show_file_preview` setting by teaching
  `TabSwitcherDelegate::set_selected_index` to consult a new
  `TabSwitcherSettings`. It stacks on `8231a43e45` and additionally assumes the
  upstream `selected_file_project_path` / `selected_file` shape. Only the
  hover-revert half of that stack was isolatable.
- `1de16662ee` works around a Windows cpal default-device failure by resolving
  `device.id()` through `host.device_by_id`. ZZZ's `resolve_device` takes an
  `AudioDeviceKind` and returns the default device directly, so the concrete
  endpoint resolution can be added, but the change is Windows-only and cannot be
  exercised on this Linux host; it is recorded as `C` rather than ported blind.
- `d3267a39ff` landed as a clean `A` after conflict resolution and is not
  listed here.
- `05d5e10362` documents `agent.subagent_model` and reasoning effort for
  subagents. ZZZ has neither that setting nor the native agent panel it
  describes.
- `077458d949` adds Claude Sonnet 5.5 and Haiku 5.5 support to
  `crates/anthropic` and `crates/copilot_chat`. Both crates are absent in ZZZ,
  and `copilot_chat` is a rejected account-bound route.

## Verification

| Check                                                       | Result  | Notes                                                          |
| ----------------------------------------------------------- | ------- | -------------------------------------------------------------- |
| Live `git ls-remote` and `git fetch --no-tags`              | PASS    | `FETCH_HEAD=36682547bea1c3d4fa60c08d341cf60081853941`          |
| Previous baseline ancestry                                  | PASS    | `7eccdcecb5` is an ancestor of `FETCH_HEAD`                    |
| `cargo check --locked -p tab_switcher`                      | PASS    | hover-revert A pick                                            |
| `cargo test --locked -p tab_switcher`                       | PASS    | 12 tests pass, including `test_open_item_on_modifiers_release` |
| `cargo check --locked -p terminal_view -p agent_ui`         | PASS    | terminal scrollbar A pick                                      |
| `cargo test --locked -p terminal_view`                      | PASS    | 69 tests pass                                                  |
| `cargo check -p svg_preview -p project_panel --all-targets` | PASS    | SVG preview B port                                             |
| `cargo test --locked -p project_panel` SVG preview          | PASS    | `test_open_svg_preview_for_single_file_worktree` passes        |
| `git diff --check` and `cargo fmt --all -- --check`         | PASS    | only pre-existing `crates/project` baseline drift remains      |
| macOS and Windows runtime checks                            | NOT RUN | host is Linux                                                  |
| `cargo test --workspace`                                    | NOT RUN | not requested                                                  |

The general reviewed baseline advances to
`36682547bea1c3d4fa60c08d341cf60081853941`.
