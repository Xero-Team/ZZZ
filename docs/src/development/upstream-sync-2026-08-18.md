---
title: Upstream Sync 2026-08-18
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-08-18

## Scope

- Target branch: `sync/upstream-2026-08-18` from `main` at
  `14646e7843f37ad399edfcbf1344642f10b76f99`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Reviewed upstream head: `aa3718614b3ade75524be6f8b2e101bd1166e02c`
- Live upstream head queried: `aa3718614b3ade75524be6f8b2e101bd1166e02c`
- Query time: `2026-08-18T05:25:43+02:00`
- Requested range starts after `027cf0def75e5c027504f402a6a6c0dcac11f178`

`A` is a complete safe absorption or an already-equivalent local change.
`B` needs a local equivalent port or further API review and is deliberately not
claimed as synchronized unless a local commit is listed. `C` is rejected by
ZZZ's local-first, no-account, ACP-only boundary, or failed isolation.

The reviewed baseline is now `aa3718614b3ade75524be6f8b2e101bd1166e02c`.
The first fetch was depth-1 and made `027cf0def7` look like a non-ancestor;
`--shallow-since=2026-08-06` restored ancestry. Counts: 29 A, 17 B, 70 C
(after the 2026-09-22 re-audit ported ten commits).

## Decisions

