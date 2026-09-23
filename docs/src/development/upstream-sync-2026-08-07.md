---
title: Upstream Sync 2026-08-07
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-08-07

## Scope

- Target branch: `main` at `18f569e34a6e9f24e41a989967011261b12f0bae`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Reviewed upstream head: `027cf0def75e5c027504f402a6a6c0dcac11f178`
- Live upstream head queried: `027cf0def75e5c027504f402a6a6c0dcac11f178`
- Query time: `2026-08-07T19:16:41+02:00`
- Requested range starts after `7b030b500810b04cf5fb4aa5973be99a502d9f36`

`A` is a complete safe absorption or an already-equivalent local change.
`B` needs a local equivalent port or further API review and is deliberately not
claimed as synchronized unless a local commit is listed. `C` is rejected by
ZZZ's local-first, no-account, ACP-only boundary.

The reviewed baseline is now `027cf0def75e5c027504f402a6a6c0dcac11f178`.
The live upstream query at `2026-08-07T19:16:41+02:00` returned
`027cf0def75e5c027504f402a6a6c0dcac11f178`; commits after the reviewed range
are not claimed as audited or synchronized.

## Decisions

| Upstream | Class | Local commit       | Disposition                                                                                  |
| -------- | ----- | ------------------ | -------------------------------------------------------------------------------------------- |
| 86531872 | A     | c2d4bc52           | Already absorbed.                                                                            |
| a1510de5 | C     | --                 | Native agent permission runtime.                                                             |
| 4a1df1f7 | A     | c54f7ba6           | Already absorbed.                                                                            |
| c97b7c0e | B     | 907a5699           | Web fixes kept; unrelated missing benchmark declaration omitted.                             |
| 8e4e5a39 | A     | 564b97a3           | Already absorbed.                                                                            |
| a5d1afa5 | A     | f7e8509e           | Cherry-picked with `-x -s`.                                                                  |
| 65f3428f | C     | --                 | Collaboration panel.                                                                         |
| ab92195a | A     | 5b0b0103           | Cherry-picked with `-x -s`.                                                                  |
| 8780e3a1 | B     | 894ef9eb           | Undo errors ported; `TrashId` redesign omitted.                                              |
| 95106f9c | C     | --                 | Staff/server-gated project-panel behavior is unavailable in ZZZ.                             |
| a8b57a25 | C     | --                 | Release-channel rollout retains upstream flag policy.                                        |
| 1efdc3e6 | B     | 52b43af9           | Response-first LSP refresh ported after test conflict.                                       |
| fa1d0362 | A     | 0dad7fca           | Cherry-picked with `-x -s`.                                                                  |
| fee527c7 | B     | b60b2857           | Wide-table scrolling ported; test harness omitted.                                           |
| 945764f9 | A     | c84f5957           | Cherry-picked with `-x -s`.                                                                  |
| b2131e9d | C     | --                 | Cross-thread GPUI Web dispatcher APIs diverge locally.                                       |
| b6ebe0ff | A     | 5febe56e           | Cherry-picked with `-x -s`.                                                                  |
| 1102219f | B     | d39c57d3           | C-column fragments ported; preview lifecycle omitted.                                        |
| e4ac280d | C     | --                 | Subscription provider extraction.                                                            |
| 50ac7dc9 | A     | 398732c2           | Cherry-picked with `-x -s`.                                                                  |
| 6dcb0e57 | B     | 80649c76           | RelPath normalization ported; provider routing omitted.                                      |
| baacd359 | C     | --                 | Call diagnostics.                                                                            |
| 424a6824 | C     | --                 | wasm_thread fork-only dependency redirect; no behavior to retain.                            |
| 06b6160d | B     | 7a9c6347           | Private macOS blur API removed; local ctor retained.                                         |
| 82aef443 | C     | --                 | Unused cross-platform idle scheduler API is too broad to add.                                |
| 5333ca1a | A     | 9b60ca92           | Cherry-picked; avoids redundant Git access checks.                                           |
| bdb28659 | C     | --                 | `docs/theme` build configuration is outside this audit scope.                                |
| 97961c2a | B     | 0f3d13db           | Web-compatible scrollbar clock ported.                                                       |
| c2db0f1a | A     | b49ee113           | Cherry-picked with `-x -s`.                                                                  |
| 5ccbbbd8 | C     | --                 | Unused GPUI grid API and public enum rename omitted.                                         |
| ba4cb2a2 | A     | a3852a84           | Cherry-picked with `-x -s`.                                                                  |
| f85349be | A     | --                 | Already equivalent through `TrashedEntry` retry semantics.                                   |
| 007ffc79 | A     | 62f6d9f4           | Cherry-picked with `-x -s`.                                                                  |
| b005c0de | B     | 12a774d3           | Local deactivation behavior ported; collab UI omitted.                                       |
| 12a19dcc | C     | --                 | Native agent sandbox.                                                                        |
| dc1e815e | B     | 8737f0f0           | Pending keybinding wins over IME; stale focus omitted.                                       |
| a11083f9 | B     | 76c3d6ca           | Appearance callback deferred past App borrow.                                                |
| e24eeb71 | B     | --                 | Version follow to v1.15.0; superseded by the v1.17.0 follow `9e40f452` (no separate commit). |
| b9256fa8 | C     | --                 | Upstream npm build infrastructure.                                                           |
| f620cbc0 | C     | --                 | Native agent sandbox bundling.                                                               |
| f52fd9ac | C     | --                 | Broad macOS outbound-drag framework cannot be safely isolated.                               |
| 431734c9 | C     | --                 | Collaboration contact finder.                                                                |
| 33f1112f | C     | --                 | Upstream-hosted theme schema/type migration has no local publish path.                       |
| 36911f8c | B     | 1fb5195e           | Linux window-decoration docs and comments ported.                                            |
| 25929703 | A     | 0d45383e           | Cherry-picked; enables Emmet in JSX/TSX function bodies.                                     |
| 6109c2e6 | A     | 78f1350b           | Cherry-picked; linked editing supports custom-element names.                                 |
| b535bec7 | A     | 249294b1           | Cherry-picked; local notebook cell deletion action.                                          |
| 5e549b87 | B     | d836d391           | Local Python toolchain guidance kept; remote routing omitted.                                |
| b9301f5c | C     | --                 | Native agent thread workflow.                                                                |
| f851d82e | A     | --                 | Equivalent left-biased local cursor anchor already exists.                                   |
| 200fb85c | C     | --                 | Depends on unabsorbed bracket-cache and boundary-query architecture.                         |
| 410a8a06 | B     | f5292f42           | Targeted semantic-token refresh preserves other servers.                                     |
| 3652f301 | C     | --                 | Copilot authentication split.                                                                |
| d88f6821 | B     | c67516df           | Windows Vim/Helix Escape dismisses notifications.                                            |
| a473ea63 | C     | --                 | Broad GPUI/image lifecycle contract cannot be safely isolated.                               |
| 27ca0526 | B     | f2ef295c           | Font fallback docs ported; unrelated formatting restored.                                    |
| c9d1d0dd | C     | --                 | Dependency-only helper relocation has no independent behavior.                               |
| 9677f83f | C     | --                 | Triage automation.                                                                           |
| a6a23c7b | C     | --                 | Dependency-only fuzzy cleanup has no independent behavior.                                   |
| cdf3ccd0 | C     | --                 | Extension refactor introduces telemetry events.                                              |
| f9a5bf91 | B     | e306a908           | MCP configuration prompts route to the active local workspace.                               |
| 933c85b1 | C     | --                 | ZZZ has no upstream MCP or external-agent server-list settings UI.                           |
| a8491e63 | C     | --                 | macOS drag restoration requires an inseparable platform lifecycle.                           |
| 79cc17c2 | C     | --                 | GPUI scrolling redesign is not independently isolatable.                                     |
| ae99a867 | B     | 75a1d657           | X11 repaint ported; local scroll throttling preserved.                                       |
| 5786fee5 | C     | --                 | Community automation.                                                                        |
| 08994c41 | B     | 8f91496e           | CLI opens wait for session restoration or its first window.                                  |
| 790dcefb | C     | --                 | Sweep prompt path requires a large edit-prediction model/API rewrite not present in ZZZ.     |
| 998fbf30 | B     | a8c2c6f0           | Submodules retain their own local project identities.                                        |
| 9dc8880b | A     | e6870cc9           | Cherry-picked; `path:line` selects an already-open target file.                              |
| 5638be1f | C     | --                 | ChatGPT subscription authentication.                                                         |
| 9a631e54 | C     | --                 | Upstream `path` crate is absent; local `paths` has separate GPL scope.                       |
| b6b2148b | A     | 70a78f1d           | Cherry-picked; class constructors use the existing `type.class` scope.                       |
| ae394f3d | C     | --                 | Staff-only edit-prediction policy.                                                           |
| e99616cd | B     | 55443eb0           | Linux decorations honor non-resizable/minimizable window options.                            |
| b7de7640 | B     | c68240c9           | Compose preserves omitted entrypoints per specification defaults.                            |
| 26103320 | B     | 8cc7a956           | Windows path normalization is host-platform independent.                                     |
| 2ec29977 | A     | --                 | Already equivalent in the existing-connection remote action; telemetry omitted.              |
| 5f180e06 | A     | d32e10d1           | Multiple-selection editor key context.                                                       |
| 58a3c0fa | A     | a318fc4c           | Project-panel dock default documentation corrected.                                          |
| 864ff0ba | B     | a5674342           | String lifecycle commands use `/bin/sh -c`; fixtures adapted locally.                        |
| 0b3621db | A     | 86a4d99f           | Cherry-picked; local test constructor adaptation follows.                                    |
| a5615f09 | B     | 0d061096           | Panel saved size resets when default_size changes.                                           |
| b209000d | A     | e72a6785           | 32-bit Linux installer architectures rejected.                                               |
| 4f047acc | C     | --                 | V4 cursor-marker route is absent from ZZZ's edit_prediction API.                             |
| 9c7a5c94 | A     | be9e42c6           | CLI `--existing` option documented.                                                          |
| 56cf49bc | B     | 075a221b           | Gruvbox parameter colors adapted to divergent local theme data.                              |
| c7aea6cb | C     | --                 | Requires absent GPUI ExternalDragPayload/FileDragPaths and PlatformWindow drag APIs.         |
| 1ac840ab | B     | fc016550           | WSL host-path translation retained with local remote/drop APIs.                              |
| 59cb143c | C     | --                 | Triage automation.                                                                           |
| 2318f45f | C     | --                 | Broad MultiWorkspace/recent-project lifecycle rewrite; no isolated safe port.                |
| 779c35d2 | B     | 64f759a7           | ACP terminal disables configured Git pagers.                                                 |
| f99da3a4 | C     | --                 | GPT subscription provider icon.                                                              |
| f56ff65c | A     | --                 | Superseded by later punctuation revert; current code is equivalent.                          |
| 98f39bfc | C     | --                 | Community automation.                                                                        |
| 5e03f2d3 | B     | e9872283           | Solo diffs hide generic multibuffer controls.                                                |
| 5e1fd392 | C     | --                 | Depends on the unabsorbed diff-base protocol and broad editor/project graph changes.         |
| 21f16f7b | C     | --                 | Broad crate-graph/lockfile refactor has no independent product behavior.                     |
| 90d024b8 | B     | 1d7d9197           | Folded-row tab coordinate fix adapted to current editor API.                                 |
| ce6f3af5 | C     | --                 | SCP/SFTP transport argument model diverges; safe port requires the larger remote rewrite.    |
| 66ed3027 | C     | --                 | Native agent panel UI; no ACP-only surface.                                                  |
| 538a4a26 | C     | --                 | Wezel build scenario infrastructure.                                                         |
| 8886dcb0 | C     | --                 | Benchmark/test dispatcher plumbing only; no local runtime behavior.                          |
| 35cb7558 | B     | 338cf158           | Configurable `gutter.git_gutter_width` setting.                                              |
| 849ec589 | C     | --                 | Native agent terminal path.                                                                  |
| 2d9680fc | B     | e7f37912           | Project-panel undo/redo enabled on all channels; lockfile downgrade omitted.                 |
| be8c6f9f | C     | --                 | Large cross-platform renderer resource rewrite cannot be isolated safely.                    |
| b036368c | C     | --                 | Native agent sidebar UI, outside ACP-only scope.                                             |
| 7759e9f9 | C     | --                 | Removes an upstream-only auto-watch flag absent from ZZZ's feature policy.                   |
| 41c0f28b | C     | --                 | Requires absent `MergeBaseWithWorktree` protocol variant.                                    |
| b5764581 | B     | 111dc69b, 8f06ca23 | Re-resolve local MCP settings after worktree changes; adapted the worktree-event test.       |
| 4aad57fd | C     | --                 | Broad remote workspace lifetime/recent-project flow rewrite.                                 |
| e717010c | C     | --                 | WSL streaming fallback is coupled to upstream remote transport lifecycle.                    |
| 184e124b | C     | --                 | Per-line terminal CWD resolver requires a divergent terminal path model.                     |
| 1ade7854 | C     | --                 | Search regex assertion rewrite depends on upstream search engine semantics.                  |
| c6e0868c | C     | --                 | Vim indentation change spans language/multibuffer APIs not present locally.                  |
| bbd198f5 | C     | --                 | Large semantic-token ordering rewrite requires absent server-order plumbing.                 |
| b8c75f17 | C     | --                 | Extension provider icon/dependency path has no independent ZZZ repository surface.           |
| a12e3c06 | C     | --                 | Feature-flag removal is coupled to upstream project-panel flag policy.                       |
| 4601ead4 | C     | --                 | Large WebGL backend and shader/dependency port exceeds isolated scope.                       |
| 20ce54f8 | A     | 70674296           | Worktree path pairing fix cherry-picked cleanly.                                             |
| 300972be | C     | --                 | Upstream benchmark infrastructure.                                                           |
| 158c16f5 | C     | --                 | Broad sidebar/MultiWorkspace replacement and persistence rewrite.                            |
| 381953d4 | C     | --                 | Collaboration panel.                                                                         |
| 655ed138 | A     | 793d392a           | Inactive Wayland windows no longer update IME position.                                      |
| 8e18ab0c | A     | 65b181d6           | ShellBuilder stdin redirect fixed for POSIX and Fish.                                        |
| f25b256f | B     | 872d9480           | Terminal tree-branch word boundary adapted to consolidated module.                           |
| 2d9e6278 | B     | f963ba68           | Helix multi-key cursor anchor refresh; Windows test omitted.                                 |
| 6943d736 | C     | --                 | Copilot OAuth cleanup.                                                                       |
| 0fb9a9da | C     | --                 | Copilot settings path.                                                                       |
| 6153542c | C     | --                 | Copilot credentials path.                                                                    |
| 4b407d0f | A     | --                 | Current movement already matches the reverted punctuation behavior.                          |
| 02c6dd95 | B     | --                 | Version follow to v1.16.0; superseded by the v1.17.0 follow `9e40f452` (no separate commit). |
| 38df25d5 | C     | --                 | Benchmark-only dispatcher readiness API; no product behavior.                                |
| a8cae3bd | A     | 750b3195           | Multi-modifier gestures no longer synthesize standalone modifiers.                           |
| 00cba838 | C     | --                 | Large Mermaid/GPUI dependency and excluded agent-panel integration.                          |
| 65a5c89a | A     | --                 | Already equivalent in ZZZ's parse-again/auto-indent scheduling.                              |
| 8c259313 | C     | --                 | External-agent promotional documentation.                                                    |
| c305d68c | C     | --                 | Dependency/lockfile-only Mermaid bump with no independent ZZZ behavior.                      |
| 82878540 | C     | --                 | Upstream benchmark infrastructure.                                                           |
| d61e80b8 | B     | 747fe837           | Git panel Changes/History focus navigation.                                                  |
| b5796233 | C     | --                 | Native agent terminal runtime.                                                               |
| d0f797a3 | B     | ba2b7d99           | Shared `Arc` font sources retained; fallback iterator already equivalent.                    |
| 51db7df7 | A     | --                 | Already equivalent through the existing `b60b2857` Markdown table-scroll port.               |
| b914ba5c | A     | e71420c7           | Python dunder variables receive attribute.special highlighting.                              |
| 101ca00a | B     | 50f5c0ba           | Punctuation boundaries adapted to consolidated editor.rs call sites.                         |
| 6b2aa1c9 | C     | --                 | Reveal-policy API has no ZZZ caller; no independent behavior to retain.                      |
| 87e698fb | C     | --                 | Native agent-thread UI behavior, outside ACP-only scope.                                     |
| c24358d9 | C     | --                 | Dependency/lockfile bump and broad warning-only churn.                                       |
| d356b2f5 | C     | --                 | Subscription OpenAI compaction and account-bound route.                                      |
| c95e0c51 | C     | --                 | Community PR automation mapping.                                                             |
| cc053a4a | C     | --                 | AccessKit accessibility chain is absent from current ZZZ GPUI.                               |
| ca1ef7a4 | B     | 2fe2e97e           | LSP unit params/results compatibility; existing smol channel used in tests.                  |
| 027cf0de | B     | 673c0c6f           | Markdown preview honors editor scrollbar.show settings.                                      |

