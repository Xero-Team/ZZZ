---
title: Upstream Sync 2026-10-09
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-10-09

## Scope

- Target branch: `sync/upstream-2026-10-09` from local `main` at
  `84356b5c210ae68e79f68a1ece2395779fe80d61`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `279fe070bb389b79652e52065b2f001edcc0b11b`
- Reviewed head: `e7c03209fc16e4e5e1221f632b6b2d14c319c86c`
- Live upstream head queried: `f0b38ac75ea769b75287ff1370242d88b3878c2d`
- Query time: `2026-10-09T12:34:02+02:00`
- Reviewed range: the first 20 commits after `279fe070`, oldest first
- Remaining after this batch: 91 commits through the queried live head

The prior reviewed baseline is an ancestor of `FETCH_HEAD`. This batch stays
fixed at twenty commits; commits added after the reviewed head remain queued.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                         |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------- |
| eca87ab896 | B     | 0c3e3dfcdf   | Removed the redundant effect-flush guard in ZZZ's divergent `App` layout.                                           |
| ad4512a2e5 | B     | e9191bd1e1   | Routed inactive editor, Markdown preview, and SVG preview tab actions to their owning pane.                         |
| e4945b5d09 | C     | --           | Reconnection reuse needs `RemoteConnection::remote_platform`, which ZZZ does not expose.                            |
| 2117a376b9 | B     | 635452e8af   | Reused tab point cursor scans and fold chunks through ZZZ's current TabMap APIs.                                    |
| 8385157b2f | A     | 3055ef2e9a   | Clean absorption of robust local Rust documentation URL handling and regression coverage.                           |
| 2ff34954a0 | C     | --           | Native-agent terminal-tool test only; the native agent path remains absent.                                         |
| 96dec006df | C     | --           | Documents upstream Homebrew casks that do not ship ZZZ.                                                             |
| d32fa003ab | C     | --           | Native-agent grep-tool configuration has no ACP-only ZZZ caller.                                                    |
| 96837d78cb | B     | 111570dcac   | Added the developer-tool category to all eight existing ZZZ macOS bundle variants.                                  |
| c62f92d6aa | A     | 6d3b24d5e1   | Clean absorption preventing workspace reuse across different remote hosts.                                          |
| 70db535ccf | A     | --           | Already equivalent: `StackFrameList::build_entries` clears `self.error` on its successful path.                     |
| cad56bd65a | B     | da8b71088c   | Registered Env files without a shell LSP, retaining ZZZ docs and local language coverage.                           |
| 533fe08cf  | C     | --           | Broad TOML upgrade and `SharedString` migration is dependency churn and a preparatory cross-workspace type rewrite. |
| cb811b48c7 | B     | e6901c689e   | Deferred bracket highlight invalidation until the replacement parse result is ready.                                |
| 7aec07ccde | A     | 447bd88a3f   | Clean absorption of Python debug adapter `envFile` processing.                                                      |
| d6b7f24b7  | C     | --           | Linux release and Sentry-symbol workflow change has no retained ZZZ release pipeline.                               |
| 763287731e | A     | 9db64fbf50   | Clean absorption aligning Markdown task checkboxes to their first text line.                                        |
| 1dd9bbfd71 | C     | --           | Global element-id caching is entangled with Zed's superseded Frame and AccessKit-writer architecture.               |
| ed54236e41 | A     | 0a19ee17aa   | Clean absorption of DAP error-format variable interpolation.                                                        |
| e7c03209fc | A     | 2795c1caf5   | Clean absorption preventing Rust declarative-macro completion flicker.                                              |

Totals: seven `A`, six `B`, seven `C`.

## Applied Work

The six direct `A` changes used `git cherry-pick -x -s`. `70db535ccf` is
already equivalent, so it creates no local code change. Each `B` commit has
`Upstream`, `Retained`, and `Omitted` trailers.

- `0c3e3dfcdf` makes outermost GPUI updates flush effects based only on
  `pending_updates`; ZZZ's concurrent App layout was retained.
