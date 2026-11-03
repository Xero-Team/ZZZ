---
title: Build and Optimization Pipeline
description: "How ZZZ trades compile time for runtime speed, and how to run PGO and BOLT builds."
---

# Build and Optimization Pipeline

ZZZ is built for how it runs, not for how quickly it compiles. This page
records the priority order behind the profiles in `Cargo.toml`, what the
packaging scripts do on top of them, and how the profile-guided optimization
pipeline fits in.

## Priority order {#priority-order}

Every build knob is judged in this order:

1. Compute cost at runtime
2. Memory cost at runtime
3. Binary size
4. Everything else
5. Compile time

Compile time is last, but it is not free to regress. A change that only helps
compile time is acceptable while runtime is unchanged. A change that helps
compile time and hurts runtime is not.

## Profiles {#profiles}

| Profile        | Use             | Settings                                                      |
| -------------- | --------------- | ------------------------------------------------------------- |
| `dev`          | Day to day      | opt-level 0, `debug = "limited"`, 16 CGUs, incremental        |
| `dbg`          | Debugging       | `dev` with full debug info                                    |
| `release`      | Shipping        | opt-level 3, fat LTO, 1 CGU, `panic = "abort"`, stripped      |
| `release-fast` | Profiling       | `release` without LTO, 16 CGUs, full debug info, not stripped |
| `linux-native` | Local max speed | `release` with 1 CGU for the `zzz` crate                      |

Two details are easy to misread:

- `[profile.release.package] zzz = { codegen-units = 16 }` costs little
  runtime. Fat LTO merges and re-optimizes the IR of every crate, so the
  pre-LTO split mostly buys parallelism in the largest crate's codegen.
  `linux-native` turns it back to 1 for people who want the last few percent.
- `[profile.release] strip = true` is deliberate. `release` is the shipping
  profile, and packaging strips the artifacts it copies anyway. Builds that
  need symbols opt out per build, as described below.

`release-fast` keeps `panic = "abort"` on purpose. A profiling build that
unwinds measures different code than the binary you ship.

## One entry point {#entry-point}

`cargo xtask build` is the only supported way to produce artifacts. The
implementation is Rust in `tooling/xtask`: a planner assembles the task graph,
a scheduler runs it with a shared CPU budget, and every platform's packaging
lives in `tooling/xtask/src/tasks/bundle`. Shell helpers remain only for
non-build concerns such as `script/linux` installing system packages.

```sh
cargo xtask build                        # host target, release profile
cargo xtask build --target aarch64-apple-darwin
cargo xtask dist --all                   # default artifacts for each target this host can build
cargo xtask bundle --skip-build          # package what is already built
cargo xtask build --print-plan           # the task graph as JSON
cargo xtask doctor                       # can this machine build a release?
```

Options that used to be spelled differently per script:

| Option        | Meaning                                                                                    |
| ------------- | ------------------------------------------------------------------------------------------ |
| `--target`    | Target triple, repeatable. Defaults to the host.                                           |
| `--profile`   | `release`, `release-fast`, `linux-native`.                                                 |
| `--formats`   | `tar.gz`, `deb`, `rpm`, `appimage`, `flatpak`, `snap`, `app`, `dmg`, `exe`, `msi`, `inno`. |
| `--container` | `auto` (releases), `always`, `never`.                                                      |
| `--pgo`       | `auto`, `use`, `off`. `--no-pgo` is the explicit opt-out.                                  |
| `--jobs`      | Total CPU budget for the _whole_ plan, split across targets.                               |
| `--offline`   | Fetch first, then build with no network in the container.                                  |

`cargo xtask build` packages artifacts, so it only accepts the release-family
profiles; a `dev` or `dbg` build is `cargo build` or `cargo run`. That is also
why `--container auto` can treat every `build` invocation as a release: there is
no development profile to exempt.

`--print-plan` emits the graph the scheduler executes, so CI can inspect or
replay a build without running it. Every task carries its own command,
environment, working directory, and container image.

`cargo xtask bundle` packages without planning. Linux splits its formats: the
tarball and the deb/rpm packages run inside the toolchain image, while AppImage,
Flatpak, and Snap are host post-steps on top of that tarball. macOS produces
`app` and `dmg` (assembled in the image when cross-compiling, signed and
notarized only on macOS), and Windows produces the portable `exe` in the image
while the `msi` and `inno` installers need a Windows host.