## Applied Work

The work branch contains the listed A/B local commits. Every direct upstream
commit was created with `git cherry-pick -x -s`; B commits retain their full
`Upstream:` trailer and explain omissions. No remote branch, pull request, or
upstream remote was created.

All B candidates through the reviewed upstream head now have a local commit or
an explicit reclassification to A/C. The reviewed baseline is advanced through
`027cf0def75e5c027504f402a6a6c0dcac11f178`.

## Continuation 2026-08-07

- Continuation branch: `sync/upstream-2026-08-07-101ca00a-b`
- Upstream head queried at `2026-08-07`:
  `6b2aa1c90aeb72cadfd30ed12141ed9e2569eded`
- Confirmed again with `git ls-remote` at `2026-08-07T05:33:03+02:00`:
  `6b2aa1c90aeb72cadfd30ed12141ed9e2569eded`
- A fresh `git ls-remote` at `2026-08-07T15:53:40+02:00` reports current
  upstream `refs/heads/main` at `027cf0def75e5c027504f402a6a6c0dcac11f178`.
- The initial continuation review ended at
  `101ca00a1352ed71ef398f21b47836565d1998e3`. A follow-up extension reviewed
  the eight commits through the live head below under a separate temporary ref.

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
  `52b43af90621dd29667ad6238811043ce9c19a1b`. A direct cherry-pick conflicted
  only with independently inserted diagnostics tests; the local port answers
  `workspace/diagnostic/refresh` before scheduling pulls and adds a regression
  test. No account, telemetry, collaboration, agent, or provider path was
  retained.
