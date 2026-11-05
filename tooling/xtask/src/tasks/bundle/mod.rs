//! `cargo xtask bundle`: the single packaging implementation for every target.
//!
//! Each platform used to have its own shell entry point with its own flags
//! (`--deb --rpm`, `-d`, PowerShell's `-Architecture`). The command below
//! replaces all of them with one option grammar:
//!
//! ```text
//! cargo xtask bundle --target <triple> --profile <profile> --formats <list>
//! ```
//!
//! The platform is implied by the target. A macOS target builds an `.app` and a
//! `.dmg` on either host; a Windows target builds the portable binaries with
//! msvc-wine on Linux, and the MSI/Inno installers on Windows itself. Everything
//! that can be shared — version detection, remote-server embedding, licenses,
//! FFmpeg, sentry upload — lives in the shared modules instead of being copied
//! per platform.

#![allow(
    clippy::disallowed_methods,
    reason = "bundling is a CLI workflow; there is no UI thread to block"
)]

pub mod appimage;
pub mod ffmpeg;
pub mod flatpak;
pub mod freebsd;
pub mod github_release;
pub mod licenses;
pub mod linux;
pub mod macos;
pub mod macos_sdk;
pub mod remote_server;
pub mod runtime_deps;
pub mod snap;
pub mod util;
pub mod windows;

use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Args;

use crate::tasks::build::model::{Channel, Format, Platform, Profile, Target};

#[derive(Args)]
pub struct BundleArgs {
    /// Target triple (repeatable). Defaults to the host target.
    #[arg(long = "target", value_parser = parse_target)]
    targets: Vec<Target>,
    /// Cargo profile to build, or to package from with `--skip-build`.
    #[arg(long, value_enum, default_value = "release")]
    profile: Profile,
    /// Release channel baked into the binary and the bundle name. Defaults to
    /// the shipping channel, so `cargo xtask bundle` produces a release build;
    /// `cargo run` and `cargo build` stay on `dev`.
    #[arg(long, value_enum, default_value = "stable")]
    channel: Option<Channel>,
    /// Comma-separated artifact formats (repeatable). Defaults to the target's
    /// shipped default.
    #[arg(long = "formats", value_enum, value_delimiter = ',')]
    formats: Vec<Format>,
    /// Cargo target directory (defaults to `CARGO_TARGET_DIR`, then `target`).
    #[arg(long)]
    target_dir: Option<PathBuf>,
    /// Reuse existing binaries and archives instead of compiling.
    #[arg(long)]
    skip_build: bool,
    /// Parallel Cargo jobs for the host build.
    #[arg(long)]
    jobs: Option<u32>,
    /// Do not download anything; fail when a pinned input is missing.
    #[arg(long)]
    offline: bool,
    /// macOS: copy the bundle into /Applications.
    #[arg(long)]
    install: bool,
    /// macOS: open the bundle or DMG when it is ready.
    #[arg(long)]
    open: bool,
    /// Linux: prove the editor starts when host copies of bundled libraries are unusable.
    #[arg(long)]
    robustness: bool,
    /// macOS: skip code signing and notarization even when credentials exist.
    #[arg(long)]
    no_sign: bool,
    /// Version recorded in the Snap manifest (defaults to crates/zzz's version).
    #[arg(long)]
    release_version: Option<String>,
}

/// Everything a platform bundler needs. Constructed once per target so the
/// modules never read the process environment behind the caller's back.
pub struct Context {
    pub workspace: PathBuf,
    pub target: Target,
    pub profile: Profile,
    pub channel: Channel,
    pub formats: BTreeSet<Format>,
    pub target_dir: PathBuf,
    pub skip_build: bool,
    pub jobs: u32,
    pub offline: bool,
    pub install: bool,
    pub open: bool,
    pub robustness: bool,
    pub sign: bool,
    pub release_version: String,
}

pub fn run_bundle(args: BundleArgs) -> Result<()> {
    let workspace = std::env::current_dir()?;
    let channel = args
        .channel
        .unwrap_or_else(|| Channel::detect_shipping(&workspace));
    let targets = if args.targets.is_empty() {
        vec![Target::host_default()]
    } else {
        args.targets.clone()
    };
    let target_dir_input = args
        .target_dir
        .clone()
        .or_else(|| std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from));
    let target_dir = match target_dir_input {
        Some(path) if path.is_absolute() => path,
        Some(path) => workspace.join(path),
        None => workspace.join("target"),
    };
    let release_version = match args.release_version.clone() {
        Some(version) => version,
        None => util::crate_version(&workspace)?,
    };
    let jobs = args.jobs.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map_or(1, |count| u32::try_from(count.get()).unwrap_or(1))
    });
    if jobs == 0 {
        bail!("--jobs must be at least 1");
    }

    for target in targets {
        let formats = if args.formats.is_empty() {
            target.default_formats()
        } else {
            args.formats.clone()
        };
        validate(target, &formats)?;
        let context = Context {
            workspace: workspace.clone(),
            target,
            profile: args.profile,
            channel,
            formats: formats.into_iter().collect(),
            target_dir: target_dir.clone(),
            skip_build: args.skip_build,
            jobs,
            offline: args.offline,
            install: args.install,
            open: args.open,
            robustness: args.robustness,
            sign: !args.no_sign,
            release_version: release_version.clone(),
        };
        bundle(&context)?;
    }
    Ok(())
}

