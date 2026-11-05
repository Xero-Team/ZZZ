//! FreeBSD cross toolchain configuration.
//!
//! The two FreeBSD targets can be built natively (on a FreeBSD host) or
//! cross-compiled from Linux. The native path needs nothing beyond the pinned
//! Rust target; the cross path needs a sysroot and a linker/CC driver, supplied
//! through the environment so no machine-specific path ends up in the
//! repository.
//!
//! Variables (all optional except the sysroot on a cross build):
//!
//! - `ZZZ_FREEBSD_SYSROOT`: a FreeBSD sysroot with `usr/lib`, `usr/include`,
//!   and `lib` directories.
//! - `CARGO_TARGET_<TRIPLE>_LINKER` (Cargo's own form) or
//!   `ZZZ_FREEBSD_LINKER_<TRIPLE>`: the linker driver, e.g.
//!   `clang --target=x86_64-unknown-freebsd --sysroot=...`.
//! - `CC_<TRIPLE>` (cc-rs's form) or `ZZZ_FREEBSD_CC_<TRIPLE>`: C compiler.
//! - `ZZZ_FREEBSD_RUSTFLAGS`: extra flags appended to `RUSTFLAGS`.
//! - `RUSTUP_TOOLCHAIN` (Cargo/rustup's own form) or `ZZZ_FREEBSD_RUSTC`: a
//!   complete cross toolchain; an installed `rust-std` for the target needs no
//!   override at all.
//!
//! `<TRIPLE>` is the Rust target in upper case with dashes replaced by
//! underscores, e.g. `X86_64_UNKNOWN_FREEBSD`.

#![allow(
    clippy::disallowed_methods,
    reason = "toolchain detection reads the environment and probes rustup"
)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, bail};

use crate::tasks::build::model::{Platform, Target};
use crate::tasks::bundle::util;

pub const SYSROOT_VARIABLE: &str = "ZZZ_FREEBSD_SYSROOT";

/// Everything a FreeBSD build needs on top of `rustup target add`.
pub struct FreeBSDEnvironment {
    pub sysroot: Option<PathBuf>,
    pub linker: Option<OsString>,
    pub c_compiler: Option<OsString>,
    pub rustup_toolchain: Option<OsString>,
    pub extra_rustflags: String,
}

impl FreeBSDEnvironment {
    /// Resolves the environment for one target. `sysroot` overrides the
    /// environment variable, so a plan and a direct bundle call can share the
    /// same configuration.
    pub fn detect(target: Target, sysroot: Option<&Path>) -> Result<Self> {
        if target.platform() != Platform::FreeBSD {
            bail!("{} is not a FreeBSD target", target.triple());
        }
        let sysroot = match sysroot {
            Some(path) => Some(path.to_path_buf()),
            None => std::env::var_os(SYSROOT_VARIABLE).map(PathBuf::from),
        };
        if let Some(path) = &sysroot
            && !path.is_dir()
        {
            bail!(
                "{} points at {}, which is not a directory",
                SYSROOT_VARIABLE,
                path.display()
            );
        }
        Ok(Self {
            sysroot,
            linker: variable(target, "LINKER")
                .or_else(|| std::env::var_os(format!("CARGO_TARGET_{}_LINKER", key(target)))),
            c_compiler: variable(target, "CC")
                .or_else(|| std::env::var_os(format!("CC_{}", key(target)))),
            rustup_toolchain: std::env::var_os("RUSTUP_TOOLCHAIN")
                .or_else(|| std::env::var_os("ZZZ_FREEBSD_RUSTC")),
            extra_rustflags: std::env::var("ZZZ_FREEBSD_RUSTFLAGS").unwrap_or_default(),
        })
    }

    pub fn is_cross(&self) -> bool {
        Platform::host() != Platform::FreeBSD
    }

    /// Fails early with the variable names to set when the cross toolchain is
    /// incomplete. Native FreeBSD builds need neither a sysroot nor a linker
    /// override.
    pub fn require_cross(&self, target: Target) -> Result<()> {
        if !self.is_cross() {
            return Ok(());
        }
        let Some(sysroot) = &self.sysroot else {
            bail!(
                "cross-building {} needs {SYSROOT_VARIABLE} pointing at a FreeBSD sysroot; \
                 see docs/src/development/freebsd.md",
                target.triple()
            );
        };
        for directory in ["usr/lib", "usr/include", "lib"] {
            if !sysroot.join(directory).is_dir() {
                bail!(
                    "{} does not look like a FreeBSD sysroot: {} is missing",
                    sysroot.display(),
                    sysroot.join(directory).display()
                );
            }
        }
        if self.linker.is_none() {
            bail!(
                "cross-building {} needs CARGO_TARGET_{}_LINKER (or {}) set to a \
                 clang/lld driver with --target={}",
                target.triple(),
                key(target),
                variable_name(target, "LINKER"),
                target.triple()
            );
        }
        Ok(())
    }