- `e9191bd1e1` gives inactive tabs a focus target and resolves preview actions
  against that tab's item and pane rather than the active pane.
- `635452e8af` shares tab-stop cursors in ordered TabMap synchronization while
  preserving ZZZ's current cursor signature.
- `111570dcac` sets `category = "Developer Tool"` for Dev, Nightly, Preview,
  and Stable ZZZ bundle metadata and their binary-specific variants.
- `da8b71088c` gives `.env` its own grammar registration, redaction queries,
  highlighting, docs, and test coverage; `.envrc` stays Shell Script.
- `e6901c689e` preserves old bracket-color highlights while parsing after an
  edit, then atomically clears and replaces them. ZZZ keeps its local
  contrast-adjusted accent colors.

## Rejected Work

- `e4945b5d09` failed `cargo check -p remote`: the retained reconnect path
  calls upstream-only `RemoteConnection::remote_platform`. The attempted
  cherry-pick was removed completely.
- `2ff34954a0` and `d32fa003ab` target the deliberately absent native-agent
  tool path, so they have no ACP-only caller.
- `96dec006df` would advertise a Zed Homebrew cask rather than a package that
  installs ZZZ.
- `533fe08cf` combines a dependency upgrade with a broad internal type
  migration, including collaboration-only tests, for a later upstream change.
- `d6b7f24b7` modifies release automation and Sentry symbol upload behavior,
  neither of which is a shipped local editor path.
- `1dd9bbfd71` conflicts across ZZZ's frame-builder and completed semantic
  snapshot boundaries. Its implementation assumes the absent upstream
  Frame/AccessKit writer path, so a partial cache port is not isolatable.

## Verification

| Check                                                                 | Result  | Notes                                                                                      |
| --------------------------------------------------------------------- | ------- | ------------------------------------------------------------------------------------------ |
| Live `git ls-remote` and `git fetch --no-tags`                        | PASS    | `FETCH_HEAD=f0b38ac75ea769b75287ff1370242d88b3878c2d`                                      |
| Previous baseline ancestry                                            | PASS    | `279fe070` is an ancestor of `FETCH_HEAD`                                                  |
| `cargo check --locked -p gpui`                                        | PASS    | effect-flush B port                                                                        |
| Preview tab action checks and context-menu tests                      | PASS    | `editor`, `markdown_preview`, `svg_preview`, and `workspace` compile; 3 focused tests pass |
| `cargo check --locked -p editor` and TabMap regression                | PASS    | `test_tab_map_sync_edits` passes                                                           |
| Open Docs regression                                                  | PASS    | `test_open_docs` passes                                                                    |
| `cargo metadata --locked --no-deps` and `cargo check --locked -p zzz` | PASS    | macOS bundle metadata parses and ZZZ compiles                                              |
| Cross-host workspace regression                                       | PASS    | `test_find_existing_workspace_does_not_reuse_workspace_from_another_remote` passes         |
| Env language regression and targeted Prettier                         | PASS    | `.env` / `.envrc` / Shell Script test and changed docs format check pass                   |
| Bracket reparse regression                                            | PASS    | `test_bracket_colorization_retained_during_reparse` passes                                 |
| Python debug `envFile` regressions                                    | PASS    | file loading and explicit-env precedence tests pass                                        |
| `cargo check --locked -p markdown`                                    | PASS    | task-list checkbox alignment                                                               |
| DAP interpolation regressions                                         | PASS    | interpolation and response-error tests pass                                                |
| Rust completion-label regressions                                     | PASS    | Rust and Go focused label tests pass                                                       |
| `git diff --check` and `cargo fmt --all -- --check`                   | PASS    | final source checks before report creation                                                 |
| macOS and Windows runtime checks                                      | NOT RUN | host is Linux                                                                              |
| `cargo test --workspace`                                              | NOT RUN | not requested                                                                              |

The general reviewed baseline advances to
`e7c03209fc16e4e5e1221f632b6b2d14c319c86c`.