fn bundle(context: &Context) -> Result<()> {
    eprintln!(
        "bundling {} (profile {}, channel {}, formats: {})",
        context.target.triple(),
        context.profile.key(),
        context.channel.as_str(),
        context
            .formats
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    match context.target.platform() {
        Platform::Linux => linux::bundle(context),
        Platform::Mac => macos::bundle(context),
        Platform::Windows => windows::bundle(context),
        Platform::FreeBSD => freebsd::bundle(context),
    }
}

fn validate(target: Target, formats: &[Format]) -> Result<()> {
    for format in formats {
        if !target.supports(*format) {
            bail!("{} does not support the {format} format", target.triple());
        }
    }

    if target.platform() == Platform::Windows
        && formats
            .iter()
            .any(|format| matches!(format, Format::Msi | Format::Inno))
        && Platform::host() != Platform::Windows
    {
        bail!(
            "the MSI and Inno installers need a Windows host; from Linux build the \
             portable binaries with --formats exe"
        );
    }

    if target.platform() == Platform::Mac && Platform::host() != Platform::Mac && target.is_cross()
    {
        // The cross path can assemble an unsigned bundle; signing and
        // notarization are Apple-only.
        for format in formats {
            if !matches!(format, Format::App | Format::Dmg) {
                bail!("the macOS cross path only produces app and dmg bundles");
            }
        }
    }
    Ok(())
}

impl Context {
    pub fn arch(&self) -> &'static str {
        self.target.arch().as_str()
    }

    /// Directory holding the profile's raw Cargo output. The Linux bundle keeps
    /// host binaries under the triple; macOS and Windows also nest them there.
    pub fn artifact_dir(&self) -> PathBuf {
        self.target_dir
            .join(self.target.triple())
            .join(self.profile.artifact_directory())
    }

    /// Directory the archive and packages are written to.
    pub fn bundle_dir(&self) -> PathBuf {
        match self.target.platform() {
            Platform::Linux | Platform::FreeBSD => {
                self.target_dir.join(self.profile.artifact_directory())
            }
            Platform::Mac | Platform::Windows => self.artifact_dir(),
        }
    }

    pub fn profile_flag(&self) -> Vec<String> {
        match self.profile {
            Profile::Release => vec!["--release".to_owned()],
            other => vec!["--profile".to_owned(), other.key().to_owned()],
        }
    }
}

fn parse_target(value: &str) -> Result<Target, String> {
    value.parse::<Target>().map_err(|error| error.to_string())
}

/// Formats that the Linux container image can produce. Everything else needs a
/// host tool (`flatpak-builder`, `snapcraft`), so the planner runs it after the
/// containerized bundle.
pub fn container_formats(formats: &BTreeSet<Format>) -> Vec<Format> {
    formats
        .iter()
        .copied()
        .filter(|format| matches!(format, Format::TarGz | Format::Deb | Format::Rpm))
        .collect()
}

pub fn host_only_formats(formats: &BTreeSet<Format>) -> Vec<Format> {
    formats
        .iter()
        .copied()
        .filter(|format| matches!(format, Format::AppImage | Format::Flatpak | Format::Snap))
        .collect()
}

/// The desktop identity of a channel. Shared by the Linux desktop entry, the
/// AppImage AppDir, and the Flatpak manifest.
pub struct AppIdentity {
    pub name: &'static str,
    pub id: String,
    pub suffix: &'static str,
    pub icon: &'static str,
}

pub fn app_identity(channel: Channel) -> AppIdentity {
    match channel {
        Channel::Stable => AppIdentity {
            name: "ZZZ",
            id: "dev.zzz.ZZZ".to_owned(),
            suffix: "",
            icon: "app-icon",
        },
        Channel::Preview => AppIdentity {
            name: "ZZZ Preview",
            id: "dev.zzz.ZZZ-Preview".to_owned(),
            suffix: "-preview",
            icon: "app-icon-preview",
        },
        Channel::Nightly => AppIdentity {
            name: "ZZZ Nightly",
            id: "dev.zzz.ZZZ-Nightly".to_owned(),
            suffix: "-nightly",
            icon: "app-icon-nightly",
        },
        Channel::Dev => AppIdentity {
            name: "ZZZ Dev",
            id: "dev.zzz.ZZZ-Dev".to_owned(),
            suffix: "-dev",
            icon: "app-icon-dev",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_are_split_between_container_and_host() {
        let formats: BTreeSet<Format> =
            [Format::TarGz, Format::Deb, Format::AppImage, Format::Snap]
                .into_iter()
                .collect();
        assert_eq!(
            container_formats(&formats),
            vec![Format::TarGz, Format::Deb]
        );
        assert_eq!(
            host_only_formats(&formats),
            vec![Format::AppImage, Format::Snap]
        );
    }

    #[test]
    fn channel_identity_matches_the_packaged_names() {
        assert_eq!(app_identity(Channel::Stable).suffix, "");
        assert_eq!(app_identity(Channel::Stable).id, "dev.zzz.ZZZ");
        assert_eq!(app_identity(Channel::Dev).id, "dev.zzz.ZZZ-Dev");
        assert_eq!(app_identity(Channel::Nightly).name, "ZZZ Nightly");
    }

    #[test]
    fn only_windows_formats_are_rejected_elsewhere() {
        assert!(validate(Target::LinuxX86_64, &[Format::Deb]).is_ok());
        assert!(validate(Target::LinuxX86_64, &[Format::Msi]).is_err());
        assert!(validate(Target::MacAarch64, &[Format::Dmg]).is_ok());
    }
}
