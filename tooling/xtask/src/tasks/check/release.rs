//! Requires the release metadata to agree on one version.
//!
//! `crates/zzz/Cargo.toml` is the only place the ZZZ version is declared, and
//! `Cargo.lock`, `CHANGELOG.md`, and the release tags all follow it. Upstream
//! absorption bumps the version by editing the manifest by hand, which leaves
//! the lockfile entry stale until something builds the workspace, and leaves
//! the changelog untouched entirely. `script/xtask compliance` and
//! `.github/workflows/compliance_check.yml` derive the tag from the manifest,
//! so a stale lockfile or a missing `[Unreleased]` section produces a tag that
//! describes a tree nobody published.
//!
//! This gate reads the three machine-readable sources and fails on drift. It
//! deliberately does not police the changelog wording; see
//! `docs/src/development/releasing.md`.

use std::sync::LazyLock;

use anyhow::{Result, bail};
use regex::Regex;

const PACKAGE_MANIFEST: &str = "crates/zzz/Cargo.toml";
const LOCKFILE: &str = "Cargo.lock";
const CHANGELOG: &str = "CHANGELOG.md";
const PACKAGE_NAME: &str = "zzz";

/// The sections `CHANGELOG.md` keeps under `[Unreleased]`. Keep a line under
/// each even when it only says `- N/A`, so the next entry has a home.
const CHANGELOG_SECTIONS: &[&str] = &["### Added", "### Changed", "### Removed", "### Fixed"];

const UNRELEASED_HEADING: &str = "## [Unreleased]";

static SEMVER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d+\.\d+\.\d+$").expect("the release version pattern is a valid regex")
});

pub fn check() -> Result<()> {
    check_at(&super::Repo::discover()?)
}

/// The gate, rooted at an injected repository.
pub fn check_at(repo: &super::Repo) -> Result<()> {
    let manifest = repo.read(PACKAGE_MANIFEST)?;
    let lockfile = repo.read(LOCKFILE)?;
    let changelog = repo.read(CHANGELOG)?;

    let failures = collect_failures(&manifest, &lockfile, &changelog);
    if failures.is_empty() {
        return Ok(());
    }
    bail!(
        "release metadata disagrees (keep crates/zzz/Cargo.toml, Cargo.lock, and CHANGELOG.md in sync; see docs/src/development/releasing.md):\n{}",
        failures.join("\n")
    )
}

/// Every disagreement across the three sources, in reporting order.
///
/// Kept free of file IO so each failure mode is testable without fixtures on
/// disk.
fn collect_failures(manifest: &str, lockfile: &str, changelog: &str) -> Vec<String> {
    let mut failures = Vec::new();

    let declared = manifest_version(manifest);
    let declared = match declared {
        Ok(Some(version)) => {
            if SEMVER.is_match(&version) {
                Some(version)
            } else {
                failures.push(format!(
                    "{PACKAGE_MANIFEST} declares {version:?}, which is not a plain major.minor.patch release version"
                ));
                None
            }
        }
        Ok(None) => {
            failures.push(format!(
                "{PACKAGE_MANIFEST} has no `[package] version` to derive the release tag from"
            ));
            None
        }
        Err(error) => {
            failures.push(format!("{PACKAGE_MANIFEST} is not valid TOML: {error}"));
            None
        }
    };

    let locked = match lockfile_version(lockfile, PACKAGE_NAME) {
        Ok(Some(version)) => Some(version),
        Ok(None) => {
            failures.push(format!(
                "{LOCKFILE} has no `{PACKAGE_NAME}` package entry to compare against"
            ));
            None
        }
        Err(error) => {
            failures.push(format!("{LOCKFILE} is not valid TOML: {error}"));
            None
        }
    };

    if let (Some(declared), Some(locked)) = (&declared, &locked)
        && declared != locked
    {
        failures.push(format!(
            "{LOCKFILE} pins {PACKAGE_NAME} {locked} but {PACKAGE_MANIFEST} declares {declared}; run `cargo check -p {PACKAGE_NAME}` to refresh the lockfile"
        ));
    }

    failures.extend(changelog_failures(changelog));
    failures
}

/// The version declared under `[package]` in a manifest.
fn manifest_version(manifest: &str) -> Result<Option<String>> {
    let parsed: toml::Value = toml::from_str(manifest)?;
    let version = parsed
        .get("package")
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str);
    Ok(version.map(str::to_owned))
}

/// The version `Cargo.lock` records for `name`.
fn lockfile_version(lockfile: &str, name: &str) -> Result<Option<String>> {
    let parsed: toml::Value = toml::from_str(lockfile)?;
    let version = parsed
        .get("package")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .find(|package| package.get("name").and_then(toml::Value::as_str) == Some(name))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str);
    Ok(version.map(str::to_owned))
}

