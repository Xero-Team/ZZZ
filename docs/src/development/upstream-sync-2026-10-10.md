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

## Continued Batch

### Scope

- Target branch: `sync/upstream-2026-10-10`, continuing the same local base
  `85d925f5d0`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `cb981d5ac6ad8f491e86181a7b326342dc069206`
- Reviewed head: `df48a139ce9c292738b523252c12f044697e673f`
- Live upstream head queried: `93d745668805029476e6d7fbb19652116fed51de`
- Query time: `2026-10-11T11:05:57+02:00`
- Reviewed range: the next 20 commits after `cb981d5a`, oldest first
- Remaining after this batch: 8 commits through the queried live head

The previous baseline is an ancestor of `FETCH_HEAD`. This batch stays fixed at
twenty commits; the later commits remain queued.

### Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                      |
| ---------- | ----- | ------------ | -------------------------------------------------------------------------------------------------------------------------------- |
| 9ab0715969 | C     | --           | Upgrades the ACP SDK to 3.2.0 and registers sessions through prepared-request and cancellation handles; ZZZ pins 2.2.0.          |
| 95d35c6efd | C     | --           | Creates the absent `gpui_apple` crate and moves `SurfaceSource`, `PaintSurface`, and `paint_surface` to `objc2-core-video`.      |
| 4f6d97b9de | C     | --           | Frame and hang telemetry measured against per-window refresh rates; needs the absent `hang_telemetry` crate and emits telemetry. |
| d058ddbbc1 | C     | --           | Restyles the collaboration panel, which ZZZ does not ship.                                                                       |
| b01d137ad7 | B     | c77eb6949e   | Text finder escapes selection-seeded raw text in regex mode; the `QuerySeed` move into `project_search.rs` was omitted.          |
| 679ad7c304 | A     | a69f8f4c4e   | Clean absorption fixing `uniform_list` item layout far down long lists.                                                          |
| a2a92fe7c4 | B     | ac53c1c55c   | `gpui_platform` forwards `gpui_web`'s `multithreaded` feature; the conflicting manifest pick was resolved onto ZZZ's crate list. |
| 7230a5acc5 | B     | 6370af054a   | Markdown hit testing defers the inter-line gap fallback so later table cells still win; the wrapped-table suite was omitted.     |
| 9f94d13160 | B     | c01da21cb6   | Markdown preview stores nav history so going back returns to it, and Vim binds `ctrl-o`/`ctrl-i`; the test was rewritten.        |
| 41dfc9953b | A     | f8bd0f94df   | Clean absorption bumping the managed Node runtime to v24.21.0.                                                                   |
| f0b38ac75e | C     | --           | Lockfile-only dependency bump with no independent ZZZ behavior.                                                                  |
| 1c9dd3e70f | C     | --           | Moves shared agent state to ACP v2 session and tool IDs; ZZZ has no `acp_v2` usage.                                              |
| 017c1398fd | C     | --           | Drops the min height of the upstream `default_picker`; ZZZ's remote-project picker is a different localized modal.               |
| e778eec483 | C     | --           | Introduces a Zed-owned `ToolCallLocation` over `acp_v2` location types absent from ZZZ.                                          |
| 089abd6917 | C     | --           | Adds Bedrock models and provider wiring for the absent `bedrock` crate.                                                          |
| 90d8514059 | A     | 791f068ec4   | Clean absorption fixing blurred window backgrounds on macOS 27.                                                                  |
| 93b4e5edec | C     | --           | Releases edit-prediction files in the absent `edit_prediction` crate.                                                            |
| 129c68ff4b | C     | --           | Edit-prediction subscription messaging plus `client` plan helpers for Zed's hosted organizations.                                |
| efed1f245a | C     | --           | FIM stop-sequence handling in the absent `edit_prediction` crate.                                                                |
| df48a139ce | C     | --           | Adds Claude Opus 5.5 BYOK support for the absent `bedrock` crate.                                                                |

Totals: three `A`, four `B`, thirteen `C`.

### Applied Work

The three direct `A` changes used `git cherry-pick -x -s`. Each `B` commit has
`Upstream`, `Retained`, and `Omitted` trailers.

- `a69f8f4c4e` computes the visible range and each item origin in `f64` before
  converting the small offsets back to `Pixels`, so `uniform_list` no longer
  drifts far down long lists.
