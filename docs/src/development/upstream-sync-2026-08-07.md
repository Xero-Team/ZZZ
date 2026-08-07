---
title: Upstream Sync 2026-08-07
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-08-07

## Scope

- Target branch: `main` at `33cec29132ac8ad489e0dd1c8541911881f2174f`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Upstream head: `101ca00a1352ed71ef398f21b47836565d1998e3`
- Head queried: `2026-08-07T01:25:06+02:00`
- Requested range starts after `7b030b500810b04cf5fb4aa5973be99a502d9f36`

`A` is a complete safe absorption or an already-equivalent local change.
`B` needs a local equivalent port or further API review and is deliberately not
claimed as synchronized unless a local commit is listed. `C` is rejected by
ZZZ's local-first, no-account, ACP-only boundary.

## Decisions

| Upstream | Class | Local commit | Disposition                                                       |
| -------- | ----- | ------------ | ----------------------------------------------------------------- |
| 86531872 | A     | e301aba8     | Already absorbed.                                                 |
| a1510de5 | C     | --           | Native agent permission runtime.                                  |
| 4a1df1f7 | A     | 2841a150     | Already absorbed.                                                 |
| c97b7c0e | B     | 1f504bf7     | Web fixes kept; unrelated missing benchmark declaration omitted.  |
| 8e4e5a39 | A     | a16fa6de     | Already absorbed.                                                 |
| a5d1afa5 | A     | 9f294f0f     | Cherry-picked with `-x -s`.                                       |
| 65f3428f | C     | --           | Collaboration panel.                                              |
| ab92195a | A     | 99958903     | Cherry-picked with `-x -s`.                                       |
| 8780e3a1 | B     | 5ac7e91c     | Undo errors ported; `TrashId` redesign omitted.                   |
| 95106f9c | C     | --           | Staff/server-gated project-panel behavior is unavailable in ZZZ.  |
| a8b57a25 | C     | --           | Release-channel rollout retains upstream flag policy.             |
| 1efdc3e6 | B     | 6fdb6647     | Response-first LSP refresh ported after test conflict.            |
| fa1d0362 | A     | b2206202     | Cherry-picked with `-x -s`.                                       |
| fee527c7 | B     | 4adbd64d     | Wide-table scrolling ported; test harness omitted.                |
| 945764f9 | A     | b6b9c9dc     | Cherry-picked with `-x -s`.                                       |
| b2131e9d | C     | --           | Cross-thread GPUI Web dispatcher APIs diverge locally.            |
| b6ebe0ff | A     | d67e88ac     | Cherry-picked with `-x -s`.                                       |
| 1102219f | B     | 937879bb     | C-column fragments ported; preview lifecycle omitted.             |
| e4ac280d | C     | --           | Subscription provider extraction.                                 |
| 50ac7dc9 | A     | 6cd56a79     | Cherry-picked with `-x -s`.                                       |
| 6dcb0e57 | B     | 08584285     | RelPath normalization ported; provider routing omitted.           |
| baacd359 | C     | --           | Call diagnostics.                                                 |
| 424a6824 | C     | --           | wasm_thread fork-only dependency redirect; no behavior to retain. |
| 06b6160d | B     | 063594c4     | Private macOS blur API removed; local ctor retained.              |
| 82aef443 | C     | --           | Unused cross-platform idle scheduler API is too broad to add.     |
| 5333ca1a | A     | 2200e0e8     | Cherry-picked; avoids redundant Git access checks.                |
| bdb28659 | C     | --           | `docs/theme` build configuration is outside this audit scope.     |
| 97961c2a | B     | 170539df     | Web-compatible scrollbar clock ported.                            |
| c2db0f1a | A     | 227dd724     | Cherry-picked with `-x -s`.                                       |
| 5ccbbbd8 | C     | --           | Unused GPUI grid API and public enum rename omitted.              |
| ba4cb2a2 | A     | 2b99b9b3     | Cherry-picked with `-x -s`.                                       |
| f85349be | A     | --           | Already equivalent through `TrashedEntry` retry semantics.        |
| 007ffc79 | A     | 4e2deeed     | Cherry-picked with `-x -s`.                                       |
| b005c0de | B     | 379c16e8     | Local deactivation behavior ported; collab UI omitted.            |
| 12a19dcc | C     | --           | Native agent sandbox.                                             |
| dc1e815e | B     | --           | GPUI IME dispatch needs local lifecycle review.                   |
| a11083f9 | B     | --           | GPUI callback reentrancy needs local review.                      |
| e24eeb71 | C     | --           | Upstream release metadata.                                        |
| b9256fa8 | C     | --           | Upstream npm build infrastructure.                                |
| f620cbc0 | C     | --           | Native agent sandbox bundling.                                    |
| f52fd9ac | B     | --           | macOS drag API needs platform review.                             |
| 431734c9 | C     | --           | Collaboration contact finder.                                     |
| 33f1112f | B     | --           | Theme schema compatibility review required.                       |
| 36911f8c | B     | --           | Documentation not independently reviewed.                         |
| 25929703 | B     | --           | Grammar update pending generated-file review.                     |
| 6109c2e6 | B     | --           | Grammar update pending generated-file review.                     |
| b535bec7 | B     | --           | Notebook action needs local UI review.                            |
| 5e549b87 | B     | --           | REPL documentation not independently reviewed.                    |
| b9301f5c | C     | --           | Native agent thread workflow.                                     |
| f851d82e | B     | --           | Edit-prediction local-model review needed.                        |
| 200fb85c | B     | --           | Language parser change needs regression review.                   |
| 410a8a06 | B     | --           | LSP refresh depends on 1efdc3e6 adaptation.                       |
| 3652f301 | C     | --           | Copilot authentication split.                                     |
| d88f6821 | B     | --           | Release-note UI interaction requires local review.                |
| a473ea63 | B     | --           | Image viewer resource lifecycle review needed.                    |
| 27ca0526 | B     | --           | Documentation not independently reviewed.                         |
| c9d1d0dd | B     | --           | Dependency cleanup conflicts with local gpui_util refactor.       |
| 9677f83f | C     | --           | Triage automation.                                                |
| a6a23c7b | B     | --           | Dependency cleanup not needed for behavior.                       |
| cdf3ccd0 | C     | --           | Extension refactor introduces telemetry events.                   |
| f9a5bf91 | B     | --           | Safe MCP workspace routing awaits local call-chain port.          |
| 933c85b1 | B     | --           | No equivalent upstream server-list UI in ZZZ settings.            |
| a8491e63 | B     | --           | macOS drag restoration needs platform review.                     |
| 79cc17c2 | B     | --           | GPUI scroll API review required.                                  |
| ae99a867 | B     | a1e7b876     | X11 repaint ported; local scroll throttling preserved.            |
| 5786fee5 | C     | --           | Community automation.                                             |
| 08994c41 | B     | --           | Session/CLI lifecycle review required.                            |
| 790dcefb | B     | --           | Allowed self-hosted provider behavior awaits local port.          |
| 998fbf30 | B     | --           | Git submodule worktree model review required.                     |
| 9dc8880b | B     | --           | File-finder navigation needs local path review.                   |
| 5638be1f | C     | --           | ChatGPT subscription authentication.                              |
| 9a631e54 | B     | --           | License metadata requires package review.                         |
| b6b2148b | B     | --           | Grammar update pending generated-file review.                     |
| ae394f3d | C     | --           | Staff-only edit-prediction policy.                                |
| e99616cd | B     | --           | Window-state API needs GPUI review.                               |
| b7de7640 | B     | --           | Dev-container Compose behavior needs integration review.          |
| 26103320 | B     | --           | Cross-platform path behavior needs Windows coverage.              |
| 2ec29977 | B     | --           | Remote-project preference needs local remote flow review.         |
| 5f180e06 | B     | --           | Editor key context needs regression review.                       |
| 58a3c0fa | B     | --           | Documentation awaits settings review.                             |
| 864ff0ba | B     | --           | Dev-container lifecycle execution needs review.                   |
| 0b3621db | A     | aa68f130     | Cherry-picked; local test constructor adaptation follows.         |
| a5615f09 | B     | --           | Panel layout change needs UI review.                              |
| b209000d | B     | --           | Linux installer behavior needs packaging review.                  |
| 4f047acc | B     | --           | Edit-prediction local-model review needed.                        |
| 9c7a5c94 | B     | --           | CLI documentation not independently reviewed.                     |
| 56cf49bc | B     | --           | Theme data update needs snapshot review.                          |
| c7aea6cb | B     | --           | Wayland outbound drag needs platform review.                      |
| 1ac840ab | B     | --           | WSL remote drop needs remote-flow review.                         |
| 59cb143c | C     | --           | Triage automation.                                                |
| 2318f45f | B     | --           | Multi-workspace close behavior needs UI review.                   |
| 779c35d2 | B     | --           | ACP terminal change needs ACP test review.                        |
| f99da3a4 | C     | --           | GPT subscription provider icon.                                   |
| f56ff65c | B     | --           | Superseded by later punctuation revert.                           |
| 98f39bfc | C     | --           | Community automation.                                             |
| 5e03f2d3 | B     | --           | Solo diff UI needs local review.                                  |
| 5e1fd392 | B     | --           | Git diff-base setting needs settings review.                      |
| 21f16f7b | B     | --           | Cargo build optimization needs build review.                      |
| 90d024b8 | B     | 5f35fd30     | Folded-row tab coordinate fix adapted to current editor API.      |
| ce6f3af5 | B     | --           | Remote transfer quoting needs remote test review.                 |
| 66ed3027 | B     | --           | ACP panel control needs ACP UI review.                            |
| 538a4a26 | C     | --           | Wezel build scenario infrastructure.                              |
| 8886dcb0 | B     | --           | GPUI test dispatcher API needs test review.                       |
| 35cb7558 | B     | --           | Git-gutter setting needs settings review.                         |
| 849ec589 | C     | --           | Native agent terminal path.                                       |
| 2d9680fc | B     | --           | Transient undo feature flag requires release policy review.       |
| be8c6f9f | B     | --           | Renderer resource changes need platform review.                   |
| b036368c | B     | --           | ACP sidebar UI needs ACP review.                                  |
| 7759e9f9 | B     | --           | Feature-flag cleanup depends on earlier flag policy.              |
| 41c0f28b | B     | --           | Remote branch-diff compatibility needs remote tests.              |
| b5764581 | B     | --           | Safe MCP refresh needs local context-server port.                 |
| 4aad57fd | B     | --           | Remote workspace lifetime needs remote-flow review.               |
| e717010c | B     | --           | WSL remote streaming needs platform testing.                      |
| 184e124b | B     | --           | Per-line terminal CWD resolver diverges locally.                  |
| 1ade7854 | B     | --           | Regex engine semantics need search test review.                   |
| c6e0868c | B     | --           | Vim indentation behavior needs Vim tests.                         |
| bbd198f5 | B     | --           | Semantic-token ordering needs LSP regression review.              |
| b8c75f17 | B     | --           | Extension provider UI needs extension-path review.                |
| a12e3c06 | B     | --           | Feature-flag removal depends on earlier policy.                   |
| 4601ead4 | B     | --           | Large WebGL backend needs dedicated platform port.                |
| 20ce54f8 | B     | --           | Worktree path identity needs project tests.                       |
| 300972be | C     | --           | Upstream benchmark infrastructure.                                |
| 158c16f5 | B     | --           | Workspace replacement needs UI lifecycle review.                  |
| 381953d4 | C     | --           | Collaboration panel.                                              |
| 655ed138 | B     | --           | Wayland IME behavior needs Linux test review.                     |
| 8e18ab0c | B     | --           | ShellBuilder redirect needs terminal test review.                 |
| f25b256f | B     | --           | Terminal word-boundary behavior needs terminal tests.             |
| 2d9e6278 | B     | --           | Helix multi-key behavior needs Vim tests.                         |
| 6943d736 | C     | --           | Copilot OAuth cleanup.                                            |
| 0fb9a9da | C     | --           | Copilot settings path.                                            |
| 6153542c | C     | --           | Copilot credentials path.                                         |
| 4b407d0f | B     | --           | Reverts transient punctuation behavior.                           |
| 02c6dd95 | C     | --           | Upstream release metadata.                                        |
| 38df25d5 | B     | --           | GPUI scheduler readiness needs review.                            |
| a8cae3bd | B     | --           | Gesture dispatch needs GPUI tests.                                |
| 00cba838 | B     | --           | Mermaid UI behavior needs preview tests.                          |
| 65a5c89a | B     | --           | Parser scheduling needs language tests.                           |
| 8c259313 | C     | --           | External-agent promotional documentation.                         |
| c305d68c | B     | --           | Mermaid dependency update needs lockfile review.                  |
| 82878540 | C     | --           | Upstream benchmark infrastructure.                                |
| d61e80b8 | B     | --           | Git panel focus needs UI tests.                                   |
| b5796233 | C     | --           | Native agent terminal runtime.                                    |
| d0f797a3 | B     | --           | WGPU memory behavior needs renderer review.                       |
| 51db7df7 | B     | --           | Overlaps Markdown table-scrolling port.                           |
| b914ba5c | B     | --           | Grammar update pending generated-file review.                     |
| 101ca00a | B     | --           | Editor word movement needs dedicated regression port.             |