## Continued Batch

### Scope

- Target branch: `sync/upstream-2026-10-09`, continuing the same local base
  `84356b5c210ae68e79f68a1ece2395779fe80d61`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `e7c03209fc16e4e5e1221f632b6b2d14c319c86c`
- Reviewed head: `aecb6364b3de29ff32ea9f26cb8d85f95969bc28`
- Live upstream head queried: `1c9dd3e70f60e68b47f52845aee1da4be4ec13b8`
- Query time: `2026-10-09T15:09:17+02:00`
- Reviewed range: the next 20 commits after `e7c03209`, oldest first
- Remaining after this batch: 72 commits through the queried live head

The prior reviewed baseline is an ancestor of `FETCH_HEAD`. This batch stays
fixed at twenty commits; later commits remain queued.

### Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                |
| ---------- | ----- | ------------ | -------------------------------------------------------------------------------------------------------------------------- |
| d544470d1d | C     | --           | Workspace-wide dependency and lockfile rewrite includes absent commercial, collaboration, and native-agent surfaces.       |
| cd5bdc41c4 | C     | --           | Moves terminal tabs into the deliberately absent native Agent Panel.                                                       |
| dff535449c | C     | --           | Preparatory ACP v2 source plumbing has no negotiated v2 ACP caller and crosses native-agent persistence and adapter paths. |
| 3f81fdc4da | A     | 842f57104d   | Clean absorption correcting the displayed `format_on_save` default.                                                        |
| b92c029538 | B     | 59994758ed   | Made Markdown preview panes cloneable while preserving ZZZ workspace-relative image resolution.                            |
| a6169ca969 | B     | 3423e05f10   | Separated remote stdin, stdout, and log forwarding in ZZZ's divergent server loop.                                         |
| 204362325c | C     | --           | Upstream release, Sentry, bundling, and `zed` package tooling are not a retained local pipeline.                           |
| eec3376016 | C     | --           | The capability check needs `Anchor::buffer_id`, absent from ZZZ's consolidated editor implementation.                      |
| e7c8cbeed6 | B     | 954e611e5e   | Added the documented `suggest_extensions` opt-out to ZZZ's divergent suggestion module.                                    |
| 0c70487f83 | B     | 06055224af   | Removed the legacy PHP language-ID override while retaining ZZZ's current adapter APIs.                                    |
| 8eb2fd211f | C     | --           | `tabular_data_preview` is absent locally, so there is no current caller for the comparator.                                |
| 8fda6e4653 | C     | --           | Requires the rejected v2 content-source definitions from `dff535449c`.                                                     |
| cac9d17a6c | C     | --           | The upstream commit-context-menu component has no local caller; importing it would add an unused public menu path.         |
| 6810cde896 | B     | bcc45e78cd   | Ported value-kind syntax coloring through ZZZ's localized debugger variable rows.                                          |
| 285d975043 | B     | f9aad5ee6d   | Retained independent migration and theme-cache startup work; the deleted upstream `crates/zed` path remains omitted.       |
| 7c4f6ddad1 | A     | 26e205db8b   | Clean absorption fixing JetBrains `Shift-Enter` precedence in buffer search.                                               |
| 93a9c1a0bc | C     | --           | Nix WebRTC update is collaboration infrastructure, which ZZZ does not ship.                                                |
| 2240542744 | C     | --           | Single-file matching depends on upstream's divergent `File` and language-registry path architecture.                       |
| a1b71072e5 | C     | --           | Lockfile-only cleanup depends on the omitted upstream `crates/zed` Nix dependency change.                                  |
| aecb6364b3 | B     | 6d6d503124   | Captured inlay-hint filtering snapshots after local response conversion in ZZZ's VFS-aware dispatcher.                     |

Totals: two `A`, seven `B`, eleven `C`.

### Applied Work

The two direct `A` changes used `git cherry-pick -x -s`. Each `B` commit
has `Upstream`, `Retained`, and `Omitted` trailers.

