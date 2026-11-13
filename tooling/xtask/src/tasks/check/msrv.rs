//! Requires the pinned Rust toolchain to agree across every machine-readable
//! source that records it.
//!
//! `rust-toolchain.toml`, `clippy.toml`, `Cargo.toml`'s `rust-version`, and the
//! CI MSRV matrix were aligned by hand in the governance-audit commit, but
//! nothing stops a later change from bumping one and leaving the others behind.
//! A stale `msrv` silently changes Clippy's version-aware suggestions, and a
//! stale CI matrix stops testing the version the workspace promises. This gate
//! reads all four and fails on drift or an unparsable version.

use std::sync::LazyLock;

use anyhow::{Context as _, Result, bail};
use regex::Regex;

static SEMVER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d+\.\d+(?:\.\d+)?$").expect("valid semver pattern"));

/// (label, path, marker identifying the line that holds the version).
const SOURCES: &[(&str, &str, &str)] = &[
    (
        "rust-toolchain.toml channel",
        "rust-toolchain.toml",
        "channel",
    ),
    ("clippy.toml msrv", "clippy.toml", "msrv"),
    ("Cargo.toml rust-version", "Cargo.toml", "rust-version"),
    ("CI MSRV matrix", ".github/workflows/ci.yaml", "toolchain:"),
];

pub fn check() -> Result<()> {
    check_at(&super::Repo::discover()?)
}

/// The gate, rooted at an injected repository.
pub fn check_at(repo: &super::Repo) -> Result<()> {
    let mut readings = Vec::new();
    for &(label, path, marker) in SOURCES {
        let source = repo.read(path)?;
        let version = find_version(&source, marker).with_context(|| {
            format!("{label}: no quoted version on the `{marker}` line in {path}")
        })?;
        readings.push((label.to_owned(), version));
    }

    let failures = compare(&readings);
    if failures.is_empty() {
        return Ok(());
    }
    bail!(
        "MSRV sources disagree or are invalid (keep rust-toolchain.toml, clippy.toml, Cargo.toml, and the CI matrix in sync):\n{}",
        failures.join("\n")
    )
}

/// Returns the first double-quoted value on the line containing `marker`.
fn find_version(source: &str, marker: &str) -> Option<String> {
    let line = source
        .lines()
        .find(|line| line.contains(marker) && line.contains('"'))?;
    let after_marker = line.split_once(marker)?.1;
    let (_, quoted) = after_marker.split_once('"')?;
    let (value, _) = quoted.split_once('"')?;
    Some(value.to_owned())
}

/// Compares every reading against the first one and validates the version
/// shape. Kept free of file IO so drift and malformed versions are testable.
fn compare(readings: &[(String, String)]) -> Vec<String> {
    let Some((_, reference)) = readings.first() else {
        return vec!["no MSRV sources were read".to_owned()];
    };
    let mut failures = Vec::new();
    for (label, version) in readings {
        if !SEMVER.is_match(version) {
            failures.push(format!("{label} has an invalid version {version:?}"));
        } else if version != reference {
            failures.push(format!("{label} is {version:?}, expected {reference:?}"));
        }
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::{SEMVER, compare, find_version};

    fn readings(values: &[(&str, &str)]) -> Vec<(String, String)> {
        values
            .iter()
            .map(|(label, version)| ((*label).to_owned(), (*version).to_owned()))
            .collect()
    }

    #[test]
    fn finds_the_version_on_each_source_line() {
        assert_eq!(
            find_version("channel = \"1.99.1\"\n", "channel").as_deref(),
            Some("1.99.1")
        );
        assert_eq!(
            find_version("msrv = \"1.99.1\"\n", "msrv").as_deref(),
            Some("1.99.1")
        );
        assert_eq!(
            find_version("rust-version = \"1.99.1\"\n", "rust-version").as_deref(),
            Some("1.99.1")
        );
        assert_eq!(
            find_version(
                "        toolchain: [\"1.99.1\", \"stable\"]\n",
                "toolchain:"
            )
            .as_deref(),
            Some("1.99.1")
        );
    }

    #[test]
    fn accepts_only_a_plain_release_version() {
        assert!(SEMVER.is_match("1.99.1"));
        assert!(SEMVER.is_match("1.99"));
        assert!(!SEMVER.is_match("stable"));
        assert!(!SEMVER.is_match("1.99.1-nightly"));
        assert!(!SEMVER.is_match("\"1.99.1\""));
    }

    #[test]
    fn accepts_matching_versions() {
        let readings = readings(&[("channel", "1.99.1"), ("msrv", "1.99.1")]);
        assert!(compare(&readings).is_empty());
    }

    #[test]
    fn reports_drift_and_invalid_versions() {
        let readings = readings(&[
            ("channel", "1.99.1"),
            ("msrv", "1.98.0"),
            ("rust-version", "stable"),
        ]);
        let failures = compare(&readings);
        assert_eq!(failures.len(), 2, "{failures:?}");
        assert!(
            failures.iter().any(|failure| failure.contains("expected")),
            "{failures:?}"
        );
        assert!(
            failures.iter().any(|failure| failure.contains("invalid")),
            "{failures:?}"
        );
    }

    /// The gate end to end: four fixture files that disagree must fail, and
    /// agreeing files must pass, without touching the repository itself.
    #[test]
    fn a_fixture_repository_reports_drift_between_sources() {
        use crate::tasks::check::Repo;
        use std::fs;

        let directory = tempfile::tempdir().expect("a temporary directory");
        let repo = Repo::at(directory.path());
        fs::create_dir_all(repo.path(".github/workflows")).expect("the fixture workflow directory");
        fs::write(repo.path("rust-toolchain.toml"), "channel = \"1.99.1\"\n").expect("toolchain");
        fs::write(repo.path("clippy.toml"), "msrv = \"1.99.1\"\n").expect("clippy");
        fs::write(repo.path("Cargo.toml"), "rust-version = \"1.99.1\"\n").expect("manifest");
        fs::write(
            repo.path(".github/workflows/ci.yaml"),
            "        toolchain: [\"1.99.1\", \"stable\"]\n",
        )
        .expect("workflow");

        super::check_at(&repo).expect("agreeing fixture sources must pass");

        fs::write(repo.path("clippy.toml"), "msrv = \"1.98.0\"\n").expect("drifted clippy");
        let error = super::check_at(&repo).expect_err("drifted sources must fail");
        assert!(error.to_string().contains("1.98.0"), "{error}");
    }

    #[test]
    fn reports_an_empty_reading_list() {
        assert_eq!(compare(&[]), vec!["no MSRV sources were read"]);
    }
}
