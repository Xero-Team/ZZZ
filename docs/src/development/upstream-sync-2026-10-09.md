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
