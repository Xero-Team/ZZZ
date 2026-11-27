//! The Miri gate.
//!
//! Miri interprets every executed instruction, so a workspace-wide run is not
//! practical: one crate costs minutes. `cargo xtask miri` therefore runs a
//! curated list, and each entry records the Miri settings its tests need
//! instead of flattening every crate onto one permissive default.
//!
//! `.cargo/config.toml` keeps the permissive developer default for a bare
//! `cargo +nightly miri test`. The flags below are passed as an explicit
//! `MIRIFLAGS`, which wins over that config entry because it is not `force`d.
//! `cargo xtask check miri` fails when that stops being true, and when a listed
//! crate leaves the workspace.
//!
//! See `docs/src/development/testing.md` for when to add a crate to the list.

// The gate drives cargo and streams its output, like the other xtask tasks.
#![allow(
    clippy::disallowed_methods,
    reason = "a test gate drives cargo and streams its output to the terminal"
)]

use std::process::Command;

use anyhow::{Context as _, Result, bail};
use clap::Parser;

/// The nightly toolchain Miri runs on. `rust-toolchain.toml` pins the release
/// toolchain and Miri only ships on nightly, so the override is explicit.
pub const DEFAULT_TOOLCHAIN: &str = "nightly";

/// Flags `.cargo/config.toml` has to keep in its `MIRIFLAGS` default: the
/// executor threads the test harness leaves alive look like a leak, and the
/// suite reads the filesystem. The task overrides this per target.
pub const CONFIG_BASELINE_FLAGS: &[&str] = &["-Zmiri-ignore-leaks", "-Zmiri-disable-isolation"];

/// A crate whose tests run under Miri, plus the settings those tests need.
pub struct MiriTarget {
    /// The package name, as `cargo -p` takes it.
    pub package: &'static str,
    /// What the crate's `unsafe` code does that Miri is here to check. A crate
    /// with no `unsafe` reachable from its tests does not belong on the list.
    pub reason: &'static str,
    /// The tests leave a process-global executor alive until the process exits,
    /// which Miri reports as a leak. Only crates verified to keep such state
    /// alive set this; the flag switches leak detection off for the whole run.
    pub ignore_leaks: bool,
    /// The tests read the clock, the environment, or the filesystem, so the
    /// sandbox has to be off. Prefer making the test hermetic over setting this.
    pub disable_isolation: bool,
}

/// The curated list `cargo xtask miri` runs.
///
/// Entries are crates whose tests exercise real `unsafe` and that pass under
/// Miri today. Order is reporting order; keep a crate near its dependencies so
/// a failure reads top-down.
pub const TARGETS: &[MiriTarget] = &[
    MiriTarget {
        package: "scheduler",
        reason: "the executor polls pinned futures through `ManuallyDrop`, \
                 `get_unchecked_mut`, and `Waker`s rebuilt from raw pointers",
        ignore_leaks: false,
        disable_isolation: false,
    },
    MiriTarget {
        package: "release_channel",
        reason: "`set_env` mutates the process environment, which edition 2024 \
                 makes unsafe for exactly the concurrent-access race Miri models",
        ignore_leaks: false,
        disable_isolation: false,
    },
];

#[derive(Parser)]
pub struct MiriArgs {
    /// Test these packages instead of the curated list. Repeatable. A package
    /// outside the list runs with neither `-Zmiri-ignore-leaks` nor
    /// `-Zmiri-disable-isolation`, because its settings are unverified.
    #[arg(long, short)]
    package: Vec<String>,

    /// Extra `MIRIFLAGS` entries, for probing a model locally: for example
    /// `--flag=-Zmiri-tree-borrows` or `--flag=-Zmiri-strict-provenance`.
    #[arg(long = "flag", allow_hyphen_values = true)]
    flags: Vec<String>,

    /// Use this toolchain instead of `nightly`.
    #[arg(long, default_value = DEFAULT_TOOLCHAIN)]
    toolchain: String,

    /// Print the curated list and exit.
    #[arg(long)]
    list: bool,
}

pub fn run_miri(args: MiriArgs) -> Result<()> {
    if args.list {
        for target in TARGETS {
            println!(
                "{}\t{}\t{}",
                target.package,
                flags_for(target).join(" "),
                target.reason
            );
        }
        return Ok(());
    }

    verify_miri_installed(&args.toolchain)?;

    let runs = plan_runs(&args);
    let mut failures = Vec::new();
    for (package, mut flags) in runs {
        flags.extend(args.flags.iter().cloned());

        println!(
            "miri: testing {package}{}",
            if flags.is_empty() {
                String::new()
            } else {
                format!(" [MIRIFLAGS={}]", flags.join(" "))
            }
        );

        let status = cargo_miri(&args.toolchain, &["test", "-p", &package, "--lib"])
            .env("MIRIFLAGS", flags.join(" "))
            .status()
            .with_context(|| format!("failed to run Miri for {package}"))?;

        if !status.success() {
            failures.push(package);
        }
    }

    if failures.is_empty() {
        return Ok(());
    }
    bail!(
        "Miri rejected {} crate(s): {}",
        failures.len(),
        failures.join(", ")
    )
}

