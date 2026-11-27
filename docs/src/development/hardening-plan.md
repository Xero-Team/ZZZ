---
title: Fork Hardening Plan
description: "Machine-checked guarantees ZZZ holds instead of relying on review."
---

# Fork Hardening Plan

A fork drifts in ways review cannot catch: a file grows by ten lines a
week, a batch records an absorption it never proved, a build keeps
consuming a profile that describes last quarter's code. Each item below
replaces one of those open-ended expectations with something a command
answers. The order runs from checks that touch no product code to changes
that touch the renderer or the string layer, so the list can stop at any
line and leave the tree green.

## Landed

### Decision ledgers {#decision-ledgers}

[Upstream Cherry-Pick](./upstream-cherrypick.md#decision-ledgers) explains
the two generated files: one records the absorbed commits and whether the
local history proves them, the other records the skipped ones. The gate
fails on a SHA that appears in both, on a recent absorption nothing backs,
and on a ledger that does not reproduce from the reports.

Why it exists: before it, a class A decision lived only in a prose table.
"Did we actually take that commit" had no answer that a command could
give, and a re-audit had to rebuild the answer by hand.

### Source file-size ceilings {#file-size}

`cargo xtask check file-size` fails when a counted source file grows past
the size recorded in
`tooling/xtask/src/tasks/check/file_size_baseline.tsv`. Files over 800
lines need a row; a file that shrinks keeps the lower number, so a split
cannot be undone by a later addition.

Why it exists: the workspace has several hundred files past the size where
a file holds one idea, and `.rules` asks for a new module rather than
another thousand lines. A soft cap that nobody measures is a preference.
The baseline is the measurement, and growing a large file becomes a visible
edit to a reviewed file instead of a side effect of an unrelated change.

Test modules are out of scope: `*_tests.rs`, `*_test.rs`, and `tests/` are
skipped, so a fixture-heavy test file does not compete for the budget of
the code it covers.

### PGO profile age {#pgo-age}

[Profile-guided optimization](./build-pipeline.md#pgo) already refuses a
profile that was produced for a different target, profile, toolchain, or
manifest. It now also refuses one the tree has outgrown: `pgo train`
records the commit it started from, and a build that finds the profile
more than 500 commits behind `HEAD` fails instead of shipping optimizations
aimed at code that no longer exists. `--stale-profile-ok` overrides it.

Why it exists: the fingerprint answered "is this profile valid" and nothing
answered "is it still about this code". A profile trained once and reused
for a year is valid the whole time, and the slowdown is invisible in every
check ZZZ runs.

## Queued

These are candidates, not commitments. Each needs the layer beneath it
before the check can exist.

- **Animation gate.** ZZZ has no motion vocabulary: `Animation` in
  `crates/gpui` carries a duration and an easing curve, and every call site
  picks its own numbers. A gate that rejects off-token timing needs the
  tokens first.
- **Spring animation.** `crates/gpui/src/elements/animation.rs` supports
  duration plus easing and nothing else, so anything that should settle
  rather than stop is approximated with a curve.
- **Plural-aware and bidi-aware text.** `i18n::tr` maps a key to one
  string. There is no plural selection and no direction handling, which
  every locale beyond English and Chinese will eventually need.

## See also

- [Build and Optimization Pipeline](./build-pipeline.md#gates) for the
  gate list the checks above join.
- [Code Review Policy](./review-policy.md) for what review is still
  expected to catch.
