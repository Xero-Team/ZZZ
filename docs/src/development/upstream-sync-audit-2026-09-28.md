---
title: Upstream Sync Audit 2026-09-28
description: Full audit of the un-audited Zed upstream gap (May–June 2026) for missed commits.
---

# Upstream Sync Audit 2026-09-28

## Scope

- Local base: `sync/upstream-2026-09-28-backport-hide-button` at `99964453fb`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Upstream reference: `bda9c0bd43a8d235d82adb01ea5bc875b861ecfc`
- Gap window audited: `2026-05-01` through `2026-06-20`
- Reviewed baseline: unchanged at `bda9c0bd43`

The first recorded sync report (`upstream-sync-2026-07-03.md`) started at a
`2026-06-20` candidate boundary, so every upstream commit before it was
never classified. This audit covers that whole gap. It supersedes the
initial scan in an earlier revision of this file that flagged `#58133`
and `#58366` as confirmed misses before they were re-checked against the
local tree.

## Method

Upstream commits were enumerated from `bda9c0bd43` and cross-referenced
with local `main` using four independent signals:

1. PR number `(#NNNNN)` in the subject.
2. Normalized subject (PR number stripped, lowercased).
3. `git patch-id --stable` equality.
4. Verbatim presence of added code lines in the local tree.

Candidates with no signal were then read against the local source. The
gap contains `1972` upstream commits (of which `~1700` are non-merge);
`638` produced no signal, and `415` had under 50% added-line presence.
Because ZZZ removed whole surfaces (native agent runtime, edit
prediction, inline assistant, in-tree model providers, collab, cloud)
and refactored others (git panel commit history, `mermaid_render`), most
candidates are either already-equivalent locally or intentionally absent.

## Result

**No additional philosophy-safe missed commits were found.**
`c049193fd93b2c27d3e4a8a40f5e1739db4640b1` (`#54971`, ported as
`2de3161a49`) remains the only genuine miss in the window.

Corrections to the earlier backport report:

- `0711d9e4` (`#58133`): already equivalent. Local `git_panel` uses a
  `CommitHistory::Loading` enum instead of the upstream
  `commit_history_shas: Vec<Oid>`, so the stale-loading fix does not
  apply as-is.
- `5791ebc2` (`#58366`): already present. The local keymaps already use
  `GitCommit > Editor && mode == auto_height`.
- `6d605b39` (`#58188`), `b93d14ed` (`#56055`), `654a864b` (`#58062`):
  rejected. The in-tree LLM commit-message and edit-prediction surfaces
  were deliberately removed; `crates/migrator` even strips
  `commit_message_instructions` as a retired key.
- `56184851` (`#57741`), `4eab0696` (`#58341`), `4e949a1c` (`#58325`),
  `3d4d8796` (`#58514`): absent or diverged architecture
  (`paint_*_shadows`, `mermaid_render`, `mermen` rev, commit-view diff
  hunk controls).
- `137e677a` (`#58712`, Wayland IME), `7e6e731b` (`#58136`, Python
  splat), `3c0f5a04` (`#58176`, Python exceptions), `626dc16f`
  (`#58269`, title prefixes), `f99fe5d8` (`#58741`, `SHORT_SHA_LENGTH`),
  and `eb2223c0` (`#54878`, `buffer_font_fallbacks`) were all confirmed
  present despite producing no signal.

## Rejected candidates

Every candidate below is `C`. They are recorded so they are not
re-flagged on future scans. The remaining `~550` no-signal candidates
are present verbatim in the local tree (`A`) and are not listed.

