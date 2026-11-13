//! Requires every disabled test to say why it is disabled.
//!
//! A bare `#[ignore]` is an unfinished test with no owner: the reader cannot
//! tell a platform gap from a flake from a feature that never landed, and
//! nothing prompts a later session to revisit it. Every ignored test in the
//! workspace already carries a reason string, so this gate holds that bar
//! rather than introducing one. See `docs/src/development/testing.md`.
//!
//! `#[should_panic]` is reported but not rejected: it is a legitimate shape
//! for a test that pins a documented panic, and the count is small enough to
//! keep visible without a gate.

use anyhow::{Result, bail};

/// Matches the attribute only when it carries no reason string.
const BARE_IGNORE: &str = r"#\[ignor[e]\]";

/// The attribute as it appears in source. Assembled from two pieces so this
/// file does not match the gate it implements.
const BARE_IGNORE_ATTRIBUTE: &str = concat!("#[ignor", "e]");

pub fn check() -> Result<()> {
    check_at(&super::Repo::discover()?)
}

/// The gate, rooted at an injected repository.
pub fn check_at(repo: &super::Repo) -> Result<()> {
    let matches = repo.grep(&[
        "--no-color",
        "--line-number",
        "--extended-regexp",
        "-e",
        BARE_IGNORE,
        "--",
        "*.rs",
        ":(exclude)vendor/**",
        ":(exclude)crates/grammars/vendor/**",
    ])?;

    let findings = bare_ignore_findings(&matches);
    report_should_panic(repo)?;

    if findings.is_empty() {
        return Ok(());
    }
    bail!(
        "ignored tests must say why: use `#[ignore = \"reason\"]` so the next reader knows what would un-ignore it (see docs/src/development/testing.md):\n{}",
        findings.join("\n")
    )
}

/// Every `#[ignore]` that is code, not a doc comment.
///
/// `git grep` output is `path:line:content`, and the content may contain
/// colons of its own, so only the first two separators are consumed. A match
/// inside a comment is documentation about the attribute, not a use of it.
fn bare_ignore_findings(matches: &str) -> Vec<String> {
    matches
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter(|line| {
            let content = line.splitn(3, ':').nth(2).unwrap_or(line);
            match content.find(BARE_IGNORE_ATTRIBUTE) {
                Some(offset) => !content[..offset].contains("//"),
                None => false,
            }
        })
        .map(str::to_owned)
        .collect()
}

fn report_should_panic(repo: &super::Repo) -> Result<()> {
    let matches = repo.grep(&[
        "--no-color",
        "--line-number",
        "--extended-regexp",
        "-e",
        r"#\[should_panic",
        "--",
        "*.rs",
        ":(exclude)vendor/**",
        ":(exclude)crates/grammars/vendor/**",
    ])?;
    let count = matches
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    if count > 0 {
        println!("note: {count} tests use #[should_panic]");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{BARE_IGNORE_ATTRIBUTE, bare_ignore_findings};

    #[test]
    fn a_bare_attribute_is_reported() {
        let matches = format!("crates/foo/src/lib.rs:12:    {BARE_IGNORE_ATTRIBUTE}\n");
        assert_eq!(
            bare_ignore_findings(&matches),
            vec![matches.trim().to_owned()]
        );
    }

    #[test]
    fn a_documented_reason_passes() {
        let with_reason = BARE_IGNORE_ATTRIBUTE.replacen(']', " = \"needs a GPU\"]", 1);
        let matches = format!("crates/foo/src/lib.rs:12:    {with_reason}\n");
        assert!(bare_ignore_findings(&matches).is_empty(), "{matches}");
    }

    #[test]
    fn a_doc_comment_mentioning_the_attribute_is_not_a_use() {
        let matches = format!(
            "crates/foo/src/lib.rs:17://! Tests are marked `{BARE_IGNORE_ATTRIBUTE}` by default.\n"
        );
        assert!(bare_ignore_findings(&matches).is_empty(), "{matches}");
    }

    #[test]
    fn blank_git_grep_output_is_empty() {
        assert!(bare_ignore_findings("").is_empty());
        assert!(bare_ignore_findings("\n").is_empty());
    }

    #[test]
    fn findings_from_several_files_are_kept_in_order() {
        let matches = format!("a.rs:1:{0}\nb.rs:2:{0}\n", BARE_IGNORE_ATTRIBUTE);
        assert_eq!(bare_ignore_findings(&matches).len(), 2);
    }

    #[test]
    fn a_line_without_separators_is_reported_rather_than_dropped() {
        let matches = format!("{BARE_IGNORE_ATTRIBUTE}\n");
        assert_eq!(bare_ignore_findings(&matches).len(), 1);
    }
}
