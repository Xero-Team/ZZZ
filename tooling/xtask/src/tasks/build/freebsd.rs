//! FreeBSD cross toolchain configuration.
//!
//! The two FreeBSD targets can be built natively (on a FreeBSD host) or
//! cross-compiled from Linux. The native path needs nothing beyond the pinned
//! Rust target; the cross path needs a sysroot and a linker/CC driver.
//!
//! There are two ways to supply the cross toolchain:
//!
//! 1. The toolchain image (`Dockerfile-toolchain`) ships both FreeBSD 14.5
//!    sysroots, clang/lld wrappers, and the two `rust-std` targets. When the
//!    planner runs a FreeBSD bundle inside that image it sets
//!    `ZZZ_FREEBSD_IN_CONTAINER=1`; this module then resolves the bundled paths
//!    instead of requiring host-side variables.
//! 2. On a host without the image, supply everything through the environment:
//!
//! - `ZZZ_FREEBSD_SYSROOT_<TRIPLE>` (or the generic `ZZZ_FREEBSD_SYSROOT` for
//!   a single-target build): a FreeBSD sysroot with `usr/lib`, `usr/include`,
//!   and `lib`. Two architectures need two separate values.
//! - `CARGO_TARGET_<TRIPLE>_LINKER` (Cargo's own form) or
//!   `ZZZ_FREEBSD_LINKER_<TRIPLE>`: an executable linker driver. Cargo does not
//!   split this value on spaces, so a driver with arguments must be a wrapper
//!   script (the image installs `/usr/local/bin/zzz-freebsd-*-clang`).
//! - `CC_<triple>` (cc-rs's form, lower case with underscores) or
//!   `ZZZ_FREEBSD_CC_<TRIPLE>`: the C compiler wrapper.
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

use crate::tasks::build::model::{Arch, Platform, Target};
use crate::tasks::bundle::util;

pub const SYSROOT_VARIABLE: &str = "ZZZ_FREEBSD_SYSROOT";
/// Set by `container.rs::wrap` for commands that run inside the toolchain image.
/// It tells the planner and the FreeBSD checks that the image provides the
/// sysroots and wrappers, so a host-side `require_cross` must not reject a plan
/// just because the host does not have them.
pub const IN_CONTAINER_VARIABLE: &str = "ZZZ_FREEBSD_IN_CONTAINER";

/// Where the image unpacks each architecture's `base.txz`.
pub const IMAGE_SYSROOT_X86_64: &str = "/opt/freebsd/sysroot/amd64";
pub const IMAGE_SYSROOT_AARCH64: &str = "/opt/freebsd/sysroot/arm64";
/// Linker/CC wrappers installed by the image. Each hard-codes one target and
/// its sysroot, because Cargo invokes the linker without telling it which
/// target it is linking for.
pub const IMAGE_WRAPPER_X86_64: &str = "/usr/local/bin/zzz-freebsd-x86_64-clang";
pub const IMAGE_WRAPPER_AARCH64: &str = "/usr/local/bin/zzz-freebsd-aarch64-clang";

/// The image's clang/lld wrapper for a FreeBSD target.
pub fn image_wrapper(target: Target) -> Option<&'static str> {
    if target.platform() != Platform::FreeBSD {
        return None;
    }
    Some(match target.arch() {
        Arch::X86_64 => IMAGE_WRAPPER_X86_64,
        Arch::Aarch64 => IMAGE_WRAPPER_AARCH64,
    })
}

/// Everything a FreeBSD build needs on top of `rustup target add`.
#[derive(Clone)]
pub struct FreeBSDEnvironment {
    pub sysroot: Option<PathBuf>,
    pub linker: Option<OsString>,
    pub c_compiler: Option<OsString>,
    pub rustup_toolchain: Option<OsString>,
    pub extra_rustflags: String,
    /// The whole cross toolchain comes from the image; host-side checks are
    /// satisfied without probing paths that only exist inside the container.
    pub in_container: bool,
}

/// Whether this process is running inside the toolchain image.
pub fn in_container() -> bool {
    std::env::var_os(IN_CONTAINER_VARIABLE).is_some()
}

