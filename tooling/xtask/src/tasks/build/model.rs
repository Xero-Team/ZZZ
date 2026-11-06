//! Value types shared by the build planner, the scheduler, and the artifact
//! manifest.
//!
//! These are the vocabulary of `cargo xtask build`: a target is a Rust triple
//! with the packaging rules attached, a profile is a Cargo profile plus the
//! directory its artifacts land in, and a format is a shippable file family.
//! Everything here is serializable because `--print-plan` emits the same values
//! the scheduler consumes.

use std::fmt;
use std::str::FromStr;

use anyhow::{Result, bail};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// A Cargo profile this repository knows how to build.
///
/// The mapping matters in two places: the flags passed to Cargo and the
/// directory the artifacts land in. `dev` is the only profile whose name and
/// output directory differ (`debug`).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    Dev,
    Dbg,
    Release,
    ReleaseFast,
    LinuxNative,
}

impl Profile {
    pub fn is_release(self) -> bool {
        !matches!(self, Profile::Dev | Profile::Dbg)
    }

    /// Directory under `target/` that Cargo writes this profile's artifacts to.
    pub fn artifact_directory(self) -> &'static str {
        match self {
            Profile::Dev => "debug",
            Profile::Dbg => "dbg",
            Profile::Release => "release",
            Profile::ReleaseFast => "release-fast",
            Profile::LinuxNative => "linux-native",
        }
    }

    /// Stable key used in cache paths and artifact manifests.
    pub fn key(self) -> &'static str {
        match self {
            Profile::Dev => "dev",
            Profile::Dbg => "dbg",
            Profile::Release => "release",
            Profile::ReleaseFast => "release-fast",
            Profile::LinuxNative => "linux-native",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Arch {
    X86_64,
    Aarch64,
}

impl Arch {
    pub fn as_str(self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64",
            Arch::Aarch64 => "aarch64",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Platform {
    Linux,
    Mac,
    Windows,
    FreeBSD,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Linux => "linux",
            Platform::Mac => "mac",
            Platform::Windows => "windows",
            Platform::FreeBSD => "freebsd",
        }
    }

    pub fn host() -> Self {
        match std::env::consts::OS {
            "macos" => Platform::Mac,
            "windows" => Platform::Windows,
            "freebsd" => Platform::FreeBSD,
            _ => Platform::Linux,
        }
    }
}

/// A build target, named after the Rust triple it compiles for so that plans
/// and manifests are readable without a lookup table.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ValueEnum,
)]
pub enum Target {
    #[serde(rename = "x86_64-unknown-linux-gnu")]
    #[value(name = "x86_64-unknown-linux-gnu")]
    LinuxX86_64,
    #[serde(rename = "aarch64-unknown-linux-gnu")]
    #[value(name = "aarch64-unknown-linux-gnu")]
    LinuxAarch64,
    #[serde(rename = "x86_64-unknown-linux-musl")]
    #[value(name = "x86_64-unknown-linux-musl")]
    LinuxX86_64Musl,
    #[serde(rename = "aarch64-unknown-linux-musl")]
    #[value(name = "aarch64-unknown-linux-musl")]
    LinuxAarch64Musl,
    #[serde(rename = "x86_64-apple-darwin")]
    #[value(name = "x86_64-apple-darwin")]
    MacX86_64,
    #[serde(rename = "aarch64-apple-darwin")]
    #[value(name = "aarch64-apple-darwin")]
    MacAarch64,
    #[serde(rename = "x86_64-pc-windows-msvc")]
    #[value(name = "x86_64-pc-windows-msvc")]
    WindowsX86_64,
    #[serde(rename = "aarch64-pc-windows-msvc")]
    #[value(name = "aarch64-pc-windows-msvc")]
    WindowsAarch64,
    #[serde(rename = "x86_64-unknown-freebsd")]
    #[value(name = "x86_64-unknown-freebsd")]
    FreeBSDX86_64,
    #[serde(rename = "aarch64-unknown-freebsd")]
    #[value(name = "aarch64-unknown-freebsd")]
    FreeBSDAarch64,
}

impl Target {
    pub const ALL: [Target; 10] = [
        Target::LinuxX86_64,
        Target::LinuxAarch64,
        Target::LinuxX86_64Musl,
        Target::LinuxAarch64Musl,
        Target::MacX86_64,
        Target::MacAarch64,
        Target::WindowsX86_64,
        Target::WindowsAarch64,
        Target::FreeBSDX86_64,
        Target::FreeBSDAarch64,
    ];

    /// Targets whose bundles are produced for a release. The musl targets only
    /// exist so the editor can embed a static remote server; they are not
    /// distributed on their own.
    pub const DISTRIBUTABLE: [Target; 6] = [
        Target::LinuxX86_64,
        Target::MacAarch64,
        Target::MacX86_64,
        Target::WindowsX86_64,
        Target::FreeBSDX86_64,
        Target::FreeBSDAarch64,
    ];