- `fee527c70fe3a701c28e4fb29b2acd61fcd1e23f`: B, local commit
  `b60b28579e69bc3755f83c2df311c4a0f8dbc8dd`. Only the table container's
  horizontal scrolling was retained; the upstream rendering-test harness was
  omitted.
- `b2131e9df8aa16d7d287f0b098df3088310d00d6`: C. The Fetch change requires
  a cross-thread GPUI Web mailbox and browser-window Send/Sync redesign that
  does not match ZZZ's API, so a partial port would be unsafe.
- `1102219f812293a09da88eff1df95fbcedde6b09`: B, local commit
  `d39c57d3f0ecd46a5f54e89ee53a3860eeff58d6`. The local preview resolver now
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
  `86a4d99fb3ab85da5e745d8b8ea12f69427c123f`, followed by test-only local
  adaptation `2b946aa98d`. The inline terminal height rounding fix cherry-picked
  cleanly; its regression fixture was adapted to ZZZ's fallible builder API.
- `424a68244aa9b8ac9d4766e51b0824d2b2174bd7`: C. The commit only redirects
  `wasm_thread` to an upstream Zed fork through dependency metadata. It has no
  independent behavior to retain and would add a fork-only dependency change.
- `bdb28659c278ffca840f5a4dd24edc86b8269d5c`: C. It changes `docs/theme`,
  which is build configuration expressly outside the documentation audit scope;
  no product behavior was reviewed or imported.
