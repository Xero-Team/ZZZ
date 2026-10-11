---
title: Upstream Sync 2026-10-10
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-10-10

## Scope

- Target branch: `sync/upstream-2026-10-10`, branched from
  `sync/upstream-2026-10-09` at `85d925f5d0`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `6110aa61e1da78837dd3f91d5323ce43f098779a`
- Reviewed head: `cb981d5ac6ad8f491e86181a7b326342dc069206`
- Live upstream head queried: `93d745668805029476e6d7fbb19652116fed51de`
- Query time: `2026-10-11T10:36:02+02:00`
- Reviewed range: the first 20 commits after `6110aa61`, oldest first
- Remaining after this batch: 28 commits through the queried live head

The previous reviewed baseline is an ancestor of `FETCH_HEAD`. This batch stays
fixed at twenty commits; commits added after the reviewed head remain queued.

## Decisions

| Upstream   | Class | Local commit           | Disposition                                                                                                                 |
| ---------- | ----- | ---------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| 46b11cc550 | C     | --                     | Settings sidebar scrolling needs `container_query`, a GPUI layout API absent from ZZZ.                                      |
| 7ca96c2a58 | C     | --                     | Broad remote secret-transport rewrite across divergent remote, shell, and server modules; no coherent isolatable slice.     |
| 4882254fae | C     | --                     | Batched SSH environment probe depends on the absent local `RemoteEnvironment`, `os_version`, and `home_dir` machinery.      |
| a34a062bb7 | C     | --                     | Free-plan edit-prediction messaging targets the absent `edit_prediction` and `ai_onboarding` crates and Zed's hosted plans. |
| da0a7a78cf | B     | 4f378bb8e5             | Scrollbar tracks round to their parent's corners under a content mask; reveal-policy plumbing omitted.                      |
| cb73ee1d45 | A     | 781e24a085, 902569ccbe | Clean absorption enabling search in the debugger terminal panel; unused import dropped locally.                             |
| 4ce1fdd576 | B     | 4c9055ad46             | Git checkpoints are skipped for threads that cannot restore them; local `UserMessageId` gating retained.                    |
| dc3fb21676 | A     | 8a0eb8be5f             | Already equivalent: ZZZ's `push_markdown_image` maps `TextAlign` to flex justification; added the regression test.          |
| 3914477312 | C     | --                     | ACP v2 compaction sharing needs v2 wire handling absent from ZZZ's negotiated v1 sessions.                                  |
| 4240146813 | B     | 93d34955e5             | Outstanding ACP elicitations are cancelled on transport disconnect through the existing foreground queue.                   |
| 67b41bf976 | C     | --                     | Dependency and lockfile bump of the ACP SDK with no independent ZZZ behavior.                                               |
| f1a10a5227 | C     | --                     | Adds checkpoint-action telemetry to the agent panel.                                                                        |
| 34d68ec5e6 | A     | 66c0828b03             | Clean absorption preventing word duplication when converting case with multiple cursors in one word.                        |
| 19c69782eb | C     | --                     | Dependency and lockfile pin for a `khronos-egl` fork; dependency churn without independent local behavior.                  |
| 20aff31323 | C     | --                     | Updates a fixture in the absent `edit_prediction` crate's test suite.                                                       |
| b0048cde74 | C     | --                     | Browser-only Fetch path in the unshipped web target; also adds example and npm test scaffolding.                            |
| f495dd0f77 | C     | --                     | Adds a broad connected-display API across every GPUI platform backend with no ZZZ caller.                                   |
| 8f41da2520 | C     | --                     | Threads-sidebar theme fix depends on the absent `panel_background_for_overlay` color API.                                   |
| b47a4ca595 | B     | d94369630d             | Branch-picker delete copy distinguishes remote-tracking branches, localized in en and zh-CN.                                |
| cb981d5ac6 | B     | 76575b4fab             | Zero-width LSP hover ranges fall back to the surrounding word and keep the popover stable.                                  |

Totals: three `A`, five `B`, twelve `C`.

## Applied Work

The two direct `A` changes used `git cherry-pick -x -s`. `dc3fb21676` is already
equivalent in product code and adds only its regression test. Each `B` commit
has `Upstream`, `Retained`, and `Omitted` trailers.

- `781e24a085` registers a `BufferSearchBar` on each debugger pane toolbar,
  implements `SubView::as_searchable` for the terminal kind, wraps the running
  state with search action handlers, and refreshes terminal toolbars when the
  terminal entity is set. `902569ccbe` drops the upstream-only `TaskExt` import
  that ZZZ's `gpui` does not export.
