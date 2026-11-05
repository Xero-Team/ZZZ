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

You need a FreeBSD sysroot, a clang/lld linker driver, and the target's
`rust-std`. Point the build at them through the environment; no path is baked
into the repository:

```sh
export ZZZ_FREEBSD_SYSROOT=/path/to/freebsd-sysroot
export CARGO_TARGET_X86_64_UNKNOWN_FREEBSD_LINKER="clang --target=x86_64-unknown-freebsd --sysroot=$ZZZ_FREEBSD_SYSROOT -fuse-ld=lld"
export CARGO_TARGET_AARCH64_UNKNOWN_FREEBSD_LINKER="clang --target=aarch64-unknown-freebsd --sysroot=$ZZZ_FREEBSD_SYSROOT -fuse-ld=lld"
export CC_x86_64_unknown_freebsd="clang --target=x86_64-unknown-freebsd --sysroot=$ZZZ_FREEBSD_SYSROOT"
export CC_aarch64_unknown_freebsd="clang --target=aarch64-unknown-freebsd --sysroot=$ZZZ_FREEBSD_SYSROOT"
```

The sysroot must contain `usr/lib`, `usr/include`, and `lib`. Additional
linker flags go in `ZZZ_FREEBSD_RUSTFLAGS`. If you installed a complete cross
toolchain instead of the target's standard library, select it with
`RUSTUP_TOOLCHAIN` (or `ZZZ_FREEBSD_RUSTC`).

Build both targets together:

```sh
cargo xtask build \
  --target x86_64-unknown-freebsd \
  --target aarch64-unknown-freebsd \
  --formats tar.gz
```

`cargo xtask build` checks the grammars, builds the remote-server matrix, and
packages the tarballs. Add `--channel preview` or `--channel nightly` to build
a different release channel. `cargo xtask doctor` reports whether the sysroot
and both linkers are configured.

FreeBSD targets build on the host even when `--container auto` is in effect:
the Linux toolchain image does not contain the FreeBSD sysroot.

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