## Applied Work

The work branch contains the listed A/B local commits. Every direct upstream
commit was created with `git cherry-pick -x -s`; B commits retain their full
`Upstream:` trailer and explain omissions. No remote branch, pull request, or
upstream remote was created.

The B entries with no local commit remain deliberately unabsorbed. Therefore
this report records a partial review only and does not advance a baseline.

## Continuation 2026-08-07

- Continuation branch: `sync/upstream-2026-08-07-101ca00a-b`
- Upstream head queried at `2026-08-07`:
  `6b2aa1c90aeb72cadfd30ed12141ed9e2569eded`
- The reviewed history remains the requested range ending at
  `101ca00a1352ed71ef398f21b47836565d1998e3`. It was fetched under the
  temporary ref `refs/upstream-sync-tmp/20260807-101ca00a` only.

The following fifteen candidates were individually re-read with their parent,
complete diff, and current ZZZ call chain:

- `95106f9cde3a6e7b622b6c390c82cf426d7daaa1`: C. Its docs describe
  project-panel undo/redo that is only enabled through the upstream
  staff/server feature-flag route, so the behavior is not available to
  document truthfully in ZZZ.
- `a8b57a2529abcfe263e4e9b38cbd61b60705d9ef`: C. Its only behavior is an
  upstream Preview/Stable release-channel rollout and would retain that
  server/staff policy locally.