### Parallelism {#parallelism}

Cargo parallelizes inside one invocation; it cannot coordinate several. The
scheduler treats `--jobs` as a budget for the entire plan and gives each ready
task a share, so a Linux bundle, a macOS cross bundle, and a Windows cross
bundle can run at the same time without oversubscribing the machine. A task
in the plan is only started once its dependencies finished and the budget has
room for it.

Parallel targets share caches that were not written for concurrent use. `zig`
gets one content-addressed global cache under `target/zig-cache`, which is safe
to share, while `cargo-zigbuild` gets a wrapper directory per target: its shared
links are created with a check-then-`symlink` that loses to a concurrent build,
and the loser exits with `File exists (os error 17)` before `cargo` starts.

A cross build also leaves an empty `a.o` in the crate directory of every cc-rs
dependency. `zig cc --version`, which cc-rs runs to identify the compiler,
writes it there. That covers the vendored grammars and the vendored `psm`, and
`.gitignore` names both so the matrix fingerprint stays clean.

### Artifact manifest {#manifest}

`cargo xtask dist` writes `dist/manifest.json` and `dist/SHA256SUMS` next to
copies of the artifacts. The manifest records what produced each file — commit
and dirty state, version, channel, profile, job budget, container image and its
id, PGO fingerprint per target, and the checksum — so a file can be traced
without rebuilding it. `cargo xtask verify checksums dist` re-checks the set,
and `cargo xtask verify artifacts --target <triple>` re-runs the per-binary
checks (glibc floor, `--version`).

### Repository gates {#gates}

Everything that does not need a compiler is `cargo xtask check`. The default
run covers the unfinished-work markers, the keymaps (shared files must not spell
the primary modifier `cmd-`, and no binding object may repeat a key), the
`LICENSE-AGPL` symlink in every crate directory, the philosophy boundary, the
upstream rejection ledger, and the `run:` patterns in the workflow files.
`cargo xtask check shellcheck`, `cargo xtask check vendor`, and
`cargo xtask check links` are opt-in, because they need tools the default
runner does not install.

`cargo xtask clippy` is the lint gate. It runs the philosophy preflight, then
three `cargo clippy` passes: workspace lints with `--all-targets`, the manifest
lints that a `[workspace.lints]` table cannot reach, and `future_not_send`
restricted to the packages where it is high-signal. Locally, it then runs
`cargo machete`, `typos`, and `buf` when those tools are installed; CI sets
`GITHUB_ACTIONS`, which skips that block.

`script/check-philosophy` and `script/check-upstream-ledger` stay as scripts:
they are policy engines made of product rules and ledger rows, and the gates
invoke them. `script/clippy`, `script/check-licenses`, `script/check-todos`,
`script/check-keymaps`, and `script/shellcheck-scripts` are forwarding shims so
older docs and editor tasks keep working.

### Containers {#container-builds}

The build host's C library ends up in the artifact. Every libc symbol the linker
resolves is versioned against that host, so a bundle built on a rolling
distribution starts nowhere else:

