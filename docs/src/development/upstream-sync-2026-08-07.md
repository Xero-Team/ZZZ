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

| Upstream | Class | Local commit | Disposition                                                            |
| -------- | ----- | ------------ | ---------------------------------------------------------------------- |
| 86531872 | A     | e301aba8     | Already absorbed.                                                      |
| a1510de5 | C     | --           | Native agent permission runtime.                                       |
| 4a1df1f7 | A     | 2841a150     | Already absorbed.                                                      |
| c97b7c0e | B     | 1f504bf7     | Web fixes kept; unrelated missing benchmark declaration omitted.       |
| 8e4e5a39 | A     | a16fa6de     | Already absorbed.                                                      |
| a5d1afa5 | A     | 9f294f0f     | Cherry-picked with `-x -s`.                                            |
| 65f3428f | C     | --           | Collaboration panel.                                                   |
| ab92195a | A     | 99958903     | Cherry-picked with `-x -s`.                                            |
| 8780e3a1 | B     | 5ac7e91c     | Undo errors ported; `TrashId` redesign omitted.                        |
| 95106f9c | C     | --           | Staff/server-gated project-panel behavior is unavailable in ZZZ.       |
| a8b57a25 | C     | --           | Release-channel rollout retains upstream flag policy.                  |
| 1efdc3e6 | B     | 6fdb6647     | Response-first LSP refresh ported after test conflict.                 |
| fa1d0362 | A     | b2206202     | Cherry-picked with `-x -s`.                                            |
| fee527c7 | B     | 4adbd64d     | Wide-table scrolling ported; test harness omitted.                     |
| 945764f9 | A     | b6b9c9dc     | Cherry-picked with `-x -s`.                                            |
| b2131e9d | C     | --           | Cross-thread GPUI Web dispatcher APIs diverge locally.                 |
| b6ebe0ff | A     | d67e88ac     | Cherry-picked with `-x -s`.                                            |
| 1102219f | B     | 937879bb     | C-column fragments ported; preview lifecycle omitted.                  |
| e4ac280d | C     | --           | Subscription provider extraction.                                      |
| 50ac7dc9 | A     | 6cd56a79     | Cherry-picked with `-x -s`.                                            |
| 6dcb0e57 | B     | 08584285     | RelPath normalization ported; provider routing omitted.                |
| baacd359 | C     | --           | Call diagnostics.                                                      |
| 424a6824 | C     | --           | wasm_thread fork-only dependency redirect; no behavior to retain.      |
| 06b6160d | B     | 063594c4     | Private macOS blur API removed; local ctor retained.                   |
| 82aef443 | C     | --           | Unused cross-platform idle scheduler API is too broad to add.          |
| 5333ca1a | A     | 2200e0e8     | Cherry-picked; avoids redundant Git access checks.                     |
| bdb28659 | C     | --           | `docs/theme` build configuration is outside this audit scope.          |
| 97961c2a | B     | 170539df     | Web-compatible scrollbar clock ported.                                 |
| c2db0f1a | A     | 227dd724     | Cherry-picked with `-x -s`.                                            |
| 5ccbbbd8 | C     | --           | Unused GPUI grid API and public enum rename omitted.                   |
| ba4cb2a2 | A     | 2b99b9b3     | Cherry-picked with `-x -s`.                                            |
| f85349be | A     | --           | Already equivalent through `TrashedEntry` retry semantics.             |
| 007ffc79 | A     | 4e2deeed     | Cherry-picked with `-x -s`.                                            |
| b005c0de | B     | 379c16e8     | Local deactivation behavior ported; collab UI omitted.                 |
| 12a19dcc | C     | --           | Native agent sandbox.                                                  |
| dc1e815e | B     | 59d89954     | Pending keybinding wins over IME; stale focus omitted.                 |
| a11083f9 | B     | e4fdf292     | Appearance callback deferred past App borrow.                          |
| e24eeb71 | C     | --           | Upstream release metadata.                                             |
| b9256fa8 | C     | --           | Upstream npm build infrastructure.                                     |
| f620cbc0 | C     | --           | Native agent sandbox bundling.                                         |
| f52fd9ac | C     | --           | Broad macOS outbound-drag framework cannot be safely isolated.         |
| 431734c9 | C     | --           | Collaboration contact finder.                                          |
| 33f1112f | C     | --           | Upstream-hosted theme schema/type migration has no local publish path. |
| 36911f8c | B     | 701e66ef     | Linux window-decoration docs and comments ported.                      |
| 25929703 | A     | 80cc1fdf     | Cherry-picked; enables Emmet in JSX/TSX function bodies.               |
| 6109c2e6 | A     | 2872d245     | Cherry-picked; linked editing supports custom-element names.           |
| b535bec7 | A     | ecc41447     | Cherry-picked; local notebook cell deletion action.                    |
| 5e549b87 | B     | 1a9ea676     | Local Python toolchain guidance kept; remote routing omitted.          |
| b9301f5c | C     | --           | Native agent thread workflow.                                          |
| f851d82e | A     | --           | Equivalent left-biased local cursor anchor already exists.             |
| 200fb85c | C     | --           | Depends on unabsorbed bracket-cache and boundary-query architecture.   |
| 410a8a06 | B     | 14fc451b     | Targeted semantic-token refresh preserves other servers.               |
| 3652f301 | C     | --           | Copilot authentication split.                                          |
| d88f6821 | B     | ad7db886     | Windows Vim/Helix Escape dismisses notifications.                      |
| a473ea63 | C     | --           | Broad GPUI/image lifecycle contract cannot be safely isolated.         |
| 27ca0526 | B     | f09e3aab     | Font fallback docs ported; unrelated formatting restored.              |
| c9d1d0dd | C     | --           | Dependency-only helper relocation has no independent behavior.         |
| 9677f83f | C     | --           | Triage automation.                                                     |
| a6a23c7b | C     | --           | Dependency-only fuzzy cleanup has no independent behavior.             |
| cdf3ccd0 | C     | --           | Extension refactor introduces telemetry events.                        |
| f9a5bf91 | B     | 1ad7cdbb     | MCP configuration prompts route to the active local workspace.         |
| 933c85b1 | C     | --           | ZZZ has no upstream MCP or external-agent server-list settings UI.     |
| a8491e63 | C     | --           | macOS drag restoration requires an inseparable platform lifecycle.     |
| 79cc17c2 | C     | --           | GPUI scrolling redesign is not independently isolatable.               |
| ae99a867 | B     | a1e7b876     | X11 repaint ported; local scroll throttling preserved.                 |
| 5786fee5 | C     | --           | Community automation.                                                  |
| 08994c41 | B     | 5f2ed7e3     | CLI opens wait for session restoration or its first window.            |
| 790dcefb | B     | --           | Safe self-hosted behavior needs a local provider/test adaptation.      |
| 998fbf30 | B     | f6d8bc25     | Submodules retain their own local project identities.                  |
| 9dc8880b | A     | a115c679     | Cherry-picked; `path:line` selects an already-open target file.        |
| 5638be1f | C     | --           | ChatGPT subscription authentication.                                   |
| 9a631e54 | C     | --           | Upstream `path` crate is absent; local `paths` has separate GPL scope. |
| b6b2148b | A     | 00daf646     | Cherry-picked; class constructors use the existing `type.class` scope. |
| ae394f3d | C     | --           | Staff-only edit-prediction policy.                                     |
| e99616cd | B     | d4bfa33b     | Linux decorations honor non-resizable/minimizable window options.      |
| b7de7640 | B     | --           | Dev-container Compose behavior needs integration review.               |
| 26103320 | B     | --           | Cross-platform path behavior needs Windows coverage.                   |
| 2ec29977 | B     | --           | Remote-project preference needs local remote flow review.              |
| 5f180e06 | B     | --           | Editor key context needs regression review.                            |
| 58a3c0fa | B     | --           | Documentation awaits settings review.                                  |
| 864ff0ba | B     | --           | Dev-container lifecycle execution needs review.                        |
| 0b3621db | A     | aa68f130     | Cherry-picked; local test constructor adaptation follows.              |
| a5615f09 | B     | --           | Panel layout change needs UI review.                                   |
| b209000d | B     | --           | Linux installer behavior needs packaging review.                       |
| 4f047acc | B     | --           | Edit-prediction local-model review needed.                             |
| 9c7a5c94 | B     | --           | CLI documentation not independently reviewed.                          |
| 56cf49bc | B     | --           | Theme data update needs snapshot review.                               |
| c7aea6cb | B     | --           | Wayland outbound drag needs platform review.                           |
| 1ac840ab | B     | --           | WSL remote drop needs remote-flow review.                              |
| 59cb143c | C     | --           | Triage automation.                                                     |
| 2318f45f | B     | --           | Multi-workspace close behavior needs UI review.                        |
| 779c35d2 | B     | --           | ACP terminal change needs ACP test review.                             |
| f99da3a4 | C     | --           | GPT subscription provider icon.                                        |
| f56ff65c | B     | --           | Superseded by later punctuation revert.                                |
| 98f39bfc | C     | --           | Community automation.                                                  |
| 5e03f2d3 | B     | --           | Solo diff UI needs local review.                                       |
| 5e1fd392 | B     | --           | Git diff-base setting needs settings review.                           |
| 21f16f7b | B     | --           | Cargo build optimization needs build review.                           |
| 90d024b8 | B     | 5f35fd30     | Folded-row tab coordinate fix adapted to current editor API.           |
| ce6f3af5 | B     | --           | Remote transfer quoting needs remote test review.                      |
| 66ed3027 | B     | --           | ACP panel control needs ACP UI review.                                 |
| 538a4a26 | C     | --           | Wezel build scenario infrastructure.                                   |
| 8886dcb0 | B     | --           | GPUI test dispatcher API needs test review.                            |
| 35cb7558 | B     | --           | Git-gutter setting needs settings review.                              |
| 849ec589 | C     | --           | Native agent terminal path.                                            |
| 2d9680fc | B     | --           | Transient undo feature flag requires release policy review.            |
| be8c6f9f | B     | --           | Renderer resource changes need platform review.                        |
| b036368c | B     | --           | ACP sidebar UI needs ACP review.                                       |
| 7759e9f9 | B     | --           | Feature-flag cleanup depends on earlier flag policy.                   |
| 41c0f28b | B     | --           | Remote branch-diff compatibility needs remote tests.                   |
| b5764581 | B     | --           | Safe MCP refresh needs local context-server port.                      |
| 4aad57fd | B     | --           | Remote workspace lifetime needs remote-flow review.                    |
| e717010c | B     | --           | WSL remote streaming needs platform testing.                           |
| 184e124b | B     | --           | Per-line terminal CWD resolver diverges locally.                       |
| 1ade7854 | B     | --           | Regex engine semantics need search test review.                        |
| c6e0868c | B     | --           | Vim indentation behavior needs Vim tests.                              |
| bbd198f5 | B     | --           | Semantic-token ordering needs LSP regression review.                   |
| b8c75f17 | B     | --           | Extension provider UI needs extension-path review.                     |
| a12e3c06 | B     | --           | Feature-flag removal depends on earlier policy.                        |
| 4601ead4 | B     | --           | Large WebGL backend needs dedicated platform port.                     |
| 20ce54f8 | B     | --           | Worktree path identity needs project tests.                            |
| 300972be | C     | --           | Upstream benchmark infrastructure.                                     |
| 158c16f5 | B     | --           | Workspace replacement needs UI lifecycle review.                       |
| 381953d4 | C     | --           | Collaboration panel.                                                   |
| 655ed138 | B     | --           | Wayland IME behavior needs Linux test review.                          |
| 8e18ab0c | B     | --           | ShellBuilder redirect needs terminal test review.                      |
| f25b256f | B     | --           | Terminal word-boundary behavior needs terminal tests.                  |
| 2d9e6278 | B     | --           | Helix multi-key behavior needs Vim tests.                              |
| 6943d736 | C     | --           | Copilot OAuth cleanup.                                                 |
| 0fb9a9da | C     | --           | Copilot settings path.                                                 |
| 6153542c | C     | --           | Copilot credentials path.                                              |
| 4b407d0f | B     | --           | Reverts transient punctuation behavior.                                |
| 02c6dd95 | C     | --           | Upstream release metadata.                                             |
| 38df25d5 | B     | --           | GPUI scheduler readiness needs review.                                 |
| a8cae3bd | B     | --           | Gesture dispatch needs GPUI tests.                                     |
| 00cba838 | B     | --           | Mermaid UI behavior needs preview tests.                               |
| 65a5c89a | B     | --           | Parser scheduling needs language tests.                                |
| 8c259313 | C     | --           | External-agent promotional documentation.                              |
| c305d68c | B     | --           | Mermaid dependency update needs lockfile review.                       |
| 82878540 | C     | --           | Upstream benchmark infrastructure.                                     |
| d61e80b8 | B     | --           | Git panel focus needs UI tests.                                        |
| b5796233 | C     | --           | Native agent terminal runtime.                                         |
| d0f797a3 | B     | --           | WGPU memory behavior needs renderer review.                            |
| 51db7df7 | B     | --           | Overlaps Markdown table-scrolling port.                                |
| b914ba5c | B     | --           | Grammar update pending generated-file review.                          |
| 101ca00a | B     | --           | Editor word movement needs dedicated regression port.                  |

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
- Confirmed again with `git ls-remote` at `2026-08-07T05:33:03+02:00`:
  `6b2aa1c90aeb72cadfd30ed12141ed9e2569eded`