- `8a0eb8be5f` adds the upstream HTML-block image-alignment regression against
  ZZZ's renderer, `parse_html` option, and `markdown_image` selector.
- `4f378bb8e5` inherits parent corner radii for scrollbar tracks, paints them
  under a content mask, and sets the final radius and border before installing
  markdown code-block scrollbars. The upstream `reveal_policy` plumbing was
  omitted because ZZZ's `ScrollbarState` has no reveal policy.
- `4c9055ad46` takes a git checkpoint on prompt send only when the thread can
  restore one, so external-agent and subagent sends no longer block. The
  upstream `ClientUserMessageId` helper and `agent_ui` refactor were omitted.
- `93d34955e5` cancels outstanding ACP elicitations on transport EOF or failure,
  including retained idle session threads and the connection-scoped store,
  shared between the on-close callback and the SDK-future failure path.
- `d94369630d` distinguishes remote-tracking branch deletion in the branch
  picker tooltip and button label and localizes the copy in en and zh-CN.
- `76575b4fab` tracks a `FallbackHoverState` so an LSP hover that omits its
  range falls back to the surrounding word and no longer dismisses on small
  cursor moves.

## Rejected Work

- `46b11cc550` needs `container_query`, which ZZZ's `gpui` does not expose, and
  its dry-run cherry-pick conflicts across the local settings layout.
- `7ca96c2a58` is a coordinated remote-secret rewrite spanning `remote`, `util`,
  `remote_server`, and `context_server`; removing its divergent pieces leaves no
  complete, independently testable invariant.
- `4882254fae` batches SSH environment probing through `RemoteEnvironment`, but
  ZZZ removed the `os_version` and `home_dir` discovery this depends on.
- `a34a062bb7` and `20aff31323` target the absent `edit_prediction` and
  `ai_onboarding` crates and promote Zed's hosted free-plan behavior.
- `3914477312` shares ACP v2 compaction updates, but ZZZ's sessions negotiate v1
  and have no v2 transport, so the change would add unused v2 state.
- `67b41bf976` and `19c69782eb` are dependency and lockfile changes: an ACP SDK
  bump and a `khronos-egl` fork pin with no independent ZZZ behavior.
- `f1a10a5227` adds agent-panel telemetry, which ZZZ keeps absent.
- `b0048cde74` fixes a browser Fetch path in the unshipped web target and adds
  example and npm test scaffolding.
- `f495dd0f77` adds a connected-display API across every GPUI backend with no
  current ZZZ caller, so it would import unused public API.
- `8f41da2520` depends on the absent `panel_background_for_overlay` theme color.

## Verification

| Check                                               | Result  | Notes                                                                         |
| --------------------------------------------------- | ------- | ----------------------------------------------------------------------------- |
| Live `git ls-remote` and `git fetch --no-tags`      | PASS    | `FETCH_HEAD=93d745668805029476e6d7fbb19652116fed51de`                         |
| Previous baseline ancestry                          | PASS    | `6110aa61` is an ancestor of `FETCH_HEAD`                                     |
| Debugger terminal search regression                 | PASS    | `debugger_ui` focused test and `cargo check --locked -p debugger_ui` pass     |
| Markdown scrollbar and image-alignment regressions  | PASS    | `cargo check --locked -p markdown -p ui` and focused `markdown` tests pass    |
| ACP checkpoint and elicitation checks               | PASS    | `cargo check --locked -p acp_thread -p agent_servers` passes                  |
| Editor case-conversion and hover regressions        | PASS    | `cargo check --locked -p editor` and focused tests pass                       |
| Branch-picker copy and i18n catalogs                | PASS    | `cargo check --locked -p git_ui`; en and zh-CN key sets match                 |
| `git diff --check` and `cargo fmt --all -- --check` | PASS    | Final source checks                                                           |
| Report Prettier                                     | PASS    | `npx prettier --check src/development/upstream-sync-2026-10-10.md`            |
| Rejection ledger backfill and check                 | PASS    | `./script/backfill-upstream-ledger` and `./script/check-upstream-ledger` pass |
| macOS and Windows runtime checks                    | NOT RUN | Host is Linux                                                                 |
| `cargo test --workspace`                            | NOT RUN | Not requested                                                                 |

The general reviewed baseline advances to
`cb981d5ac6ad8f491e86181a7b326342dc069206`.