- `97961c2a5f1ecb74f0cfdc6dbf87f5229138fe5c`: B, local commit
  `0f3d13dbd4e31a89b17d0445b36cd3f11b567f42`. The scrollbar uses the
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
  `12a774d341c4c7598a9c617b571b84812959fcab`. The Go To Line dialog and
  project-panel rename/new entry inputs now retain state on window
  deactivation, including a project-panel regression test. The collab-panel
  channel rename was omitted because ZZZ excludes collaboration UI and its
  cloud/social routing.
- `dc1e815e47835095748cbb036541992abd9ac826`: B, local commit
  `8737f0f00930ac2c48242d917457069c73e25c11`. GPUI now lets a pending
  multi-stroke keybinding consume the next printable key before macOS IME
  handling, while pending input from another focus is ignored. The existing
  marked-text/IME path remains intact; no remote or agent behavior is involved.
- `a11083f9a79495e9c7ddee0c5782f22d07695c31`: B, local commit
  `76c3d6ca67ce9a2bb290e1a21e53923055ff6ab1`. Native appearance callbacks
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
  `1fb5195e5ec1442979f016761d667366a6bbbd2f`. The existing Linux
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
  `0d45383e383af0e9302f72ea1bbe6af30ac25bff`. The complete grammar-only
  change enables Emmet completions inside JSX and TSX return and arrow-function
  bodies. It introduces no generated output, provider, account, telemetry, or
  agent path.