- The reviewed history remains the requested range ending at
  `101ca00a1352ed71ef398f21b47836565d1998e3`. It was fetched under the
  temporary ref `refs/upstream-sync-tmp/20260807-101ca00a` only.

The following twenty candidates were individually re-read with their parent,
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
- `dc1e815e47835095748cbb036541992abd9ac826`: B, local commit
  `59d89954bf66901289d642bf1052a5fec35adc63`. GPUI now lets a pending
  multi-stroke keybinding consume the next printable key before macOS IME
  handling, while pending input from another focus is ignored. The existing
  marked-text/IME path remains intact; no remote or agent behavior is involved.
- `a11083f9a79495e9c7ddee0c5782f22d07695c31`: B, local commit
  `e4fdf292259e9a73f9fef2ad02f7e891e80adad4`. Native appearance callbacks
  are deferred to the foreground executor so AppKit cannot re-enter a live App
  borrow. The test platform and GPUI regression test were retained; macOS
  native code was inspected but not run on Linux.
- `f52fd9ac44b298c089491d9920daa22964c32cc8`: C. Its 853-line change adds a
  new public GPUI external-drag protocol, AppKit drag-session ownership, test
  platform support, and project-panel routing. It cannot be reduced to a
  standalone stability fix or exercised on this Linux host without importing
  the broad native lifecycle framework.
