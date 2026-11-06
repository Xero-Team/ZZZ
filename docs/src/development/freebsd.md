---
title: Building ZZZ for FreeBSD
description: "Build and package ZZZ for FreeBSD x86_64 and aarch64."
---

# Building ZZZ for FreeBSD

ZZZ builds for `x86_64-unknown-freebsd` and `aarch64-unknown-freebsd` through
the same `cargo xtask` pipeline as Linux. You can build natively on a FreeBSD
host or cross-compile from Linux.

## Native build (FreeBSD host) {#native}

Install the system dependencies and the Rust toolchain:

```sh
script/freebsd
rustup target add x86_64-unknown-freebsd
rustup target add aarch64-unknown-freebsd
```

Then build and package the host target:

```sh
cargo run
cargo test --workspace
cargo xtask bundle --target x86_64-unknown-freebsd
```

`cargo run` is a development build and stays on the `dev` channel.
`cargo xtask bundle` packages a `stable` build into
`target/release/zzz-freebsd-x86_64.tar.gz`.

## Cross build (Linux host) {#cross}

`cargo xtask build` containerizes FreeBSD targets on a Linux host. The
toolchain image (`Dockerfile-toolchain`) ships both FreeBSD 14.5 sysroots
(amd64 and arm64), a clang/lld wrapper per architecture, and the two `rust-std`
targets. Build both targets together:

```sh
cargo xtask build \
  --target x86_64-unknown-freebsd \
  --target aarch64-unknown-freebsd \
  --formats tar.gz
```

The planner rebuilds the image when its inputs change, so you do not maintain
the sysroots yourself. `cargo xtask build` checks the grammars, builds the
remote-server matrix, and packages the tarballs. Add `--channel preview` or
`--channel nightly` to build a different release channel.

Each architecture uses its own sysroot under
`/opt/freebsd/sysroot/{amd64,arm64}`. The image verifies the `base.txz`
archives against the pinned 14.5-RELEASE SHA256 sums, so the libc and headers
in every FreeBSD artifact are reproducible.

> **Note:** A native FreeBSD host builds its own targets without the image.
> Use `cargo xtask bundle --target x86_64-unknown-freebsd` there.

### Host-provided toolchain {#host-toolchain}

Drop `--container never` to build FreeBSD targets on the host. You then supply
a sysroot, an executable clang/lld wrapper, and the target's `rust-std` through
the environment. No path is baked into the repository:

```sh
export ZZZ_FREEBSD_SYSROOT_X86_64_UNKNOWN_FREEBSD=/path/to/amd64-sysroot
export ZZZ_FREEBSD_SYSROOT_AARCH64_UNKNOWN_FREEBSD=/path/to/arm64-sysroot
export CARGO_TARGET_X86_64_UNKNOWN_FREEBSD_LINKER=/path/to/x86_64-clang-wrapper
export CARGO_TARGET_AARCH64_UNKNOWN_FREEBSD_LINKER=/path/to/aarch64-clang-wrapper
export CC_x86_64_unknown_freebsd=/path/to/x86_64-clang-wrapper
export CC_aarch64_unknown_freebsd=/path/to/aarch64-clang-wrapper
```

Each sysroot must contain `usr/lib`, `usr/include`, and `lib`. The two
architectures need separate values; a single `ZZZ_FREEBSD_SYSROOT` is accepted
only when you build one target. Cargo does not split a linker value on spaces,
so the linker must be one executable wrapper that supplies
`--target=<triple> --sysroot=<path> -fuse-ld=lld`. Additional linker flags go
in `ZZZ_FREEBSD_RUSTFLAGS`. If you installed a complete cross toolchain
instead of the target's standard library, select it with `RUSTUP_TOOLCHAIN`
(or `ZZZ_FREEBSD_RUSTC`).

`cargo xtask doctor` reports whether the sysroot and both linkers are
configured. Without the variables, the image supplies everything, so a Linux
host can still build every target.

## What the tarball contains {#contents}

The archive holds the `zzz` and `cli` binaries, the generated license
document, the desktop entry, and the icons, in the same `.app` layout as the
Linux tarball. Shared libraries come from the FreeBSD system rather than the
bundle. Install the runtime dependencies listed by `script/freebsd` before
running the editor.

## Troubleshooting {#troubleshooting}

### Cargo errors claiming that a dependency is using unstable features

Try `cargo clean` and `cargo build`.

### The linker cannot find FreeBSD libraries

Check `ZZZ_FREEBSD_SYSROOT`, the `--sysroot` in the linker driver, and
`cargo xtask doctor`. The cross build passes the same sysroot to the linker,
`cc`, and bindgen.