- `3423e05f10` keeps remote RPC handling responsive when either output stream
  blocks and adds Linux regression coverage. The unrelated deleted local
  log-rotation test module was not restored.
- `59994758ed` adapts the Markdown preview split clone to retain ZZZ's
  `workspace_directory` constructor state for workspace-relative image links.
- `954e611e5e` adds `suggest_extensions`, defaulting to `true`, and makes both
  generic and Emmet suggestions honor it. The setting is documented; the
  divergent settings-page and notification split-menu UI was not imported.
- `06055224af` uses the extension manifest's PHP language IDs instead of a
  legacy adapter special case.
- `bcc45e78cd` colors debugger values according to their declared or inferred
  boolean, string, numeric, or null-like kind while preserving local labels.
- `f9aad5ee6d` skips formatting already-identical SQL migrations and reuses the
  immutable neutral color scale. Upstream app-startup and settings changes tied
  to absent `crates/zed` code were omitted.
- `6d6d503124` moves the local inlay-hint snapshot after response conversion
  and adds coverage for a buffer edit while the hint request is pending.

### Rejected Work

- `d544470d1d` is broad dependency churn spanning absent product surfaces;
  isolating its mechanical changes would not yield a coherent local behavior.
- `cd5bdc41c4` targets the native Agent Panel, which remains absent by policy.
- `dff535449c` and its dependent `8fda6e4653` add protocol-v2 source storage,
  projection, and command definitions before a current v2 ACP transport exists.
- `204362325c`, `93a9c1a0bc`, and `a1b71072e5` are upstream release,
  collaboration, Nix/WebRTC, or lockfile infrastructure outside ZZZ's product
  paths.
- `eec3376016` was attempted as a B port, then completely reverted when
  `cargo check -p editor` failed because local `multi_buffer::Anchor` lacks the
  required `buffer_id` API.
- `8eb2fd211f` targets an absent crate. `cac9d17a6c` needs an absent upstream
  commit-context-menu component and has no local menu caller.
- `2240542744` requires upstream's filesystem-path API across the divergent
  `File`, worktree, and language matching abstractions.

### Verification

| Check                                                                           | Result   | Notes                                                                              |
| ------------------------------------------------------------------------------- | -------- | ---------------------------------------------------------------------------------- |
| Live `git ls-remote` and `git fetch --no-tags`                                  | PASS     | `FETCH_HEAD=1c9dd3e70f60e68b47f52845aee1da4be4ec13b8`                              |
| Previous baseline ancestry                                                      | PASS     | `e7c03209` is an ancestor of `FETCH_HEAD`                                          |
| Markdown preview split regression                                               | PASS     | `splitting_preview_clones_into_new_pane` passes                                    |
| Remote-server blocked stderr and stdout regressions                             | PASS     | Both focused `remote_server` tests pass on Linux                                   |
| `cargo check --locked -p debugger_ui`                                           | PASS     | Debugger value-color B port                                                        |
| `cargo check --locked -p extensions_ui`, its 3 library tests, and docs Prettier | PASS     | Extension-suggestion setting B port                                                |
| `cargo check --locked -p language_extension`                                    | PASS     | PHP manifest language-ID B port                                                    |
| `cargo check --locked -p sqlez -p theme` and `cargo test --locked -p sqlez`     | PASS     | 18 migration/database tests pass                                                   |
| Inlay-hint stale-snapshot regression                                            | PASS     | `test_fetch_inlay_hints_after_buffer_edit` passes                                  |
| JetBrains buffer-search regression                                              | PASS     | `test_shift_enter_selects_previous_match_with_jetbrains_keymap` passes             |
| Attempted editor hunk-controls B port                                           | CONFLICT | Reverted after missing local `Anchor::buffer_id` prevented `cargo check -p editor` |
| `git diff --check` and `cargo fmt --all -- --check`                             | PASS     | Final source checks                                                                |
| macOS and Windows runtime checks                                                | NOT RUN  | Host is Linux                                                                      |
| `cargo test --workspace`                                                        | NOT RUN  | Not requested                                                                      |