- `33f1112fc2aff8d27a910c2d1b6379bb51905512`: C. The 559-line migration
  changes theme content types, importer conversions, schema generation, and
  references to an upstream-hosted schema version. ZZZ has no independent
  schema publishing path, so the format declaration cannot be retained safely.
- `36911f8cabc0f76f611dedf7c760b08a22b6cdf4`: B, local commit
  `701e66ef1cc59071a3226683f2f8f67b51a90321`. The existing Linux
  `window_decorations` setting now describes client/server decorations and the
  GNOME Wayland limitation in defaults, schema docs, and settings reference.
  Only local ZZZ wording was used; unrelated pre-existing formatting was left
  untouched.

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
PASS cargo check --locked -p gpui
PASS cargo test --locked -p gpui --lib test_input_handler_pending
PASS cargo test --locked -p gpui --lib test_appearance_change_runs_after_app_update
PASS cargo check --locked -p settings_content
PASS cargo check --locked -p gpui
PASS npx prettier --check docs/src/reference/all-settings.md before restoring unrelated formatting
FAIL npx prettier --check docs/src/reference/all-settings.md after restoration
     Existing unrelated preview-tabs indentation remains intentionally untouched
```

### Continuation, grammar and notebook review

- `259297035a3fd64be4fb36042c229f59f074e38b`: A, local commit
  `80cc1fdf4ad44f55567996ed36a001ad729734bc`. The complete grammar-only
  change enables Emmet completions inside JSX and TSX return and arrow-function
  bodies. It introduces no generated output, provider, account, telemetry, or
  agent path.
- `6109c2e6d83b81f653d20f13cc25a909518b2c31`: A, local commit
  `2872d2455d195ef2c703dd5c9fdb2af8c472474e`. The complete safe change lets
  linked editing keep JSX and TSX custom-element tag names synchronized across
  `-`, including its editor regression coverage. No network or account path is
  involved.
- `b535bec7bf42bff5692f90022dc6c1049fdc1e91`: A, local commit
  `ecc41447b6aae8eea1a8fd2dc127603126a1d1a7`. The local notebook
  `DeleteCell` action, toolbar control, and command-mode key bindings were
  retained. Its call path updates only local `cell_order` and `cell_map`; the
  existing save path serializes `NotebookEditor::to_notebook`, without invoking
  a kernel, remote project, provider, account, or agent route.
- `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`: B, local commit
  `1a9ea676d030bdb3c648051a3a2e19353b3dbadd`. Retained local documentation
  distinguishing the manually selected Python toolchain from the REPL kernel.
  Existing remote Jupyter-server documentation and all remote-kernel routing
  were explicitly omitted.
- `f851d82e880c152d0f41e4a60d363aa82b4b8114`: A, already equivalent. The
  upstream V4 `udiff::prediction_edits_for_single_file_diff` route no longer
  exists in ZZZ. Its active replacement calls
  `zeta::compute_edits_and_cursor_position`, whose insertion branch already
  keeps `anchor_before(buffer_offset)` fixed and applies
  `offset_within_insertion`, the left-biased cursor behavior of the upstream
  fix. No obsolete V4 path was reintroduced.
- `200fb85c902b823ccc8bec3ae87fda3059424e27`: C. Although bracket recovery
  is local and privacy-safe, the full diff moves the matcher to a new module,
  changes the cache from ZZZ's indexed `Vec<Option<_>>` to a lazy hash map, and
  relies on unbounded boundary queries from unabsorbed `146a6bf9`. Its ERROR
  recovery and cross-chunk preservation are coupled: a direct dry-run
  cherry-pick also conflicts with ZZZ's deleted `editor/src/input.rs` and the
  divergent buffer and test APIs. Porting only the ERROR-node portion would
  retain neither its cross-chunk safety invariant nor its regression coverage,
  so no code was changed.

Continuation verification:

```text
PASS git diff --check
PASS cargo fmt --check
PASS cargo check --locked -p grammars
PASS cargo check --locked -p editor
BLOCKED cargo test --locked -p editor --lib <linked-editing regression>
        Existing duplicate hover_links test definitions prevent the editor test binary from building.