    pub fn triple(self) -> &'static str {
        match self {
            Target::LinuxX86_64 => "x86_64-unknown-linux-gnu",
            Target::LinuxAarch64 => "aarch64-unknown-linux-gnu",
            Target::LinuxX86_64Musl => "x86_64-unknown-linux-musl",
            Target::LinuxAarch64Musl => "aarch64-unknown-linux-musl",
            Target::MacX86_64 => "x86_64-apple-darwin",
            Target::MacAarch64 => "aarch64-apple-darwin",
            Target::WindowsX86_64 => "x86_64-pc-windows-msvc",
            Target::WindowsAarch64 => "aarch64-pc-windows-msvc",
            Target::FreeBSDX86_64 => "x86_64-unknown-freebsd",
            Target::FreeBSDAarch64 => "aarch64-unknown-freebsd",
        }
    }

    pub fn arch(self) -> Arch {
        match self {
            Target::LinuxX86_64
            | Target::LinuxX86_64Musl
            | Target::MacX86_64
            | Target::WindowsX86_64
            | Target::FreeBSDX86_64 => Arch::X86_64,
            Target::LinuxAarch64
            | Target::LinuxAarch64Musl
            | Target::MacAarch64
            | Target::WindowsAarch64
            | Target::FreeBSDAarch64 => Arch::Aarch64,
        }
    }

    pub fn platform(self) -> Platform {
        match self {
            Target::LinuxX86_64 | Target::LinuxAarch64 => Platform::Linux,
            Target::LinuxX86_64Musl | Target::LinuxAarch64Musl => Platform::Linux,
            Target::MacX86_64 | Target::MacAarch64 => Platform::Mac,
            Target::WindowsX86_64 | Target::WindowsAarch64 => Platform::Windows,
            Target::FreeBSDX86_64 | Target::FreeBSDAarch64 => Platform::FreeBSD,
        }
    }

    pub fn is_musl(self) -> bool {
        matches!(self, Target::LinuxX86_64Musl | Target::LinuxAarch64Musl)
    }

    pub fn host_default() -> Self {
        match Platform::host() {
            Platform::Mac => match host_arch() {
                Arch::Aarch64 => Target::MacAarch64,
                Arch::X86_64 => Target::MacX86_64,
            },
            Platform::Windows => match host_arch() {
                Arch::Aarch64 => Target::WindowsAarch64,
                Arch::X86_64 => Target::WindowsX86_64,
            },
            Platform::Linux => match host_arch() {
                Arch::Aarch64 => Target::LinuxAarch64,
                Arch::X86_64 => Target::LinuxX86_64,
            },
            Platform::FreeBSD => match host_arch() {
                Arch::Aarch64 => Target::FreeBSDAarch64,
                Arch::X86_64 => Target::FreeBSDX86_64,
            },
        }
    }

    /// Whether producing this target on the current host is a cross build.
    pub fn is_cross(self) -> bool {
        self.platform() != Platform::host() || self.arch() != host_arch()
    }

    /// Stable, human-readable name used in task ids and archive names.
    pub fn short(self) -> String {
        format!("{}-{}", self.platform().as_str(), self.arch().as_str())
    }

    pub fn default_formats(self) -> Vec<Format> {
        match self.platform() {
            Platform::Linux | Platform::FreeBSD => vec![Format::TarGz],
            Platform::Mac => vec![Format::App, Format::Dmg],
            Platform::Windows => vec![Format::Exe],
        }
    }

    pub fn supports(self, format: Format) -> bool {
        match format {
            Format::TarGz => {
                matches!(self.platform(), Platform::Linux | Platform::FreeBSD) && !self.is_musl()
            }
            Format::Deb | Format::Rpm | Format::AppImage | Format::Flatpak | Format::Snap => {
                self.platform() == Platform::Linux && !self.is_musl()
            }
            Format::App | Format::Dmg => self.platform() == Platform::Mac,
            Format::Exe | Format::Msi | Format::Inno => self.platform() == Platform::Windows,
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.triple())
    }
}

impl FromStr for Target {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        for target in Target::ALL {
            if target.triple() == value {
                return Ok(target);
            }
        }

        // Accept the `<platform>-<arch>` spelling used by task ids and artifact
        // names, so a plan copied out of a manifest can be fed back in.
        let normalized = value.replace('_', "-");
        for target in Target::ALL {
            if target.short().replace('_', "-") == normalized {
                return Ok(target);
            }
        }