| Upstream | Class | Local commit | Disposition                                                    |
| -------- | ----- | ------------ | -------------------------------------------------------------- |
| 417d0330 | B     | f7e177b8     | Web RAF wakes only on demand.                                  |
| 4ed3738c | B     | a910a8e7     | Phase-locked repeating spinners.                               |
| 38ca9106 | B     | 3efaf5c6     | Hover listeners after layout.                                  |
| 65308f40 | B     | bad00395     | Gemini 3.6 Flash.                                              |
| 3b90a9be | C     | --           | Timed RPC log groups need absent elapsed tracker.              |
| d2779c34 | A     | 53146bd0     | Cherry-picked with `-x -s`.                                    |
| 803467f3 | C     | --           | Text finder regex highlight needs missing picker preview API.  |
| be52d3b7 | A     | e438687e     | Cherry-picked with `-x -s`.                                    |
| e9b5778e | A     | e49339cc     | Cherry-picked with `-x -s`.                                    |
| 08827f92 | B     | 5b0d95cd     | `terminal.starts_open`.                                        |
| 59b2ebf1 | A     | de45206d     | Cherry-picked with `-x -s`.                                    |
| 371a7d4b | C     | --           | `crates/lsp_locations` is absent.                              |
| 1271f8b0 | C     | --           | rustc 1.97 bump is 66-file lockfile/agent/collab churn.        |
| f99aad78 | C     | --           | Guild labeling workflow.                                       |
| 492acd6c | C     | --           | Process-group revert unisolatable from local terminal.         |
| 0812d210 | C     | --           | GPT-5.6 Sol default mixed with subscribed provider.            |
| 4e8057d7 | C     | --           | CI assignee workflow.                                          |
| d4010e91 | C     | --           | Project-panel undo trash confirm unisolatable.                 |
| 4bd19937 | C     | --           | Mermaid `~~~` fences; local markdown diverged.                 |
| e4671f71 | C     | --           | Malformed-task toast conflicts in `zzz.rs`.                    |
| 069449ab | C     | --           | mdBook smart-punctuation docs infra.                           |
| c65e08a8 | A     | c17d2ed1     | Cherry-picked with `-x -s`.                                    |
| 9e236090 | C     | --           | Unused per-window histogram APIs.                              |
| a1860ac1 | C     | --           | Hosted Claude pricing docs.                                    |
| 6634c945 | B     | 2260d67f9b   | Git panel copy-path actions; ported after review.              |
| bd1b83a4 | B     | b4c301e099   | Git panel collapsible sections; ported after review.           |
| c6b01d8a | B     | a3c4dd2111   | Optional git stash message; ported after review.               |
| 7807e4b1 | A     | acfaeb99     | Cherry-picked with `-x -s`.                                    |
| a4916265 | B     | 89c1511729   | OpenAI reasoning-summary separators; ported after review.      |
| c83adb3d | C     | --           | `SymbolKind` RPC; `language_core` conflict.                    |
| d71f1461 | C     | --           | `stacksafe` lockfile-only bump.                                |
| 992c7d46 | B     | 4a20d45f0a   | GitHub release request timeout; ported after review.           |
| 83dc1967 | B     | 7b4c1a46f9   | Worktree ignore-rule anchoring; ported after review.           |
| c0979ee0 | C     | --           | OpenAI subscribed default model.                               |
| c7537bdf | C     | --           | Brand-writer marketing skill rename.                           |
| daec37bd | A     | 61450f38     | Cherry-picked with `-x -s`.                                    |
| 6bd93fc3 | C     | --           | OpenAI subscribed context windows.                             |
| a3d65153 | C     | --           | Encrypted-content error factored for native agent thread.      |
| 315ea374 | C     | --           | OpenAI subscribed Codex limits.                                |
| 1c9cbd3b | C     | --           | Native agent terminal shrink.                                  |
| 6ae52316 | C     | --           | Extension proto version/lockfile only.                         |
| 897ba9ad | C     | --           | Remote markdown images; preview/agent_ui diverged.             |
| fdf5de99 | A     | b50c1326     | Cherry-picked with `-x -s`.                                    |
| b13f6c71 | B     | 9e40f452     | Follow upstream app version to 1.17.0.                         |
| cdf33ac2 | C     | --           | Markdown table autosize needs unused gpui API + lockfile.      |
| a034d870 | C     | --           | Canceled worktree leak; tests conflict.                        |
| 52894d3f | C     | --           | `filterText` completions; `editor/src/completions.rs` deleted. |
| 770a977c | C     | --           | On-type formatting; deleted editor modules.                    |
| bc463bc2 | A     | 2ab1c18b     | Cherry-picked with `-x -s`.                                    |
| 93f6b2e5 | C     | --           | Python shim + lockfile conflict.                               |
| fc952d52 | A     | 0522f9d2     | Cherry-picked with `-x -s`.                                    |
| ff9f114c | C     | --           | New `Svg` binary-data API has no current isolated caller.      |
| ba0e2a94 | C     | --           | OpenAI subscribed compaction logs.                             |
| c05e3463 | C     | --           | ChatGPT subscription compaction.                               |
| a8fafdd7 | C     | --           | ChatGPT subscription routing headers.                          |
| 7733b992 | A     | 1250adb4     | Cherry-picked with `-x -s`.                                    |
| 03e5ad8a | A     | f6e15bb4     | Cherry-picked with `-x -s`.                                    |
| dd04a229 | C     | --           | Unused `Animation` max-FPS API.                                |
| b4150535 | C     | --           | Terminal hyperlinks; `alacritty.rs` deleted.                   |
| 4efba716 | B     | 131b9d6d     | Non-Unicode search; local `LineHint`.                          |
| 18be72fd | C     | --           | File-scanner rewrite; settings/worktree conflict.              |
| 17d71d2b | C     | --           | csv_preview filter files deleted locally.                      |
| 0307288d | C     | --           | Unused benchmark/threaded-dispatcher API.                      |
| cd6d7055 | C     | --           | Extension publishing docs split.                               |
| d8664715 | C     | --           | csv_preview copy; renderer conflict.                           |
| 0ad5441b | C     | --           | Display-position selections; editor conflict.                  |
| 2cb57850 | A     | 9693b5d5     | Cherry-picked with `-x -s`.                                    |
| 47825fe0 | B     | ba686f3ca0   | Measure invisible replacement width; ported after review.      |
| 24e25552 | A     | f10fa494     | Cherry-picked with `-x -s`.                                    |
| 30f806c4 | B     | d3dad3d954   | JetBrains CamelHump subword navigation; ported after review.   |
| 5fa87423 | A     | ba9894c3     | Cherry-picked with `-x -s`.                                    |
| cdc537c6 | C     | --           | csv_preview tabular rewrite.                                   |
| a21007b7 | C     | --           | Unused profiler rewrite.                                       |
| 52b24181 | C     | --           | `gpui_apple` crate split; macOS-only rewrite.                  |
| f0685e0a | C     | --           | Parent-revision blame; git/proto conflict.                     |
| 3cf86bed | C     | --           | OpenAI subscribed account models.                              |
| 9f164a0d | C     | --           | Native agent Chat Completions share.                           |
| 9bde578e | C     | --           | ACP dedicated thread needs absent `spawn_dedicated`.           |
| 1e3d8b5a | B     | fb304e06b8   | Share the search snapshot behind an Arc; ported after review.  |
| 56b1e79a | A     | 4cc4126f     | Cherry-picked with `-x -s`.                                    |
| 939d2d70 | A     | --           | Already equivalent title_bar test-support.                     |
| f4199ae0 | A     | 7f3fc8bd     | Cherry-picked with `-x -s`.                                    |
| bc538def | A     | 2b2f9fc5     | Cherry-picked with `-x -s`.                                    |
| bc6095f2 | C     | --           | Buffer-header menu; `element/mouse.rs` deleted.                |
| f543a764 | A     | 81a751b591   | Invisible-character matching; cherry-picked after review.      |
| b2d9c2e1 | C     | --           | Native agent panel rewind.                                     |
| b47d8ac4 | A     | dcffb59d     | Cherry-picked with `-x -s`.                                    |
| 6c706fb9 | A     | 0ab6e94d     | Cherry-picked with `-x -s`.                                    |
| 632d805d | C     | --           | Native agent `ask_user` elicitation.                           |
| 1f36e4b0 | A     | b5ee7092     | Cherry-picked with `-x -s`.                                    |
| 098e4407 | C     | --           | OpenAI subscribed ungated catalog.                             |
| 8968bf78 | C     | --           | Non-UTF-8 git blobs; staged_diff deleted.                      |
| 984bf4d4 | C     | --           | Anthropic compaction; provider conflict.                       |
| eedd2016 | A     | b8238eb7     | Cherry-picked with `-x -s`.                                    |
| 91a6890d | C     | --           | Sidebar onboarding title wrap.                                 |
| 511ac170 | A     | bdb2b214     | Cherry-picked with `-x -s`.                                    |
| 90eb566f | C     | --           | Sidebar reorder; MultiWorkspace conflict.                      |
| dbf7f638 | C     | --           | Anthropic compaction context; completion conflict.             |
| 30f73707 | B     | fd3438cd     | Gemini 3.7 Flash.                                              |
| bf65fd4d | C     | --           | Legal ToS/Privacy absolute URLs.                               |
| e0931d5a | C     | --           | Invisibles reuse; display_map/benchmarks deleted.              |
| db7c1d38 | C     | --           | Community CI permissions.                                      |
| 378d6254 | A     | 89572004     | Cherry-picked with `-x -s`.                                    |
| a8b5f6b9 | A     | 556ff92e     | Cherry-picked with `-x -s`.                                    |
| dfb69669 | A     | 3108a484     | Cherry-picked with `-x -s`.                                    |
| 6dee3fc7 | C     | --           | Diagnostic related info; language proto conflict.              |
| d0bfe0a4 | C     | --           | Single-hunk nav; staged/unstaged_diff deleted.                 |
| eb38ea55 | C     | --           | Community champions list.                                      |
| eb548352 | C     | --           | Markdown highlight overflow; markdown conflict.                |
| 8b1497db | C     | --           | Unused spring animation API.                                   |
| 7bddd16a | A     | bdce4cc5     | Cherry-picked with `-x -s`.                                    |
| 07a0bd12 | C     | --           | Unused frame-time debug overlay.                               |
| 6721ea2e | C     | --           | Reset dock panels; workspace conflict.                         |
| 64d14ea8 | C     | --           | System pinentry; askpass conflict.                             |
| fd90c0af | A     | 20b57034     | Cherry-picked with `-x -s`.                                    |
| aa371861 | C     | --           | Autofix workflow clippy flag.                                  |

