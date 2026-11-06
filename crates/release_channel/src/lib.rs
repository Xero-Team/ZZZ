//! Provides constructs for the ZZZ app version and release channel.

#![deny(missing_docs)]

use std::{env, str::FromStr, sync::LazyLock};

use gpui::{App, Global};
use semver::Version;

/// The raw release channel name from the build environment or embedded build
/// metadata.
///
/// A shipping build is driven by `cargo xtask bundle --channel <name>`, which
/// sets `ZZZ_RELEASE_CHANNEL` at compile time. The file stays the fallback for
/// `cargo run` and `cargo build`, where no packaging step sets the variable, so
/// a development build remains `dev` without any extra flags.
static RAW_RELEASE_CHANNEL_NAME: LazyLock<String> = LazyLock::new(|| {
    let file = include_str!("../../zzz/RELEASE_CHANNEL");
    let runtime_override = if cfg!(debug_assertions) {
        env::var("ZZZ_RELEASE_CHANNEL").ok()
    } else {
        None
    };
    let channel_override = option_env!("ZZZ_RELEASE_CHANNEL")
        .filter(|channel| !channel.trim().is_empty())
        .or(runtime_override.as_deref());
    resolve_raw_release_channel(channel_override, file)
});

/// Picks the raw channel name by priority: an explicit override (the
/// compile-time packaging channel, or a runtime one in debug builds) wins over
/// the embedded file. Blank overrides are ignored so a stray empty build flag
/// cannot bake an invalid identity into the binary and make `RELEASE_CHANNEL`
/// panic on first access.
fn resolve_raw_release_channel(channel_override: Option<&str>, file: &str) -> String {
    channel_override
        .map(str::trim)
        .filter(|channel| !channel.is_empty())
        .map_or_else(|| file.trim().to_owned(), str::to_owned)
}

/// stable | dev
pub static RELEASE_CHANNEL_NAME: LazyLock<String> =
    LazyLock::new(|| RELEASE_CHANNEL.dev_name().to_owned());

#[doc(hidden)]
pub static RELEASE_CHANNEL: LazyLock<ReleaseChannel> =
    LazyLock::new(
        || match ReleaseChannel::from_str(&RAW_RELEASE_CHANNEL_NAME) {
            Ok(channel) => channel,
            _ => panic!("invalid release channel {}", *RAW_RELEASE_CHANNEL_NAME),
        },
    );

/// The app identifier for the current release channel, Windows only.
#[cfg(target_os = "windows")]
pub fn app_identifier() -> &'static str {
    match *RELEASE_CHANNEL {
        ReleaseChannel::Dev => "ZZZ-Editor-Dev",
        ReleaseChannel::Stable => "ZZZ-Editor-Stable",
    }
}

/// The Git commit SHA that ZZZ was built at.
#[derive(Clone, Eq, Debug, PartialEq)]
pub struct AppCommitSha(String);

struct GlobalAppCommitSha(AppCommitSha);

impl Global for GlobalAppCommitSha {}

impl AppCommitSha {
    /// Creates a new [`AppCommitSha`].
    pub fn new(sha: String) -> Self {
        AppCommitSha(sha)
    }

    /// Returns the global [`AppCommitSha`], if one is set.
    pub fn try_global(cx: &App) -> Option<AppCommitSha> {
        cx.try_global::<GlobalAppCommitSha>()
            .map(|sha| sha.0.clone())
    }

    /// Sets the global [`AppCommitSha`].
    pub fn set_global(sha: AppCommitSha, cx: &mut App) {
        cx.set_global(GlobalAppCommitSha(sha))
    }

    /// Returns the full commit SHA.
    pub fn full(&self) -> String {
        self.0.clone()
    }

    /// Returns the short (7 character) commit SHA.
    pub fn short(&self) -> String {
        self.0.chars().take(7).collect()
    }
}

struct GlobalAppVersion(Version);

impl Global for GlobalAppVersion {}

/// The version of ZZZ.
pub struct AppVersion;

