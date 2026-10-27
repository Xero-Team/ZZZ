---
title: Upstream Sync Audit 2026-10-27
description: Full re-audit of every recorded Zed upstream decision after the 2026-10-27 sync.
---

# Upstream Sync Audit 2026-10-27

## Scope

- Local branch: `sync/upstream-2026-10-27`
- Local base: `main` at `4d3b091b22be4a46f9fbb40df60c7473c1caf7e3`
- Upstream: `https://github.com/zed-industries/zed.git` `refs/heads/main`
- Audited upstream head: `37faef81f74fc1bb08be9939c4cffc8ede6ced2f`
- Previous reviewed baseline: `79fe6a645f68f8c9230578f154bf99852d9f716f`
- Reports enumerated: 33 non-audit `upstream-sync-*.md` reports
- Recorded decision rows: 1379 in report tables, plus 34 block-form rows
- Unique upstream SHAs after latest-wins deduplication: 1397 (233 A, 437 B, 727 C)
- `upstream-sync-2026-10-27.md`: 51 rows (1 A, 28 B, 22 C) covering 51 commits
- Existing audit reports reviewed: 2026-09-22, 2026-09-28, and 2026-10-05

This audit covers the whole recorded absorption history, not just the
2026-10-27 batch. The reviewed upstream baseline does not advance: it
already sits at the live upstream head `37faef81f7`.

## Method

Every decision table in every sync report was parsed, as were the handful
of `C` rows written as report sections rather than table rows. The counts
below come from the same parser that `./script/backfill-upstream-ledger`
uses, so they match the ledger the checker validates.

For each recorded `C` decision the audit cross-referenced later local
`sync:` commits, `Upstream:` trailers, and `cherry picked from commit`
lines, then re-ran the isolation test against the current tree. Later
reports win over earlier ones: when the same SHA appears in several
reports, the newest row is authoritative and the ledger already records
that resolution. Sixteen SHAs appear more than once this way; they are
listed under "Superseded decisions" rather than treated as conflicts.

## Findings

### F1: `fdcb198110` needs a macOS 12 guard for `isLowPowerModeEnabled`

The `B` port `db34c508be` calls `NSProcessInfo.isLowPowerModeEnabled`
once per frame. That selector was introduced in macOS 12, while
`.cargo/config.toml` still pins `MACOSX_DEPLOYMENT_TARGET = 10.15.7`
because `f14592c4a5` correctly stays `C` as release metadata. On a 10.15
or 11 host the message would be sent to `NSProcessInfo`, which does not
respond to it.

`MacPlatform::power_mode` now returns `PowerMode::Unknown` below 12.0.0
using the existing `os_version` helper, matching the guard already used
for `setAllowsAutomaticKeyEquivalentLocalization:` and
`setDefaultApplicationAtURL:`. The iOS and Windows/Linux defaults are
unchanged. `f14592c4a5` remains `C`; the port is corrected rather than
the classification.

### F2: `fdcb198110` drops a forced render that arrives inside the throttle interval

`db34c508be` made the low-power and inactive-window paths throttle a
requested frame. When the request arrives too early the closure calls
`complete_frame` and returns, and the next request carries no
`force_render` of its own, so `Window::refresh` never runs and the view
cache still replays stale atlas tiles after a GPU device recovery. The
gap predates the port: local `main` already throttled inactive and
thermally loaded windows in `8190a4432b`, and `a27025f9e7` added the
`force_render` refresh inside the draw branch without deferring it past
the throttle. `fdcb198110` only widened the throttle to low power mode, so
this is inherited rather than a regression the port introduced. Recording
it here keeps the absorptions complete.

The closure now remembers a deferred force in `deferred_force_render`,
takes it with `mem::take` at entry, and uses the effective flag for the
throttle decision, the draw condition, and `Window::refresh`.
`test_throttled_forced_render_is_not_lost` serves a frame, sends a forced
request inside the default 33.333 ms inactive interval, waits past it, and
then sends an ordinary request; it asserts the forced render is served by
that later frame, and it fails with the deferred flag removed.

### Non-findings re-checked

The following `C` decisions were re-checked against the current tree and
stay `C`:

- `1808df02c0` (rewrites the absent `gpui_apple` Metal renderer),
  `484bcbfea6` (ohos gating with no local target), `e645f02245` (no GPG
  wrapper in `askpass`), `182d15a629` (icon with no caller), and
  `24a2823456` (wgpu git revision against a vendored v29) all still lack
  a retained caller or an absorptive target.
- `087340e2d7` moves `crash-context` to a `minidump-writer` git SHA to fix
  a ⌘-Q crash. ZZZ does have `crates/crashes`, but the patch only rewrites
  the manifest and lockfile to a revision pinned inside the
  `minidump-writer` repository; there is no local code path to replay, so
  the isolation test fails and the row stays `C`.
- `f14592c4a5` raises the deployment floor to 12 and swaps a deprecated
  icon API. ZZZ has no `iconForFileType` call site, and the floor is
  release metadata that `.cargo/config.toml` deliberately keeps at
  10.15.7.

No other definite defect was confirmed in path-position parsing,
navigation history, context menus, retained focus, Metal staging, title
bar path styles, ACP line handling, the DirectWrite changes, or the
completion-label guard.

## Superseded decisions

These SHAs carry more than one recorded decision. The newest report wins,
and the ledger records only that row.