- `1efdc3e63e5dd148d9beeb56c574e8dea768883d`: B, local commit
  `6fdb66471ff634f383bc63bf5e0dc0a70d619175`. A direct cherry-pick conflicted
  only with independently inserted diagnostics tests; the local port answers
  `workspace/diagnostic/refresh` before scheduling pulls and adds a regression
  test. No account, telemetry, collaboration, agent, or provider path was
  retained.
- `fee527c70fe3a701c28e4fb29b2acd61fcd1e23f`: B, local commit
  `4adbd64d52cc442350833618909b414c59f4b84b`. Only the table container's
  horizontal scrolling was retained; the upstream rendering-test harness was
  omitted.
- `b2131e9df8aa16d7d287f0b098df3088310d00d6`: C. The Fetch change requires
  a cross-thread GPUI Web mailbox and browser-window Send/Sync redesign that
  does not match ZZZ's API, so a partial port would be unsafe.
- `1102219f812293a09da88eff1df95fbcedde6b09`: B, local commit
  `937879bb9ac6b5b48b5c0717bcd5442e31b0c983`. The local preview resolver now
  accepts `#LlineCcolumn` and ranges; the upstream preview reuse, navigation,
  scroll lifecycle, language detection, and project-panel rewrite were
  omitted because ZZZ already has a separate link resolver implementation.
