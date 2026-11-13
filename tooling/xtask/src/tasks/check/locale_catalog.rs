//! Rejects structural damage to the locale catalogs that the other locale
//! gates cannot see.
//!
//! `check-locale-keys`, `check-locale-fallbacks`, and `check-locale-placeholders`
//! all parse a catalog into a map, which silently keeps the last of two
//! identical JSON keys. A duplicate key therefore looks like a clean tree to
//! every one of them (and to the runtime `i18n::load_catalog`). Keymaps already
//! reject repeated keys with a `NoDuplicateKeys` visitor; this gate applies the
//! same rule to the catalogs, and adds the remaining parse-time invariants:
//! every entry key is a dot-separated lowercase identifier and every value is
//! a non-blank string. All of these were clean across both catalogs when the
//! gate was added.

use std::collections::BTreeMap;

use anyhow::{Result, bail};
use serde::Deserialize;

use super::keymaps::NoDuplicateKeys;

const EN_CATALOG: &str = "assets/locales/en.json";
const ZH_CATALOG: &str = "assets/locales/zh-CN.json";

#[derive(Deserialize)]
struct LocaleDocument {
    entries: BTreeMap<String, String>,
}

pub fn check() -> Result<()> {
    check_at(&super::Repo::discover()?)
}

/// The gate, rooted at an injected repository.
pub fn check_at(repo: &super::Repo) -> Result<()> {
    let mut findings = Vec::new();
    for path in [EN_CATALOG, ZH_CATALOG] {
        let source = repo.read(path)?;
        findings.extend(check_source(path, &source));
    }

    if findings.is_empty() {
        return Ok(());
    }
    bail!(
        "locale catalog structure is invalid (reject duplicate keys, non-conforming keys, and blank values):\n{}",
        findings.join("\n")
    )
}

/// Returns `file: problem` findings for one catalog. Kept free of file IO so
/// the rules are testable without depending on the test runner's directory.
fn check_source(path: &str, source: &str) -> Vec<String> {
    let mut findings = Vec::new();

    if let Err(error) = serde_json::from_str::<NoDuplicateKeys>(source) {
        findings.push(format!("{path}: {error}"));
    }

    match serde_json::from_str::<LocaleDocument>(source) {
        Ok(document) => {
            for (key, value) in &document.entries {
                if !is_valid_key(key) {
                    findings.push(format!(
                        "{path}: key `{key}` is not a dot-separated lowercase identifier"
                    ));
                }
                if value.trim().is_empty() {
                    findings.push(format!("{path}: value for `{key}` is blank"));
                }
            }
        }
        Err(error) => findings.push(format!(
            "{path}: failed to parse as a string catalog: {error}"
        )),
    }

    findings
}

/// Catalog keys are flat namespaces like `menu.app` or `editor.git.count_one`.
fn is_valid_key(key: &str) -> bool {
    let segments: Vec<&str> = key.split('.').collect();
    segments.len() >= 2
        && segments.iter().all(|segment| {
            !segment.is_empty()
                && segment.chars().all(|character| {
                    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
                })
        })
}

#[cfg(test)]
mod tests {
    use super::{check_source, is_valid_key};

    const PATH: &str = "catalog.json";

    fn findings(source: &str) -> Vec<String> {
        check_source(PATH, source)
    }

    #[test]
    fn accepts_a_well_formed_catalog() {
        let source = r#"{"entries": {"menu.app": "ZZZ", "editor.count_one": "{} item"}}"#;
        assert!(findings(source).is_empty(), "{:?}", findings(source));
    }

    #[test]
    fn rejects_a_duplicate_key() {
        let source = r#"{"entries": {"menu.app": "first", "menu.app": "second"}}"#;
        let result = findings(source);
        assert_eq!(result.len(), 1, "{result:?}");
        assert!(result[0].contains("duplicate key"), "{result:?}");
    }

    #[test]
    fn rejects_a_non_conforming_key() {
        for key in ["Menu.App", "single", "a..b", "has space.here"] {
            let source = format!(r#"{{"entries": {{"{key}": "value"}}}}"#);
            assert!(
                findings(&source)
                    .iter()
                    .any(|finding| finding.contains("dot-separated")),
                "key {key:?} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_a_blank_value() {
        let source = r#"{"entries": {"menu.app": "   "}}"#;
        assert!(
            findings(source)
                .iter()
                .any(|finding| finding.contains("is blank")),
            "{:?}",
            findings(source)
        );
    }

    #[test]
    fn rejects_a_non_string_value() {
        let source = r#"{"entries": {"menu.app": 42}}"#;
        assert!(
            findings(source)
                .iter()
                .any(|finding| finding.contains("string catalog")),
            "{:?}",
            findings(source)
        );
    }

    #[test]
    fn key_format_requires_at_least_two_segments() {
        assert!(is_valid_key("menu.app"));
        assert!(is_valid_key("a.b_c9"));
        assert!(!is_valid_key("app"));
    }
}
