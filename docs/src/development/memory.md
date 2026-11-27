---
title: Diagnosing Memory Growth
description: "How to tell a slow leak from a large baseline in ZZZ, and where to go once you know it is growing."
---

# Diagnosing Memory Growth

A slow leak rarely crashes anything. It looks like an editor that was fine at
lunchtime and swaps by evening, and it is invisible to peak-memory tools: the
peak is reached immediately and then the line climbs past it. What distinguishes
the two cases is the _slope_ over time, so every measurement below is built
around producing a series rather than a single reading.

## Two numbers, and what each one lies about

| Metric                 | Source                                                                            | What it misses                                                                                  |
| ---------------------- | --------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `resident_bytes`       | `sysinfo`, i.e. `/proc` on Linux, `proc_pidinfo` on macOS, working set on Windows | Freed pages the allocator keeps, and on macOS the compressed pages that Activity Monitor counts |
| `heap_committed_bytes` | mimalloc's own statistics                                                         | Anything allocated outside mimalloc                                                             |
| `peak_resident_bytes`  | mimalloc's own statistics                                                         | Growth after the peak, which is exactly what a leak does                                        |

`heap_committed_bytes` is the useful one for leak hunting: it rises when live
allocations outgrow what the allocator already holds, so a leak shows up there
even when the OS has not reclaimed anything yet. `resident_bytes` is the number
users see, so it decides whether something is a problem; the heap number decides
whether the cause is retention or fragmentation.

