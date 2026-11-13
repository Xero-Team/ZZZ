//! Locates the repository root and reads files relative to it.
//!
//! Gates used to resolve paths against the process working directory. That is
//! fine in production, where `cargo xtask check` runs from the root, and
//! untestable everywhere else: a test cannot change the working directory
//! without racing every other test in the binary.
//!
//! `Repo` is the injected root. Production code discovers it once per run;
//! tests build a temporary tree and point a `Repo` at it, so a gate can be
//! exercised end to end against fixture files instead of only through its
//! pure helpers.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};

pub struct Repo {
    root: PathBuf,
}

impl Repo {
    /// The repository that contains the working directory.
    pub fn discover() -> Result<Self> {
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .context("failed to run git rev-parse")?;
        if !output.status.success() {
            bail!(
                "not inside a git repository: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let root = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if root.is_empty() {
            bail!("git rev-parse reported an empty repository root");
        }
        Ok(Self {
            root: PathBuf::from(root),
        })
    }

    /// A repository rooted at `root`, used by fixture tests.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    pub fn read(&self, relative: &str) -> Result<String> {
        let path = self.path(relative);
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))
    }

    /// Expands a `git ls-files` pathspec inside the repository.
    ///
    /// The command exits 0 with empty output when nothing matches, including
    /// when the pathspec is misspelled or the index is empty. Every caller
    /// treats "no findings" as success, so an empty result fails closed.
    pub fn tracked(&self, pathspec: &str) -> Result<Vec<PathBuf>> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["ls-files", pathspec])
            .output()
            .context("failed to run git ls-files")?;
        if !output.status.success() {
            bail!(
                "git ls-files failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        parse_tracked_files(&String::from_utf8_lossy(&output.stdout), pathspec)
    }

    /// Runs `git grep` inside the repository and returns its output.
    ///
    /// Returns an empty string when nothing matched, which is `git grep`'s
    /// exit code 1.
    pub fn grep(&self, arguments: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .arg("grep")
            .args(arguments)
            .output()
            .context("failed to run git grep")?;

        match output.status.code() {
            Some(0) => Ok(String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_owned()),
            Some(1) => Ok(String::new()),
            _ => bail!(
                "git grep failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        }
    }
}

fn parse_tracked_files(stdout: &str, pathspec: &str) -> Result<Vec<PathBuf>> {
    let files: Vec<PathBuf> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(PathBuf::from)
        .collect();
    if files.is_empty() {
        bail!(
            "git ls-files {pathspec:?} matched no tracked files; the pathspec may be wrong, or the git index empty or unreadable"
        );
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::parse_tracked_files;

    #[test]
    fn empty_git_ls_files_output_fails_closed() {
        let error = parse_tracked_files("", "*.rs").expect_err("empty output must fail");
        assert!(
            error.to_string().contains("matched no tracked files"),
            "unexpected error: {error}"
        );
        assert!(
            error.to_string().contains("*.rs"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn whitespace_only_output_fails_closed() {
        assert!(parse_tracked_files("\n", "*.rs").is_err());
    }

    #[test]
    fn collects_every_line_as_a_path() {
        use std::path::PathBuf;
        let files = parse_tracked_files("a.rs\nb/c.rs\n", "*.rs").expect("paths should parse");
        assert_eq!(files, vec![PathBuf::from("a.rs"), PathBuf::from("b/c.rs")]);
    }
}