- `f8bd0f94df` bumps the managed Node runtime from v24.11.0 to v24.21.0.
- `791f068ec4` uses the `UnderWindowBackground` visual-effect material and sets
  the behind-window blending mode, restoring blur on macOS 27.
- `ac53c1c55c` gives `gpui_platform` a `multithreaded` feature (on by default)
  that forwards to `gpui_web`, and declares `gpui_web` without default features
  in the workspace.
- `c77eb6949e` tags each text-finder seed as query syntax or raw text and
  escapes raw selections in regex mode. The enum lives in `text_finder.rs`
  because ZZZ's project search never absorbed the earlier shared refactor.
- `c01da21cb6` records a `MarkdownPreviewView` nav-history entry on
  deactivation and binds `ctrl-o`/`ctrl-i` in the `MarkdownPreview` Vim
  context.
- `6370af054a` records the first inter-line gap and keeps scanning, so a
  position inside a later overlapping table cell hits its text before the gap
  fallback.

### Rejected Work

- `9ab0715969`, `1c9dd3e70f`, and `e778eec483` all build on ACP v2 and the ACP
  SDK 3.x prepared-request API. ZZZ pins `agent-client-protocol = 2.2.0` and its
  `acp_thread` has no `acp_v2` usage, so the v2 session, tool-ID, and location
  models have no local transport.
- `95d35c6efd` creates the absent `gpui_apple` crate and changes GPUI's public
  surface API to `objc2-core-video`; ZZZ's `gpui_macos` keeps the `core-video`
  shape.
- `4f6d97b9de` depends on the absent `hang_telemetry` crate and is frame and
  hang telemetry, which ZZZ keeps absent.
- `d058ddbbc1` targets the collaboration panel, which is not shipped.
- `089abd6917` and `df48a139ce` add models and provider wiring for the absent
  `bedrock` crate.
- `93b4e5edec`, `129c68ff4b`, and `efed1f245a` target the absent
  `edit_prediction` crate; `129c68ff4b` also adds `client` subscription and
  organization plan helpers for Zed's hosted plans.
- `f0b38ac75e` is a lockfile-only bump with no independent local behavior.
- `017c1398fd` removes the min height from the upstream `default_picker`; ZZZ's
  remote-project picker is a different localized modal with no such component,
  so there is no equivalent line to change.

### Verification

| Check                                                                | Result  | Notes                                                                          |
| -------------------------------------------------------------------- | ------- | ------------------------------------------------------------------------------ |
| Live `git ls-remote` and `git fetch --no-tags`                       | PASS    | `FETCH_HEAD=93d745668805029476e6d7fbb19652116fed51de`                          |
| Previous baseline ancestry                                           | PASS    | `cb981d5a` is an ancestor of `FETCH_HEAD`                                      |
| `cargo check --locked -p gpui -p gpui_platform -p node_runtime`      | PASS    | `uniform_list`, web feature, and Node-version ports                            |
| `cargo check --locked -p gpui_macos`                                 | PASS    | macOS-27 blur port compiles; runtime NOT RUN                                   |
| `cargo check --locked -p search` and `cargo test --locked -p search` | PASS    | `text_finder` regex-escape unit test and focused suite pass                    |
| `cargo check --locked -p markdown_preview --tests` and focused test  | PASS    | `go_back_returns_to_preview_after_opening_another_file` passes                 |
| `cargo check --locked -p markdown` and gap non-regression test       | PASS    | `test_hit_testing_outside_text_preserves_source_indices` passes                |
| `cargo fmt --all -- --check` and `git diff --check`                  | PASS    | Final source checks                                                            |
| Report Prettier                                                      | PASS    | `npx prettier --check src/development/upstream-sync-2026-10-10.md`             |
| Rejection ledger backfill and check                                  | PASS    | `./script/backfill-upstream-ledger` and `./script/check-upstream-ledger` pass  |
| `markdown` table hit-testing tests                                   | NOT RUN | Four table tests already panic on the clean baseline; not caused by this batch |
| macOS and Windows runtime checks                                     | NOT RUN | Host is Linux                                                                  |
| `cargo test --workspace`                                             | NOT RUN | Not requested                                                                  |

The general reviewed baseline advances to
`df48a139ce9c292738b523252c12f044697e673f`.