impl AppVersion {
    /// Load the app version from env.
    pub fn load(
        pkg_version: &str,
        build_id: Option<&str>,
        commit_sha: Option<AppCommitSha>,
    ) -> Version {
        // A malformed override must not take the app down during startup, so fall
        // through to the embedded package version, which is valid by construction.
        let mut version: Version = match env::var("ZZZ_APP_VERSION") {
            Ok(from_env) => match Version::parse(&from_env) {
                Ok(version) => version,
                Err(error) => {
                    log::error!("invalid ZZZ_APP_VERSION {from_env:?}: {error}");
                    package_version(pkg_version)
                }
            },
            Err(env::VarError::NotPresent) => package_version(pkg_version),
            Err(error) => {
                log::error!("failed to read ZZZ_APP_VERSION: {error}");
                package_version(pkg_version)
            }
        };
        let mut pre = String::from(RELEASE_CHANNEL.dev_name());

        if let Some(build_id) = build_id {
            pre.push('.');
            pre.push_str(&build_id);
        }

        if let Some(sha) = commit_sha {
            pre.push('.');
            pre.push_str(&sha.0);
        }
        if let Ok(build) = semver::BuildMetadata::new(&pre) {
            version.build = build;
        }

        version
    }

    /// Returns the global version number.
    pub fn global(cx: &App) -> Version {
        if cx.has_global::<GlobalAppVersion>() {
            cx.global::<GlobalAppVersion>().0.clone()
        } else {
            Version::new(0, 0, 0)
        }
    }
}

fn package_version(pkg_version: &str) -> Version {
    match Version::parse(pkg_version) {
        Ok(version) => version,
        Err(error) => {
            log::error!("invalid package version {pkg_version:?}: {error}");
            Version::new(0, 0, 0)
        }
    }
}

/// A ZZZ release channel.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum ReleaseChannel {
    /// The development release channel.
    ///
    /// Used for local debug builds of ZZZ.
    #[default]
    Dev,

    /// The Stable release channel.
    Stable,
}

struct GlobalReleaseChannel(ReleaseChannel);

impl Global for GlobalReleaseChannel {}

/// Initializes the release channel.
pub fn init(app_version: Version, cx: &mut App) {
    cx.set_global(GlobalAppVersion(app_version));
    cx.set_global(GlobalReleaseChannel(*RELEASE_CHANNEL))
}

/// Initializes the release channel for tests that rely on fake release channel.
pub fn init_test(app_version: Version, release_channel: ReleaseChannel, cx: &mut App) {
    cx.set_global(GlobalAppVersion(app_version));
    cx.set_global(GlobalReleaseChannel(release_channel))
}

impl ReleaseChannel {
    /// All release channels.
    pub const ALL: [ReleaseChannel; 2] = [ReleaseChannel::Dev, ReleaseChannel::Stable];

    /// Returns the global [`ReleaseChannel`].
    pub fn global(cx: &App) -> Self {
        cx.global::<GlobalReleaseChannel>().0
    }

    /// Returns the global [`ReleaseChannel`], if one is set.
    pub fn try_global(cx: &App) -> Option<Self> {
        cx.try_global::<GlobalReleaseChannel>()
            .map(|channel| channel.0)
    }

    /// Returns whether we want to poll for updates for this [`ReleaseChannel`]
    pub fn poll_for_updates(&self) -> bool {
        !matches!(self, ReleaseChannel::Dev)
    }

    /// Returns the display name for this [`ReleaseChannel`].
    pub fn display_name(&self) -> &'static str {
        match self {
            ReleaseChannel::Dev => "ZZZ Dev",
            ReleaseChannel::Stable => "ZZZ",
        }
    }

    /// Returns the programmatic name for this [`ReleaseChannel`].
    pub fn dev_name(&self) -> &'static str {
        match self {
            ReleaseChannel::Dev => "dev",
            ReleaseChannel::Stable => "stable",
        }
    }

    /// Returns the application ID that's used by Wayland as application ID
    /// and WM_CLASS on X11.
    /// This also has to match the bundle identifier for ZZZ on macOS.
    pub fn app_id(&self) -> &'static str {
        match self {
            ReleaseChannel::Dev => "dev.zzz.ZZZ-Dev",
            ReleaseChannel::Stable => "dev.zzz.ZZZ",
        }
    }
}

/// Error indicating that release channel string does not match any known release channel names.
#[derive(Copy, Clone, Debug, Hash, PartialEq)]
pub struct InvalidReleaseChannel;

impl FromStr for ReleaseChannel {
    type Err = InvalidReleaseChannel;

