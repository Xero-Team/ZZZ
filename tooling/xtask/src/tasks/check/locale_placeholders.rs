//! Requires the en and zh-CN catalogs to agree on the format placeholders in
//! every translated value, and to use well-formed Rust-format syntax.
//!
//! `check locale-keys` compares only the key sets, and `check locale-fallbacks`
//! reads the in-code `tr` fallbacks. Neither looks at the catalog *values*. A
//! translation that drops or adds a Rust-format placeholder (`{}`, `{0}`,
//! `{name}`, `{:?}`) shifts every later argument passed through
//! `replacen`/`format!`, so the mismatch must fail the gate.
//!
//! Escaped braces (`{{` and `}}`) are literals, not placeholders. Comparing
//! sorted multisets rather than sequences lets a translator reorder named or
//! positional arguments without tripping the gate.
//!
//! A value whose en and zh sides are *both* malformed (an unclosed `{`, a
//! stray `}`, or a nested `{`) would compare equal and slip through a pure
//! parity check, so every value is also syntax-checked on its own first.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use serde::Deserialize;

const EN_CATALOG: &str = "assets/locales/en.json";
const ZH_CATALOG: &str = "assets/locales/zh-CN.json";

#[derive(Deserialize)]
struct LocaleDocument {
    entries: BTreeMap<String, String>,
}

pub fn check() -> Result<()> {
    let en = load(EN_CATALOG)?;
    let zh = load(ZH_CATALOG)?;
    let mut findings = check_syntax(EN_CATALOG, &en.entries);
    findings.extend(check_syntax(ZH_CATALOG, &zh.entries));
    findings.extend(find_mismatches(&en.entries, &zh.entries));

    if findings.is_empty() {
        return Ok(());
    }
    bail!(
        "i18n catalog values have malformed placeholders or disagree across catalogs (keep `{{}}`/`{{name}}` well-formed and identical):\n{}",
        findings.join("\n")
    )
}

fn load(path: &str) -> Result<LocaleDocument> {
    let contents =
        fs::read_to_string(Path::new(path)).with_context(|| format!("failed to read {path}"))?;
    serde_json::from_str(&contents).with_context(|| format!("failed to parse {path}"))
}

/// Checks every value of one catalog on its own, so a syntax error is caught
/// even when both catalogs happen to contain the same broken placeholder.
fn check_syntax(catalog: &str, entries: &BTreeMap<String, String>) -> Vec<String> {
    let mut findings = Vec::new();
    for (key, value) in entries {
        if let Err(problem) = placeholders(value) {
            findings.push(format!(
                "{catalog}: key `{key}` has a malformed placeholder: {problem}"
            ));
        }
    }
    findings
}

/// Key-set parity is the `locale-keys` gate's job, so a key present in only one
/// catalog is skipped here. A value with malformed syntax is already reported by
/// `check_syntax`; that key is skipped from the comparison to avoid double noise.
fn find_mismatches(en: &BTreeMap<String, String>, zh: &BTreeMap<String, String>) -> Vec<String> {
    let mut findings = Vec::new();
    for (key, en_value) in en {
        let Some(zh_value) = zh.get(key) else {
            continue;
        };
        let (Ok(en_slots), Ok(zh_slots)) = (placeholders(en_value), placeholders(zh_value)) else {
            continue;
        };
        if en_slots != zh_slots {
            findings.push(format!(
                "{key}: {EN_CATALOG} has {en_slots:?}, {ZH_CATALOG} has {zh_slots:?}"
            ));
        }
    }
    findings
}

/// Returns the sorted Rust-format placeholders, or the first syntax problem.
/// `{{` and `}}` are escaped literal braces and do not count as placeholders.
fn placeholders(value: &str) -> Result<Vec<String>, &'static str> {
    let bytes = value.as_bytes();
    let mut slots = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'{' if bytes.get(index + 1) == Some(&b'{') => index += 2,
            b'{' => match value[index + 1..].find('}') {
                Some(end) => {
                    let inner = &value[index + 1..index + 1 + end];
                    if inner.contains('{') {
                        return Err("a placeholder contains a nested `{`");
                    }
                    slots.push(inner.to_owned());
                    index += end + 2;
                }
                None => return Err("an unclosed `{` has no matching `}`"),
            },
            b'}' if bytes.get(index + 1) == Some(&b'}') => index += 2,
            b'}' => return Err("a stray `}` is not escaped as `}}`"),
            _ => index += 1,
        }
    }
    slots.sort();
    Ok(slots)
}

#[cfg(test)]
mod tests {
    use super::{BTreeMap, check_syntax, find_mismatches, placeholders};

    fn catalog(values: &[(&str, &str)]) -> BTreeMap<String, String> {
        values
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn slots(value: &str) -> Vec<String> {
        placeholders(value).expect("test value should be well formed")
    }

    #[test]
    fn extracts_format_placeholders() {
        assert_eq!(slots("Show {} more"), vec![String::new()]);
        assert_eq!(slots("{0} / {1}"), vec!["0", "1"]);
        assert_eq!(slots("Hi {name} {:?}"), vec![":?", "name"]);
    }

    #[test]
    fn ignores_escaped_braces() {
        assert!(slots("literal {{ and }}").is_empty());
        // The stash prompt mixes one placeholder with escaped literal braces.
        assert_eq!(
            slots("{} stash@{{{}}}?"),
            vec![String::new(), String::new()]
        );
    }

    #[test]
    fn rejects_malformed_placeholder_syntax() {
        assert!(placeholders("unclosed { here").is_err());
        assert!(placeholders("stray } here").is_err());
        assert!(placeholders("nested {a{b} here").is_err());
    }

    #[test]
    fn reports_a_malformed_value_in_a_single_catalog() {
        // Both sides being malformed would compare equal and slip through a
        // pure parity check, so each value is scanned on its own.
        let en = catalog(&[("k", "unclosed {")]);
        let findings = check_syntax("en.json", &en);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].contains("malformed"), "{findings:?}");
    }

    #[test]
    fn reports_a_dropped_placeholder() {
        let en = catalog(&[("k", "{} items in {name}")]);
        let zh = catalog(&[("k", "{} 个项目")]);
        let findings = find_mismatches(&en, &zh);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].starts_with("k:"), "{findings:?}");
    }

    #[test]
    fn allows_reordered_named_placeholders() {
        let en = catalog(&[("k", "{first} then {second}")]);
        let zh = catalog(&[("k", "{second} 然后 {first}")]);
        assert!(find_mismatches(&en, &zh).is_empty());
    }

    #[test]
    fn ignores_keys_missing_from_one_catalog() {
        let en = catalog(&[("k", "{}")]);
        let zh = catalog(&[("other", "x")]);
        assert!(find_mismatches(&en, &zh).is_empty());
    }
}
