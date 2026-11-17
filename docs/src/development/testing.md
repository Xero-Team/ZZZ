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