PASS cargo check --locked -p repl
PASS cargo test --locked -p repl --lib notebook test_open_single_file_notebook
FAIL cargo test --locked -p repl --lib notebook test_run_cell_with_missing_interpreter_shows_error
     Existing missing i18n::GlobalI18nService; unrelated to DeleteCell.
PASS cd docs && npx prettier --write src/repl.md
PASS cd docs && npx prettier --check src/repl.md
NOT RUN f851d82e: behavior was already present and no source changed.
NOT RUN 200fb85c: rejected after complete diff, caller, cache, and dry-run conflict review; no source changed.
```

### Continuation, semantic-token refresh review

- `410a8a06ed7252a6243314da2bd9c390b1a29f9b`: B, local commit
  `14fc451bbd92f9c96f61d10d9b656219ab3467b3`. ZZZ's newer per-server refresh
  API was adapted so a refresh evicts only the source server's raw tokens. If
  the source server has no request to make, ZZZ preserves and reprojects other
  servers' cached tokens instead of clearing all semantic highlighting. The
  upstream updates for code lenses, colors, links, symbols, folding ranges,
  and its dynamic-registration test rewrite were omitted: ZZZ already has
  separate per-server removal APIs for those data kinds, while the upstream
  test helpers no longer match local APIs. This is local LSP cache behavior;
  no provider, account, telemetry, cloud, or agent route was included.

Continuation verification:

```text
PASS cargo fmt --check
PASS git diff --check
PASS cargo check --locked -p project
PASS cargo test --locked -p project --lib targeted_refresh_keeps_other_servers_raw_tokens
```

### Continuation, Windows Vim notification review

- `d88f68217b370f7a66d0c4b4971db31c0dff6df7`: B, local commit
  `ad7db88634aaf1faa86e5f18b59e580071de544b`. Retained only the Windows Vim
  and Helix normal-mode `Escape` bindings that try `menu::Cancel` before the
  generic editor cancel binding. The result dismisses a visible local workspace
  notification and otherwise falls through normally. The upstream Windows-only
  workspace-notification test was omitted because this Linux host cannot
  compile or execute that platform path. No update, account, telemetry, cloud,
  agent, or provider behavior was retained.

Continuation verification:

```text
PASS git diff --check
PASS cargo check --locked -p vim
PASS cargo test --locked -p vim --lib test_escape_cancels
PASS cargo fmt --check
NOT RUN upstream Windows notification regression: Linux host.
```

### Continuation, image-viewer lifecycle review

- `a473ea63a8bc199a73a7e44de3e1d9252e3ca895`: C. The texture-leak intent is
  useful and local, but its complete diff adds GPUI asset-cache and atlas test
  APIs, a window image-drop ownership contract, a 349-line asynchronous
  image-viewer prefetch/display state machine, visual tests, dev dependencies,
  and lockfile changes. The current ZZZ image-viewer/GPUI ownership model does
  not expose that contract, and upstream history includes an earlier reversion
  of image-resource cleanup. A partial drop call would risk freeing a texture
  still used by another view, so no renderer or dependency change was made.

### Continuation, font fallback documentation review

- `27ca0526293f8fb4b8bf05afa2fa82ffaa7e3106`: B, local commit
  `f09e3aabd70ae3c74368469180b8bff84bd61a07`. Retained documentation for the
  existing buffer, UI, and terminal fallback settings. `TerminalSettings` and
  `terminal_view` confirm that terminal fallbacks inherit buffer fallbacks when
  unset. No account, telemetry, cloud, provider, or agent setting was involved.
  The one unrelated `preview_tabs` indentation change from targeted Prettier was
  restored rather than included.

Continuation verification:

```text
PASS npx prettier --check docs/src/appearance.md
PASS npx prettier --write src/appearance.md src/reference/all-settings.md before restoring unrelated indentation
FAIL npx prettier --check src/reference/all-settings.md after restoration
     Existing preview_tabs indentation remains intentionally untouched.