The macOS caveat is worth stating plainly: `resident_bytes` is the resident set,
not the physical footprint that Activity Monitor shows. The two diverge when the
kernel compresses pages, which is why VS Code's process explorer showed numbers
users did not recognise for years before it switched to footprint
([microsoft/vscode#194322](https://github.com/microsoft/vscode/issues/194322)).
ZZZ does not report footprint yet, so do not compare `resident_bytes` against
Activity Monitor on a memory-pressured machine.

## Sampling a running editor

The `zzz: report memory usage` action (command palette) samples once, logs the
result and shows it in a notification. The same action also walks the calling
thread's live heap blocks and logs them bucketed by size, which separates "many
small objects are being retained" from "a few large buffers are retained" — a
distinction the totals cannot make. The walk is linear in the number of live
blocks, so it runs on request only, and it sees just the thread that dispatched
the action. For a series, set
`ZZZ_MEMORY_LOG_INTERVAL_SECS` before launching:

```sh
ZZZ_MEMORY_LOG_INTERVAL_SECS=60 cargo run
```

Sampling is off unless that variable is set, because a shipped editor should not
carry a background timer nobody asked for. `0` and negative values also leave it
off, so a launcher can set the variable unconditionally. Each sample is one log
line:

```text
memory usage: resident 412.1 MiB, virtual 1 GiB, heap committed 210.4 MiB (peak 240.1 MiB), reserved 1 GiB, 3216 pages, peak resident 512.7 MiB
```

Reproduce the same workflow for at least half an hour before drawing a
conclusion. A jump that appears once and settles is a cache warming up; a line
that keeps climbing after the workflow repeats is the leak.

The counters are cheap — a sample is a `/proc` read plus a JSON snapshot of
mimalloc's statistics — so an interval of a minute costs nothing worth
measuring. What must stay out of a shipped build is stack capture: recording
which call site allocated each block costs orders of magnitude more, and belongs
in the dedicated builds described below.

## Measuring it as a workload instead

`crates/editor/benches/editor_frame_workload.rs` runs a fixed editor workload
and prints one memory sample per interval when given a duration:

```sh
cargo run -p editor --bench editor_frame_workload -- \
  <source> <typing> <resize-tsv> --duration=1800 --sample-interval=60
```

`cargo perf-soak` then reports the least-squares slope of every metric the run
prints:

```sh
cargo perf-soak --minutes=30 -- ./target/release-fast/deps/editor_frame_workload ...
```

Each sample carries its own `elapsed_secs`, and `perf-soak` prefers that over
the time the subprocess finished. Without it, one long run would stamp every
line with the same value and the slope would be undefined. A flat
`heap_committed_bytes` slope over a repeating workload is the evidence that a
change did not leak; a climbing one is the evidence that it did.

The workload binary measures `resident_bytes` only. It installs
`bench_metrics::CountingAllocator` as its global allocator, so mimalloc's
statistics do not describe it, and reporting them would be misleading.

## Finding the allocation site

Once a slope says "growing", the next question is "growing where", and that
needs per-allocation stacks. Two changes to the build make the standard tools
usable:

```sh
# 1. Drop mimalloc so LD_PRELOAD-based tools can see individual allocations.
# 2. Keep symbols; the release profile strips most of them by default.
cargo run --no-default-features --config 'profile.release-fast.debug="full"'
```

With that binary, `heaptrack` and `valgrind --tool=massif` attribute allocations
to call sites. Both are Linux-only; `dhat` (`dhat-rs` for Rust) works on every
platform ZZZ ships. All three want an optimised build — profiling a `dev` build
measures a program nobody runs.

An alternative that needs no external tool: build with
`ZTRACING_WITH_MEMORY=1` and `--features zzz/tracy`, which installs
`tracy_client::ProfiledAllocator` and records allocation stacks in the Tracy
profiler. That allocator wraps `std::alloc::System` and cannot coexist with the
mimalloc global allocator, so it also requires `--no-default-features`.

Because mimalloc is linked statically, probes have to target its symbols rather
than libc's. `script/memleak-zzz.bt` does that with bpftrace: it pairs
`mi_malloc_aligned` with `mi_free`, prints how many blocks are outstanding, and
on exit prints the allocation stacks of live blocks over a size threshold.

```sh
sudo bpftrace script/memleak-zzz.bt target/release/zzz 1048576
```

This needs no rebuild and no special allocator, which makes it the cheapest way
to attribute growth in a build a user already has. The count line answers "is it
still accumulating"; the stacks answer "from where".

## Returning memory when the process goes quiet

mimalloc purges on a delay that new allocations keep pushing back, so a process
that stops working can hold committed memory it no longer uses. A freed 192 MiB
block stayed resident indefinitely in
[microsoft/mimalloc#1422](https://github.com/microsoft/mimalloc/issues/1422);
one non-forced collect dropped it to 6 MiB.

ZZZ samples memory every 30 seconds on a background task and, once the numbers
have been unchanged for three consecutive samples, calls `mi_collect(false)`
once. Unchanged counters are the signal that the pages are idle: collecting
while memory is moving would just hand the OS pages that the next burst faults
straight back in.

The check is on by default because it costs nothing when the editor is in use.
`ZZZ_MEMORY_IDLE_RELEASE=0` turns it off; any other value, or no value at all,
leaves it on.

## mimalloc options

One of mimalloc v3's defaults suits a server that wants throughput more than a
desktop app that wants a small footprint. ZZZ turns it off in
`crates/cli/src/main.rs`, and `.cargo/config.toml` repeats it for `cargo run`
and `cargo test`. An explicit value in the environment always wins.

| Variable             | Default in ZZZ | Why                                                                                    |
| -------------------- | -------------- | -------------------------------------------------------------------------------------- |
| `MIMALLOC_ALLOW_THP` | `0`            | 2 MiB huge pages inflate resident memory by ~300 MB for a mostly small-object workload |

`MIMALLOC_ARENA_EAGER_COMMIT=0` is the obvious next candidate and was measured
to do nothing here: a 200 MB allocation burst held 199.9 MiB resident with and
without it, because the editor's small objects never reach the arena path. It is
not set, so that the defaults stay honest until a workload that exercises arenas
justifies it.

The option that looks tempting and is not: `MIMALLOC_PURGE_DELAY=0` returns
every freed block to the OS immediately, which is exactly what an allocation-heavy
editor does constantly. Measured elsewhere at −57% throughput and, on another
workload, −31% throughput with no resident-memory saving
([CPython v2→v3 report](https://python.ca/nas/python/mimalloc-v2-v3-performance-report.html),
[RustCFML notes](https://github.com/RustCFML/RustCFML/blob/main/docs/memory.md)).
The idle collect above gets the same benefit without paying per free.

## What ZZZ does not do

There is no upload path. Measurements stay in the local log file or in the
notification, and `script/check-philosophy` rejects any telemetry endpoint,
telemetry macro, or upload module in `crates/`. Crash reporting works the same
way: local minidumps only.

GPUI's `leak-detection` feature is a different tool. It tracks strong-reference
cycles between entities and reports the allocation site of handles that were
never released — useful for "this panel never goes away", useless for "this
cache grows without bound", because the latter keeps every reference reachable.

## Related

- [Keeping ZZZ Lean](./bloat.md)
- [Build and Optimization Pipeline](./build-pipeline.md)
- [Using Debuggers](./debuggers.md)
