---
title: Upstream Sync 2026-10-17
description: Selective Zed upstream sync audit.
---

# Upstream Sync 2026-10-17

## Scope

- Target branch: `sync/upstream-2026-10-17`, branched from `main` at
  `b3209dc047`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `9f63850f7affbd57772a4f99554d4fd333b50a6f`
- Reviewed head: `56934ec0560bce8929025497411de7f5322fb6ca`
- Live upstream head queried: `56934ec0560bce8929025497411de7f5322fb6ca`
- Query time: `2026-10-17T12:00:00+02:00`
- Reviewed range: every commit after `9f63850f7a`, oldest first
- Remaining after this batch: 0 commits through the queried live head

The previous reviewed baseline is an ancestor of `FETCH_HEAD`. Only four
commits landed upstream since the 2026-10-16 audit, so the whole range fits
in this batch.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                       |
| ---------- | ----- | ------------ | --------------------------------------------------------------------------------------------------------------------------------- |
| 2d4c483194 | B     | a2a154efa4   | Adds the `use_display_p3` macOS window colorspace option; renderer hunk re-homed from absent `gpui_apple` to `gpui_macos`.        |
| 322cd509dc | A     | --           | Already equivalent: ZZZ's `GlobalWatcher` dispatch thread is process-global, so no watcher-owned executor can be torn down early. |
| 670a779a5e | C     | --           | Adds opt-in Subsecond hot reloading, five new dependencies, and lockfile churn with no current ZZZ caller.                        |
| 56934ec056 | C     | --           | Patches ACP v2 metadata by key; ZZZ imports only `schema::v1` and has no `acp/v2.rs` or `MetadataUpdate::V2`.                     |

Totals: one `A` (already equivalent, no code change), one `B`, two `C`
across four commits.

## Applied Work

- `a2a154efa4` is a `B` port of `2d4c483194`.

## Per-Commit Narrative

#### 2d4c483194 — Display P3 Metal colorspace (B)

The upstream change adds a `use_display_p3` workspace setting and threads it
through `WindowOptions`, `WindowParams`, and the shared Apple Metal renderer.
ZZZ has no `crates/gpui_apple`; its macOS backend still lives in
`crates/gpui_macos`, so the renderer hunk was re-homed there. The local
renderer uses the older `objc` `msg_send!` style and already depends on
`core-graphics`, so the colorspace is installed with
`CGColorSpace::create_with_name(kCGColorSpaceDisplayP3)` and a
`setColorspace:` message. The settings UI addition was localized through the
existing `lt` helper with matching `en.json` and `zh-CN.json` entries.

#### 322cd509dc — watcher-owned executor (A, already equivalent)

Upstream makes `OsWatcher` own a `BackgroundExecutor` so dropping a watcher
cannot wake a dispatch task whose scheduler is already gone. ZZZ has no
`OsWatcher`; `GlobalWatcher` runs its dispatch loop on a dedicated
process-global thread created in `global_watcher()`, and `FsWatcher` handles
only unregister callbacks. There is no watcher-owned scheduler to tear down
early, so the deadlock the upstream patch prevents cannot occur locally.

#### 670a779a5e — Subsecond hot reloading (C)

The commit introduces an opt-in `hot-reload` feature, a new
`crates/gpui/src/hot_reload.rs` module, a `gpui_hot_reload` build cfg, public
`subsecond` re-exports, and five new optional dependencies
(`async-tungstenite`, `dioxus-cli-config`, `dioxus-devtools-types`, `smol`,
`subsecond`) plus `Cargo.lock` churn. ZZZ has no caller for the feature and
no `dx serve --hot-patch` workflow, so this is upstream-only development
infrastructure rather than an existing local path.

#### 56934ec056 — ACP v2 metadata patches (C)

The commit rewrites `acp_thread` metadata handling around `acp_v2`,
`MetadataUpdate::V2`, `AgentSessionInfoUpdate`, and the v2 list notification
lane, and touches `agent_servers/src/acp/v2.rs`. ZZZ imports only
`agent_client_protocol::schema::v1 as acp`, has a flat
`agent_servers/src/acp.rs`, and has no v2 adapter or v2 metadata helper. The
required protocol and type architecture is absent.

## Verification

| Check                                                                                      | Result  | Notes                                                 |
| ------------------------------------------------------------------------------------------ | ------- | ----------------------------------------------------- |
| Live `git ls-remote` and `git fetch --no-tags`                                             | PASS    | `FETCH_HEAD=56934ec0560bce8929025497411de7f5322fb6ca` |
| Previous baseline ancestry                                                                 | PASS    | `9f63850f7a` is an ancestor of `FETCH_HEAD`           |
| `cargo check --locked -p gpui -p settings_content -p settings -p settings_ui -p workspace` | PASS    | settings and window-option wiring                     |
| `cargo check --locked -p zzz`                                                              | PASS    | application crate                                     |
| `./script/clippy -p gpui -p settings_content -p settings -p settings_ui -p workspace`      | PASS    | targeted lint                                         |
| `cargo fmt --check`                                                                        | PASS    | clean                                                 |
| `git diff --check`                                                                         | PASS    | clean                                                 |
| `./script/check-philosophy`                                                                | PASS    | philosophy gate                                       |
| Locale key parity (`en.json` vs `zh-CN.json`)                                              | PASS    | 3659 entries each, identical key sets                 |
| macOS `gpui_macos` compile and runtime checks                                              | NOT RUN | host is Linux and no Darwin target is installed       |
| Windows and macOS runtime checks                                                           | NOT RUN | host is Linux                                         |
| `cargo test --workspace`                                                                   | NOT RUN | not requested                                         |

The general reviewed baseline advances to
`56934ec0560bce8929025497411de7f5322fb6ca`.