/// The image's sysroot for a FreeBSD target, if the target has one.
pub fn image_sysroot(target: Target) -> Option<&'static str> {
    if target.platform() != Platform::FreeBSD {
        return None;
    }
    Some(match target.arch() {
        Arch::X86_64 => IMAGE_SYSROOT_X86_64,
        Arch::Aarch64 => IMAGE_SYSROOT_AARCH64,
    })
}

impl FreeBSDEnvironment {
    /// Resolves the environment for one target. `sysroot` overrides the
    /// per-target variable, so a plan and a direct bundle call can share the
    /// same configuration. Inside the container the bundled paths win.
    pub fn detect(target: Target, sysroot: Option<&Path>) -> Result<Self> {
        if target.platform() != Platform::FreeBSD {
            bail!("{} is not a FreeBSD target", target.triple());
        }
        let in_container = in_container();
        let sysroot = if in_container {
            Some(PathBuf::from(
                image_sysroot(target).expect("FreeBSD target"),
            ))
        } else {
            match sysroot {
                Some(path) => Some(path.to_path_buf()),
                None => variable(target, "SYSROOT")
                    .or_else(|| std::env::var_os(SYSROOT_VARIABLE))
                    .map(PathBuf::from),
            }
        };
        if let Some(path) = &sysroot
            && !in_container
            && !path.is_dir()
        {
            bail!(
                "{} points at {}, which is not a directory",
                sysroot_variable(target),
                path.display()
            );
        }
        let linker = if in_container {
            Some(OsString::from(
                image_wrapper(target).expect("FreeBSD target"),
            ))
        } else {
            variable(target, "LINKER")
                .or_else(|| std::env::var_os(format!("CARGO_TARGET_{}_LINKER", key(target))))
        };
        let c_compiler = if in_container {
            Some(OsString::from(
                image_wrapper(target).expect("FreeBSD target"),
            ))
        } else {
            variable(target, "CC").or_else(|| std::env::var_os(format!("CC_{}", cc_key(target))))
        };
        Ok(Self {
            sysroot,
            linker,
            c_compiler,
            rustup_toolchain: std::env::var_os("RUSTUP_TOOLCHAIN")
                .or_else(|| std::env::var_os("ZZZ_FREEBSD_RUSTC")),
            extra_rustflags: std::env::var("ZZZ_FREEBSD_RUSTFLAGS").unwrap_or_default(),
            in_container,
        })
    }

    pub fn is_cross(&self) -> bool {
        Platform::host() != Platform::FreeBSD
    }