/// Structural requirements on the changelog's unreleased section.
fn changelog_failures(changelog: &str) -> Vec<String> {
    let Some(unreleased) = changelog.find(UNRELEASED_HEADING) else {
        return vec![format!(
            "{CHANGELOG} has no `{UNRELEASED_HEADING}` section; every release note lands there first"
        )];
    };
    let unreleased = &changelog[unreleased..];
    let section = unreleased.split("\n## ").next().unwrap_or(unreleased);

    CHANGELOG_SECTIONS
        .iter()
        .filter(|heading| !section.contains(*heading))
        .map(|heading| {
            format!("{CHANGELOG} has no `{heading}` subsection under `{UNRELEASED_HEADING}`")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        CHANGELOG, LOCKFILE, PACKAGE_MANIFEST, collect_failures, lockfile_version, manifest_version,
    };

    fn manifest(version: &str) -> String {
        format!("[package]\nname = \"zzz\"\nversion = \"{version}\"\n")
    }

    fn lockfile(version: &str) -> String {
        format!(
            "version = 4\n\n[[package]]\nname = \"gpui\"\nversion = \"0.1.0\"\n\n[[package]]\nname = \"zzz\"\nversion = \"{version}\"\n"
        )
    }

    fn changelog() -> String {
        "## [Unreleased]\n\n### Added\n\n- Something.\n\n### Changed\n\n- N/A\n\n### Removed\n\n- N/A\n\n### Fixed\n\n- N/A\n"
            .to_owned()
    }

    #[test]
    fn reads_the_manifest_and_lockfile_versions() {
        assert_eq!(
            manifest_version(&manifest("1.29.0")).expect("valid manifest"),
            Some("1.29.0".to_owned())
        );
        assert_eq!(
            lockfile_version(&lockfile("1.29.0"), "zzz").expect("valid lockfile"),
            Some("1.29.0".to_owned())
        );
    }

    #[test]
    fn a_missing_version_is_reported_rather_than_defaulted() {
        assert_eq!(
            manifest_version("[package]\nname = \"zzz\"\n").expect("valid manifest"),
            None
        );
        assert_eq!(
            lockfile_version(&lockfile("1.29.0"), "missing").expect("valid lockfile"),
            None
        );
    }

    #[test]
    fn agreement_passes() {
        assert!(
            collect_failures(&manifest("1.29.0"), &lockfile("1.29.0"), &changelog()).is_empty(),
            "an agreeing tree must pass"
        );
    }

    #[test]
    fn a_stale_lockfile_fails() {
        let failures = collect_failures(&manifest("1.30.0"), &lockfile("1.29.0"), &changelog());
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].contains("pins zzz 1.29.0"), "{failures:?}");
    }

    #[test]
    fn a_version_that_is_not_a_release_version_fails() {
        let failures = collect_failures(&manifest("1.30"), &lockfile("1.30"), &changelog());
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].contains("major.minor.patch"), "{failures:?}");
    }

    #[test]
    fn a_missing_unreleased_section_fails() {
        let changelog = "## [1.0.0]\n\n### Added\n\n- Something.\n";
        let failures = collect_failures(&manifest("1.29.0"), &lockfile("1.29.0"), changelog);
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].contains("[Unreleased]"), "{failures:?}");
    }

    #[test]
    fn a_missing_section_under_unreleased_fails() {
        let changelog = "## [Unreleased]\n\n### Added\n\n- Something.\n";
        let failures = collect_failures(&manifest("1.29.0"), &lockfile("1.29.0"), changelog);
        assert_eq!(failures.len(), 3, "{failures:?}");
        assert!(
            failures
                .iter()
                .all(|failure| failure.contains("subsection")),
            "{failures:?}"
        );
    }

    #[test]
    fn sections_of_a_later_release_do_not_satisfy_unreleased() {
        let changelog = "## [Unreleased]\n\n### Added\n\n- Something.\n\n## [1.0.0]\n\n### Changed\n\n- X.\n\n### Removed\n\n- X.\n\n### Fixed\n\n- X.\n";
        let failures = collect_failures(&manifest("1.29.0"), &lockfile("1.29.0"), changelog);
        assert_eq!(failures.len(), 3, "{failures:?}");
    }

    /// The gate end to end, against a fixture tree instead of the workspace.
    ///
    /// This is what `Repo` exists for: the gate resolves every path through
    /// the injected root, so a test can exercise the real file IO without
    /// changing the process working directory.
    #[test]
    fn a_fixture_repository_passes_and_reports_drift() {
        use crate::tasks::check::Repo;
        use std::fs;

        let directory = tempfile::tempdir().expect("a temporary directory");
        let repo = Repo::at(directory.path());
        fs::create_dir_all(repo.path("crates/zzz")).expect("the fixture manifest directory");
        fs::write(repo.path(PACKAGE_MANIFEST), manifest("1.29.0")).expect("the fixture manifest");
        fs::write(repo.path(LOCKFILE), lockfile("1.29.0")).expect("the fixture lockfile");
        fs::write(repo.path(CHANGELOG), changelog()).expect("the fixture changelog");

        super::check_at(&repo).expect("an agreeing fixture repository must pass");

        fs::write(repo.path(LOCKFILE), lockfile("1.28.0")).expect("a stale fixture lockfile");
        let error = super::check_at(&repo).expect_err("a stale lockfile must fail");
        assert!(error.to_string().contains("pins zzz 1.28.0"), "{error}");
    }

    #[test]
    fn unparsable_sources_are_reported() {
        let failures = collect_failures("not toml", "not toml", &changelog());
        assert_eq!(failures.len(), 2, "{failures:?}");
        assert!(
            failures
                .iter()
                .all(|failure| failure.contains("not valid TOML")),
            "{failures:?}"
        );
    }
}
