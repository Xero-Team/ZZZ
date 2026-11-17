---
name: profiling-performance
description:
  "Measures ZZZ performance before and after a change with the repository's own
  harnesses: cargo perf-test for #[perf] tests, cargo bench for criterion benches,
  and cargo perf-compare to diff two runs. Use when a change claims a speedup, a
  regression appears, or the user asks to profile, benchmark, or compare
  performance. Do not use for micro-optimizing code without a measurement."
license: CC-BY-4.0
compatibility: opencode
version: 1.0.0
metadata:
  author: Xero Team <maintainers@example.com>
  tags:
    - performance
    - benchmarks
    - profiling
  prompt-language: English
  user-language: Simplified Chinese
---

Follow `REFERENCE.md` in this skill first. It carries the two harnesses, their
flags, and the ground rules. A number without a baseline is an anecdote; report it
as one or do not report it.

# Profiling Performance

You measure first, change second, and measure again with the same command. This
repository ships two local harnesses that CI never runs, so a regression stays
invisible until someone measures it — that someone is this workflow.

## Purpose

Produce a baseline and a comparable after-run for one workload, so a performance
claim rests on numbers from the same harness, machine, and command, and so the
remaining uncertainty is stated instead of rounded away.

## Communication Rules

- Keep this skill prompt written in English.
- Interact with the user in Simplified Chinese.
- Write benchmark names, commands, and identifiers in English; write the report
  in Simplified Chinese with the raw numbers preserved.
- When a technical term could be ambiguous, add a short Chinese gloss followed by
  the English term in parentheses.

## When to Use

- The user asks to profile, benchmark, or compare performance.
- A change is motivated by speed, memory, or allocation count.
- A suspected regression needs a verdict.
- A `#[perf]` test or a criterion bench is added or changed.

Do not use this skill for micro-optimizations justified by reading code alone, for
startup time of the app binary outside these harnesses, or for load testing a
server. Say the measurement is unavailable rather than inventing one.

## Operating Rules

- Measure before changing code. If there is no baseline, establishing one is the
  first task, not an optional step.
- Use the same harness, command, profile, machine, and ident shape for the before
  and after run. A `perf-test` number and a `bench` number are not comparable.
- Never edit a benchmark to reach a target. A benchmark is the measuring device;
  changing it changes the question.
- Never run two benchmarks concurrently. Contention invalidates both numbers.
- No `RUSTFLAGS` or `target-cpu=native` experiments to make a number look better.
  The harness profiles are the ones CI and maintainers use.
- Keep measured cases independent. One change per measurement, or the delta cannot
  be attributed.
- Optimize in the repository's priority order: compute, then memory, then binary
  size, then compile time (`docs/src/development/build-pipeline.md`).
- A failed or noisy run is reported as such. Do not average two runs by hand to
  hide variance, and do not present one lucky run as the result.
- These harnesses are local-only. Never claim CI verified a performance change.

## Prerequisites

- The pinned toolchain for the `release-fast` profile.
- `#[perf]` tests exist for `perf-test` targets (`gpui`, `search`, `terminal`,
  `util`, `vim`), or a criterion bench exists in the target crate.
- `hyperfine` on `PATH`, which `cargo perf-test` shells out to.
- A quiet machine. Report the hardware and whether other work was running.

## Procedure

1. Read `AGENTS.md`, `docs/src/development/build-pipeline.md`, and this skill's
   `REFERENCE.md`.
2. Pick the harness that already measures the workload. Prefer `cargo perf-test -p
   <crate>` when a `#[perf]` test covers it; use `cargo bench -p <crate>` for a
   criterion bench; use the standalone `*_benchmarks` driver only when neither
   covers the case.
3. Confirm the measuring device. Read the test or bench and state in one line what
   it exercises. Do not modify it.
4. Record the baseline with an ident: `cargo perf-test -p <crate> -- --json=before`
   or the criterion baseline output. Save the command and the raw output.
5. Make the change, then rerun with the identical command and a second ident.
6. Compare: `cargo perf-compare after before` for `perf-test` runs, or the bench's
   own before/after output. Keep the produced table.
7. Verify correctness separately. A faster path that changes behavior is not an
   improvement; run the crate's tests before reporting a win.
8. Report the deltas that matter, the variance you observed, the machine, and what
   remains unmeasured.

## Examples

```text
This change clones the whole buffer to search it; measure whether avoiding the
clone actually helps.
```

```text
Compare the editor frame workload before and after the text-layout change.
```

## Troubleshooting

- Numbers swing between runs: report the spread, rerun once, and check for
  background work. A difference inside the spread is not a result.
- `cargo perf-test` fails to start: `hyperfine` is the usual cause, and a missing
  `#[perf]` test in the crate is the other.
- `cargo perf-compare` mixes crates: it reads every `.perf-runs/<ident>.*.json`,
  so stale files from earlier idents pollute the comparison. Use a fresh ident.
- The baseline predates the toolchain bump: say so; the profile flags changed and
  the comparison is weaker.
- The workload needs a GUI or platform that this host lacks: report the measurement
  as unavailable rather than substituting a different workload.

## Limitations

- Neither harness runs in CI, so the numbers are local and machine-specific.
- Wall-clock comparisons across machines or toolchain versions are not comparable.
- `perf-test` reports test timings, not end-to-end frame time; use
  `crates/editor/benches/editor_frame_workload.rs` when frame time, RSS, or
  allocation count is the question.
- This skill measures and compares. It does not choose an optimization; propose one
  and measure it.

## Final Response

Summarize in Simplified Chinese:

- the workload and the harness used, with the exact command
- baseline and after numbers, with variance
- the delta and whether it exceeds the spread
- correctness checks run before and after
- the machine and toolchain, and what remains unmeasured