## Applied Work

The work branch contains the listed A/B local commits. Every direct upstream
commit was created with `git cherry-pick -x -s`; B commits retain their full
`Upstream:` trailer and explain omissions. No remote branch, pull request, or
upstream remote was created.

App version followed upstream: `crates/zzz` is now `1.17.0`.

`9bde578e` was cherry-picked then fully reverted after `cargo check` failed on
absent `SchedulerLocalExecutor` / `spawn_dedicated`. `4efba716` was adapted to
`LineHint`. Duplicate `#[test]` from the `4ed3738c` port was dropped in
`1d63e442`.

## Per-commit notes

### B ports

- `417d0330`: optional `frame_waker`, invalidator wakeup, web RAF on demand.
  Omitted upstream `FrameDirtyAccumulator`.
- `4ed3738c`: `Animation::repeat_synced` and spinner phase lock. Omitted
  reduce-motion / `simulate_next_frame` harness.
- `38ca9106`: hover-after-layout. Tests use local `draw().clear()`.
- `65308f40` / `30f73707`: Gemini 3.6 and 3.7 Flash on existing
  `google_ai::Model`. Omitted upstream thinking-level helpers.
- `08827f92`: `terminal.starts_open`. Omitted settings_ui page metadata and
  `all-settings.md`.
