---
title: Upstream Sync Audit 2026-11-03
description: Full re-audit of every recorded Zed upstream decision after the 2026-11-02 sync.
---

# Upstream Sync Audit 2026-11-03

## Scope

- Local branch: `main`
- Local base: `00b0a702465545172c9fa21e169978ce848b0b02`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Audited upstream head: `00b86477f8b81c8d20328b096200f91452a572df`
- Previous audited baseline: `37faef81f74fc1bb08be9939c4cffc8ede6ced2f`
- Reports enumerated: 38 non-audit `upstream-sync-*.md` reports
- Recorded decision rows: 1453 in report tables
- Unique upstream SHAs after latest-wins deduplication: 1434
  (249 A, 468 B, 717 C)
- Batches reviewed for the first time since the 2026-10-27 audit:
  2026-10-28, 2026-10-29, 2026-10-31, 2026-11-02, and 2026-11-02 (b)

This audit covers the whole recorded absorption history, not just the
batches added after the previous audit. The reviewed upstream baseline
does not advance: it already sits at the live upstream head
`00b86477f8b8`.

Correction (2026-11-03, audit): five `C` decisions were recoverable and
are now `B`; two `A` rows claimed commits that had never landed; and one
`B` port had dropped the upstream deletion of six `tab_content`
overrides. The affected reports carry dated correction notes, and 27
stale local-commit references now name the commits that are actually on
`main`.

## Method

Every decision table in every sync report was parsed, and the counts
below come from the same parser that `./script/backfill-upstream-ledger`
uses. Each recorded `A` or `B` local commit was checked against
`git cat-file` and `git merge-base --is-ancestor <commit> main`; each
recorded `C` was cross-referenced against later local `sync:` commits,
`Upstream:` trailers, and `cherry picked from commit` lines.

The isolation test was re-run for `C` rows whose justification was
`conflict`, `diverged`, or `not isolatable`, and for every `C` row with a
small diff that still named a present local file. Diffs at or below 60
changed lines were triaged individually. Later reports win over earlier
ones; the ledger already records that resolution.

## Findings

### F1: `e4945b5d09` and `1dd9bbfd71` were ports without a merge

Both commits were recorded `C` on the 2026-10-09 report after failed
attempts, yet ports for them existed on the unmerged
`sync/upstream-2026-10-07` branch as `d8124889a4` and `48978a9ba7`. Both
patches still apply to the current tree, and neither needs the absent
`RemoteConnection::remote_platform` API: the SSH reconnect reuses the
existing `PathStyle`, and the element-id cache maps onto ZZZ's
`FrameBuilder` element stack.

They are now `B` and landed on `main` as `70577d4138` (remote) and
`c10e21c3e9` (gpui). `cargo test -p remote --lib` passes 36 tests and
`cargo test -p gpui --lib` passes 289 tests, including the new
`global_element_id_tests`.

### F2: two `A` rows were never absorbed

`eb96feb8` (repaint the gutter when bookmarks change) and `0b1bf8dc`
(remove the stray `println!` in `Statement::single`) were recorded `A`
with local commits `eab14afc` and `0b7b9f2a` that do not exist in any
ref. The current tree still had the defects: editors did not observe the
bookmark store, and `Statement::single` still printed its result type on
every query.

They are now ported as `f3e56a7102` and `f603def34f`. The bookmark port
keeps both ends of the invariant: the store notifies itself, and the
editor observes the store, so `workspace::ClearBookmarks` repaints every
split. Upstream's regression test is ported as
`test_clearing_bookmark_store_notifies_editor`.

### F3: two `C` rows were rejected on stale conflict reasons

`d8664715` (copy CSV cells and column headers on right click) was
rejected as a "renderer conflict", but `crates/csv_preview` still has
both renderer files; the port adds a shared
`with_copy_on_right_click` helper with localized hints.
`d0bfe0a4` (keep hunk navigation for single-hunk diffs) was rejected
because the staged and unstaged diff views are gone, but the project and
solo diff views still carry the same `nth(1)` guard.