    /// Fails early with the variable names to set when the cross toolchain is
    /// incomplete. Native FreeBSD builds and containerized builds need neither a
    /// host sysroot nor host linker overrides.
    pub fn require_cross(&self, target: Target) -> Result<()> {
        if !self.is_cross() || self.in_container {
            return Ok(());
        }
        let Some(sysroot) = &self.sysroot else {
            bail!(
                "cross-building {} needs {} (or {}) pointing at a FreeBSD sysroot; \
                 see docs/src/development/freebsd.md",
                target.triple(),
                sysroot_variable(target),
                SYSROOT_VARIABLE
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
                "cross-building {} needs CARGO_TARGET_{}_LINKER (or {}) set to an \
                 executable clang/lld driver with --target={}",
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
    /// `rustflags` are the flags the bundler already needs (rpath, PGO, ...).
    /// The target and sysroot are baked into the linker wrapper, so the sysroot
    /// is not appended to `RUSTFLAGS` as a bare `--sysroot` (that would override
    /// rustc's own sysroot and break the build). `CFLAGS`/`CXXFLAGS` and the
    /// bindgen argument carry the sysroot for C code instead.
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
        let sysroot_value: OsString = sysroot.as_os_str().to_os_string();

        command
            .env(format!("CARGO_TARGET_{}_LINKER", key(target)), linker)
            .env(sysroot_variable(target), &sysroot_value)
            .env("ZZZ_FREEBSD_SYSROOT", &sysroot_value);
        if let Some(cc) = &self.c_compiler {
            command.env(format!("CC_{}", cc_key(target)), cc);
        }
        // bindgen and the C builds need the sysroot headers. `BINDGEN_*` is a
        // clang argument; `CFLAGS`/`CXXFLAGS` reach cc-rs. `ZZZ_FREEBSD_SYSROOT`
        // is also exported for build scripts that look for it; the linker
        // wrapper itself hard-codes the path and needs none of these.
        let sysroot_flag = format!("--sysroot={}", sysroot.display());
        command
            .env("BINDGEN_EXTRA_CLANG_ARGS", &sysroot_flag)
            .env("CFLAGS", &sysroot_flag)
            .env("CXXFLAGS", &sysroot_flag);
        if !rustflags.trim().is_empty() || !self.extra_rustflags.is_empty() {
            command
                .env("RUSTFLAGS", compose(rustflags, &self.extra_rustflags))
                .env_remove("CARGO_ENCODED_RUSTFLAGS");
        }
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

fn sysroot_variable(target: Target) -> String {
    variable_name(target, "SYSROOT")
}

/// `CARGO_TARGET_<TRIPLE>` variables use the triple in upper case with dashes
/// replaced by underscores.
fn key(target: Target) -> String {
    target.triple().to_ascii_uppercase().replace('-', "_")
}

/// cc-rs looks up `CC_<triple>` with the triple's dashes replaced by
/// underscores but in its original (lower) case.
fn cc_key(target: Target) -> String {
    target.triple().replace('-', "_")
}

/// Adds the target's rust-std unless a complete toolchain was supplied or the
/// standard is already installed. Inside the image the targets are baked in, so
/// no `rustup target add` (and no network) is attempted.
pub fn ensure_rust_std(target: Target, environment: &FreeBSDEnvironment) -> Result<()> {
    if environment.in_container || environment.rustup_toolchain.is_some() {
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
        assert_eq!(
            sysroot_variable(Target::FreeBSDX86_64),
            "ZZZ_FREEBSD_SYSROOT_X86_64_UNKNOWN_FREEBSD"
        );
    }

    #[test]
    fn cc_keys_keep_the_lower_case_triple() {
        assert_eq!(cc_key(Target::FreeBSDX86_64), "x86_64_unknown_freebsd");
        assert_eq!(cc_key(Target::FreeBSDAarch64), "aarch64_unknown_freebsd");
    }

    #[test]
    fn image_sysroots_are_distinct_per_architecture() {
        assert_eq!(
            image_sysroot(Target::FreeBSDX86_64),
            Some(IMAGE_SYSROOT_X86_64)
        );
        assert_eq!(
            image_sysroot(Target::FreeBSDAarch64),
            Some(IMAGE_SYSROOT_AARCH64)
        );
        assert_ne!(IMAGE_SYSROOT_X86_64, IMAGE_SYSROOT_AARCH64);
    }

    #[test]
    fn rustflags_are_composed_without_duplicate_spaces() {
        assert_eq!(compose("", ""), "");
        assert_eq!(compose("-C lto", ""), "-C lto");
        assert_eq!(compose("-C lto", "-C opt=2"), "-C lto -C opt=2");
        assert_eq!(compose(" ", " -C opt=2 "), "-C opt=2");
    }

    #[test]
    fn a_containerized_cross_build_does_not_require_host_paths() {
        let environment = FreeBSDEnvironment {
            sysroot: Some(PathBuf::from(IMAGE_SYSROOT_X86_64)),
            linker: None,
            c_compiler: None,
            rustup_toolchain: None,
            extra_rustflags: String::new(),
            in_container: true,
        };
        assert!(environment.require_cross(Target::FreeBSDX86_64).is_ok());
        assert!(ensure_rust_std(Target::FreeBSDX86_64, &environment).is_ok());
    }

    #[test]
    fn native_freebsd_builds_do_not_require_a_sysroot() {
        let environment = FreeBSDEnvironment {
            sysroot: None,
            linker: None,
            c_compiler: None,
            rustup_toolchain: None,
            extra_rustflags: String::new(),
            in_container: false,
        };
        if Platform::host() == Platform::FreeBSD {
            assert!(environment.require_cross(Target::FreeBSDX86_64).is_ok());
        }
    }
}