    /// Applies the resolved variables to a `cargo build` invocation.
    ///
    /// `rustflags` are the flags the bundler already needs (rpath, PGO, ...);
    /// the sysroot and the caller's extra flags are appended here so Cargo sees
    /// one `RUSTFLAGS` value instead of a target-specific override that would
    /// replace the bundler's flags entirely.
    pub fn apply(&self, target: Target, command: &mut Command, rustflags: &str) -> Result<()> {
        if let Some(toolchain) = &self.rustup_toolchain {
            command.env("RUSTUP_TOOLCHAIN", toolchain);
        }
        if !self.is_cross() {
            if !rustflags.trim().is_empty() || !self.extra_rustflags.is_empty() {
                command
                    .env("RUSTFLAGS", compose(rustflags, &self.extra_rustflags))
                    .env_remove("CARGO_ENCODED_RUSTFLAGS");
            }
            return Ok(());
        }
        self.require_cross(target)?;
        let sysroot = self.sysroot.as_ref().expect("checked in require_cross");
        let linker = self.linker.as_ref().expect("checked in require_cross");
        let sysroot_flag = format!("--sysroot={}", sysroot.display());

        command.env(format!("CARGO_TARGET_{}_LINKER", key(target)), linker);
        if let Some(cc) = &self.c_compiler {
            command.env(format!("CC_{}", key(target)), cc);
        }
        command
            .env("BINDGEN_EXTRA_CLANG_ARGS", &sysroot_flag)
            .env("CFLAGS", &sysroot_flag)
            .env("CXXFLAGS", &sysroot_flag)
            .env(
                "RUSTFLAGS",
                compose(
                    rustflags,
                    &format!("{sysroot_flag} {}", self.extra_rustflags),
                ),
            )
            .env_remove("CARGO_ENCODED_RUSTFLAGS");
        Ok(())
    }
}

fn compose(base: &str, extra: &str) -> String {
    [base.trim(), extra.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn variable(target: Target, suffix: &str) -> Option<OsString> {
    std::env::var_os(variable_name(target, suffix)).filter(|value| !value.is_empty())
}

fn variable_name(target: Target, suffix: &str) -> String {
    format!("ZZZ_FREEBSD_{suffix}_{}", key(target))
}

/// `CARGO_TARGET_<TRIPLE>` variables use the triple in upper case with dashes
/// replaced by underscores.
fn key(target: Target) -> String {
    target.triple().to_ascii_uppercase().replace('-', "_")
}

/// Adds the target's rust-std unless a complete toolchain was supplied or the
/// standard is already installed. The FreeBSD targets are not listed in
/// `rust-toolchain.toml`, so a cross build either runs `rustup target add` or
/// points `RUSTUP_TOOLCHAIN` at a toolchain that ships them.
pub fn ensure_rust_std(target: Target, environment: &FreeBSDEnvironment) -> Result<()> {
    if environment.rustup_toolchain.is_some() {
        return Ok(());
    }
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .is_ok_and(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .any(|line| line.trim() == target.triple())
        });
    if installed {
        return Ok(());
    }
    util::run(
        Command::new("rustup").args(["target", "add", target.triple()]),
        "rustup target add",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_target_keys_match_cargo_convention() {
        assert_eq!(key(Target::FreeBSDX86_64), "X86_64_UNKNOWN_FREEBSD");
        assert_eq!(
            variable_name(Target::FreeBSDAarch64, "LINKER"),
            "ZZZ_FREEBSD_LINKER_AARCH64_UNKNOWN_FREEBSD"
        );
    }

    #[test]
    fn rustflags_are_composed_without_duplicate_spaces() {
        assert_eq!(compose("", ""), "");
        assert_eq!(compose("-C lto", ""), "-C lto");
        assert_eq!(compose("-C lto", "-C opt=2"), "-C lto -C opt=2");
        assert_eq!(compose(" ", " -C opt=2 "), "-C opt=2");
    }

    #[test]
    fn native_freebsd_builds_do_not_require_a_sysroot() {
        let environment = FreeBSDEnvironment {
            sysroot: None,
            linker: None,
            c_compiler: None,
            rustup_toolchain: None,
            extra_rustflags: String::new(),
        };
        if Platform::host() == Platform::FreeBSD {
            assert!(environment.require_cross(Target::FreeBSDX86_64).is_ok());
        }
    }
}
