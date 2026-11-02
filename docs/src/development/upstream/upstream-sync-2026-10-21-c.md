---
title: Upstream Sync 2026-10-21 (c)
description: Selective Zed upstream sync audit, third batch.
---

# Upstream Sync 2026-10-21 (c)

## Scope

- Target branch: `sync/upstream-2026-10-21c`, branched from `main` at
  `76824047ae`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `e80af525755c5500763df78f85c3b5e2c24eb117`
- Reviewed head: `3dbf6a8101b0032e5c3e9061bdc756086d971b31`
- Live upstream head queried: `3dbf6a8101b0032e5c3e9061bdc756086d971b31`
- Query time: `2026-10-21T17:57:00+02:00`
- Reviewed range: every commit after `e80af52575`, oldest first
- Remaining after this batch: 0 commits through the queried live head

The previous reviewed baseline is an ancestor of `FETCH_HEAD`. Six commits
landed after the second 2026-10-21 batch, so the whole range fits in this
batch.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                                                                      |
| ---------- | ----- | ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0a9eb81e6c | C     | --           | Migrates macOS keyboard and event bindings to objc2. ZZZ's `gpui_macos` has a different renderer/window architecture, and the host cannot build it.              |
| cb1937a8e7 | C     | --           | Migrates macOS display bindings to objc2 on the same diverged `gpui_macos` base and cannot be verified on this host.                                             |
| e83f1c9495 | C     | --           | Migrates macOS clipboard bindings to objc2 on the same diverged `gpui_macos` base and cannot be verified on this host.                                           |
| 11262f9256 | B     | 7a0e36fad3   | Explains a project-panel move when the destination name already exists, and preserves `AlreadyExists` across the remote protocol.                                |
| 1563b405f9 | C     | --           | Requires the per-registration dynamic-selector model from #59243, which ZZZ does not carry; its `DynamicRegistrations` only holds watched files and diagnostics. |
| 3dbf6a8101 | A     | 05851384ec   | Clean absorption: node runtime discovers npm-backed servers in pnpm and Yarn projects.                                                                           |

Totals: one `A`, one `B`, four `C` across six commits.

## Applied Work

- `05851384ec` is a direct `git cherry-pick -x -s` of `3dbf6a8101`.
- `7a0e36fad3` is a `B` port of `11262f9256`.

## Per-Commit Narrative

#### 0a9eb81e6c — macOS keyboard and event objc2 migration (C)

Rewrites `gpui_macos/src/events.rs` and the keyboard bindings against
objc2. ZZZ's macOS platform layer has been restructured around a different
renderer and window implementation, so the patch conflicts throughout.
The host is Linux and has no usable macOS target, so the port could not be
compiled or run.

#### cb1937a8e7 — macOS display objc2 migration (C)

Migrates display and display-link bindings to retained `NSScreen` objects.
The local `gpui_macos/src/display.rs` has diverged from the upstream base
and the result cannot be checked on this host. Absorbing an unverifiable
platform rewrite risks breaking the macOS build.

#### e83f1c9495 — macOS clipboard objc2 migration (C)

Rewrites `pasteboard.rs` around typed pasteboards and retained Foundation
objects. Same divergence and verification constraints as the other macOS
objc2 migrations.

#### 11262f9256 — destination-exists move failure (B)

Rename collisions now return `io::ErrorKind::AlreadyExists` instead of a
plain anyhow string. Untyped errors map to `ErrorCode::AlreadyExists` in
the proto layer, so the code survives the remote round trip. Undo and the
project panel share `already_exists_error`, `project_path_display` is
shared, and `explain_move_error` names the source and destination in the
notification. Includes the proto error tests and the remote rename
regression test. The notification `copied_text` helper and the panel UI
test that depends on absent notification builder fields are omitted. The
port adds `proto` as a direct project_panel dependency.

#### 1563b405f9 — dynamic registration document selectors (C)

Tracks `didChange` and `didSave` registrations per registration and
applies their document selectors. ZZZ's `DynamicRegistrations` is the older
shape that only stores watched files and diagnostics, and its `lsp_store`
has no `dynamic_registration.rs` or per-registration sync options. The
change depends on the unabsorbed #59243 refactor.

#### 3dbf6a8101 — npm-backed servers in pnpm and Yarn (A)

Detects pnpm and Yarn project layouts when probing for npm-backed language
server binaries, so setup no longer fails with `EBADDEVENGINES`. Includes
the node-runtime tests. The patch applied cleanly.

## Verification

| Check                                                                                    | Result  | Notes                                                 |
| ---------------------------------------------------------------------------------------- | ------- | ----------------------------------------------------- |
| Live `git ls-remote` and `git fetch --no-tags`                                           | PASS    | `FETCH_HEAD=3dbf6a8101b0032e5c3e9061bdc756086d971b31` |
| Previous baseline ancestry                                                               | PASS    | `e80af52575` is an ancestor of `FETCH_HEAD`           |
| `cargo check --locked -p fs -p project_panel -p proto -p remote_server`                  | PASS    | move failure path                                     |
| `cargo test --locked -p proto --lib error::`                                             | PASS    | 2 tests                                               |
| `cargo test --locked -p remote_server --lib test_remote_rename_entry_onto_existing_path` | PASS    | 1 test                                                |
| `cargo test --locked -p node_runtime`                                                    | PASS    | 7 tests                                               |
| `cargo fmt --check`                                                                      | PASS    | clean                                                 |
| `git diff --check 76824047ae..HEAD`                                                      | PASS    | clean                                                 |
| `./script/check-philosophy`                                                              | PASS    | philosophy gate                                       |
| Locale key parity (`en.json` vs `zh-CN.json`)                                            | PASS    | unchanged; no new keys                                |
| macOS `gpui_macos` builds and runtime checks                                             | NOT RUN | host is Linux                                         |
| Windows builds                                                                           | NOT RUN | host is Linux                                         |
| `cargo test --workspace`                                                                 | NOT RUN | not requested                                         |

The general reviewed baseline advances to
`3dbf6a8101b0032e5c3e9061bdc756086d971b31`.