- `6dcb0e57b5a6d12cc6bab21c11488fd76db0eaaf`: B, local commit
  `0858428551c5801793d9858dc456b63839290f97`. The shared path serializer now
  uses `RelPath` for Unix-style model context paths. Provider selection,
  token, telemetry, data-collection, Copilot, account, and cloud-routing
  behavior was explicitly omitted.
- `82aef44308540b576e4e51fb379efa71614e5c91`: C. The commit adds a new
  scheduler/GPUI/Web idle-execution API without a ZZZ caller and requires a
  cross-platform Web `requestIdleCallback` implementation; this is broader
  than an independently useful stability fix.
- `5333ca1af7900d82cb939436ea5b7020ae7f317a`: A, local commit
  `2200e0e8b8d12cfda8ab162084302862c49fd39b`. The complete safe Git access
  cache change cherry-picked cleanly and removes redundant recomputation for
  `.git/` file events.
- `90d024b88abc91264d9a0ad260eb4f365fa695c3`: B, local commit
  `5f35fd308f8b7db996c65fc6c10e8aaae4a31c39`. The folded-row column-selection
  panic fix was adapted to ZZZ's current editor API, including the required
  tab-row conversion. The direct cherry-pick conflicted because the upstream
  dependency's tab-row model is not an ancestor of ZZZ; unrelated public API
  widening was omitted.
