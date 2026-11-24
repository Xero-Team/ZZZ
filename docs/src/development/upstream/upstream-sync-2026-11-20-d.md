---
title: Upstream Sync 2026-11-20 (D)
description: Selective Zed upstream sync, one out-of-order port after the 2026-11-20 batch C.
---

# Upstream Sync 2026-11-20 (D)

## Scope

- Local base: `sync/upstream-2026-11-20` at `03b588229b`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Previous reviewed baseline: `4a7d10473a3ed32c10026e549381f064dd020114`
- New reviewed baseline: `4a7d10473a3ed32c10026e549381f064dd020114` (unchanged)
- Remaining after this batch: 24 commits, starting with `df1ba676f2`

**The reviewed baseline does not move in this batch.** Two commits were
skipped ahead of the one landed here, and they stay unreviewed until the
next batch covers them. Advancing the baseline to `10c56fa0c6` would drop
them from the queue, which is the failure mode that left `bc29bcfe72`
unabsorbed for four months.

## Skipped ahead, still unreviewed

| Upstream   | Size       | Subject                                                        | Why not in this batch                  |
| ---------- | ---------- | -------------------------------------------------------------- | -------------------------------------- |
| df1ba676f2 | 903 lines  | agent_ui: Organize ACP config options in the agent composer    | Needs a batch of its own; not started. |
| 0cc1ffb6b3 | 3285 lines | agent: Support global skills over SSH, WSL, and dev containers | Needs a batch of its own; not started. |

## Decisions

| Upstream   | Class | Local commit | Disposition                                                                                       |
| ---------- | ----- | ------------ | ------------------------------------------------------------------------------------------------- |
| 10c56fa0c6 | B     | 48b7eca8e5   | Delimits capture references that sit before non-ASCII text in a regex replacement. Payload below. |

Totals: zero `A`, one `B`, zero `C`.

## Per-Commit Narrative

### 10c56fa0c6 — capture references before non-ASCII text (B)

Retained: `SearchQuery::with_replacement` bakes the escape sequences and
capture references into `replacement_template` once, so `replacement_for`
no longer rebuilds the template for every hit. `delimit_capture_references`
rewrites `$1ème` as `${1}ème` unless a group with that name exists, because
`fancy_regex` otherwise reads an unbraced reference up to the first
character that is neither alphanumeric nor `_`, and the reference expands
to nothing.

Omitted: nothing. The only conflict was the inline escape-sequence block in
`replacement_for`, which upstream moves into
`prepare_replacement_template`; the port deletes the inline copy. Local
needs `io::{self, BufRead, BufReader, Read}` rather than the three-item
import, because the surrounding read loop matches `io::ErrorKind` and the
new helpers keep that shape.

## Verification

| Check                                                      | Result | Notes                                  |
| ---------------------------------------------------------- | ------ | -------------------------------------- |
| `cargo check --offline -p project -p search --all-targets` | PASS   |                                        |
| `cargo test -p project --lib search::tests`                | PASS   | 5 passed, including the non-ASCII case |

## Known defect found while porting

`cargo check -p title_bar --all-targets` fails with
`E0004: non-exhaustive patterns: RemoteConnectionOptions::Mock(_) not covered`.
The variant is gated on `remote`'s own `cfg(any(test, feature = "test-support"))`
(`crates/remote/src/remote_client.rs:1363`), while the arm in
`crates/title_bar/src/title_bar.rs:543` is gated on `title_bar`'s own copy of
the same predicate. Feature unification under `--all-targets` enables
`remote/test-support` through a dev-dependency without enabling
`title_bar/test-support`, so the variant exists while the arm is compiled out.
The two predicates can never line up. This predates the sync — it reproduces at
`484eb80cc5` and earlier — and `cargo check --workspace` without `--all-targets`
does not see it. Fixing it means either an `#[allow(unreachable_patterns)]`
fallback arm, which would mask future variants, or moving the connection
display data onto `RemoteConnectionOptions` so `title_bar` stops matching.