/// The `(package, flags)` pairs this invocation runs, in order.
fn plan_runs(args: &MiriArgs) -> Vec<(String, Vec<String>)> {
    if args.package.is_empty() {
        return TARGETS
            .iter()
            .map(|target| (target.package.to_owned(), flags_for(target)))
            .collect();
    }

    args.package
        .iter()
        .map(|package| {
            if let Some(target) = target_for(package) {
                (package.clone(), flags_for(target))
            } else {
                println!(
                    "miri: {package} is not on the curated list; running it with Miri's defaults"
                );
                (package.clone(), Vec::new())
            }
        })
        .collect()
}

fn target_for(package: &str) -> Option<&'static MiriTarget> {
    TARGETS.iter().find(|target| target.package == package)
}

/// The Miri flags a target runs with, before any `--flag` additions.
fn flags_for(target: &MiriTarget) -> Vec<String> {
    let mut flags = Vec::new();
    if target.ignore_leaks {
        flags.push("-Zmiri-ignore-leaks".to_owned());
    }
    if target.disable_isolation {
        flags.push("-Zmiri-disable-isolation".to_owned());
    }
    flags
}

fn cargo_miri(toolchain: &str, arguments: &[&str]) -> Command {
    let mut command = Command::new("cargo");
    command
        .arg(format!("+{toolchain}"))
        .arg("miri")
        .args(arguments);
    command
}

/// Fails with the rustup commands that fix it, instead of letting the first
/// crate fail on a missing `cargo-miri`.
fn verify_miri_installed(toolchain: &str) -> Result<()> {
    let output = cargo_miri(toolchain, &["--version"])
        .output()
        .context("failed to run `cargo miri`, so Miri cannot be checked")?;
    if output.status.success() {
        return Ok(());
    }

    bail!(
        "Miri is not available on the `{toolchain}` toolchain; install it with \
         `rustup toolchain install {toolchain} --component miri`, or \
         `rustup component add miri --toolchain {toolchain}` if the toolchain \
         is already installed:\n{}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
}

#[cfg(test)]
mod tests {
    use super::{MiriArgs, MiriTarget, TARGETS, flags_for, plan_runs};

    fn args(packages: &[&str], flags: &[&str]) -> MiriArgs {
        MiriArgs {
            package: packages.iter().map(|package| package.to_string()).collect(),
            flags: flags.iter().map(|flag| flag.to_string()).collect(),
            toolchain: super::DEFAULT_TOOLCHAIN.to_owned(),
            list: false,
        }
    }

    #[test]
    fn an_empty_list_runs_every_curated_target_in_order() {
        let runs = plan_runs(&args(&[], &[]));
        let expected: Vec<&str> = TARGETS.iter().map(|target| target.package).collect();
        assert_eq!(
            runs.iter()
                .map(|(package, _)| package.as_str())
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn leak_detection_is_only_off_where_the_target_says_so() {
        for target in TARGETS {
            let flags = flags_for(target);
            assert_eq!(
                flags.iter().any(|flag| flag == "-Zmiri-ignore-leaks"),
                target.ignore_leaks,
                "{} disagrees with its flags: {flags:?}",
                target.package
            );
        }
    }

    #[test]
    fn a_package_outside_the_list_gets_no_flags() {
        let runs = plan_runs(&args(&["definitely-not-curated"], &[]));
        assert_eq!(runs.len(), 1);
        assert!(runs[0].1.is_empty());
    }

    #[test]
    fn a_curated_package_keeps_its_own_flags_when_named_explicitly() {
        let target = TARGETS
            .iter()
            .find(|target| target.ignore_leaks || target.disable_isolation);
        let Some(target) = target else {
            return;
        };
        let runs = plan_runs(&args(&[target.package], &[]));
        assert_eq!(runs[0].1, flags_for(target));
    }

    #[test]
    fn both_flags_appear_when_a_target_needs_both() {
        let target = MiriTarget {
            package: "fixture",
            reason: "test fixture",
            ignore_leaks: true,
            disable_isolation: true,
        };
        assert_eq!(
            flags_for(&target),
            vec![
                "-Zmiri-ignore-leaks".to_owned(),
                "-Zmiri-disable-isolation".to_owned()
            ]
        );
    }
}