```
/lib64/libc.so.6: version `GLIBC_2.45' not found
```

Release profiles therefore build inside the pinned toolchain image by default
(`--container auto`). A development build never reaches this path at all: it is
a `cargo build` on the host, where the debugger, `strace`, and a fast rebuild
matter more than the glibc floor.

```sh
cargo xtask build --container always     # require the image
cargo xtask build --container never      # accept the host's floor
```

The image is defined by `Dockerfile-toolchain`. Its tag is a hash of the files
that define the toolchain, so editing the Dockerfile, `script/linux`,
`script/install-cmake` (or the release helper it sources), or
`rust-toolchain.toml` produces a new tag instead of silently reusing a stale
image. `cargo xtask doctor` prints the tag and the exact build command when the
image is missing; the plan builds it on demand before the first task that needs
it.

The base image is pinned by digest, so a rebuild does not pick up a newer Ubuntu
point release. `ZZZ_BASE_IMAGE=debian:bookworm` swaps the base for a one-off
image when you need to; both known-good bases sit at or below the supported
glibc floor of 2.35.

The container keeps its own target directory (`target/container`) and mounts
only Cargo's download caches (`registry`, `git`) from `.cache/container-cargo`.
Cargo does not fingerprint the C library or the linker, so pointing it at the
host `target/` would relink stale objects built against the host glibc; mounting
the whole cargo home would hide the toolchain binaries that live in it.

A proxy configured on the host is forwarded to the container. Both engines copy
`http_proxy` and the other proxy variables verbatim, which breaks when the proxy
listens on the host loopback: inside the container that address is the container
itself. `cargo xtask` rewrites those addresses to the engine's alias for the
build machine (`host.containers.internal` for podman, `host.docker.internal` for
Docker) and passes the rewritten values itself, so a local proxy keeps working
for the image build and for the tasks the image runs.

Rootless podman is the better engine here: it maps container root to you, so the
artifacts it writes into the mounted tree stay yours. A rootful Docker daemon
writes them as root instead, and removing `target/container` then needs `sudo`.
Both work; only the file ownership differs.

The image covers the Linux build, the macOS cross build (zig, `cargo-zigbuild`,
Zed's `cargo-bundle`, `hfsprogs`, libdmg-hfsplus, and a macOS SDK), and PGO
(llvm-profdata, Xvfb, a software rasterizer). The image pins SDK 14.5 so the
image tag keeps describing all of its inputs. Cross-building outside the image
goes through `script/ensure-macos-sdk`, which resolves the newest release
instead; both work because the bindgen step asserts `TARGET_OS_OSX` for the
libclang builds that lack clang's `define_target_os_macros` extension.

Every image also carries LLVM 23 from apt.llvm.org. bindgen picks its builtin
include path from the `clang` it finds, and the distribution's LLVM 14 cannot
parse the pinned SDK, so the macOS bindings would fail to generate on a Linux
host. The same LLVM drives `clang-cl` in the Windows layer, whose MSVC STL
refuses compilers older than 19.

Windows cross builds need the `msvc-wine` layer, which downloads Microsoft's
MSVC toolset and Windows SDK and runs `cl`, `link`, and `fxc` under wine. The
planner asks for it by itself, so a Windows target needs no extra flag and no
manual image build:

```sh
cargo xtask build --target x86_64-pc-windows-msvc   # builds the -msvc image once
```

The tag gains an `-msvc` suffix, so a Windows image is never mistaken for the
plain one. Building that layer accepts Microsoft's license, which is why it is a
separate variant that only plans with Windows targets trigger. `ZZZ_WITH_MSVC=1`
forces the variant when you want the image built ahead of time.

`cc` asks for a driver named `clang-cl` on a MSVC target, and clang takes that
mode from the name it is invoked as. Ubuntu's llvm-14 is too old for the pinned
toolset: MSVC's standard library refuses a Clang older than 19 in C++ mode, and
`libmimalloc-sys` compiles as C++ there because mimalloc wants its C++ atomics
path; `lld-link` 14 also cannot read the function-ID records in the toolset's
runtime libraries, so every link warns about invalid PDB item indexes. The layer
installs clang-19 and lld-19 from LLVM's own archive and points `clang-cl` and
`lld-link` at them. The archiver and resource compiler stay on Ubuntu's llvm-14,
since they only read and write COFF objects. msvc-wine needs a `wine64` name on
`PATH` too, and the `wine64` package ships only `wine64-stable`. All three names
are symlinks in their own layers, so none of them refetches Microsoft's toolset.

`psm`, which `stacker` needs, hits the same kind of gap: off Windows it
assembles GAS sources, and a MSVC-mode driver cannot preprocess a `.s` file.
The vendored patch stages each source under its `.S` spelling, which asks the
driver for the C preprocessor, and passes the target defines the source reads.
`clang-cl` then runs the preprocessor as a separate step and writes its result
to `<stem>.asm` in the current directory, where `clang` would use the system
temporary directory; the patch compiles from `OUT_DIR` so that intermediate
does not land in the vendored tree.

Dependencies built with CMake need one more piece. `libopus`, which `rodio`
links for Opus playback, arrives through `opusic-sys`, whose build script runs
CMake. A Linux host cannot find a Visual Studio installation, and CMake only
takes `CMAKE_SYSTEM_NAME` from a toolchain file, so `cargo xtask` writes one per
target under `target/container/msvc/` and points `CMAKE_TOOLCHAIN_FILE_<triple>`
at it: `clang-cl`, `lld-link`, and `llvm-lib` with the same MSVC headers `cc`
uses. `clang-cl` splits `INCLUDE` on the host's separator, so msvc-wine's
Windows-style path list never resolves there; the toolchain file lists the
directories as `CMAKE_C_STANDARD_INCLUDE_DIRECTORIES` instead, which CMake turns
into `-imsvc` flags. The image ships `ninja` because the `cmake` crate asks for
a Visual Studio generator on an MSVC target and panics without one, while a
generator named after Ninja is the branch it takes instead.

The layer pins its toolset (`MSVC_VERSION=18.0`) and SDK
(`SDK_VERSION=10.0.26100`) because `vsdownload.py` otherwise resolves the newest
ones from Microsoft, and the image tag would stop describing the image content.

Native macOS signing and notarization, and native Windows MSI/Inno Setup
installers, stay on their own hosts. They need Apple and Microsoft tooling that
does not run on Linux, and a container is not the reason those steps exist.

Two inputs are still fetched at build time rather than baked into the image: the
`dugite` git binary that the macOS bundle embeds, and the pinned FFmpeg
development package the Windows build links against. Both are checksum-verified,
and the FFmpeg package is cached under `.deps/`; a cross build therefore needs
network on its first run, and `--offline` is for the Linux path.

Check the floor of any binary with:

```sh
cargo xtask verify glibc-floor path/to/zzz --max 2.35
```

## Packaging {#packaging}

`cargo xtask bundle --target <linux triple>` is the most elaborate path; the
planner calls the same code with the container, profile, and PGO variables
already set. It stages the cached remote-server matrix, builds `zzz` and `cli`,
then:

1. Reads `ZZZ_PGO_*` and `ZZZ_BOLT_*` to instrument, optimize, or prepare the
   host binary. The planner supplies them; see
   [Profile-guided optimization](#pgo).
2. Sets `CARGO_PROFILE_<PROFILE>_STRIP=none` for the host build when PGO, BOLT,
   or a Sentry upload needs the symbol table. The `remote_server` archives are
   gzipped before this point, so they keep the profile default.
3. Runs `llvm-bolt` when a profile was given.
4. Uploads symbols with `sentry-cli` when `ZZZ_ENABLE_SENTRY_UPLOAD=1`.
5. Runs `llvm-objcopy --strip-all` on the three binaries, which restores the
   shipped size after step 2.

Order matters: symbols are consumed before they are stripped. Stripping in the
profile for those builds would silently produce symbol-less Sentry uploads and
un-BOLTable binaries.

The macOS and Windows bundlers rely on the profile strip and upload `.dwarf` or
`.pdb` artifacts before signing. They have no BOLT path yet.

## Allocator {#allocator}

`mimalloc` is a default feature of the `zzz` crate, so every build of the
editor uses it. The system allocator is the one hot path shared by parsing,
text layout, search, and the UI, which makes it the cheapest place to buy
throughput and to lower fragmentation. Build with `--no-default-features` to
get glibc malloc back when you compare allocation behaviour.

## What is already at the limit {#at-the-limit}

- `opt-level = 3`, fat LTO, one CGU per crate, `panic = "abort"`.
- `cargo xtask bundle --profile linux-native` adds `-C target-cpu=native` for
  local builds; `script/install-linux --native` is the same thing plus install.
- Tracy spans, `profiling` counters, `frame-diagnostics`, and `stacker` are off
  unless you ask for them, so the default binary carries no instrumentation.
- The release profile enables no debug assertions and no overflow checks.

## Measured effects {#measured}

The numbers below come from `crates/editor/benches/editor_frame_workload.rs`
on a 32-core Linux machine, using the fixtures in
`.tmp/gpui-refactor/phase-0/workloads/` and 10 interleaved runs per
configuration. All values are median nanoseconds per frame.

| Workload       | glibc malloc |  mimalloc | mimalloc + PGO |
| -------------- | -----------: | --------: | -------------: |
| cached frames  |      557,695 |   543,488 |        426,441 |
| typing         |    2,921,888 | 2,847,899 |      2,606,454 |
| scroll         |      700,472 |   646,290 |        522,090 |
| resize         |      179,580 |   161,304 |        138,472 |
| peak RSS (KiB) |      325,094 |   386,474 |        371,182 |

- mimalloc: 2.5 to 10 percent faster frames, 19 percent more peak RSS. The
  allocator keeps more memory resident. `MIMALLOC_ARENA_EAGER_COMMIT=0` cuts the
  growth to 8 percent and gives back the typing gain.
- PGO: 10 to 23 percent faster frames on top of mimalloc, and 4 percent less
  peak RSS. This run trained and measured the same workload, so treat it as an
  upper bound. The scripted session in `script/pgo-train` is broader, and the
  shipped `release` build starts from a stronger baseline, so the real gain is
  smaller. Measure it before turning the profile on by default.
- The profile is `release-fast`, which has no LTO. The shipped `release` profile
  starts from a better baseline, so expect smaller but still material gains.

## Next steps {#next-steps}

These are ordered by expected runtime return. None of them run in CI, and none
should be adopted without a before and after measurement.

1. Measure a PGO release on the shipped `release` profile. The 10 to 23 percent
   in the table above is a self-trained upper bound measured on `release-fast`
   without LTO, so the real gain is smaller. The pipeline and the gate exist;
   what is missing is a measurement of the artifact you actually ship.
2. BOLT on top of PGO. Typically another 5 to 15 percent from layout alone, and
   the flags are already in the script. Not measured here; `llvm-bolt` was not
   available on the machine.
3. A baseline `-C target-cpu` for distributed x86_64 builds (`x86-64-v2`).
   Cheap, but it needs a compatibility decision.
4. `log` release filtering (`release_max_level_info` or `_warn`). Removes the
   formatting work of the `debug!` and `trace!` call sites and shrinks the
   binary. It also removes logs that support requests rely on.
5. Crash symbol quality. `[profile.release] debug = false` means there is no
   DWARF to upload, so crash reports resolve function names but not file and
   line. `debug = "line-tables-only"` fixes that at the cost of build time and
   on-disk debug sections.
6. Dependency deduplication. Duplicate package names still cost extra
   compilations; this is mostly compile time and size, not runtime.
7. CI caching. `.github/workflows/ci.yaml` shares no cache between jobs, and the
   generated sccache and target-cache steps are dormant.

## Profile-guided optimization {#pgo}

PGO needs two full builds. The first is instrumented and only exists to be run;
the second consumes the profile it produced. `cargo xtask pgo` drives both
stages, and the release path refuses to skip the profile silently.

```sh
cargo xtask pgo train    # 1. instrumented build, training session, merged profile
cargo xtask pgo build    # 2. shipping build that consumes the profile
cargo xtask pgo status   # where the profile lives, and how big it is
cargo xtask pgo verify   # profile summary, fails when the profile is too small
```

`train` and `build` accept `--target`, `--formats`, `--jobs`, `--container`,
and `--offline`. A profile only applies to a build with identical flags,
target, and compiler, so the two stages agree on everything that reaches the
compiler; `--formats` only decides what `pgo build` packages, because training
always builds the tarball it extracts.

PGO is **required** for non-`dev` channels on Linux targets. `cargo xtask build`
fails when the channel is `stable`, `preview`, or `nightly` and no profile for
that exact target and profile exists. `--no-pgo` is the explicit escape hatch
for a one-off build; the artifact manifest records the absence, so a release
built without a profile is visible as such. macOS and Windows have no training
harness yet: `--pgo auto` builds without a profile there, and `--pgo use` says
so instead of shipping a build that silently ignores the profile.

`train` builds the instrumented bundle under Xvfb itself, so no wrapper is
needed:

```sh
cargo xtask pgo train
```

On a machine without a GPU, or inside a container, the trainer sets
`LIBGL_ALWAYS_SOFTWARE=1` so Mesa falls back to its software rasterizer.

State lives under `target/pgo/<fingerprint>`: `raw/*.profraw` from the training
run, `bundle/` for the extracted instrumented bundle, and `merged.profdata`. The
fingerprint hashes the target, the profile, `rust-toolchain.toml`, and
`Cargo.toml`, so a toolchain bump invalidates the profile instead of reusing one
that no longer applies. `cargo xtask pgo status` prints the path for the host
target.
`llvm-profdata` has to match the LLVM version of the Rust toolchain that
produced the raw profiles: the tool prefers the copy shipped with that
toolchain, falls back to `PATH`, and refuses a merger from a different LLVM
major version. `rustup component add llvm-tools-preview` provides the matching
copy; `LLVM_PROFDATA` overrides it.

### Training {#pgo-training}

`script/pgo-train` runs the session. It generates one 100,000-line Rust file in a
throwaway folder, starts the instrumented editor on that folder with an isolated
`--user-data-dir`, opens the file through the file finder, types 16 lines of
Rust, runs a buffer search, scrolls 90 wheel steps, resizes through five window
geometries, and quits with the quit command.

The script uses `--user-data-dir` and opens a folder rather than the file itself.
A workspace opened on a single file shows an empty buffer, so the keystrokes
would land nowhere. It also sets `ZZZ_ALLOW_EMULATED_GPU=1`: without a GPU ZZZ
shows an "Unsupported GPU" prompt, and that prompt eats the keystrokes that
follow. The trainer does not trust the input either: it saves the typed text,
greps the file for a marker, and retries or fails when the editor dropped a line.
A session that types into a start screen still writes a profile, just a useless
one.

The shutdown is part of the workload. LLVM flushes the profile from an exit
handler, so the editor has to exit through its own quit action; a killed process
writes nothing. The fixture worktree stays untrusted on purpose: restricted mode
starts no language server and downloads nothing, which keeps the session
repeatable.

The training set decides the result. This session covers input, buffer edits,
display map updates, tree-sitter, search, scrolling, and layout at several window
sizes. It does not cover the terminal, the project finder, the agent panel, or
git. Extend `script/pgo-train` when those paths get hot in real use.

### CI {#pgo-ci}

The release job builds with a profile when the repository variable `ZZZ_PGO` is
`1`. It then installs `xvfb`, `xdotool`, Mesa's software rasterizer, and the
toolchain's `llvm-tools-preview` component, and runs

```sh
cargo xtask pgo train --target x86_64-unknown-linux-gnu --container never --offline
```

The bundle step then calls `cargo xtask build --pgo auto`, which finds the
profile through the same fingerprint the trainer wrote. Training runs outside
the network namespace because the instrumented bundle is not shipped. When
`ZZZ_PGO` is unset the step passes `--pgo off`, which records the decision in the
manifest instead of failing a channel build for a missing profile.

Expect about twice the release build time plus a minute of training. The profile
for a binary this size is roughly 190 MB and covers about 510,000 functions.

## BOLT {#bolt}

BOLT runs on a PGO build and takes either `perf` data or BOLT's own instrumented
output:

1. Build with `ZZZ_BOLT_PREPARE=1`, which adds `--emit-relocs,-znow` to the
   link. `perf2bolt` and `llvm-bolt` reject a binary without relocations.
2. Record a session, or run the BOLT-instrumented build, as in
   [Training](#pgo-training). Run the editor from the extracted bundle: its
   rpath expects the bundled `lib`.
3. Rebuild with `ZZZ_BOLT_PERF` or `ZZZ_BOLT_FDATA`, which runs `llvm-bolt`
   before the final strip.

`llvm-bolt` and `perf2bolt` must be on `PATH`; the build fails fast when they are
missing. The bundle also needs the tree-sitter CLI and FFmpeg that a normal
`cargo xtask bundle` run needs.

Record BOLT data from the same editing session that trains PGO. BOLT only
reorders the instructions the profile says are hot, so a build trained on opening
a single file rearranges the wrong code.

## Measuring {#measuring}

- `cargo perf-test` runs the perf harness under `--profile release-fast`, which
  keeps its debug info.
- `crates/editor/benches/editor_frame_workload.rs` measures frame time, RSS,
  and allocation count end to end.
- `cargo perf-compare` compares two runs; it needs `hyperfine`.

## How other projects do it {#comparison}

- ripgrep ships from a dedicated `release-lto` profile (fat LTO, one CGU,
  `panic = "abort"`) and keeps the default profile cheap.
- Bevy optimizes dependencies in dev with `[profile.dev.package."*"]` and
  relies on `lld` or `mold` plus optional dynamic linking to keep iteration
  fast, instead of trading runtime speed for compile time.
- Firefox is the reference PGO and BOLT pipeline: official builds instrument,
  run a workload, merge, and re-link with BOLT.
- Upstream Zed uses thin LTO and one CGU, and sets neither `panic = "abort"`
  nor stripping. This fork is stricter about runtime and pays for it in build
  time.

## See also {#see-also}

- [Performance](../performance.md) for CPU and tracing profiles.
- [Keeping ZZZ Lean](./bloat.md) for size and dependency work.