- `6109c2e6d83b81f653d20f13cc25a909518b2c31`: A, local commit
  `78f1350be4c9899f1f2250d3146424c5b75f7a0e`. The complete safe change lets
  linked editing keep JSX and TSX custom-element tag names synchronized across
  `-`, including its editor regression coverage. No network or account path is
  involved.
- `b535bec7bf42bff5692f90022dc6c1049fdc1e91`: A, local commit
  `249294b14ff63466e62bd67f483a1d8e2dd26ddc`. The local notebook
  `DeleteCell` action, toolbar control, and command-mode key bindings were
  retained. Its call path updates only local `cell_order` and `cell_map`; the
  existing save path serializes `NotebookEditor::to_notebook`, without invoking
  a kernel, remote project, provider, account, or agent route.
- `5e549b871fb87d1038d9b1b242bf7d4d4e3b4d8f`: B, local commit
  `d836d391af0f20e8c412f43df9080463fa2dcdd6`. Retained local documentation
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
  `f5292f422cffbbf71c578c69fa5141cc7bd389b8`. ZZZ's newer per-server refresh
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
  `c67516df0b777265a2448b6eaba3fca642b65a2f`. Retained only the Windows Vim
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
  `f2ef295ca81af9dc09c68421bbe5f0f550816090`. Retained documentation for the
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
  `e306a90874eea89bd4ed5ffc7325e35db97ae2b4`. Retained the neutral local MCP
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
  `a8c2c6f07c3279d1b62c46e93ad2e67c02fbed40`. Retained the local Git identity
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
     same failure is reproduced on commit 24de1f1b9ab582bb4dc3fd8213ef0afab5df33dc
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
  `55443eb0b09cfb2eb999f9350440c877a43937a0`. Retained the Linux-safe subset:
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

### Continuation, Dev Container Compose entrypoint review

- `b7de76402cdffafe5e678554d45a965ddfb8e94c`: B, local commit
  `c68240c966a11c1be34a5bd362291ecec3052c9a`. Retained the specification
  default: omitted `overrideCommand` remains true for image/Dockerfile builds
  and becomes false for Docker Compose, while explicit values win. The
  conflicting upstream fixture assertions were omitted in favor of ZZZ's
  existing manifest expectations. No account, telemetry, agent, provider, or
  remote behavior was introduced.

Verification:

```text
PASS cargo check --locked -p dev_container
PASS cargo test --locked -p dev_container --lib override_command
PASS cargo fmt --check -p dev_container
PASS git diff --check
```

### Continuation, Windows path normalization review

- `2610332077ee05f4c9e2e1caa7d80a4dcde3deb6`: B, local commit
  `8cc7a956f8`. ZZZ has no upstream `crates/path`; its equivalent
  `PathStyle` implementation lives in `crates/util/src/paths.rs`. The local
  port lexically normalizes Windows drive, UNC, rooted, and relative paths,
  treating both slash forms as separators without consulting host-platform
  `std::path::Path` parsing. The upstream `crates/path` file was omitted
  because that crate is absent from ZZZ.

Verification:

```text
PASS cargo fmt --check -p util
PASS cargo check --locked -p util
PASS cargo test --locked -p util --lib test_normalize_windows_path_regardless_of_host_platform
PASS git diff --check
```

### Continuation, editor, UI, ACP, and dev-container review

- `5f180e06dc4e776d016ee7a52a39f0b00731af1a`: A, local commit
  `d32e10d167`. Added the `multiple_selections` editor key context; the
  complete three-line change cherry-picked cleanly.
- `58a3c0fa0e9e32d1d2e554b56e8da969fd1d3621`: A, local commit
  `a318fc4c6a`. Corrected project-panel dock defaults in the two local docs;
  settings already use `right`.
- `864ff0ba3fb1bbb0c8074e63156ade8c5f835347`: B, local commit `a5674342f5`.
  String-form Dev Container lifecycle commands now execute through
  `/bin/sh -c`; array forms remain direct. Upstream fixture rewrites and
  manifest formatting were omitted because local fixtures independently
  diverged.
- `a5615f092d82acde9accf56e675ba0c6444bd1fe`: B, local commit `0d0610967a`.
  Dock settings changes clear persisted panel size when `default_size` changes.
  The behavior was adapted to ZZZ's current four-subscription Dock entry.
- `b209000d28820ed69fe5fa9c0749a12c6a44f0f4`: A, local commit `e72a6785e3`.
  The installer no longer maps 32-bit Linux architectures to incompatible
  64-bit artifacts.
- `4f047acc1780eceed5be1f3dfd3f9ee652ed14cb`: C. Its V4
  `prediction_edits_for_single_file_diff` and cursor-marker machinery are not
  present in ZZZ; the active edit-prediction path uses a different API, so no
  safe symbol-level port exists.
- `9c7a5c9485669f57a22bc9c07b0f856cf1829a34`: A, local commit `be9e42c6a3`.
  Documented the already-supported `--existing` CLI option.