PASS cargo fmt --check
PASS git diff --check
```

### Continuation, fuzzy dependency review

- `c9d1d0ddfec3c4e5c75e67b13a79a2ab2c76f687`: C. The complete diff only
  relocates `truncate_to_bottom_n_sorted_by` from `util` to `gpui_util` and
  rewires `fuzzy_nucleo` dependencies, benchmark imports, and the lockfile.
  It intentionally preserves matching behavior. ZZZ's local crate split has a
  separate dependency graph, so importing this churn would have no independent
  user-facing or safety value.

### Continuation, MCP workspace routing review

- `a6a23c7b80a5cefa0487b7856335be89ace7e483`: C. The full fuzzy cleanup only
  replaces `util` imports with `path` and `gpui_util`, adds wasm-specific
  `util` module guards, and changes the lockfile. It intentionally preserves
  matching behavior; ZZZ has no standalone wasm caller requiring those broad
  utility API guards, so the dependency churn has no independent value.
- `cdf3ccd036859e76169e74d1b7e9bb5aa1a079eb`: C. The extension UI refactor
  changes extension installation, upgrade, and remote metadata presentation
  while emitting `telemetry::event!` for installation and removal. No separable
  MCP behavior is present, and retaining any installation path would violate
  ZZZ's no-telemetry boundary.
- `f9a5bf918149ba623f5af1afd5468206910ed3dc`: B, local commit
  `1ad7cdbb5eef0967600ec03f2b85223ab6dbc281`. Retained the neutral local MCP
  configuration fix: extension events now select the active `MultiWorkspace`
  and its active workspace, rather than subscribing each retained workspace to
  the same event. Uninstall still removes only matching local context-server
  settings. The regression test confirms that a retained background workspace
  receives no hidden modal. No native agent runtime, account, provider,
  telemetry, collaboration, or remote-project route was introduced.
- `933c85b13442e42530ae2632530a7f148866ca1f`: C. Its MCP server-list fix
  requires both the upstream `mcp_servers_page` store list and the
  `external_agents_page`, neither of which exists in ZZZ: the local MCP page
  intentionally exposes only the neutral timeout setting. Adding the upstream
  settings UI would pull in the excluded external-agent surface, so there is
  no independently safe UI behavior to port.

Continuation verification:

```text
PASS git diff --check
PASS cargo fmt --check
PASS cargo check --locked -p agent_ui
PASS cargo test --locked -p agent_ui --lib context_server_configuration::tests::test_configure_extension_only_opens_modal_in_active_workspace
PASS cd docs && npx prettier --write src/development/upstream-sync-2026-08-07.md
PASS cd docs && npx prettier --check src/development/upstream-sync-2026-08-07.md
FAIL cd docs && npx prettier --check src/
     Existing formatting failures: installation.md, migrate/vs-code.md, and reference/all-settings.md.