        bail!(
            "unknown target `{value}`; expected one of: {}",
            Target::ALL
                .iter()
                .map(|target| target.triple())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn host_arch() -> Arch {
    match std::env::consts::ARCH {
        "aarch64" => Arch::Aarch64,
        _ => Arch::X86_64,
    }
}

/// A shippable artifact family.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    #[value(name = "tar.gz", alias = "tar-gz", alias = "tgz", alias = "tarball")]
    TarGz,
    Deb,
    Rpm,
    #[value(name = "appimage")]
    AppImage,
    Flatpak,
    Snap,
    App,
    Dmg,
    Exe,
    Msi,
    Inno,
}

impl Format {
    pub fn as_str(self) -> &'static str {
        match self {
            Format::TarGz => "tar.gz",
            Format::Deb => "deb",
            Format::Rpm => "rpm",
            Format::AppImage => "appimage",
            Format::Flatpak => "flatpak",
            Format::Snap => "snap",
            Format::App => "app",
            Format::Dmg => "dmg",
            Format::Exe => "exe",
            Format::Msi => "msi",
            Format::Inno => "inno",
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Release channel baked into the bundle name and the desktop entry.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
pub enum Channel {
    Dev,
    Stable,
    Preview,
    Nightly,
}

impl Channel {
    /// The channel a shipping artifact uses when the caller does not name one.
    /// `cargo run` and `cargo build` are development builds and stay on `dev`.
    pub const SHIPPING: Channel = Channel::Stable;

    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Dev => "dev",
            Channel::Stable => "stable",
            Channel::Preview => "preview",
            Channel::Nightly => "nightly",
        }
    }

    /// Reads `crates/zzz/RELEASE_CHANNEL`, defaulting to `dev` when the file is
    /// missing or holds an unknown value. Development builds (`cargo run`,
    /// `cargo build`) use this; packaging uses [`Channel::detect_shipping`].
    pub fn detect(workspace: &std::path::Path) -> Self {
        let path = workspace.join("crates/zzz/RELEASE_CHANNEL");
        let Ok(contents) = std::fs::read_to_string(&path) else {
            return Channel::Dev;
        };

        Self::from_str_value(&contents).unwrap_or(Channel::Dev)
    }

    /// Parses a channel name. Unknown values return `None` so each caller can
    /// apply its own default instead of silently baking in a wrong channel.
    fn from_str_value(value: &str) -> Option<Self> {
        match value.trim() {
            "dev" => Some(Channel::Dev),
            "stable" => Some(Channel::Stable),
            "preview" => Some(Channel::Preview),
            "nightly" => Some(Channel::Nightly),
            _ => None,
        }
    }

    /// The channel a shipping artifact uses. `crates/zzz/RELEASE_CHANNEL`
    /// describes development builds, so packaging defaults to `stable` unless
    /// the caller names a channel or exports `ZZZ_RELEASE_CHANNEL`.
    pub fn detect_shipping() -> Self {
        Self::shipping_from_env(std::env::var("ZZZ_RELEASE_CHANNEL").ok().as_deref())
    }

    /// Kept separate from `detect_shipping` so the override is testable without
    /// mutating the process environment. Unknown values fall back to the
    /// shipping channel rather than baking a broken identity into an artifact.
    fn shipping_from_env(value: Option<&str>) -> Self {
        value
            .and_then(Self::from_str_value)
            .unwrap_or(Self::SHIPPING)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_round_trips_through_its_triple() {
        for target in Target::ALL {
            let parsed: Target = target.triple().parse().expect("triple parses");
            assert_eq!(parsed, target);
        }
    }

    #[test]
    fn target_accepts_short_names() {
        assert_eq!(
            "linux-x86_64".parse::<Target>().expect("short name parses"),
            Target::LinuxX86_64
        );
        assert_eq!(
            "mac-aarch64".parse::<Target>().expect("short name parses"),
            Target::MacAarch64
        );
    }

    #[test]
    fn unknown_target_is_rejected() {
        assert!("sparc-unknown-none".parse::<Target>().is_err());
    }

    #[test]
    fn profiles_map_to_their_artifact_directories() {
        assert_eq!(Profile::Dev.artifact_directory(), "debug");
        assert_eq!(Profile::Dbg.artifact_directory(), "dbg");
        assert_eq!(Profile::Release.artifact_directory(), "release");
        assert_eq!(Profile::ReleaseFast.artifact_directory(), "release-fast");
        assert_eq!(Profile::LinuxNative.artifact_directory(), "linux-native");
        assert!(Profile::Release.is_release());
        assert!(!Profile::Dev.is_release());
        assert!(!Profile::Dbg.is_release());
    }

    #[test]
    fn musl_targets_only_support_linux_bundles() {
        assert!(Target::LinuxX86_64.supports(Format::Deb));
        assert!(!Target::LinuxX86_64Musl.supports(Format::Deb));
        assert!(!Target::MacAarch64.supports(Format::Deb));
    }

    #[test]
    fn host_target_matches_the_platform_and_arch() {
        let host = Target::host_default();
        assert_eq!(host.platform(), Platform::host());
        assert!(!host.is_cross());
    }

    #[test]
    fn shipping_channel_honors_the_environment_override() {
        assert_eq!(Channel::shipping_from_env(None), Channel::Stable);
        assert_eq!(
            Channel::shipping_from_env(Some("preview")),
            Channel::Preview
        );
        assert_eq!(
            Channel::shipping_from_env(Some(" nightly ")),
            Channel::Nightly
        );
        assert_eq!(Channel::shipping_from_env(Some("dev")), Channel::Dev);
        // An unknown value must fall back to the shipping channel rather than
        // baking a broken identity into an artifact.
        assert_eq!(Channel::shipping_from_env(Some("garbage")), Channel::Stable);
        assert_eq!(Channel::shipping_from_env(Some("")), Channel::Stable);
    }
}