- `56cf49bc1afe05bbc777a7df5a01f79299ab4956`: B, local commit `075a221ba2`.
  Added `variable.parameter` colors to all local Gruvbox variants after a
  cherry-pick conflict caused by independently changed theme data.
- `779c35d256a320ddc7611706cb54c2df93618e68`: B, local commit `64f759a737`.
  Centralized `PAGER` and `GIT_PAGER` overrides for ACP terminals; native ZZZ
  agent framing was omitted.
- `5e03f2d387e629237c10a4e4fed881b64abd8fea`: B, local commit `e9872283b4`.
  Solo diff searches no longer expose the generic multibuffer fold control;
  the predicate was adapted to ZZZ's existing split-editor state.
- `2ec2997789ead364eb53b23972127d7b82b7002f`: A, already equivalent in the
  existing-connection remote action; upstream telemetry was omitted.
- `1ac840ab2f3ef68c832ad8b4c7789a8d7f5e04a2`: B, local commit `fc016550e1`.
  WSL-only dropped-file translation uses the existing local path helper.
- `c7aea6cbbd43a5849c4f8be6cbd74789e9ca48c6`: C. Required external-drag
  GPUI APIs are absent from ZZZ, so the attempted port was reverted.
- `21f16f7b5b968092cf4cd9cb38684a2854834fda`: C. The full diff is a broad
  crate extraction, Cargo.lock/workflow churn, and incremental-build policy;
  it has no independent user-facing behavior and would import unrelated
  dependency and build-graph changes.

Verification:

```text
PASS cargo fmt --check -p editor -p workspace -p dev_container -p acp_thread -p search
PASS cargo check --locked -p editor
PASS cargo check --locked -p workspace
PASS cargo check --locked -p dev_container
PASS cargo check --locked -p acp_thread
PASS cargo check --locked -p search
PASS cargo test --locked -p search --lib test_uses_primary_left_when_in_multi_buffer
PASS cargo test --locked -p dev_container --lib string_lifecycle_commands_use_shell
PASS git diff --check
FAIL cargo test --locked -p dev_container --lib
     Four existing fixture-equality tests still expect pre-spec string
     tokenization; the focused shell regression passes and no unrelated
     fixture rewrite was imported.
PASS python -m json.tool assets/themes/gruvbox/gruvbox.json
PASS docs page Prettier check for the audit page
```

### Continuation, terminal, Vim, and editor settings review

- `8e18ab0cd7cd8471a84d8d337bedeaefadd79a1e`: A, local commit `65b181d67d`.
  POSIX and Fish ShellBuilder stdin redirection now executes before parsing
  the command; the complete safe change cherry-picked cleanly.
- `f25b256f2c10e1b638031a3d8d5a524056ebf268`: B, local commit `872d9480f3`.
  Added the tree-branch glyph to Alacritty semantic escape characters in
  ZZZ's consolidated terminal module. The upstream file split and platform
  test placement were omitted.
- `2d9e6278e7cb78c974918e34a2818821668f9322`: B, local commit `f963ba68e9`.
  Refreshed selection anchors after the Helix normal-mode multi-key transition.
  The unrelated Windows notification test was omitted after test-file conflict.
- `35cb7558a9d9a6f2eaf31ce2e4dce4a0575820ef`: B, local commit `338cf158cb`.
  Added the optional `gutter.git_gutter_width` setting through local settings,
  editor layout, settings UI, defaults, VS Code import, and documentation.
  The localized settings-page insertion was adapted after cherry-pick conflict.
- `f56ff65c92b6346c16f1fed846fc736b39d9c71f`: A, already equivalent after
  the later upstream punctuation revert; no transient movement behavior was
  imported.
- `4b407d0fe4c6a8a129cc9bb82801c0af4d75f077`: A, already equivalent. The
  current ZZZ movement implementation matches the reverted upstream behavior.
- `66ed3027b8ca7fed0feeee91d1ce6346ccd4ac39`: C. The change is confined to
  the excluded native `agent_ui` panel and has no ACP-only surface.
- `a8cae3bd77f6e1c1dde98bb1dcebba0d254dbd71`: A, local commit `750b31956c`.
  GPUI now invalidates standalone-modifier synthesis after multi-modifier
  gestures; the focused regression test passed.
- `38df25d54c6f3b3ad8b94312405a53978fd496fc`: C. The complete diff changes
  only the benchmark/test dispatcher readiness path and adds no runtime UI
  behavior; importing it would add benchmark infrastructure without a local
  caller.
- `00cba838ad4e0be4b6176438551b72b2d512e9f8`: C. The 1,500-line Mermaid,
  GPUI SVG, dependency, and native agent-panel integration is too broad for
  a safe isolated port and includes the excluded agent UI surface.
- `d61e80b85debf0c56ecdcadf635ffa2660ddfd37`: B, local commit `747fe83761`.
  Git panel activation focus now follows the rendered Changes or History tab.
  The local test setup tuple and existing Git panel fields were preserved;
  the focused navigation test passed.
- `b914ba5cb3f4619450ad0de38adbb6b215752b74`: A, local commit `e71420c77c`.
  Python dunder variables are highlighted with `attribute.special`; the
  grammar-only change cherry-picked cleanly.

Verification:

