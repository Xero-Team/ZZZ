---
title: Upstream Sync 2026-10-21 (b)
description: Selective Zed upstream sync audit, second batch.
---

# Upstream Sync 2026-10-21 (b)

## Scope

- Target branch: `sync/upstream-2026-10-21b`, branched from `main` at
  `be9d1a1782`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `5adae449a366fd8f8f87d983287d95eab37e9488`
- Reviewed head: `e80af525755c5500763df78f85c3b5e2c24eb117`
- Live upstream head queried: `e80af525755c5500763df78f85c3b5e2c24eb117`
- Query time: `2026-10-21T17:48:00+02:00`
- Reviewed range: every commit after `5adae449a3`, oldest first
- Remaining after this batch: 0 commits through the queried live head

The previous reviewed baseline is an ancestor of `FETCH_HEAD`. Three
commits landed after the first 2026-10-21 batch, so the whole range fits
in this batch.

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                                        |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------------------------ |
| d814ad6efd | C     | --           | Bumps the upstream app crate to v1.27.0; release metadata for the Zed package that ZZZ does not carry.             |
| cc23f35a7d | B     | a8fb9c5c43   | Rechecks history, opened, and selected paths when the worktree changes and drops entries that are no longer files. |
| e80af52575 | B     | a3aaec9b48   | Puts outbound LSP messages and notifications on one queue so notifications are written before later requests.      |

Totals: zero `A`, two `B`, one `C` across three commits.

## Applied Work

- `a8fb9c5c43` is a `B` port of `cc23f35a7d`.
- `a3aaec9b48` is a `B` port of `e80af52575`.

## Per-Commit Narrative

#### d814ad6efd — version bump (C)

Bumps `crates/zed` to 1.27.0 and updates its package metadata. ZZZ has no
`crates/zed` package and does not follow upstream release versions, so
there is no local call site or behavior to retain.

#### cc23f35a7d — remove deleted history items (B)

History entries, the currently opened file, and selected matches are
rechecked when `WorktreeUpdatedEntries` fires. Paths that are no longer
files are removed from history, the opened path, selected matches, and
the match list, with the selected index adjusted. `FoundPath::is_file`
resolves project entries, local filesystem paths, and remote-server paths,
and conservatively keeps unknown project kinds. ZZZ keeps resource IDs on
history entries and has no `debounce_next_refresh`, so the refresh simply
calls `Picker::refresh`. Upstream's remote-server test scaffolding
(`HeadlessAppState`, `HeadlessProject`, `RemoteClient`) and its
dev-dependencies are omitted; the existing 82 file-finder tests pass.

#### e80af52575 — preserve LSP message order (B)

Replaces the separate response and notification channels with one
`OutboundMessage` queue carrying either serialized messages or closures
that serialize later. One background task drains the queue in order, so a
pending `didChange` notification is written before a later formatting
request instead of racing on a second channel. Shutdown and cancellation
notifications use the same queue. The previously disabled ordering
assertion in `test_strip_whitespace_and_format_via_lsp` is re-enabled and
passes. Upstream also reorders two inlay-hint tests to wait for
`DidOpenTextDocument`; ZZZ does not carry those multi-server tests, so the
test-only hunk is omitted.

## Verification

| Check                                                                          | Result  | Notes                                                 |
| ------------------------------------------------------------------------------ | ------- | ----------------------------------------------------- |
| Live `git ls-remote` and `git fetch --no-tags`                                 | PASS    | `FETCH_HEAD=e80af525755c5500763df78f85c3b5e2c24eb117` |
| Previous baseline ancestry                                                     | PASS    | `5adae449a3` is an ancestor of `FETCH_HEAD`           |
| `cargo check --locked -p lsp -p file_finder`                                   | PASS    | production changes                                    |
| `cargo test --locked -p file_finder --lib`                                     | PASS    | 82 tests                                              |
| `cargo test --locked -p editor --lib test_strip_whitespace_and_format_via_lsp` | PASS    | re-enabled ordering assertion                         |
| `cargo fmt --check`                                                            | PASS    | clean                                                 |
| `git diff --check be9d1a1782..HEAD`                                            | PASS    | clean                                                 |
| `./script/check-philosophy`                                                    | PASS    | philosophy gate                                       |
| Locale key parity (`en.json` vs `zh-CN.json`)                                  | PASS    | unchanged; no new keys                                |
| Windows and macOS builds                                                       | NOT RUN | host is Linux                                         |
| `cargo test --workspace`                                                       | NOT RUN | not requested                                         |

The general reviewed baseline advances to
`e80af525755c5500763df78f85c3b5e2c24eb117`.
