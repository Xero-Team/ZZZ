//! Requires a crate-level `clippy.toml` to repeat the root configuration.
//!
//! Clippy reads the single `clippy.toml` nearest the crate it is linting and
//! does not merge it with the one at the repository root, which is why
//! `crates/git/clippy.toml` repeats the workspace entries verbatim before
//! adding its own git-specific bans. Nothing enforced that repetition: when the
//! root gained a disallowed method, the copy kept compiling cleanly and
//! `cargo xtask clippy` still reported success while every source file under
//! that crate escaped the new ban. This gate compares each copy against the
//! root, so drift fails here instead of being silent.

use anyhow::{Context as _, Result, bail};
use toml::Value;

const ROOT_CONFIG: &str = "clippy.toml";

pub fn check() -> Result<()> {
    check_at(&super::Repo::discover()?)
}

/// The gate, rooted at an injected repository.
pub fn check_at(repo: &super::Repo) -> Result<()> {
    let root = parse(&repo.read(ROOT_CONFIG)?, ROOT_CONFIG)?;

    let mut findings = Vec::new();
    for member in super::lints_inheritance::workspace_members(repo)? {
        let path = format!("{member}/{ROOT_CONFIG}");
        if !repo.path(&path).is_file() {
            continue;
        }
        let copy = parse(&repo.read(&path)?, &path)?;
        findings.extend(
            missing_settings(&root, &copy)
                .into_iter()
                .map(|finding| format!("  - {path}: {finding}")),
        );
    }

    if findings.is_empty() {
        return Ok(());
    }
    bail!(
        "crate-level {ROOT_CONFIG} files use only their own settings, so they must repeat every setting in the root {ROOT_CONFIG}:\n{}",
        findings.join("\n")
    )
}

fn parse(source: &str, path: &str) -> Result<Value> {
    toml::from_str(source).with_context(|| format!("failed to parse {path}"))
}

/// Returns the root settings `copy` does not reproduce, one message each.
///
/// A copy may add settings of its own; only what the root declares is
/// required to appear identically.
fn missing_settings(root: &Value, copy: &Value) -> Vec<String> {
    let Some(root) = root.as_table() else {
        return vec!["the root configuration is not a table".to_owned()];
    };
    let Some(copy) = copy.as_table() else {
        return vec!["not a table".to_owned()];
    };

    let mut findings = Vec::new();
    for (key, expected) in root {
        let Some(actual) = copy.get(key) else {
            findings.push(format!("missing `{key}`"));
            continue;
        };
        match expected.as_array() {
            Some(expected) => findings.extend(missing_items(key, expected, actual)),
            None if actual != expected => {
                findings.push(format!("`{key}` differs from the root configuration"));
            }
            None => {}
        }
    }
    findings
}

/// Returns the elements of the root array `expected` that `actual` lacks.
///
/// Elements carrying a `path` are matched by that path, so the finding names
/// the disallowed method or type instead of reprinting a whole entry. Every
/// other element is compared as a whole value.
fn missing_items(key: &str, expected: &[Value], actual: &Value) -> Vec<String> {
    let Some(actual) = actual.as_array() else {
        return vec![format!("`{key}` is not an array")];
    };

    let mut findings = Vec::new();
    for item in expected {
        match entry_path(item) {
            Some(path) => match actual
                .iter()
                .find(|candidate| entry_path(candidate) == Some(path))
            {
                None => findings.push(format!("`{key}` is missing `{path}`")),
                Some(candidate) if candidate != item => {
                    findings.push(format!("`{key}` entry `{path}` differs from the root"));
                }
                Some(_) => {}
            },
            None if !actual.contains(item) => {
                findings.push(format!("`{key}` is missing {item}"));
            }
            None => {}
        }
    }
    findings
}

fn entry_path(value: &Value) -> Option<&str> {
    value.get("path")?.as_str()
}

#[cfg(test)]
mod tests {
    use super::{missing_settings, parse};

    fn findings(root: &str, copy: &str) -> Vec<String> {
        missing_settings(
            &parse(root, "root").expect("root fixture should parse"),
            &parse(copy, "copy").expect("copy fixture should parse"),
        )
    }

    #[test]
    fn an_identical_copy_reports_nothing() {
        let root = "msrv = \"1.100.0\"\navoid-breaking-exported-api = false\n";
        assert!(findings(root, root).is_empty());
    }

    #[test]
    fn a_missing_scalar_is_reported() {
        let root = "msrv = \"1.100.0\"\nallow-private-module-inception = true\n";
        let copy = "allow-private-module-inception = true\n";
        assert_eq!(findings(root, copy), vec!["missing `msrv`"]);
    }

    #[test]
    fn a_changed_scalar_is_reported() {
        let root = "msrv = \"1.100.0\"\n";
        let copy = "msrv = \"1.90.0\"\n";
        assert_eq!(
            findings(root, copy),
            vec!["`msrv` differs from the root configuration"]
        );
    }

    #[test]
    fn a_missing_disallowed_method_is_named_by_path() {
        let root = "disallowed-methods = [\n    { path = \"smol::Timer::after\", reason = \"test timers\" },\n]\n";
        let copy = "disallowed-methods = []\n";
        assert_eq!(
            findings(root, copy),
            vec!["`disallowed-methods` is missing `smol::Timer::after`"]
        );
    }

    #[test]
    fn a_changed_disallowed_method_is_reported() {
        let root = "disallowed-methods = [\n    { path = \"smol::Timer::after\", reason = \"test timers\" },\n]\n";
        let copy = "disallowed-methods = [\n    { path = \"smol::Timer::after\", reason = \"something else\" },\n]\n";
        assert_eq!(
            findings(root, copy),
            vec!["`disallowed-methods` entry `smol::Timer::after` differs from the root"]
        );
    }

    #[test]
    fn a_copy_may_add_its_own_entries() {
        let root = "disallowed-methods = [\n    { path = \"smol::Timer::after\", reason = \"test timers\" },\n]\n";
        let copy = "disallowed-methods = [\n    { path = \"smol::Timer::after\", reason = \"test timers\" },\n    { path = \"smol::process::Command::new\", reason = \"crate-local\" },\n]\n";
        assert!(findings(root, copy).is_empty());
    }

    #[test]
    fn a_missing_array_element_is_reported() {
        let root = "ignore-interior-mutability = [\"agent_ui::context::AgentContextKey\"]\n";
        let copy = "ignore-interior-mutability = []\n";
        assert_eq!(
            findings(root, copy),
            vec!["`ignore-interior-mutability` is missing \"agent_ui::context::AgentContextKey\""]
        );
    }

    #[test]
    fn a_non_array_copy_of_an_array_setting_is_reported() {
        let root = "ignore-interior-mutability = [\"a\"]\n";
        let copy = "ignore-interior-mutability = \"a\"\n";
        assert_eq!(
            findings(root, copy),
            vec!["`ignore-interior-mutability` is not an array"]
        );
    }

    #[test]
    fn an_empty_root_requires_nothing() {
        assert!(findings("", "msrv = \"1.100.0\"\n").is_empty());
    }
}
