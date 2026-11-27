---
title: Testing
description: "Where tests live, how to run them, and the rules a disabled test must follow."
---

# Testing

`CONTRIBUTING.md` requires tests for a change; this page covers where they
go and what the repository gates expect of them.

## Where tests live

| Kind              | Location                                                  | Notes                                                        |
| ----------------- | --------------------------------------------------------- | ------------------------------------------------------------ |
| Unit              | `#[cfg(test)]` module in the file under test              | The default. No fixtures on disk unless the logic needs one. |
| Integration       | `crates/<crate>/tests/`                                   | Drives the crate through its public surface.                 |
| GPUI              | `crates/<crate>/src/` or `.../tests/` with `test-support` | Uses `TestAppContext`; see the GPUI rules below.             |
| Visual regression | `crates/zzz/src/zzz/visual_tests.rs`                      | Screenshot baselines; ignored by default.                    |
| Randomized        | `script/randomized-test-ci`                               | Generates, builds, and minimizes randomized editor tests.    |

## Running them

```sh
cargo test --workspace                                  # everything
cargo nextest run --workspace                           # what CI runs
cargo test -p editor test_some_behavior                 # one test
cargo test -p zzz visual_tests -- --ignored --test-threads=1
```

CI runs `cargo nextest run --workspace --no-fail-fast`. A test that passes
locally under `cargo test` should pass there too; the difference is
parallelism and the timeouts below.

## Markdown conformance

`crates/markdown/src/spec_compat.rs` measures the renderer against the
published example suites. The suites are third-party data, so they are
fetched instead of vendored, and the tests skip themselves when the fixtures
are absent:

```sh
script/markdown-spec   # fetch CommonMark 0.31.2 and GFM 0.29, then run
```

The first run downloads into `target/markdown-spec`; later runs reuse those
files. `ZZZ_MARKDOWN_SPEC_DIR` points the tests at another directory, and
`ZZZ_MARKDOWN_SPEC_DUMP=<n>` prints that many mismatches instead of ten. Each
layer asserts a recorded pass count, so a regression fails the test while an
improvement only changes the report until the constant is raised.

## Timeouts

`.config/nextest.toml` gives every test a 60 second slow timeout. A test that
legitimately needs longer gets an override keyed on its package and name,
next to the existing ones:

```toml
[[profile.default.overrides]]
filter = 'package(editor) and test(test_random_blocks)'
slow-timeout = { period = "300s", terminate-after = 1 }
```

Add the override with the fix, not after CI starts timing out. The file also
groups `package(db)` tests to run one at a time, because they share a
database file.

## GPUI tests

`.rules` is the authority: use the GPUI executor timer instead of
`smol::Timer::after`, and avoid blocking the test thread while the executor
is parked. The short version:

- `cx.background_executor().timer(duration).await` for delays.
- `cx.run_until_parked()` to drive queued foreground work.
- No `std::thread::sleep` in a test that later pumps the executor.

## Miri

Miri is the only tool that executes `unsafe` against an aliasing, provenance,
and initialization model, so it is the check that catches a bad pointer cast a
normal test cannot see. It is also an interpreter: one crate costs minutes, so
the repository runs a curated list rather than the workspace.

```sh
cargo xtask miri                 # the curated list, as CI runs it
cargo xtask miri --list          # what is on the list, and why
cargo xtask miri -p collections  # one crate
```

The task installs nothing. On a machine without it, the error names the rustup
command; Miri only ships on nightly, while `rust-toolchain.toml` pins the
release toolchain, so the task always addresses `cargo +nightly miri`.
`--toolchain` picks another nightly.

`tooling/xtask/src/tasks/miri.rs` holds the list. Each entry records the Miri
settings its tests need, and the task passes them as an explicit `MIRIFLAGS`
instead of relying on one global default:

| Setting                               | Use it when                                                                                                                                                              |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `-Zmiri-ignore-leaks` (on entry)      | the tests leave a process-global executor alive to exit. Miri reports that as a leak, and the flag turns leak detection off for the whole run, so it is the last resort. |
| `-Zmiri-disable-isolation` (on entry) | the tests read the clock, the environment, or the filesystem. A hermetic test is the better fix.                                                                         |

With neither set, leak detection and sandboxing stay on, which is the point of
listing a crate at all. `.cargo/config.toml` keeps the permissive default for a
bare `cargo +nightly miri test -p <crate>` so that developer runs match what the
crate's tests expect; the task's explicit `MIRIFLAGS` wins over it because the
config entry is not `force`d. `cargo xtask check nightly-tools` fails if the entry
gains `force = true`, if a baseline flag disappears from it, or if a listed crate
is no longer a workspace member.

To add a crate, run it first and read the failure:

