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