Both landed as `a0f9c9dafd` and `72c4d7dcef`; they are now `B`.

### F4: `9f305aaf88` is portable despite the macOS-only surface

The commit makes `NSOpenPanel`/`NSSavePanel` creation null-safe. The
earlier rejection said the local macOS layer could not be verified on a
Linux host. The change applies to `MainThreadMarker` instead of the
upstream marker and cross-compiles with
`cargo check -p gpui_macos --target aarch64-apple-darwin`; runtime
behavior remains unverified. Landed as `bb877c9e92`; now `B`.

### F5: the `e3056061` port left six `tab_content` overrides behind

Upstream deletes nine `Item::tab_content` overrides in `git_ui` so those
tabs inherit the default, which calls `Label::single_line()` and uses
`TabContentParams::text_color()`. The 2026-08-27 port took the shared
control-character helpers but not the deletions, so six local views still
rendered raw control characters in their titles and kept the old
`selected ? Default : Muted` color, leaving them undimmed when the pane
lost focus.

`6fff35e31c` deletes the six overrides (file diff, project diff, solo
diff, text diff, multi diff, and commit view) and the imports they made
unused. The row stays `B` with both local commits.

### F6: 27 local-commit references no longer resolved

Reports for 2026-08-18, 2026-08-27, 2026-09-06, and 2026-10-28 name
local commits that exist in no ref, because the sync branches were
rebased before merging. The behavior is on `main` under later SHAs for
25 of them; each row now names the landed commit. The remaining two were
F2. The ledger is unaffected because it records class and reason, not
the local commit.

### F7: `fdcb198110` keeps the macOS-only half of `PowerMode`

`Platform::power_mode` is overridden only in `gpui_macos`. Upstream also
adds Linux detection through `power-profiles-daemon` and a Windows power
notification path, so `PowerMode` stays `Unknown` on those platforms and
the 30 fps low-power throttle never engages there. This is a partial
absorption, not a regression; the class stays `B`. Porting the two
backends is left as a separate platform change because the Linux half
needs D-Bus plumbing and the Windows half cannot be runtime-verified
here.

### F8: `1663af412c` still needs a dedicated multibuffer LSP re-base

The commit splits the LSP data refresh into per-buffer tasks and cancels
pending requests when excerpts change or a buffer leaves the multibuffer.
`editor.rs` still differs from the upstream pre-image by roughly 17k
changed lines, so every hunk conflicts. The behavior is wanted, but the
isolation test fails; it stays `C`.

### F9: `87ce147829` still depends on an untracked prerequisite

The git graph refs column builds on the July `fcd0f76952` table
refactor, which predates the reviewed window and is not in any decision
table. The twenty-conflict three-way application still stands; it stays
`C`.

### Non-findings re-checked

The remaining recent `C` rows were re-checked against the current tree
and stay `C`: native-agent and ACP v2 surfaces (`2daea4d488`,
`c693e6a893`, `d697904d0e`, `33d111eac4`, `7de7bfdba2`, `2e794a1ea7`,
`e5bdd01cd8`, `025d2a06ee`, `af4948a164`, `c5efa10ddd`), absent crates
(`7f9a39a9c0`, `33156f0d81`, `5882c67dc3`, `d401423073`,
`4ec98df500`), upstream infrastructure (`66b6bbb6c8`, `26abd1c959`),
and the consent bundle (`9e23c1f43e`). The `66b6bbb6c8` allowlist entry
is still correct: the rejection covers the `crates/zed` release tooling,
while only the app version number was adopted.

## Corrections applied