````text
PASS cargo fmt --check -p util -p terminal -p editor -p vim -p settings_ui
PASS cargo check --locked -p util
PASS cargo check --locked -p terminal
PASS cargo test --locked -p util --lib shell_builder (5 passed)
PASS cargo test --locked -p terminal --lib (75 passed)
PASS cargo check --locked -p vim
PASS cargo test --locked -p vim --lib test_insert_line_with_multi_keybinding_to_normal
PASS cargo check --locked -p settings_ui -p editor
PASS git diff --check

## Verification

Completed successfully before this report:

```text
cargo fmt --check
cargo metadata --no-deps --format-version=1
cargo check --locked -p project_panel
cargo test --locked -p project_panel --lib undo
cargo check --locked -p gpui_linux
git diff --check
````

The project-panel undo test selection ran ten tests successfully. macOS-only
code was formatted and inspected but cannot be compiled on this Linux host.

`cargo test --locked -p editor --lib test_columnar_selection_with_soft_wrap`
did not build because the target baseline already defines
`test_go_to_definition_link_dedup` and
`test_go_to_definition_link_dedup_no_link` twice in `hover_links.rs`. This
failure is unrelated to the selected-column fix and was not changed here.

### Final continuation review

- `790dcefb01b8dd939434265c5524c8ae476ad77d`: C. The self-hosted Sweep
  prompt path is user-owned infrastructure in principle, but its 884-line
  implementation depends on the upstream edit-prediction model/event API;
  ZZZ has no matching `SweepPrompt` route, so no partial prompt rewrite was
  introduced.
- `2ec2997789ead364eb53b23972127d7b82b7002f`: A, already equivalent in the
  existing-connection `OpenRemote` action and helper. Upstream telemetry and
  redundant dev-dependency/lockfile churn were omitted.
- `1ac840ab2f3ef68c832ad8b4c7789a8d7f5e04a2`: B, local commit `fc016550e1`.
  WSL drops now translate host Windows paths with the existing project helper;
  SSH/Docker and collaboration drops remain blocked.
- `c7aea6cbbd43a5849c4f8be6cbd74789e9ca48c6`: C. A compile attempt showed
  that `ExternalDragPayload`, `FileDragPaths`, `FileDropEvent::Ended`, and
  the `PlatformWindow` external-drag methods are absent from ZZZ's GPUI.
  The attempted port was reverted and left no retained code.
- `2318f45f4c13d6e57486d25498fe2715d21b14d0`: C. The multi-workspace and
  recent-project lifecycle rewrite is too broad to isolate without importing
  upstream UI/persistence assumptions.
- `5e1fd392f67e27fa1da91bad43eef7db1a5dec23`: C. The diff-base feature
  depends on a large protocol/editor/project graph absent from the current
  ZZZ branch; no lockfile or default-setting changes were imported.
- `ce6f3af5f7ae2bbdb002c8ce5cc38e96179de811`: C. SCP/SFTP quoting changes
  are coupled to the upstream remote transport argument model and lack a
  compatible isolated call path in ZZZ.
- `8886dcb0d4ea0e145e4512d415d3260602eca99f`: C. Test-only threaded
  dispatcher plumbing has no product caller and would add benchmark
  infrastructure only.
- `2d9680fc029853f9a8bade4db2a6440af563d0e9`: B, local commit `e7f3791250`.
  Project-panel undo/redo is enabled for all release channels; the upstream
  `schemars` downgrade was omitted.
- `be8c6f9fb356dcd40a7ff06149568753e64ee171`: C. The renderer resource
  rewrite spans Metal, WGPU, DirectX, shaders, and window lifetimes and is
  not a safe isolated Linux behavior port.
- `b036368ce9b664c93493165acbed5826410c9e5a`: C. It changes native agent
  sidebar headers, outside ZZZ's ACP-only surface.
- `7759e9f93a96c3b474a739bb33a5c36606fc2ee5`: C. The auto-watch flag is an
  upstream-only feature-policy cleanup with no independent ZZZ behavior.
- `41c0f28bf9ab5c5b7318810449ac810a74f2fdd3`: C. The required
  `MergeBaseWithWorktree` variant is absent from ZZZ; the attempted port was
  reverted after `cargo check -p project` failed.
- `b5764581d2136b48fdad826a36fabe138b887369`: B, local commits
  `111dc69b11` and test-only `8f06ca2315`. Effective local MCP settings are
  re-resolved when worktrees change; the worktree-event test was adapted to
  ZZZ's existing `Starting`/`Running` state assertion.
- `4aad57fd1f002f9feeea2b7fb6229ccbcd576cb1`: C. Remote workspace return
  semantics are coupled to upstream recent-project and sidebar lifecycles.
- `e717010c82447f4cb747fd384f87fcbb178d230d`: C. WSL streaming fallback
  requires the upstream remote transfer lifecycle and was not independently
  portable.
- `184e124bba43220e1e39f73ff63f563fc8f27c76`: C. Per-line terminal CWD
  resolution requires a different terminal hyperlink/path state model.
- `1ade7854f224512dcff21da2d47b5ad01b73d9b0`: C. Regex assertion semantics
  are coupled to the upstream search engine rewrite and current ZZZ tests do
  not exercise that API.
- `c6e0868cb8f2431938843c75dc37c44280267d53`: C. Vim auto-indentation spans
  language and multibuffer APIs that are not present as an isolated change.
- `bbd198f57ba90ff35b8b01f78442c0de1c5a02fc`: C. Semantic-token ordering
  requires upstream configured-server ordering state and a large new merge
  implementation.
- `b8c75f17170b3d3e5bfd7f11881fd629caccc95f`: C. Extension provider icon
  changes are tied to an upstream dependency/UI surface absent from ZZZ.
- `a12e3c0673295d0bc93df2f8dbc45943d59620b4`: C. Removing the project-panel
  flag is coupled to the upstream flag migration and was not needed after the
  safe enablement port.
- `4601ead4164ae56828bfd7d4566f918d4b337aaa`: C. The WebGL backend is a
  large shader, renderer, and dependency addition with no isolated behavior.
- `20ce54f8e407d575cc2d873ffa1c09a08c2689f1`: A, local commit `7067429693`.
  Worktree paths are compared by pairing rather than set membership.
- `158c16f51cd1fbad5e34befe435d35ff7f8da790`: C. Workspace replacement,
  sidebar, and persistence rewrites are inseparable UI lifecycle changes.
- `655ed1385b00c01386bfbd9b7edea439eb1eec64`: A, local commit `793d392a`.
  Inactive Wayland windows no longer update IME position.
- `65a5c89a9ea2a7ed852d4ed2983621124072677b`: A, already equivalent in
  ZZZ's current parse-again/auto-indent scheduling and `EditedBufferSnapshot`.
- `c305d68c0146530787847da507e7e972e1674e1d`: C. Dependency and lockfile
  churn only; no independent Markdown behavior was retained.
- `51db7df7501e672fdfc9fa4875f43215329fc0d9`: A, already covered by the
  prior local `b60b2857` wide-table scrolling port.
- `101ca00a1352ed71ef398f21b47836565d1998e3`: B, local commit `50f5c0ba94`.
  Punctuation runs are explicit word boundaries, with skip behavior adapted
  to ZZZ's consolidated `editor.rs` action call sites.

Verification for this continuation:

```text
PASS git diff --check
PASS cargo fmt --check
PASS cargo check --locked -p gpui_wgpu
PASS cargo check --locked -p workspace
PASS cargo check --locked -p feature_flags
PASS cargo check --locked -p project (for b5764581)
PASS cargo check --locked -p editor
PASS cargo fmt --check and npx prettier --check src/development/upstream-sync-2026-08-07.md
FAIL cargo check --locked -p gpui_linux for c7aea6cb: required external-drag GPUI APIs are absent; port reverted and classified C
FAIL cargo check --locked -p project for 41c0f28b: `MergeBaseWithWorktree` is absent; port reverted and classified C
```

### Post-baseline extension through 027cf0def7

- `6b2aa1c90aeb72cadfd30ed12141ed9e2569eded`: C. The new
  `ScrollbarRevealPolicy` preserves the default behavior and has no
  continuously-growing ZZZ caller that opts into `ScrollOnly`; adding an
  unused API and its tests would provide no independent product behavior.
- `87e698fb6fd09e69a16c66ae83060ac1e3af3fd6`: C. The change is in the
  native agent thread UI and changes context-compaction scrolling outside
  ZZZ's ACP-only boundary.
- `c24358d96cdb4ce14ecbc088462295353b0103f0`: C. The diff is a broad
  dependency/lockfile upgrade plus warning-only literal changes across
  agent, collab, onboarding, and UI crates; no isolated ZZZ behavior was
  retained.
- `d356b2f5ef334f9f7b42827fd196839b9ecd532e`: C. Server-side compaction is
  implemented in the subscription OpenAI provider and uses account-bound
  credentials and the Codex subscription endpoint.
- `c95e0c510518759a1db88892261e4152f396b549`: C. Community PR track
  mappings are repository automation, outside the product runtime scope.
- `cc053a4a6fa2fd0e8793201ed9099466af1be0b1`: C. The upstream builder
  targets an AccessKit/ARIA node chain that is absent from current ZZZ GPUI;
  there is no compatible local `author_id` storage or writer path.
- `ca1ef7a4d25f6eb0ecf9809e55fadafa9a0d28c1`: B, local commit
  `2fe2e97eaf`. Unit-valued JSON-RPC params and results now accept `{}` and
  `null`, response error/result nulls are omitted, and the tests use ZZZ's
  existing `smol::channel`; the undeclared upstream `async_channel` reference
  was omitted.
- `027cf0def75e5c027504f402a6a6c0dcac11f178`: B, local commit
  `673c0c6fcd`. Markdown preview scrollbars now use the existing editor
  scrollbar visibility setting. Direct cherry-pick conflicted with local
  Markdown rendering and notification/path-link changes, so the minimal
  adaptation was committed manually.

Verification for the post-baseline extension:

```text
PASS git diff --check
PASS cargo check --locked -p lsp
PASS cargo test --locked -p lsp test_unit_ (2 passed)
PASS cargo check --locked -p markdown_preview
PASS cargo fmt --check
PASS npx prettier --check src/development/upstream-sync-2026-08-07.md
FAIL npx prettier --check src/: existing formatting issues in installation.md, migrate/vs-code.md, and reference/all-settings.md; not modified
FAIL first cargo test --locked -p lsp test_unit_: upstream test referenced undeclared async_channel; test adapted to existing smol::channel and rerun passed
CONFLICT 027cf0def7 direct cherry-pick: local Markdown preview context diverged; cherry-pick aborted and minimal B port committed
```

Follow-up query at `2026-08-07T19:16:41+02:00` returned the same
`027cf0def75e5c027504f402a6a6c0dcac11f178` head; no new commits were present
after the reviewed baseline.