```sh
cargo xtask miri -p <crate>
```

A `can't call foreign function` error means the test reaches C through a
dependency and cannot run under Miri at all; a leak blamed on
`smol`/`async_io` reactor threads is the `ignore_leaks` case; `unsupported
operation: file open` is the isolation case. Only a crate whose tests exercise
real `unsafe` belongs on the list, and the entry's `reason` says which
invariant it is checking. `#[cfg_attr(miri, ignore)]` is the way to step around
a single test that cannot run (tree-sitter's C scanners are the common case),
with a comment naming why — the whole crate does not come off the list for one
test.

For a one-off probe, `--flag` forwards another setting without editing the
list:

```sh
cargo xtask miri --flag=-Zmiri-strict-provenance     # no integer-to-pointer casts
cargo xtask miri --flag=-Zmiri-tree-borrows          # compare aliasing models
```

Neither runs in CI: they are how you decide whether a failure is code that
should change or a model that is stricter than the code needs.

## Sanitizers

Miri stops at every foreign call, so nothing it reports covers the code on the
far side of FFI — sqlite through `sqlez`, FFmpeg, libgit2, Wayland. The LLVM
sanitizers run the same tests natively with instrumentation instead, which is
the only way to see a use-after-free in memory both sides own.

```sh
cargo xtask sanitize                     # AddressSanitizer over the curated list
cargo xtask sanitize --list              # what is on the list, and why
cargo xtask sanitize -p util             # one crate
cargo xtask sanitize --sanitizer thread  # data races instead
```

The task always addresses a nightly toolchain, because `-Zsanitizer` is a nightly
flag. It fixes three details that are easy to get wrong by hand:

- `--target x86_64-unknown-linux-gnu` is passed on a host build too. Without it,
  `RUSTFLAGS` also applies to build scripts and proc-macros, which then need an
  instrumented runtime before the crate under test compiles.
- `RUSTFLAGS` replaces the `[build] rustflags` in `.cargo/config.toml` rather
  than extending it, so the task repeats the mangling and `tokio_unstable`
  settings. Dropping `tokio_unstable` breaks any crate that uses an unstable
  tokio API.
- Tests run with `--test-threads 1`, so a report names the test that caused it
  instead of whichever thread happened to allocate.
- Artifacts go to `target/sanitize/<sanitizer>`. Cargo does not key its cache on
  `RUSTFLAGS`, and an rlib built with one sanitizer cannot link into a crate
  built with another — a shared target directory fails on whatever the previous
  run cached.

`tooling/xtask/src/tasks/sanitize.rs` holds the crate list, and every entry
carries the `unsafe` or FFI surface it is there to check. `cargo xtask check
nightly-tools` rejects a listed crate that leaves the workspace, and one that
pulls in `mimalloc`: a custom global allocator sits in front of the allocator
interceptors, so the heap checks would stop applying while the job stayed green.

`--build-std` additionally instruments the standard library. AddressSanitizer
works without it and the flag costs a multi-minute one-time rebuild, so it is off
by default. ThreadSanitizer refuses a prebuilt `core` — the flag changes the ABI,
so the task always instruments std for it.

AddressSanitizer's leak check runs at process exit. Expect the editor's test
harness to leave process-global state behind; a leak report naming a test's own
allocation is the one that matters. ThreadSanitizer is narrower on purpose — it
does not understand atomic fences or synchronization performed in inline
assembly, so it is only worth running on crates whose threads synchronize
through instrumented atomics and `pthread` primitives.

The whole sweep runs in CI on a schedule, not on pull requests: an instrumented
build of a crate and its dependency graph is far too slow to block a change. Run
the one crate you touched.

## Disabled tests

A disabled test must say why:

```rust
#[ignore = "requires a Wayland compositor and Vulkan"]
fn test_surface_creation() {}
```

`cargo xtask check test-markers` rejects a bare `#[ignore]`, so the next
reader knows what would un-ignore it. Remove the attribute once the reason is
gone; an ignored test that stays ignored for a release is a bug report
nobody reads.

Visual regression tests are the one large legitimate class: they need macOS,
Screen Recording permission, and the main thread, so they stay ignored and
run on demand. Update baselines with
`UPDATE_BASELINES=1 cargo test -p zzz visual_tests -- --ignored --test-threads=1`
and commit the images with the change that motivated them.

`#[should_panic]` is allowed but rare; the gate prints the current count so a
new one is visible in review. Prefer asserting the error value, following the
reasoning in rust-analyzer's style guide.

## Flakes

There is no retry configuration. A test that fails intermittently is either a
test bug or a product bug, and a retry hides both. Mark it
`#[ignore = "flaky; <link to the issue>"]` while it is investigated, and open
an issue rather than letting CI rerun it green.