| Report                                            | Correction                                                                      | Local commit |
| ------------------------------------------------- | ------------------------------------------------------------------------------- | ------------ |
| `upstream-sync-2026-10-09.md`                     | `e4945b5d09` moved `C` to `B`; SSH reconnect reuses the known path style        | `70577d4138` |
| `upstream-sync-2026-10-09.md`                     | `1dd9bbfd71` moved `C` to `B`; element-id hashes cached on the frame stack      | `c10e21c3e9` |
| `crates/remote/src/*`                             | Threaded `known_is_windows` through the connection pool and SSH transport       | `70577d4138` |
| `crates/gpui/src/element.rs`                      | `GlobalElementId` stores its path hash and `Hash`/`PartialEq` use it            | `c10e21c3e9` |
| `crates/gpui/src/frame.rs`                        | `ElementIdStack` keeps prefix hashes; deferred draws restore them               | `c10e21c3e9` |
| `upstream-sync-2026-08-27.md`                     | `eb96feb8` now names the commit that landed; editors observe the bookmark store | `f3e56a7102` |
| `upstream-sync-2026-08-27.md`                     | `0b1bf8dc` now names the commit that landed; the `single` debug print is gone   | `f603def34f` |
| `upstream-sync-2026-08-18.md`                     | `d8664715` moved `C` to `B`; CSV cell and header copy on right click            | `a0f9c9dafd` |
| `upstream-sync-2026-08-18.md`                     | `d0bfe0a4` moved `C` to `B`; single-hunk navigation in project and solo diffs   | `72c4d7dcef` |
| `upstream-sync-2026-10-24-c.md`                   | `9f305aaf88` moved `C` to `B`; nullable macOS open/save panels                  | `bb877c9e92` |
| `upstream-sync-2026-08-27.md`                     | `e3056061` retains the six deleted `tab_content` overrides                      | `6fff35e31c` |
| `upstream-sync-2026-08-18.md`                     | Ten stale local-commit references corrected                                     | n/a          |
| `upstream-sync-2026-08-27.md`                     | `01acd0ee` corrected to `06b5132ed9`                                            | n/a          |
| `upstream-sync-2026-09-06.md`                     | Twelve stale local-commit references corrected                                  | n/a          |
| `upstream-sync-2026-10-28-elicitation-entries.md` | `d5aa8b2838` corrected to `7ccddd429f`                                          | n/a          |

## Ledger

- Rejection rows before correction: 753
- Rejection rows after regeneration: 748
- Allowlist entries removed: 0; `66b6bbb6c8` still applies

## Verification

| Check                                                                              | Result  | Notes                                             |
| ---------------------------------------------------------------------------------- | ------- | ------------------------------------------------- |
| `cargo xtask check upstream-ledger` before the audit                               | PASS    | 753 rejection rows                                |
| `cargo check --locked -p gpui -p remote --lib`                                     | PASS    | after F1                                          |
| `cargo test --locked -p gpui --lib`                                                | PASS    | 289 tests                                         |
| `cargo test --locked -p remote --lib`                                              | PASS    | 36 tests                                          |
| `cargo check --locked -p editor -p sqlez -p git_ui -p csv_preview --all-targets`   | PASS    | after F2, F3, and F5                              |
| `cargo test --locked -p editor --lib test_clearing_bookmark_store_notifies_editor` | PASS    | 1 test                                            |
| `cargo test --locked -p sqlez --lib`                                               | PASS    | 18 tests                                          |
| `cargo test --locked -p git_ui --lib`                                              | PASS    | 128 tests                                         |
| `cargo test --locked -p csv_preview --lib`                                         | PASS    | 16 tests                                          |
| `cargo check --locked -p gpui_macos --target aarch64-apple-darwin`                 | PASS    | cross-compiles; runtime not exercised             |
| `cargo fmt --all -- --check`                                                       | PASS    | clean                                             |
| `git diff --check`                                                                 | PASS    | clean                                             |
| `cargo xtask clippy`                                                               | PASS    | workspace, manifest, and `future_not_send` passes |
| `cargo xtask check all`                                                            | PASS    | all six repository checks                         |
| Locale catalog key parity                                                          | PASS    | 3772 keys in each of `en.json` and `zh-CN.json`   |
| macOS / Windows runtime checks                                                     | NOT RUN | host is Linux                                     |
| `cargo test --workspace`                                                           | NOT RUN | narrowest per-crate checks were used              |

The Linux and Windows `PowerMode` backends from `fdcb198110` remain
unported (F7); no local caller depends on them, and the class stays `B`.
