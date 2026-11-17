# Reference: ZZZ Performance Harnesses

Normative material for `profiling-performance`. Load this before running a
measurement. Commands run from the repository root.

## Contents

- The two harnesses
- perf-test
- perf-compare
- criterion benches
- Ground rules
- Comparing honestly
- Sources

## The two harnesses

Neither runs in CI. A regression stays invisible until someone runs one.

| Harness | Entry point | Measures | Crates |
|---|---|---|---|
| `#[perf]` test profiler | `cargo perf-test -p <crate>` | wall time per annotated test, repeated by `hyperfine` | `gpui`, `search`, `terminal`, `util`, `vim` |
| criterion benches | `cargo bench -p <crate>` | criterion statistics per bench target | `editor`, `language`, `rope`, `fuzzy_nucleo`, `project_panel`, `extension_host` |

The `#[perf]` attribute lives in `crates/util_macros`; the profiler is
`tooling/perf`. Both aliases come from `.cargo/config.toml`:

```toml
perf-test = ["test", "--profile", "release-fast", "--lib", "--bins", "--tests",
  "--all-features", "--config", "target.'cfg(true)'.runner='cargo run -p perf --release'",
  "--config", "target.'cfg(true)'.rustflags=[\"--cfg\", \"perf_enabled\"]"]
perf-compare = ["run", "--profile", "release-fast", "-p", "perf",
  "--config", "target.'cfg(true)'.rustflags=[\"--cfg\", \"perf_enabled\"]", "--", "compare"]
```

Both rebuild under `release-fast`, so the first run of a session is slow. Do not
treat that churn as noise in the result you compare: wait for a stable run.

## perf-test

```sh
cargo perf-test -p <crate>                        # one crate
cargo perf-test --workspace                       # very slow, avoid casually
cargo perf-test -p <crate> -- --important         # only important tests
cargo perf-test -p <crate> -- --quiet             # no progress output
cargo perf-test -p <crate> -- --json=before       # save .perf-runs/before.<crate>.json
```

Requires `hyperfine` on `PATH`. The `--json=<ident>` run file is what
`perf-compare` consumes, so always give a baseline and an after-run distinct idents.

Tests are annotated in place:

```rust
#[perf]
#[gpui::test]
async fn test_search_simple(cx: &mut TestAppContext) { /* ... */ }
```

Report a `perf-test` number as "test timing", not as frame time. It measures the
workload inside one test, including its setup.

## perf-compare

```sh
cargo perf-compare after before
cargo perf-compare --save=perf-diff.md after before
```

Considerations that change the verdict:

- Every file matching `.perf-runs/<ident>.*.json` participates, so a stale ident
  from an earlier experiment corrupts the comparison. Use a fresh name per
  experiment and delete old run files when in doubt.
- The comparison is only meaningful between runs from the same machine, toolchain,
  and profile.
- Save the table. The report should carry real output, not a summary of it.

## criterion benches

```sh
cargo bench -p <crate> --bench <target>
```

| Crate | Targets | What they cover |
|---|---|---|
| `editor` | `editor_frame_workload` | frame time, RSS, allocation count end to end |
| `editor` | `editor_render`, `display_map` | rendering and display-map work |
| `language` | `highlight_map` | syntax highlighting maps |
| `rope` | `rope_benchmark` | rope operations |
| `fuzzy_nucleo` | `match_benchmark` | fuzzy matching |
| `project_panel` | `sorting` | entry sorting |
| `extension_host` | `extension_compilation_benchmark` | extension compile path |

All are declared with `harness = false` in their `Cargo.toml`, so criterion's own
statistics and its `--save-baseline` / `--baseline` flags apply. `crates/*_benchmarks`
are separate driver binaries, not criterion targets; use one only when no bench
covers the case.

For a frame-time question, `editor_frame_workload` is the end-to-end measurement:
it counts allocations through a `GlobalAlloc` wrapper, samples RSS, and runs
cached and scrolling frames (`WARMUP_FRAMES`, `CACHED_FRAMES`, `SCROLL_FRAMES`).
Its optional `frame-diagnostics` feature adds per-phase frame timings.

## Ground rules

From `AGENTS.md` and `docs/src/development/build-pipeline.md`:

- Never edit a benchmark to hit a target.
- Never run two benchmarks concurrently.
- No `RUSTFLAGS` or `target-cpu=native` heroics.
- Keep measured cases independent.
- No gate enforces any of this; the discipline is the method.

Optimization priority: compute, memory, binary size, compile time. A saving in a
later category does not excuse a regression in an earlier one.

## Comparing honestly

- State the workload. "Search is 3% faster" is unusable; "`test_search_simple` in
  `search`, same machine, release-fast, 5% faster" is a result.
- Report the spread when you have it. A delta inside the run-to-run spread is not a
  result, and two hand-averaged runs are not statistics.
- Separate correctness from speed. Run `cargo test -p <crate>` before claiming the
  faster path is the same behavior.
- Report unavailable measurements as unavailable. A GUI-only workload on a headless
  host is not substitutable.
- Keep one change per measurement. Two optimizations in one run produce one
  unattributable delta.

## Sources

- Repository: `AGENTS.md` (performance work), `.cargo/config.toml` (aliases),
  `tooling/perf/src/main.rs`, `crates/util_macros`,
  `crates/editor/benches/editor_frame_workload.rs`,
  `docs/src/development/build-pipeline.md`.
- Adapted from `performance-profiler` in
  https://github.com/alirezarezvani/claude-skills (MIT). The "measure first, then
  verify with the same command" discipline comes from there; the harnesses and
  commands are ZZZ's, so its Python profiler and k6 recipes are not bundled.