```

### Continuation, platform drag and scrolling review

- `a8491e63b54bf1881e10fe78bb3e7b1f8a5a2cac`: C. The full macOS drag fix
  adds `PlatformOwnedDrag` ownership to `App`, a new `FileDropEvent::Ended`
  lifecycle event, window-removal cleanup, and coordinated AppKit
  `draggingEntered`, `draggingExited`, and drag-session completion behavior.
  The original typed payload must remain suspended while external-path payloads
  are active and must be released exactly once. That contract crosses GPUI,
  macOS callbacks, and every platform's exhaustive file-drop handling; it
  cannot be safely exercised on this Linux host or reduced to a local change.
- `79cc17c216cf62d5deec7b3eed986d0f652d1c9a`: C. The horizontal-scroll
  intent is local and useful, but the complete change relocates editor
  `OngoingScroll` ownership into GPUI gesture dispatch, changes the
  `restrict_scroll_to_axis` contract, and rewrites editor scrolling plus
  markdown, search, data-table, preview, and agent-thread call sites across
  thirteen files. Current ZZZ still owns the state in `editor::ScrollManager`;
  porting only the div API would lack the touch-phase lifecycle and its
  overscroll propagation behavior, while porting the full refactor would alter
  excluded native-agent UI and broad editor input semantics. No code was
  changed.

Continuation verification:

```text
NOT RUN a8491e63: rejected after full diff and GPUI/macOS file-drop lifecycle review; no source changed.
NOT RUN 79cc17c2: rejected after full diff, gesture/editor ownership, and caller review; no source changed.
```

### Continuation, session restore and CLI review

- `08994c411cb1121a6779e194dfefba11d81d7f22`: B, local commit
  `5f2ed7e31c8a0bbfd15198a266f6d3d2d976d9af`. Retained the local startup
  ordering fix: post-startup CLI open requests wait until session restoration
  completes or a restored `MultiWorkspace` is placed, preventing an avoidable
  extra window. The first-window race means a slow remote restore does not
  block a local request; no remote routing was added. macOS bundled CLI
  integration was not runnable on this Linux host, and upstream supplied no
  automated regression test.

Continuation verification:

```text
PASS git diff --check
PASS cargo fmt --check
PASS cargo check --locked -p zzz
NOT RUN macOS bundled CLI/session-restore regression: Linux host.
```

### Continuation, self-hosted Sweep edit-prediction review

- `790dcefb01b8dd939434265c5524c8ae476ad77d`: B, not yet absorbed. The
  complete diff is within ZZZ's allowed boundary: `SweepPrompt` is reachable
  only through user-configured `open_ai_compatible_api` or local Ollama
  settings, reuses the existing manually configured endpoint and credential
  loader, and has no default service, account, telemetry, or proprietary
  Sweep-provider route. A direct `git cherry-pick -x -s` was attempted and
  aborted after conflicts in `edit_prediction_tests.rs` and
  `edit_prediction_registry.rs`, where ZZZ has independently evolved provider
  configuration and test helpers. The 884-line rewrite-window implementation
  must be adapted and tested as a separate B port rather than force-merging
  upstream architecture.

Continuation verification:

```text
PASS git cherry-pick --abort
PASS git diff --check after abort
NOT RUN cargo checks: no source was retained after the required abort.
```

### Continuation, Git submodule identity review

- `998fbf30ff2929cce866bd19b77237210db88484`: B, local commit
  `f6d8bc2547377f553a617303c83489bc3ae90f13`. Retained the local Git identity
  correction: directories below a superproject's `.git/modules` are recognized
  as submodule git directories and retain their own working-directory identity
  when projects are grouped, restored, or persisted as recent workspaces.
  Linked worktrees and bare repositories retain their existing identity paths.
  The direct `git cherry-pick -x -s` was aborted because upstream's test
  insertion conflicted with independently added ZZZ Git-path aggregation tests;
  the behavior and focused tests were then ported manually. No network,
  account, telemetry, collaboration, agent, provider, or remote-project route
  was added.

Continuation verification:

```text
PASS git diff --check
PASS cargo check --locked -p project
PASS cargo test --locked -p project --lib is_submodule_git_dir
PASS cargo test --locked -p project --lib resolve_git_worktree_to_main_repo_ignores_submodule
PASS cargo check --locked -p workspace
PASS cargo test --locked -p workspace --lib recent_workspace_identity_for_submodule
PASS cargo fmt --check -p project -p workspace
PASS cd docs && npx prettier --write src/ (unrelated rewrites restored)
PASS cd docs && npx prettier --check src/development/upstream-sync-2026-08-07.md
FAIL cd docs && npx prettier --check src/
     Existing formatting failures: installation.md, migrate/vs-code.md, and reference/all-settings.md.