- `b13f6c71`: version follow to 1.17.0. Package name stays `zzz`.
- `4efba716`: unified non-Unicode detection. `MatchPositionHint` mapped to
  `LineHint`.

### Representative C

Philosophy: subscribed/ChatGPT routes, native agent panel/thread/tools,
guild/CI/community/legal/marketing docs.

Missing architecture: `lsp_locations`, `spawn_dedicated`, elapsed RPC
tracker, deleted editor modules (`completions.rs`, `code_actions.rs`,
`element/mouse.rs`, `alacritty.rs`, staged/unstaged diffs).

Unisolatable conflicts: git_panel, markdown, askpass, csv_preview, rustc 1.97
66-file bump.

The 2026-09-22 re-audit ported `83dc1967`, `1e3d8b5a`, `a4916265`,
`992c7d46`, `30f806c4`, `47825fe0`, `f543a764`, `c6b01d8a`, `6634c945`, and
`bd1b83a4`; see `upstream-sync-audit-2026-09-22.md`.

## Verification

```text
PASS git merge-base --is-ancestor 027cf0def7 FETCH_HEAD after shallow-since fetch
PASS cargo check --locked -p gpui
PASS cargo test --locked -p gpui --lib test_frame_waker_fires_on_frame_demand
PASS cargo test --locked -p gpui --lib test_pending_next_frame_callbacks_are_not_stranded
PASS cargo test --locked -p gpui --lib hover_listeners
PASS cargo check --locked -p google_ai
PASS cargo test --locked -p google_ai test_gemini_3_6_flash_model_metadata
PASS cargo test --locked -p google_ai test_gemini_3_7_flash_model_metadata
PASS cargo test --locked -p terminal_view test_terminal_panel_starts_open_follows_setting
PASS cargo check --locked -p project
PASS cargo check --locked -p editor -p git_ui -p repl -p language -p remote -p anthropic
FAIL cargo check after 9bde578e: missing spawn_dedicated; commit reverted
FAIL cargo check -p project after 4efba716: MatchPositionHint; adapted to LineHint
NOT RUN macOS / Windows / wasm32 gpui_web runtime
NOT RUN cargo test --workspace
```

The reviewed baseline is `aa3718614b3ade75524be6f8b2e101bd1166e02c`.
Work remains on `sync/upstream-2026-08-18` at `2fe4b4bd4d` and has not been
merged to `main`.