| Upstream | Class | Reason                                            | Subject                                                                                            |
| -------- | ----- | ------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| 001d94d1 | C     | docs/policy not shipped                           | docs: Add more information for releasing extensions (#57261)                                       |
| 03872382 | C     | updater/nix upstream infra                        | Document `auto_update_extensions` setting (#58954)                                                 |
| 06826ef1 | C     | CI/release/dependency plumbing                    | Bump urllib3 to v2.7.0 (#58092)                                                                    |
| 0cd94e0d | C     | account/collab/cloud absent                       | Document Billing Manager role (#58447)                                                             |
| 10fc0fb5 | C     | dependency/lockfile churn                         | Update wgpu to 29.0.3 (#57086)                                                                     |
| 1226e28d | C     | native agent / ACP-only boundary                  | Add zed-cherry-pick agent skill (#57833)                                                           |
| 158378fc | C     | upstream guild/community automation               | Split out Windows in community PR area-track mapping (#58671)                                      |
| 19b91c4a | C     | CI/release/dependency plumbing                    | Stop stalebot from nagging on the same issues (#58418)                                             |
| 1b557c70 | C     | docs/policy not shipped                           | docs: Re-add missing cookie banner env var in build (#56889)                                       |
| 1d1a4bee | C     | user-facing text is localized via i18n            | Change "Ok" to "OK" in UI (#58744)                                                                 |
| 1d217ee3 | C     | CI/release/dependency plumbing                    | ci: Fix caching of release jobs (by having it in the first place) (#59200)                         |
| 1eefc3b9 | C     | upstream guild/community automation               | Add upvotes to the community PR board (#58645)                                                     |
| 21212496 | C     | account/collab/cloud absent                       | collab: Add a warning about modifying database schema files (#58468)                               |
| 216980f6 | C     | upstream-only infra or diverged surface           | zed: Add `OpenStatusPage` action (#57316)                                                          |
| 24fd1015 | C     | account/collab/cloud absent                       | Sort guild members case insensitively (#57977)                                                     |
| 27c566c2 | C     | docs/policy not shipped                           | Relicense Zed source code under GPL (#57948)                                                       |
| 2961f5ae | C     | native agent / ACP-only boundary                  | docs: Document external agent thread imports (#58242)                                              |
| 2ea99a81 | C     | upstream guild/community automation               | Add new area labels to track mapping (#58083)                                                      |
| 302ddfac | C     | CI/release/dependency plumbing                    | Bump dependencies with warnings (#55614)                                                           |
| 3115298f | C     | native agent / ACP-only boundary                  | agent_panel: Hide thread title edit button outside thread view (#56833)                            |
| 3c8fc259 | C     | native agent / ACP-only boundary                  | docs: Update agent server extension documentation (#58306)                                         |
| 3d2f7adb | C     | model provider surface absent                     | Document provider safety retention for designated Zed-hosted models (#58967)                       |
| 3d4d8796 | C     | upstream-only infra or diverged surface           | git_ui: Disable CommitView diff hunk controls (#58514)                                             |
| 3f5705b9 | C     | CI/release/dependency plumbing                    | extension_ci: Bump extension CLI version to `9ee3c50` (#58785)                                     |
| 4819faaa | C     | upstream guild/community automation               | Only notify slack on confirmed and common P0/P1s (#58399)                                          |
| 48db5261 | C     | CI/release/dependency plumbing                    | Bump wasm-bindgen to 0.2.120 (#56231)                                                              |
| 4c0717fa | C     | upstream guild/community automation               | PR board: Stop touching archived items (#59428)                                                    |
| 4e949a1c | C     | mermaid infra diverged                            | mermaid: Fix stack overflow when laying out deeply nested subgraphs (#58325)                       |
| 4eab0696 | C     | mermaid infra diverged                            | mermaid_render: Remove spurious debug assert (#58341)                                              |
| 4fab859a | C     | native agent / ACP-only boundary                  | agent: Enable terminal output controls for everyone (#58271)                                       |
| 50d001fe | C     | docs/policy not shipped                           | Add note about extension API changes to the contributing guidelines (#58369)                       |
| 53f1ae01 | C     | docs/policy not shipped                           | docs: Remove Alpine, clean up glibc guidance, minor wording fixes (#56674)                         |
| 555ed049 | C     | CI/release/dependency plumbing                    | ci: Publish static bubblewrap in releases (#59488)                                                 |
| 56184851 | C     | upstream-only infra or diverged surface           | gpui: Make Window::paint_*_shadows pub (#57741)                                                    |
| 654a864b | C     | upstream-only infra or diverged surface           | git_ui: Do not include git commit prompt twice (#58062)                                            |
| 6aa90e75 | C     | docs/policy not shipped                           | docs: Update actions format (#54869)                                                               |
| 6b4e27a4 | C     | upstream guild/community automation               | Ping on slack when a community-related workflow fails (#59435)                                     |
| 6d605b39 | C     | native in-tree LLM commit-message surface removed | git_ui: Add setting for custom commit message instructions (#58188)                                |
| 750a94c3 | C     | dependency/lockfile churn                         | Update to wasmtime 36.0.9 (#55811)                                                                 |
| 7eda3820 | C     | docs/policy not shipped                           | Include icon guidelines in PR templates and contribution docs (#58855)                             |
| 7f826e82 | C     | upstream-only infra or diverged surface           | Fix grammatical errors throughout the documentation (#58183)                                       |
| 81f818aa | C     | updater/nix upstream infra                        | nix: Go around a linker issue on Darwin (#58070)                                                   |
| 838fea16 | C     | docs/policy not shipped                           | docs: document llvm-objcopy --strip-debug step when self-building remote_server (#57655)           |
| 8bdd78e0 | C     | model provider surface absent                     | opencode: Update Free models (#56328)                                                              |
| 8fced01b | C     | CI/release/dependency plumbing                    | Pin workflow actions and service images to immutable SHAs (#58964)                                 |
| 969a67fc | C     | account/collab/cloud absent                       | Document Pro threshold billing behavior (#58320)                                                   |
| 9709caec | C     | CI/release/dependency plumbing                    | glsl: Bump to v0.2.4 (#58704)                                                                      |
| 971977a9 | C     | CI/release/dependency plumbing                    | extension_ci: Bump extension CLI version to `ca0fd8d` (#58476)                                     |
| a225d510 | C     | CI/release/dependency plumbing                    | Add harden-runner in audit mode to run_tests Linux jobs (#59446)                                   |
| a3669a29 | C     | updater/nix upstream infra                        | nix: Fix dev shell on Darwin (#58032)                                                              |
| a65e6773 | C     | native agent / ACP-only boundary                  | ep: Make `tree-sitter` dependency optional in `edit_prediction_metrics` (#57829)                   |
| a851320e | C     | CI/release/dependency plumbing                    | ci: Revalidate zed.dev after release (#59422)                                                      |
| a9296b9a | C     | native agent / ACP-only boundary                  | docs: Update information regarding rules/skills (#57824)                                           |
| aa6062ed | C     | native agent / ACP-only boundary                  | Stop loading deprecated agent rules (#57844)                                                       |
| b0911ccc | C     | model provider surface absent                     | opencode: Model updates (#57556)                                                                   |
| b1c1e3fc | C     | docs/policy not shipped                           | Add AI Policy to CONTRIBUTING.md (#58397)                                                          |
| b72e57da | C     | upstream guild/community automation               | Duplicate Bot: Handle failure modes better (#57663)                                                |
| b76e0bc9 | C     | upstream-only infra or diverged surface           | Fix hang detection crash when foreground stats are missing (#58464)                                |
| b7b1d1a2 | C     | upstream guild/community automation               | Duplicate Bot: Reduce noise (#58074)                                                               |
| b8dce970 | C     | CI/release/dependency plumbing                    | extension_ci: Bump extension CLI version to `2a00db0` (#57098)                                     |
| b93d14ed | C     | upstream-only infra or diverged surface           | editor: Don't bypass show_edit_prediction when navigating diagnostics (#56055)                     |
| bb460d5f | C     | upstream guild/community automation               | Duplicate Bot: Add more context for triagers (V3) (#57647)                                         |
| bedfe32f | C     | native agent / ACP-only boundary                  | agent_ui: Clarify multi-root agent warning text (#57874)                                           |
| c029cc43 | C     | CI/release/dependency plumbing                    | Bump `convert_case` to v0.11.0 (#58000)                                                            |
| c115be36 | C     | dependency/lockfile churn                         | Update wasmtime to 36.0.8 (#55611)                                                                 |
| c19c89a3 | C     | model provider surface absent                     | opencode: Model updates (#57792)                                                                   |
| c1b45aaa | C     | CI/release/dependency plumbing                    | Bump notify to fix a hang when watching or unwatching paths (#59047)                               |
| c32c037c | C     | upstream-only infra or diverged surface           | Update Pull Request template (#59062)                                                              |
| c35996db | C     | native agent / ACP-only boundary                  | agent: Promote experimental agent system prompt (#56543)                                           |
| c4e30593 | C     | model provider surface absent                     | language_models: Clear speed for OpenAI-compatible providers that don't support fast mode (#59496) |
| c70e4a2f | C     | model provider surface absent                     | opencode: Remove deprecated models (#56278)                                                        |
| c88c83d6 | C     | dependency/lockfile churn                         | Update `merman` tag SHA (#59360)                                                                   |
| cd2b8071 | C     | native agent / ACP-only boundary                  | docs: Clarify how to open new threads from the empty state (#56640)                                |
| d6cc34c1 | C     | native agent / ACP-only boundary                  | agent: Cleanup edit_file evals (#55750)                                                            |
| d8278a56 | C     | CI/release/dependency plumbing                    | ci: Add merge queue trigger to danger workflow (#58467)                                            |
| db6039d8 | C     | native agent / ACP-only boundary                  | agent: Remove open tool (#56295)                                                                   |
| dccea211 | C     | updater/nix upstream infra                        | auto_update: Add NixOS rsync install hint (#56097)                                                 |
| dd3521df | C     | upstream-only infra or diverged surface           | Limit top context identifiers and diagnostics (#58870)                                             |
| e0b43b37 | C     | account/collab/cloud absent                       | docs: Restructure nav and add Zed Business section (#51915)                                        |
| e2e7a676 | C     | dependency/lockfile churn                         | Update dependency requests to v2.33.0 [SECURITY] (#58093)                                          |
| e83c2d94 | C     | upstream guild/community automation               | Add meta signals to the community PR board (#58635)                                                |
| eb2223c0 | C     | CI/release/dependency plumbing                    | gpui_wgpu: Bump cosmic-text to v0.19.0 (#56988)                                                    |
| ebc46d7e | C     | dependency/lockfile churn                         | Update rmcp and rpassword (#56096)                                                                 |
| ed41a02d | C     | native agent / ACP-only boundary                  | ep: Make retrieved context limit configurable (#58405)                                             |
| f65b6f6e | C     | native agent / ACP-only boundary                  | agent_ui: Fix crash when filtering tools in the agent profile tool picker (#58299)                 |
| f8226fd1 | C     | native agent / ACP-only boundary                  | agent: Remove experimental plan and title agent tools (#58824)                                     |

## Verification

| Check                                           | Result  |
| ----------------------------------------------- | ------- |
| `git status --porcelain` before the audit       | PASS    |
| Upstream enumeration from the reviewed head     | PASS    |
| Source spot checks for every reviewed candidate | PASS    |
| `./script/backfill-upstream-ledger`             | PASS    |
| `./script/check-upstream-ledger`                | PASS    |
| macOS / Windows runtime checks                  | NOT RUN |

No product code changed in this audit beyond the already-landed
`2de3161a49` backport.
