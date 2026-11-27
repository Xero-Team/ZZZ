//! Requires the nightly tool gates to stay runnable.
//!
//! `cargo xtask miri` and `cargo xtask sanitize` each run a curated crate list
//! on a nightly toolchain. Two kinds of change silently break them:
//!
//! - A crate leaves the workspace, or its directory is renamed. The task then
//!   fails on a package name that no longer resolves, weeks after the change
//!   that caused it.
//! - `.cargo/config.toml` grows `force = true` under `MIRIFLAGS`. The Miri task
//!   passes its per-crate settings as an explicit `MIRIFLAGS`, which only wins
//!   while that entry is not forced; with `force`, every run falls back to the
//!   permissive default and the leak detection the gate exists for is gone.
//! - A sanitizer target pulls in `mimalloc`. The sanitizers intercept the system
//!   allocator, and a custom global allocator sits in front of every one of those
//!   interceptors, so the heap checks degrade to nothing while the job stays
//!   green.
//!
//! See `docs/src/development/testing.md`.

use anyhow::{Result, bail};
use toml::Value;

use crate::tasks::{
    miri::{CONFIG_BASELINE_FLAGS, TARGETS as MIRI_TARGETS},
    sanitize::TARGETS as SANITIZER_TARGETS,
};

const CARGO_CONFIG: &str = ".cargo/config.toml";
const WORKSPACE_MANIFEST: &str = "Cargo.toml";

pub fn check() -> Result<()> {
    check_at(&super::Repo::discover()?)
}

/// The gate, rooted at an injected repository.
pub fn check_at(repo: &super::Repo) -> Result<()> {
    let config = repo.read(CARGO_CONFIG)?;
    let manifest = repo.read(WORKSPACE_MANIFEST)?;

    let mut failures = config_failures(&config);
    let members = workspace_members(&manifest);
    failures.extend(mismatched_members(
        &members,
        "miri",
        MIRI_TARGETS.iter().map(|t| t.package),
    ));
    failures.extend(mismatched_members(
        &members,
        "sanitize",
        SANITIZER_TARGETS.iter().map(|t| t.package),
    ));

    for package in MIRI_TARGETS.iter().map(|target| target.package) {
        failures.extend(missing_manifest(repo, package, "miri"));
    }
    for package in SANITIZER_TARGETS.iter().map(|target| target.package) {
        failures.extend(missing_manifest(repo, package, "sanitize"));
        failures.extend(global_allocator_conflict(repo, package));
    }

    if failures.is_empty() {
        return Ok(());
    }
    bail!(
        "the nightly tool gates cannot run as configured (see docs/src/development/testing.md):\n{}",
        failures.join("\n")
    )
}

/// Disagreements between `.cargo/config.toml` and what `cargo xtask miri`
/// assumes about it.
fn config_failures(config: &str) -> Vec<String> {
    let parsed: Value = match toml::from_str(config) {
        Ok(parsed) => parsed,
        Err(error) => {
            return vec![format!(
                "{CARGO_CONFIG} is not valid TOML, so the Miri defaults cannot be checked: {error}"
            )];
        }
    };

    let Some(entry) = parsed.get("env").and_then(|env| env.get("MIRIFLAGS")) else {
        return vec![format!(
            "{CARGO_CONFIG} no longer sets `[env] MIRIFLAGS`; a bare `cargo +nightly miri test` \
             then fails on the executor threads the harness leaves alive"
        )];
    };

    let (value, forced) = match entry {
        Value::String(value) => (value.as_str(), false),
        Value::Table(table) => (
            table
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            table.get("force").and_then(Value::as_bool).unwrap_or(false),
        ),
        _ => {
            return vec![format!(
                "{CARGO_CONFIG} sets `[env] MIRIFLAGS` to something other than a string or a \
                 {{ value, force }} table"
            )];
        }
    };

    let mut failures = Vec::new();
    if forced {
        failures.push(format!(
            "{CARGO_CONFIG} sets `MIRIFLAGS` with `force = true`; that makes it win over the \
             per-crate flags `cargo xtask miri` passes, so leak detection and isolation would \
             silently follow the config instead"
        ));
    }

    let words: Vec<&str> = value.split_whitespace().collect();
    for flag in CONFIG_BASELINE_FLAGS {
        if !words.contains(flag) {
            failures.push(format!(
                "{CARGO_CONFIG} drops `{flag}` from `[env] MIRIFLAGS`; the default the Miri gate \
                 overrides has to keep the permissive baseline it documents"
            ));
        }
    }

    failures
}