The general reviewed baseline advances to
`aecb6364b3de29ff32ea9f26cb8d85f95969bc28`.

## Continued Batch II

### Scope

- Target branch: `sync/upstream-2026-10-09`, continuing the same local base
  `84356b5c210ae68e79f68a1ece2395779fe80d61`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `aecb6364b3de29ff32ea9f26cb8d85f95969bc28`
- Reviewed head: `73be6ac6b1975f428328bbf7fe185d3b671afec4`
- Live upstream head queried: `e778eec483d6420d09d9b7e401ceda3559daf7d9`
- Query time: `2026-10-09T15:56:01+02:00`
- Reviewed range: the next 20 commits after `aecb6364`, oldest first
- Remaining after this batch: 54 commits through the queried live head

The previous baseline and the new reviewed head are ancestors of `FETCH_HEAD`.
This batch remains fixed at twenty commits; later commits remain queued.

### Decisions

| Upstream   | Class | Local commit           | Disposition                                                                                                         |
| ---------- | ----- | ---------------------- | ------------------------------------------------------------------------------------------------------------------- |
| 9df33dea42 | C     | --                     | The v2 shared-session migration has no negotiated v2 ACP handler and entangles local v1 title state with native UI. |
| 0849124bc2 | C     | --                     | Cross-crate snapshot rewrite includes native-agent and Copilot paths; no coherent, independently testable slice.    |
| c31280f1b8 | A     | --                     | Already equivalent: local `3dd96d70` already upgrades active `rustls` to 0.23.45.                                   |
| 24aabd3fa6 | C     | --                     | Community-board label automation is upstream maintenance infrastructure.                                            |
| 2a4326a256 | C     | --                     | Duplicate-issue bot tracking is upstream maintenance infrastructure.                                                |
| 2239fa76b1 | C     | --                     | New `Label::text_center` public API has no current ZZZ caller.                                                      |
| 7c7b171ff  | B     | db6e989c54             | Copy buttons now use parsed code text, excluding enclosing blockquote and list syntax.                              |
| 403589312e | C     | --                     | ACP v2 prompt-capability models have no negotiated v2 caller and include native-agent producers.                    |
| 78a0da201  | B     | 1b276e217f             | ACP defaults now apply sequentially from each complete server response and refresh local watchers.                  |
| 53421b7f3  | A     | 68e5f2259f             | Clean absorption of Go package-level test gutter support.                                                           |
| ba8159b4d3 | C     | --                     | Wezel binary-size measurements are upstream release/benchmark infrastructure.                                       |
| 09745431b5 | C     | --                     | Hosted-model pricing documents a commercial hosted offering that ZZZ does not promote.                              |
| ef33e891bf | B     | ffb1eec8b7             | Added localized `mermaid_font_family` configuration through ZZZ's flattened theme settings.                         |
| fd1e8f04b6 | A     | --                     | Upstream has no tree delta; local `task_list_marker_for_item` already handles the loose-list Paragraph case.        |
| 0418cb5b60 | B     | c246c4468f             | Single-file worktrees no longer register a file URI as an LSP workspace folder.                                     |
| 85b3aa83a2 | C     | --                     | ACP v2 authentication definitions are preparatory shared models without a local v2 transport.                       |
| fdc1005e71 | B     | 756b429b0e, 85ff165ec6 | Remote Windows linked-worktree paths and labels use their declared path style on Unix hosts.                        |
| e7526b378a | B     | 7f404175fb             | Thinking-preview fade overlay no longer intercepts ACP Markdown selection or copying.                               |
| f87356210a | C     | --                     | ACP v2 notice definitions add unused v2 shared state without a negotiated local v2 session.                         |
| 73be6ac6b1 | C     | --                     | ACP v2 elicitation values require the unadopted v2 schema and native-agent producers.                               |

Totals: three `A`, six `B`, eleven `C`.

### Applied Work

