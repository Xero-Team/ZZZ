//! Requires every workspace member to opt into `[workspace.lints]`.
//!
//! `[workspace.lints]` denies dozens of rustc and Clippy lints, but a crate
//! only inherits them when its own manifest contains `[lints] workspace =
//! true`. A newly added member that forgets the opt-in silently escapes the
//! whole deny set while `cargo xtask clippy` still reports success for the
//! workspace. An isolated probe confirms the behaviour: an opt-in crate fails
//! to compile under `unsafe_code = "deny"`, while the same crate without the
//! opt-in compiles cleanly. Nothing enforced the opt-in before this gate.

use anyhow::{Context as _, Result, bail};
use toml::Value;

const WORKSPACE_MANIFEST: &str = "Cargo.toml";

pub fn check() -> Result<()> {
    check_at(&super::Repo::discover()?)
}

/// The gate, rooted at an injected repository.
pub fn check_at(repo: &super::Repo) -> Result<()> {
    let members = workspace_members(repo)?;
    if members.is_empty() {
        bail!(
            "{WORKSPACE_MANIFEST} lists no workspace members; the lint-inheritance gate would scan nothing"
        );
    }

    let mut inheritance = Vec::with_capacity(members.len());
    for member in &members {
        let manifest_path = format!("{member}/Cargo.toml");
        let source = repo.read(&manifest_path)?;
        let parsed: Value =
            toml::from_str(&source).with_context(|| format!("failed to parse {manifest_path}"))?;
        inheritance.push((member.clone(), opts_in(&parsed)));
    }

    let missing = missing_lints(&inheritance);
    if missing.is_empty() {
        return Ok(());
    }
    bail!(
        "workspace members do not inherit `[workspace.lints]` (add `[lints] workspace = true`, or the crate silently escapes every denied lint):\n{}",
        missing
            .iter()
            .map(|member| format!("  - {member}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn workspace_members(repo: &super::Repo) -> Result<Vec<String>> {
    let source = repo.read(WORKSPACE_MANIFEST)?;
    parse_members(&source)
}

/// Kept free of file IO so a missing or malformed member list is testable.
fn parse_members(source: &str) -> Result<Vec<String>> {
    let parsed: Value =
        toml::from_str(source).with_context(|| format!("failed to parse {WORKSPACE_MANIFEST}"))?;
    let members = parsed
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(Value::as_array)
        .with_context(|| format!("{WORKSPACE_MANIFEST} has no `workspace.members` array"))?;
    let mut names = Vec::with_capacity(members.len());
    for member in members {
        let name = member.as_str().with_context(|| {
            format!("{WORKSPACE_MANIFEST} has a non-string workspace member: {member:?}")
        })?;
        names.push(name.to_owned());
    }
    Ok(names)
}

fn opts_in(parsed: &Value) -> bool {
    parsed
        .get("lints")
        .and_then(|lints| lints.get("workspace"))
        .and_then(Value::as_bool)
        == Some(true)
}

fn missing_lints(inheritance: &[(String, bool)]) -> Vec<&str> {
    inheritance
        .iter()
        .filter(|(_, opts_in)| !opts_in)
        .map(|(member, _)| member.as_str())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{missing_lints, opts_in, parse_members};
    use toml::Value;

    fn parse(source: &str) -> Value {
        toml::from_str(source).expect("fixture should parse")
    }

    #[test]
    fn reads_the_member_list() {
        let members = parse_members("[workspace]\nmembers = [\"crates/foo\", \"crates/bar\"]\n")
            .expect("members should parse");
        assert_eq!(members, vec!["crates/foo", "crates/bar"]);
    }

    #[test]
    fn rejects_a_missing_or_malformed_member_list() {
        assert!(parse_members("[workspace]\nresolver = \"2\"\n").is_err());
        assert!(parse_members("this is not toml = = =").is_err());
        assert!(parse_members("[workspace]\nmembers = [1, 2]\n").is_err());
    }

    #[test]
    fn an_empty_member_list_is_returned_for_check_to_fail_closed() {
        assert!(
            parse_members("[workspace]\nmembers = []\n")
                .expect("empty array is valid toml")
                .is_empty()
        );
    }

    #[test]
    fn detects_the_lints_opt_in() {
        assert!(opts_in(&parse("[lints]\nworkspace = true\n")));
        assert!(!opts_in(&parse("[package]\nname = \"foo\"\n")));
        assert!(!opts_in(&parse("[lints]\nworkspace = false\n")));
        assert!(!opts_in(&parse("[lints]\nworkspace = \"true\"\n")));
    }

    #[test]
    fn reports_only_members_without_the_opt_in() {
        let inheritance = vec![
            ("crates/a".to_owned(), true),
            ("crates/b".to_owned(), false),
            ("crates/c".to_owned(), true),
        ];
        assert_eq!(missing_lints(&inheritance), vec!["crates/b"]);
    }

    #[test]
    fn passes_when_every_member_opts_in() {
        let inheritance = vec![("crates/a".to_owned(), true), ("crates/b".to_owned(), true)];
        assert!(missing_lints(&inheritance).is_empty());
    }
}
