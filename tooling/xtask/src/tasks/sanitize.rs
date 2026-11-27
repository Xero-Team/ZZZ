//! The sanitizer gate.
//!
//! Miri interprets Rust and stops at every foreign call, so the bugs that live
//! on the other side of FFI — sqlite, FFmpeg, libgit2, Wayland — are invisible
//! to it. The LLVM sanitizers run the code natively instead: they instrument
//! the binary and catch the memory errors that cross into C, at native speed.
//!
//! `cargo xtask sanitize` runs a curated crate list for the same reason
//! `cargo xtask miri` does: a workspace-wide instrumented build takes hours, and
//! the crates that matter are the ones with `unsafe` reachable from their tests.
//!
//! Four details are easy to get wrong and are fixed here:
//!
//! - `--target` is passed even for a host build. Without it, `RUSTFLAGS` also
//!   applies to build scripts and proc-macros, which then need an instrumented
//!   runtime before the real crate is even compiled.
//! - `RUSTFLAGS` replaces the `[build] rustflags` in `.cargo/config.toml`
//!   instead of extending it, so the mangling and `tokio_unstable` settings are
//!   repeated here. Dropping `tokio_unstable` breaks any crate that uses an
//!   unstable tokio API.
//! - Tests run one at a time. AddressSanitizer's leak check runs at process
//!   exit, and parallel test threads make reports land under the wrong test.
//! - Each sanitizer gets its own target directory, because cargo does not key
//!   its cache on `RUSTFLAGS` and an rlib built with one sanitizer cannot link
//!   into a crate built with another.
//!
//! See `docs/src/development/testing.md`.

// The gate drives cargo and streams its output, like the other xtask tasks.
#![allow(
    clippy::disallowed_methods,
    reason = "a test gate drives cargo and streams its output to the terminal"
)]

use std::process::Command;

use anyhow::{Context as _, Result, bail};
use clap::{Parser, ValueEnum};

/// The nightly toolchain the sanitizers need. Their flags are `-Z` options, so
/// the release toolchain `rust-toolchain.toml` pins cannot run them.
pub const DEFAULT_TOOLCHAIN: &str = "nightly";

/// The triple the task builds for. It matches the one CI runs and keeps
/// `RUSTFLAGS` away from build scripts and proc macros.
pub const TARGET: &str = "x86_64-unknown-linux-gnu";

/// Mirrors the `[build] rustflags` in `.cargo/config.toml`, which `RUSTFLAGS`
/// replaces rather than extends.
const CARGO_CONFIG_RUSTFLAGS: &[&str] = &[
    "-C",
    "symbol-mangling-version=v0",
    "--cfg",
    "tokio_unstable",
];

/// A crate whose tests run under a sanitizer.
pub struct SanitizerTarget {
    /// The package name, as `cargo -p` takes it.
    pub package: &'static str,
    /// What the crate's `unsafe` or FFI does that a sanitizer is here to check.
    /// A crate with neither reachable from its tests does not belong on the list.
    pub reason: &'static str,
}

/// The curated list `cargo xtask sanitize` runs.
///
/// Entries pass under AddressSanitizer on `TARGET` today, and each one's tests
/// reach memory the sanitizer can see and Miri cannot. Order is reporting order.
pub const TARGETS: &[SanitizerTarget] = &[
    SanitizerTarget {
        package: "util",
        reason: "libc calls (`getpwuid`, `killpg`) and the `Path`/`OsStr` transmutes that wrap them",
    },
    SanitizerTarget {
        package: "sqlez",
        reason: "sqlite owns memory Rust hands it, so a use-after-free surfaces on the C side",
    },
    SanitizerTarget {
        package: "scheduler",
        reason: "the executor rebuilds `Waker`s from raw pointers and drops pinned futures by hand",
    },
    SanitizerTarget {
        package: "release_channel",
        reason: "`set_env` mutates the process environment from tests that run in parallel",
    },
];