- `0b3621db47895cc0993aa2b934177b4aa1ba7548`: A, local commit
  `aa68f1303141a645a5629e9f6cf413b9965d501f`, followed by test-only local
  adaptation `7c37eebfb5`. The inline terminal height rounding fix cherry-picked
  cleanly; its regression fixture was adapted to ZZZ's fallible builder API.
- `424a68244aa9b8ac9d4766e51b0824d2b2174bd7`: C. The commit only redirects
  `wasm_thread` to an upstream Zed fork through dependency metadata. It has no
  independent behavior to retain and would add a fork-only dependency change.
- `bdb28659c278ffca840f5a4dd24edc86b8269d5c`: C. It changes `docs/theme`,
  which is build configuration expressly outside the documentation audit scope;
  no product behavior was reviewed or imported.
- `97961c2a5f1ecb74f0cfdc6dbf87f5229138fe5c`: B, local commit
  `170539dfc5c3f6daa690902faa46f5f9f7f418bf`. The scrollbar uses the
  workspace's existing `web_time::Instant`, retaining the WASM-compatible
  animation clock only. The direct cherry-pick lockfile conflict was resolved
  by adding the already locked workspace dependency to `ui`; no unrelated
  dependency updates, accounts, telemetry, agents, or providers were included.
- `5ccbbbd88f74a6283241db699ea00cb14f7e5a7f`: C. The complete diff adds
  unused `grid_rows_min_content` and `grid_rows_max_content` public APIs and
  renames `TemplateColumnMinSize` to `GridTemplateMinSize`. No current ZZZ
  caller needs row content sizing, so the public API churn has no independent
  local value.
- `b005c0de6728c65db431520647b2fb5c2887e14f`: B, local commit
  `379c16e8843f502d88c781d75ca85ffac9dec36d`. The Go To Line dialog and
  project-panel rename/new entry inputs now retain state on window
  deactivation, including a project-panel regression test. The collab-panel
  channel rename was omitted because ZZZ excludes collaboration UI and its
  cloud/social routing.

Continuation verification:

```text
PASS cargo fmt --check
PASS git diff --check
PASS cargo check --locked -p project
PASS cargo test --locked -p project test_workspace_diagnostics_refresh_is_answered_before_pulling
PASS cargo check --locked -p markdown
FAIL cargo test --locked -p markdown --lib (149 passed, 4 baseline GPUI window-context panics)
PASS cargo check --locked -p markdown_preview
PASS cargo test --locked -p markdown_preview resolves_preview_link_positions_without_misclassifying_web_urls
PASS cargo check --locked -p edit_prediction
PASS cargo test --locked -p edit_prediction test_buffer_path_with_id_fallback
PASS cargo check --locked -p git_ui
PASS cargo fmt --check and git diff --check after the Git cherry-pick
PASS cargo check --locked -p editor
BLOCKED cargo test --locked -p editor test_add_selection_above_below_with_fold
       Existing duplicate test definitions in crates/editor/src/hover_links.rs prevent test binary compilation
PASS cargo check --locked -p terminal_view
PASS cargo test --locked -p terminal_view test_inline_terminal_displays_all_of_its_lines
PASS cargo check --locked -p ui
PASS cargo check --locked -p go_to_line
PASS cargo check --locked -p project_panel
PASS cargo test --locked -p project_panel --lib test_rename_survives_window_deactivation
```

## Verification

Completed successfully before this report:

```text
cargo fmt --check
cargo metadata --no-deps --format-version=1
cargo check --locked -p project_panel
cargo test --locked -p project_panel --lib undo
cargo check --locked -p gpui_linux
git diff --check
```

The project-panel undo test selection ran ten tests successfully. macOS-only
code was formatted and inspected but cannot be compiled on this Linux host.

`cargo test --locked -p editor --lib test_columnar_selection_with_soft_wrap`
did not build because the target baseline already defines
`test_go_to_definition_link_dedup` and
`test_go_to_definition_link_dedup_no_link` twice in `hover_links.rs`. This
failure is unrelated to the selected-column fix and was not changed here.