    fn from_str(channel: &str) -> Result<Self, Self::Err> {
        Ok(match channel {
            "dev" => ReleaseChannel::Dev,
            "nightly" | "preview" | "stable" => ReleaseChannel::Stable,
            _ => return Err(InvalidReleaseChannel),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvRestore {
        name: String,
        original: Option<String>,
    }

    impl EnvRestore {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                original: env::var(name).ok(),
            }
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            match self.original.as_deref() {
                Some(value) => unsafe { env::set_var(&self.name, value) },
                None => unsafe { env::remove_var(&self.name) },
            }
        }
    }

    #[test]
    fn app_commit_sha_full_and_short_preserve_expected_lengths() {
        let sha = AppCommitSha::new("1234567890abcdef".to_string());

        assert_eq!(sha.full(), "1234567890abcdef");
        assert_eq!(sha.short(), "1234567");
    }

    #[test]
    fn app_version_load_uses_env_override_and_build_metadata() {
        let _lock = ENV_LOCK.lock().expect("env lock poisoned");
        let _restore = EnvRestore::new("ZZZ_APP_VERSION");
        unsafe { env::set_var("ZZZ_APP_VERSION", "1.2.3") };

        let version = AppVersion::load(
            "9.9.9",
            Some("42"),
            Some(AppCommitSha::new("abcdef0".to_string())),
        );

        assert_eq!(
            version,
            Version::parse("1.2.3+dev.42.abcdef0").expect("valid semver")
        );
    }

    #[test]
    fn app_version_load_falls_back_to_package_version_without_metadata() {
        let _lock = ENV_LOCK.lock().expect("env lock poisoned");
        let _restore = EnvRestore::new("ZZZ_APP_VERSION");
        unsafe { env::remove_var("ZZZ_APP_VERSION") };

        let version = AppVersion::load("0.9.1", None, None);

        assert_eq!(version, Version::parse("0.9.1+dev").expect("valid semver"));
    }

    #[test]
    fn app_version_load_ignores_invalid_env_override() {
        let _lock = ENV_LOCK.lock().expect("env lock poisoned");
        let _restore = EnvRestore::new("ZZZ_APP_VERSION");
        unsafe { env::set_var("ZZZ_APP_VERSION", "not-a-version") };

        let version = AppVersion::load("0.9.1", None, None);

        assert_eq!(version, Version::parse("0.9.1+dev").expect("valid semver"));
    }

    #[test]
    fn release_channel_parsing_and_properties_match_supported_values() {
        assert_eq!(ReleaseChannel::from_str("dev"), Ok(ReleaseChannel::Dev));
        assert_eq!(
            ReleaseChannel::from_str("stable"),
            Ok(ReleaseChannel::Stable)
        );
        assert_eq!(
            ReleaseChannel::from_str("preview"),
            Ok(ReleaseChannel::Stable)
        );
        assert_eq!(
            ReleaseChannel::from_str("nightly"),
            Ok(ReleaseChannel::Stable)
        );
        assert_eq!(ReleaseChannel::from_str("beta"), Err(InvalidReleaseChannel));

        // The resolved value must always be a parseable channel, whichever
        // source it came from; `RELEASE_CHANNEL` panics on an unknown name.
        assert!(!RAW_RELEASE_CHANNEL_NAME.is_empty());
        assert!(ReleaseChannel::from_str(&RAW_RELEASE_CHANNEL_NAME).is_ok());

        assert_eq!(ReleaseChannel::Dev.display_name(), "ZZZ Dev");
        assert_eq!(ReleaseChannel::Stable.display_name(), "ZZZ");
        assert_eq!(ReleaseChannel::Dev.dev_name(), "dev");
        assert_eq!(ReleaseChannel::Stable.dev_name(), "stable");
        assert_eq!(ReleaseChannel::Dev.app_id(), "dev.zzz.ZZZ-Dev");
        assert_eq!(ReleaseChannel::Stable.app_id(), "dev.zzz.ZZZ");
        assert!(!ReleaseChannel::Dev.poll_for_updates());
        assert!(ReleaseChannel::Stable.poll_for_updates());
        assert_eq!(
            ReleaseChannel::ALL,
            [ReleaseChannel::Dev, ReleaseChannel::Stable]
        );
    }

    #[test]
    fn raw_channel_prefers_an_override_and_ignores_blank_ones() {
        // The compile-time packaging channel is how `cargo xtask bundle
        // --channel` bakes an identity into a release binary; it wins over the
        // development file.
        assert_eq!(
            resolve_raw_release_channel(Some(" preview "), "dev\n"),
            "preview"
        );
        assert_eq!(resolve_raw_release_channel(None, "dev\n"), "dev");
        // A stray empty build flag must fall back to the file. Without this,
        // `RAW_RELEASE_CHANNEL_NAME` is empty and `RELEASE_CHANNEL` panics on
        // first access.
        assert_eq!(resolve_raw_release_channel(Some(""), "dev\n"), "dev");
        assert_eq!(resolve_raw_release_channel(Some("   "), "dev\n"), "dev");
    }
}