#[derive(Clone, Copy, ValueEnum)]
pub enum Sanitizer {
    /// Heap, stack, and global buffer errors, use-after-free, and leaks.
    Address,
    /// Data races between threads that share non-atomic state.
    Thread,
}

impl Sanitizer {
    fn flag(self) -> &'static str {
        match self {
            Self::Address => "-Zsanitizer=address",
            Self::Thread => "-Zsanitizer=thread",
        }
    }
}

#[derive(Parser)]
pub struct SanitizeArgs {
    /// Which sanitizer to build with.
    #[arg(long, value_enum, default_value = "address")]
    sanitizer: Sanitizer,

    /// Test these packages instead of the curated list. Repeatable.
    #[arg(long, short)]
    package: Vec<String>,

    /// Use this toolchain instead of `nightly`.
    #[arg(long, default_value = DEFAULT_TOOLCHAIN)]
    toolchain: String,

    /// Instrument the standard library too (`-Zbuild-std`). AddressSanitizer
    /// works without it; ThreadSanitizer always needs it.
    #[arg(long)]
    build_std: bool,

    /// Print the curated list and exit.
    #[arg(long)]
    list: bool,
}

pub fn run_sanitize(args: SanitizeArgs) -> Result<()> {
    if args.list {
        for target in TARGETS {
            println!("{}\t{}", target.package, target.reason);
        }
        return Ok(());
    }

    verify_sanitizer_installed(&args.toolchain, args.sanitizer)?;

    let packages = plan_runs(&args);
    let mut failures = Vec::new();
    for package in packages {
        println!(
            "sanitize: testing {package} with the {} sanitizer",
            sanitizer_name(args.sanitizer)
        );

        let status = sanitizer_command(&args, &package)
            .status()
            .with_context(|| format!("failed to run the sanitizer for {package}"))?;

        if !status.success() {
            failures.push(package);
        }
    }

    if failures.is_empty() {
        return Ok(());
    }
    bail!(
        "the {} sanitizer rejected {} crate(s): {}",
        sanitizer_name(args.sanitizer),
        failures.len(),
        failures.join(", ")
    )
}

/// The packages this invocation runs, in order.
fn plan_runs(args: &SanitizeArgs) -> Vec<String> {
    if args.package.is_empty() {
        return TARGETS
            .iter()
            .map(|target| target.package.to_owned())
            .collect();
    }

    for package in &args.package {
        if !TARGETS.iter().any(|target| target.package == package) {
            println!("sanitize: {package} is not on the curated list; running it anyway");
        }
    }
    args.package.clone()
}

fn sanitizer_command(args: &SanitizeArgs, package: &str) -> Command {
    let mut command = Command::new("cargo");
    command.arg(format!("+{}", args.toolchain));
    if instruments_std(args) {
        command.arg("-Zbuild-std");
    }
    command
        .args([
            "test",
            "-p",
            package,
            "--target",
            TARGET,
            "--target-dir",
            &target_dir(args),
            "--lib",
        ])
        .args(["--", "--test-threads", "1"])
        .env("RUSTFLAGS", rustflags(args.sanitizer));
    command
}

/// Whether this run has to rebuild the standard library from source.
///
/// ThreadSanitizer refuses a prebuilt `core`: the flag changes the ABI, so a
/// crate built with it cannot link against a `core` built without it. The error
/// names `compiler_builtins` and suggests `-Cunsafe-allow-abi-mismatch`, which
/// would hide the mismatch instead of fixing it, so the task always instruments
/// std for ThreadSanitizer.
fn instruments_std(args: &SanitizeArgs) -> bool {
    args.build_std || matches!(args.sanitizer, Sanitizer::Thread)
}