/// The workspace members declared by the root manifest.
fn workspace_members(manifest: &str) -> Vec<String> {
    let parsed: Value = match toml::from_str(manifest) {
        Ok(parsed) => parsed,
        Err(_) => return Vec::new(),
    };

    parsed
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(Value::as_array)
        .map_or_else(Vec::new, |members| {
            members
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
}

/// One failure per package a task lists that the workspace no longer declares.
fn mismatched_members<'a>(
    members: &[String],
    task: &str,
    packages: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    packages
        .map(|package| format!("crates/{package}"))
        .filter(|directory| !members.iter().any(|member| member == directory))
        .map(|directory| {
            format!(
                "`cargo xtask {task}` lists `{directory}`, but the workspace manifest does not \
                 declare it"
            )
        })
        .collect()
}

fn missing_manifest(repo: &super::Repo, package: &str, task: &str) -> Vec<String> {
    let manifest = format!("crates/{package}/Cargo.toml");
    if repo.path(&manifest).is_file() {
        return Vec::new();
    }
    vec![format!(
        "`cargo xtask {task}` lists the crate `{package}`, but {manifest} does not exist"
    )]
}

/// Fails when a sanitizer target brings `mimalloc` into the test binary.
///
/// The sanitizers intercept the system allocator. A crate that installs
/// `mimalloc` as its global allocator allocates and frees through mimalloc
/// instead, so the interceptors see nothing and the heap checks stop applying.
fn global_allocator_conflict(repo: &super::Repo, package: &str) -> Vec<String> {
    let manifest = format!("crates/{package}/Cargo.toml");
    match repo.read(&manifest) {
        Ok(contents) if contents.contains("mimalloc") => vec![format!(
            "`cargo xtask sanitize` lists the crate `{package}`, which pulls in `mimalloc`; a \
             custom global allocator bypasses the sanitizer's allocator interceptors, so the \
             heap checks would report nothing"
        )],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{config_failures, mismatched_members, workspace_members};
    use crate::tasks::{miri, sanitize};

    const GOOD_CONFIG: &str = r#"
[env]
MIRIFLAGS = { value = "-Zmiri-ignore-leaks -Zmiri-disable-isolation", force = false }
"#;

    /// A workspace manifest that declares exactly the curated crates, so a
    /// change to either list keeps this fixture honest.
    fn manifest_with_every_target() -> String {
        let mut manifest = String::from("[workspace]\nmembers = [\n");
        for package in miri::TARGETS.iter().map(|target| target.package) {
            manifest.push_str(&format!("    \"crates/{package}\",\n"));
        }
        for package in sanitize::TARGETS.iter().map(|target| target.package) {
            manifest.push_str(&format!("    \"crates/{package}\",\n"));
        }
        manifest.push_str("]\n");
        manifest
    }

    #[test]
    fn a_permissive_documented_default_passes() {
        assert!(config_failures(GOOD_CONFIG).is_empty());
        let members = workspace_members(&manifest_with_every_target());
        assert!(
            mismatched_members(
                &members,
                "miri",
                miri::TARGETS.iter().map(|target| target.package)
            )
            .is_empty()
        );
        assert!(
            mismatched_members(
                &members,
                "sanitize",
                sanitize::TARGETS.iter().map(|target| target.package)
            )
            .is_empty()
        );
    }

    #[test]
    fn a_forced_entry_fails_because_the_task_flags_would_lose() {
        let config = r#"
[env]
MIRIFLAGS = { value = "-Zmiri-ignore-leaks", force = true }
"#;
        let failures = config_failures(config);
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("force = true")),
            "unexpected failures: {failures:?}"
        );
    }

    #[test]
    fn a_missing_entry_fails() {
        let failures = config_failures("[env]\n");
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("no longer sets")),
            "unexpected failures: {failures:?}"
        );
    }

    #[test]
    fn a_dropped_baseline_flag_fails() {
        let config = r#"
[env]
MIRIFLAGS = { value = "-Zmiri-ignore-leaks", force = false }
"#;
        let failures = config_failures(config);
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("-Zmiri-disable-isolation")),
            "unexpected failures: {failures:?}"
        );
    }

    #[test]
    fn a_plain_string_entry_is_accepted() {
        let config = "[env]\nMIRIFLAGS = \"-Zmiri-ignore-leaks -Zmiri-disable-isolation\"\n";
        assert!(config_failures(config).is_empty());
    }

    #[test]
    fn undeclared_workspace_members_are_reported_per_task() {
        let failures = mismatched_members(
            &[],
            "sanitize",
            sanitize::TARGETS.iter().map(|target| target.package),
        );
        assert_eq!(
            failures.len(),
            sanitize::TARGETS.len(),
            "every curated crate should be reported: {failures:?}"
        );
        for package in sanitize::TARGETS.iter().map(|target| target.package) {
            assert!(
                failures.iter().any(|failure| failure.contains(package)),
                "{package} is not reported: {failures:?}"
            );
        }
    }
}
