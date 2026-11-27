//! How old a PGO profile has to be before a build stops trusting it.
//!
//! The fingerprint in `plan.rs` answers "is this profile valid": same target,
//! profile, toolchain, and manifest. This module answers the other question -
//! "does it still describe this tree" - by comparing `HEAD` against the commit
//! the training run started from, which `pgo generate` records in a stamp file
//! next to the merged profile.

#![allow(
    clippy::disallowed_methods,
    reason = "the profile age is measured against git"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};

/// How many local commits a profile may trail `HEAD` before a build calls it
/// stale.
///
/// A valid profile can still describe a tree that no longer exists, and that
/// failure is silent: the build succeeds and is simply no longer optimised for
/// the code it contains. Retraining costs one instrumented build plus one
/// training run, so the allowance is generous; it exists to make "trained once,
/// never again" visible, not to force a retrain after every merge.
pub const MAX_PROFILE_AGE: u64 = 500;

/// The file `pgo generate` writes beside the merged profile, naming the commit
/// it was trained at.
const TRAINING_STAMP: &str = "training.txt";

fn training_stamp(profile: &Path) -> Option<PathBuf> {
    profile
        .parent()
        .map(|directory| directory.join(TRAINING_STAMP))
}

pub fn write_training_stamp(workspace: &Path, profile: &Path) -> Result<()> {
    let stamp = training_stamp(profile)
        .ok_or_else(|| anyhow::anyhow!("the PGO profile path has no parent directory"))?;
    let commit = git_output(workspace, &["rev-parse", "HEAD"])?;
    std::fs::write(&stamp, format!("commit {commit}\n"))
        .with_context(|| format!("failed to write {}", stamp.display()))
}

/// The commits between the training run and `HEAD`, or `None` when nothing
/// records the training commit and the profile therefore cannot be dated.
pub fn profile_age(workspace: &Path, profile: &Path) -> Option<u64> {
    let contents = std::fs::read_to_string(training_stamp(profile)?).ok()?;
    let commit = contents
        .lines()
        .find_map(|line| line.strip_prefix("commit "))?;
    git_output(
        workspace,
        &["rev-list", "--count", &format!("{commit}..HEAD")],
    )
    .ok()?
    .parse()
    .ok()
}

/// Refuses a profile the tree has outgrown.
///
/// CI trains its own profile in the same run, so a stale one there means the
/// stamp is wrong rather than the build: it warns instead of failing a release
/// over bookkeeping.
pub fn check_freshness(workspace: &Path, profile: &Path, allow_stale: bool) -> Result<()> {
    let age = profile_age(workspace, profile);
    if age.is_some_and(|age| age <= MAX_PROFILE_AGE) {
        return Ok(());
    }
    let message = match age {
        Some(age) => format!(
            "the profile at {} is {age} commits old, past the {MAX_PROFILE_AGE}-commit limit",
            profile.display()
        ),
        None => format!(
            "the profile at {} records no training commit, so its age is unknown",
            profile.display()
        ),
    };
    let remedy = format!(
        "{message}; retrain it with `cargo xtask pgo train`, or pass --stale-profile-ok to build with it anyway"
    );
    if allow_stale || std::env::var_os("CI").is_some() {
        eprintln!("warning: {remedy}");
        return Ok(());
    }
    bail!("{remedy}")
}

fn git_output(workspace: &Path, arguments: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(arguments)
        .output()
        .with_context(|| format!("failed to run git {}", arguments.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(repository: &Path, arguments: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(arguments)
            .output()
            .expect("git should run");
        assert!(
            output.status.success(),
            "git {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn commit(repository: &Path) {
        git(
            repository,
            &[
                "-c",
                "user.name=test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "--allow-empty",
                "--quiet",
                "-m",
                "a commit",
            ],
        );
    }

    #[test]
    fn the_training_stamp_sits_beside_the_profile() {
        assert_eq!(
            training_stamp(Path::new("/profiles/abc/merged.profdata")),
            Some(PathBuf::from("/profiles/abc/training.txt"))
        );
    }

    #[test]
    fn the_profile_age_counts_commits_since_training() {
        let repository = tempfile::tempdir().expect("a temporary directory");
        let root = repository.path();
        git(root, &["init", "--quiet"]);
        commit(root);
        commit(root);
        let trained = git(root, &["rev-parse", "HEAD"]);
        let profile = root.join("merged.profdata");
        std::fs::write(root.join("training.txt"), format!("commit {trained}\n"))
            .expect("the stamp should be writable");

        assert_eq!(profile_age(root, &profile), Some(0));
        commit(root);
        commit(root);
        assert_eq!(profile_age(root, &profile), Some(2));
    }

    #[test]
    fn a_profile_without_a_stamp_has_no_age() {
        let repository = tempfile::tempdir().expect("a temporary directory");
        let profile = repository.path().join("merged.profdata");
        assert_eq!(profile_age(repository.path(), &profile), None);
    }

    #[test]
    fn a_stamp_naming_an_unknown_commit_has_no_age() {
        let repository = tempfile::tempdir().expect("a temporary directory");
        let root = repository.path();
        git(root, &["init", "--quiet"]);
        commit(root);
        let profile = root.join("merged.profdata");
        std::fs::write(root.join("training.txt"), "commit 0000000000\n")
            .expect("the stamp should be writable");
        assert_eq!(profile_age(root, &profile), None);
    }

    #[test]
    fn an_accepted_stale_profile_still_builds() {
        let repository = tempfile::tempdir().expect("a temporary directory");
        let profile = repository.path().join("merged.profdata");
        check_freshness(repository.path(), &profile, true)
            .expect("--stale-profile-ok must accept an undated profile");
    }
}