/// Artifacts live in their own directory per sanitizer.
///
/// An rlib compiled with one sanitizer cannot be linked into a crate compiled
/// with another: rustc reports `mixing -Zsanitizer will cause an ABI mismatch`.
/// Cargo does not key its cache on `RUSTFLAGS`, so a shared target directory
/// makes switching sanitizers fail on whatever the previous run cached.
/// `--build-std` changes the std artifacts the same way.
fn target_dir(args: &SanitizeArgs) -> String {
    let mut directory = format!("target/sanitize/{}", sanitizer_name(args.sanitizer));
    if instruments_std(args) {
        directory.push_str("-build-std");
    }
    directory
}

/// `-Zsanitizer=…` followed by the settings `RUSTFLAGS` would otherwise erase.
fn rustflags(sanitizer: Sanitizer) -> String {
    std::iter::once(sanitizer.flag())
        .chain(CARGO_CONFIG_RUSTFLAGS.iter().copied())
        .collect::<Vec<_>>()
        .join(" ")
}

fn sanitizer_name(sanitizer: Sanitizer) -> &'static str {
    match sanitizer {
        Sanitizer::Address => "address",
        Sanitizer::Thread => "thread",
    }
}

/// Fails with the rustup command that fixes it, instead of letting the first
/// crate fail on an unknown `-Z` flag.
///
/// `rustc` accepts `-Zsanitizer` before it compiles anything, so `--version` is
/// enough to tell a missing toolchain from a release toolchain that rejects the
/// flag.
fn verify_sanitizer_installed(toolchain: &str, sanitizer: Sanitizer) -> Result<()> {
    let output = Command::new("rustc")
        .arg(format!("+{toolchain}"))
        .arg(sanitizer.flag())
        .arg("--version")
        .output()
        .context("failed to run rustc, so the sanitizer flags cannot be checked")?;
    if output.status.success() {
        return Ok(());
    }

    bail!(
        "`{toolchain}` cannot run `{}`; the sanitizers are nightly-only. Install the toolchain \
         with `rustup toolchain install {toolchain} --profile minimal`:\n{}",
        sanitizer.flag(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
}

#[cfg(test)]
mod tests {
    use super::{CARGO_CONFIG_RUSTFLAGS, SanitizeArgs, Sanitizer, rustflags};

    fn args() -> SanitizeArgs {
        SanitizeArgs {
            sanitizer: Sanitizer::Address,
            package: Vec::new(),
            toolchain: super::DEFAULT_TOOLCHAIN.to_owned(),
            build_std: false,
            list: false,
        }
    }

    #[test]
    fn rustflags_lead_with_the_sanitizer() {
        let flags = rustflags(Sanitizer::Address);
        assert!(
            flags.starts_with("-Zsanitizer=address "),
            "unexpected flags: {flags}"
        );
    }

    #[test]
    fn rustflags_keep_every_cargo_config_setting() {
        let flags = rustflags(Sanitizer::Thread);
        for setting in CARGO_CONFIG_RUSTFLAGS {
            assert!(
                flags.split_whitespace().any(|word| word == *setting),
                "`{setting}` is missing from {flags}"
            );
        }
    }

    #[test]
    fn an_empty_package_list_runs_the_curated_targets() {
        let runs = super::plan_runs(&args());
        let expected: Vec<String> = super::TARGETS
            .iter()
            .map(|target| target.package.to_owned())
            .collect();
        assert_eq!(runs, expected);
    }

    #[test]
    fn thread_sanitizer_always_instruments_std() {
        let mut args = args();
        args.sanitizer = Sanitizer::Thread;
        args.build_std = false;
        assert!(super::instruments_std(&args));
    }

    #[test]
    fn address_sanitizer_instruments_std_only_when_asked() {
        let mut args = args();
        args.sanitizer = Sanitizer::Address;
        assert!(!super::instruments_std(&args));
        args.build_std = true;
        assert!(super::instruments_std(&args));
    }

    #[test]
    fn a_package_outside_the_list_still_runs() {
        let mut args = args();
        args.package = vec!["not-curated".to_owned()];
        assert_eq!(super::plan_runs(&args), vec!["not-curated".to_owned()]);
    }
}
