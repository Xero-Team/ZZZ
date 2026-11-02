---
title: Upstream Sync Audit 2026-10-05
description: Targeted re-audit of the AccessKit adapter rejection after the GPUI refactor.
---

# Upstream Sync Audit 2026-10-05

## Scope

- Local branch: `refactor/gpui-architecture`
- Local base before report corrections: `5bf1569ff13da39ccc1e3118eeeebe45df8441e5`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Live upstream head: `2117a376b97104049f60e843afa92f18fa7f5748`
- Reports enumerated: 17 non-audit `upstream-sync-*.md` reports
- Recorded decision rows: 959 total; 178 A, 307 B, and 474 C after correction
- Existing audit reports reviewed: 2026-09-22 and 2026-09-28

This is a targeted correction audit. The ledger checker identified one stale C
decision after the staged GPUI refactor supplied the architecture that was absent
when the original report was written. The general reviewed upstream baseline does
not advance.

## Method

The audit enumerated every decision-table row, cross-referenced the generated
rejection ledger with local `Upstream:` trailers and the current GPUI platform call
chain, and reran the isolation test for the conflicting SHA. Git history and the
current tree were treated as authoritative.

## Findings

### F1: `0eda7703` is no longer an absent-API rejection

`upstream-sync-2026-09-24.md` classified `0eda7703` as C because ZZZ had no
macOS AccessKit adapter to release. That finding was correct at the time. The GPUI
refactor later added `accesskit_macos::SubclassingAdapter` in `db6cb81aeb`, with
both `1d029c5f` and `0eda7703` recorded in its `Upstream:` trailers.

The retained behavior is philosophy-safe, has a real native macOS caller, depends
only on the now-present adapter, and passes the target cross-check. The final class
is B. The port drops the adapter before renderer teardown to break the native view
ownership cycle; it does not import the rejected upstream writer state.

## Corrections Applied

| Report                             | Correction                                                                     | Local commit |
| ---------------------------------- | ------------------------------------------------------------------------------ | ------------ |
| `upstream-sync-2026-09-24.md`      | `0eda7703` C → B; corrected first-run totals and added a dated superseded note | `db6cb81aeb` |
| `upstream-sync-2026-10-03-gpui.md` | Recorded the adapter continuation and final B classification                   | `db6cb81aeb` |

## Ledger

- Rejection rows before correction: 486
- Expected rejection rows after regeneration: 485
- Allowlist entries removed: 0; the allowlist contained comments only

The ledger is regenerated from the corrected decision tables rather than edited by
hand.

## Verification

| Check                                                                                              | Result | Notes                                                   |
| -------------------------------------------------------------------------------------------------- | ------ | ------------------------------------------------------- |
| `cargo check --locked -p gpui_macos --tests --target x86_64-apple-darwin --features accessibility` | PASS   | Native adapter and adapter-first teardown cross-compile |
| `./script/backfill-upstream-ledger`                                                                | PASS   | Regenerated 485 rejection rows                          |
| `./script/check-upstream-ledger`                                                                   | PASS   | No absorbed SHA remains in the ledger                   |
| Prettier on touched reports and this audit                                                         | PASS   | Only touched reports were formatted                     |