| Upstream   | Earlier report | Final report               | Final class |
| ---------- | -------------- | -------------------------- | ----------- |
| 00cba838ad | 2026-08-07 (C) | 2026-10-02                 | B           |
| 4bd1993783 | 2026-08-18 (C) | 2026-10-02                 | B           |
| 03c9c4e707 | 2026-08-27 (C) | 2026-10-02                 | B           |
| ec18126b1d | 2026-08-27 (C) | 2026-10-02                 | B           |
| 07df438626 | 2026-09-18 (A) | 2026-10-02                 | C           |
| 3cef3168   | 2026-09-18 (C) | 2026-09-22-acp-elicitation | B           |
| 68ec865b   | 2026-09-18 (C) | 2026-09-22-acp-elicitation | B           |
| f57e400a17 | 2026-09-24 (C) | 2026-09-26                 | B           |
| 8ba35e5e   | 2026-07-03 (C) | 2026-09-22-acp-elicitation | B           |
| fa66442a   | 2026-07-03 (C) | 2026-09-22-acp-elicitation | B           |
| c49a29f4   | 2026-07-03 (C) | 2026-10-02                 | B           |
| 282f47a5   | 2026-08-27 (C) | 2026-10-02                 | C           |
| 632d805d   | 2026-08-18 (C) | 2026-09-22-acp-elicitation | C           |
| 73ee8fa0   | 2026-09-04 (C) | 2026-09-22-acp-elicitation | C           |
| 6b5e15ed   | 2026-09-02 (C) | 2026-10-02                 | C           |
| c305d68c   | 2026-08-07 (C) | 2026-10-02                 | C           |

The repeated `C` rows are re-affirmations, not conflicts; they are
included so the deduplication is auditable.

## Corrections applied

| Report                              | Correction                                                                                                        | Local commit |
| ----------------------------------- | ----------------------------------------------------------------------------------------------------------------- | ------------ |
| `upstream-sync-2026-10-27.md`       | Recorded the audit correction for `fdcb198110`; the row stays `B` with a superseded note and the new local commit | `6159bc81e0` |
| `crates/gpui_macos/src/platform.rs` | Added the macOS 12 guard in `MacPlatform::power_mode`                                                             | `6159bc81e0` |
| `crates/gpui/src/window.rs`         | Deferred the forced render across throttling and added its regression test                                        | `6159bc81e0` |
| `crates/util/src/paths.rs`          | Split the `unsafe` operations in `rsplit_file_at_dot` so `multiple_unsafe_ops_per_block` passes                   | `6159bc81e0` |
| `crates/workspace/src/dock.rs`      | Replaced `map(...).unwrap_or_else(...)` with `map_or_else` so `map_unwrap_or` passes                              | `6159bc81e0` |
| `Cargo.lock`                        | Restored the `tempfile` edge to `getrandom 0.4.3`; the tree-sitter-go pin did not require the resolver churn      | `6159bc81e0` |

The two lint fixes repair regressions introduced by this branch's own
ports (`5eccbc95c5` and `254659a2c9`); `./script/clippy` failed on both
before the correction and passes on the affected crates after it.

## Ledger

- Rejection rows before correction: 727
- Rejection rows after regeneration: 727
- Allowlist entries removed: 0; the allowlist contains comments only

The ledger is regenerated from the corrected decision tables rather than
edited by hand. No class changed, so the rejection set is unchanged.

## Verification

| Check                                                              | Result           | Notes                                       |
| ------------------------------------------------------------------ | ---------------- | ------------------------------------------- |
| `./script/check-upstream-ledger` before the audit                  | PASS             | 727 rejection rows                          |
| `cargo check --locked -p gpui --lib`                               | PASS             | after the frame-throttle change             |
| `cargo test --locked -p gpui --lib`                                | PASS             | 273 tests                                   |
| `cargo test --locked -p gpui --lib window::tests::`                | PASS             | 19 tests                                    |
| Deferred-force regression with the fix reverted                    | FAIL as expected | second render never happened                |
| `cargo test --locked -p util --lib path`                           | PASS             | 77 tests                                    |
| `cargo test --locked -p workspace --lib dock`                      | PASS             | 7 tests                                     |
| `cargo check --locked -p gpui_macos --target aarch64-apple-darwin` | PASS             | cross-compiles; macOS runtime not exercised |
| `cargo metadata --locked --no-deps`                                | PASS             | lockfile unchanged by resolution            |
| `./script/clippy` (full workspace)                                 | PASS             | after the two lint fixes                    |
| `cargo fmt --check`                                                | PASS             | clean                                       |
| `git diff --check`                                                 | PASS             | clean                                       |
| `./script/check-philosophy`                                        | PASS             | philosophy gate                             |
| `./script/backfill-upstream-ledger`                                | PASS             | regenerated 727 rejection rows              |
| `./script/check-upstream-ledger` after the audit                   | PASS             | no absorbed SHA remains in the ledger       |
| macOS / Windows runtime checks                                     | NOT RUN          | host is Linux                               |
| `cargo test --workspace`                                           | NOT RUN          | not requested                               |

The macOS guard is covered by cross-compilation only; there is no unit
test for `MacPlatform::power_mode`, and `power_mode` reads the OS version
at runtime, so an actual 10.15 or 11 host remains unverified here. The
frame-throttle tests exercise the platform-agnostic closure through the
test window.