```

### Continuation, File Finder `path:line` review

- `9dc8880b2b96ae729539f9f6b971d0255a2f3b15`: A, local commit
  `a115c679606e2266300b9712840248f43fe367a1`, cherry-picked with `-x -s`.
  This local file-finder correction uses the parsed path, rather than the raw
  `path:line[:column]` query, for the create-file fallback; it reevaluates a
  preserved selection when the requested row changes; and it does not skip an
  active matching file when a position is supplied. It has no account,
  telemetry, collaboration, provider, agent, remote-project, or network path.
  No upstream behavior was omitted.

Continuation verification:

```text
PASS git diff --check
PASS cargo check --locked -p file_finder
PASS cargo fmt --check -p file_finder
FAIL cargo test --locked -p file_finder --lib path_with_position_when_target_file_is_open
FAIL cargo test --locked -p file_finder --lib row_column_numbers_query_inside_file
     Both panic before assertions in FileFinderDelegate::render_editor because
     Picker is read while already being updated. This is pre-existing: the
     same failure is reproduced on commit 6783b19d4c7ae8cefcbb0a83bf9a8682d6393900
     with cargo test --locked -p file_finder --lib test_matching_paths.
```

### Continuation, path crate license review

- `9a631e5461194bbdbbd5b4a8d1b9236a211494ee`: C. The complete upstream diff
  changes only `crates/path/Cargo.toml` from `GPL-3.0-or-later` to
  `Apache-2.0`. That independent upstream `path` crate does not exist in ZZZ.
  ZZZ instead contains `crates/paths`, with different `dirs`, `ignore`, and
  `util` dependencies, an explicit `GPL-3.0-or-later` package declaration,
  and a `LICENSE-GPL -> ../../LICENSE-GPL` symlink. Reassigning that distinct
  crate's SPDX metadata would require separate source-provenance and licensing
  review, not an upstream behavior port. No code or package metadata changed.

Continuation verification:

```text
NOT RUN cargo checks or tests: rejected after full diff, package manifest,
license symlink, workspace member, and dependency-path review; no source changed.
```

### Continuation, class-instantiation grammar review

- `b6b2148bd0e23f4dc3bfe17468ad1a6fbd89b748`: A, local commit
  `00daf64603c1ff3509e38787f8f06c01e9161393`, cherry-picked with `-x -s`.
  The JavaScript, TypeScript, and TSX highlight queries now assign constructor
  identifiers in `new` expressions to the existing `type.class` theme scope,
  matching class declarations. The complete diff contains only those three
  query changes and a focused local language-highlighting test: no grammar
  binary, generated source, dependency, lockfile, extension, account,
  telemetry, agent, provider, or network behavior changed.

Continuation verification:

```text
PASS git diff --check
PASS cargo check --locked -p languages
PASS cargo test --locked -p languages --lib test_class_instantiation_highlighting
PASS cargo fmt --check -p languages -p grammars
```

### Continuation, Linux window-option review

- `e99616cdd4663ac5d29ae92bbd7ea1629689d4c8`: B, local commit
  `d4bfa33bb4d864b30b33b0b164abf560d5bff3b2`. Retained the Linux-safe subset:
  `Window` now retains and exposes `WindowOptions::is_resizable` and
  `is_minimizable`; Wayland resize calls, Linux custom window controls, Linux
  titlebar double-click maximization, and workspace client-side resize hitboxes
  honor those options. This fixes locally configured non-resizable windows
  without changing their default behavior. The macOS titlebar-double-click
  implementation and Windows caption-button changes were omitted because they
  require native platform validation and a `PlatformWindow` trait signature
  change. No account, telemetry, collaboration, provider, agent, remote, or
  network behavior was retained.

Continuation verification:

```text
PASS git diff --check
PASS cargo check --locked -p gpui -p gpui_linux -p platform_title_bar -p workspace
PASS cargo test --locked -p gpui --lib (184 passed)
PASS cargo fmt --check -p gpui -p platform_title_bar -p workspace
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