The direct `A` change used `git cherry-pick -x -s`. `c31280f1b8` and
`fd1e8f04b6` are already-equivalent/no-tree-delta decisions and create no code
commit. Each `B` commit has `Upstream`, `Retained`, and `Omitted` trailers.

- `db6e989c54` collects text from parsed Markdown events while rendering a
  copy-enabled code block, so copied nested code excludes surrounding Markdown
  container syntax. The unrelated upstream test-suite reorganization was not
  carried across ZZZ's renderer test layout.
- `1b276e217f` applies ACP configuration defaults one at a time, replaces local
  options with the server response, notifies watchers, prevents repeated
  attempts in the same state, and keeps the task on the session. ZZZ retains
  its v1 wire boundary and beta boolean-default policy.
- `ffb1eec8b7` adds the nullable `mermaid_font_family` setting, its UI control,
  settings import default, English and Simplified Chinese strings, and docs.
  It falls back to the UI font.
- `c246c4468f` derives LSP workspace folder URIs only from directory worktree
  entries and adds a fake-Python-server regression for a file worktree.
- `756b429b0e` and `85ff165ec6` use explicit remote path style for worktree
  names and destination validation, including a Windows path regression on a
  Unix host. ZZZ keeps its current `git_ui` location instead of importing the
  upstream `git_ui_core` crate move.
- `7f404175fb` removes the pointer-blocking behavior from the constrained
  thinking-preview fade while preserving its visual gradient and scrolling.

### Rejected Work

- `9df33dea42`, `403589312e`, `85b3aa83a2`, `f87356210a`, and `73be6ac6b1`
  are ACP v2 shared-model migrations. ZZZ's current ACP sessions intentionally
  negotiate v1, so these introduce unused v2 state and, in several cases,
  native-agent producers rather than complete ACP-only behavior.
- `0849124bc2` combines fourteen independent snapshot rewrites across project,
  editor, worktree, native-agent tooling, and Copilot. Dropping the excluded
  paths leaves no coherent invariant to test.
- `24aabd3fa6`, `2a4326a256`, and `ba8159b4d3` are community-bot, automation,
  or benchmark/release infrastructure rather than shipped editor behavior.
- `2239fa76b1` has no local consumer for its new public UI method.
- `09745431b5` promotes hosted model pricing, contrary to ZZZ's silent manual
  provider policy.

### Verification

| Check                                                                                              | Result  | Notes                                                                                               |
| -------------------------------------------------------------------------------------------------- | ------- | --------------------------------------------------------------------------------------------------- |
| Live `git ls-remote` and `git fetch --no-tags`                                                     | PASS    | `FETCH_HEAD=e778eec483d6420d09d9b7e401ceda3559daf7d9`                                               |
| Previous and new baseline ancestry                                                                 | PASS    | `aecb6364` and `73be6ac6` are ancestors of `FETCH_HEAD`                                             |
| Go package gutter regression                                                                       | PASS    | `go::tests::test_go_package_test_detection` passes                                                  |
| Single-file LSP workspace regression                                                               | PASS    | `test_single_file_worktree_is_not_an_lsp_workspace_folder` passes                                   |
| `cargo check --locked -p agent_servers`                                                            | PASS    | ACP configuration-default B port                                                                    |
| `cargo check --locked -p markdown -p project -p theme_settings -p settings_content -p settings_ui` | PASS    | Markdown, LSP, Mermaid, and remote-worktree B ports                                                 |
| Windows remote-worktree regression                                                                 | PASS    | `git_store::tests::test_worktree_directory_uses_windows_style_for_remote_paths_on_unix_host` passes |
| `git diff --check` and `cargo fmt --all -- --check`                                                | PASS    | Final source checks                                                                                 |
| macOS and Windows runtime checks                                                                   | NOT RUN | Host is Linux                                                                                       |
| `cargo test --workspace`                                                                           | NOT RUN | Not requested                                                                                       |

The general reviewed baseline advances to
`73be6ac6b1975f428328bbf7fe185d3b671afec4`.
