//! Verifies that every `i18n::tr` call site in Rust sources uses a key that
//! exists in `assets/locales/en.json` and a fallback equal to its value.
//!
//! `i18n::tr` only uses the `fallback` argument when the key is missing from
//! both catalogs, so a drifted fallback is invisible at runtime and silently
//! rots. `check locale-keys` compares only the en/zh catalog *key sets*; it
//! never reads Rust sources, so neither the `Z-I18N-FB` fallback rule nor the
//! existence of in-code keys is enforced anywhere else. This gate parses the
//! call sites with `syn` and asserts that every literal key exists in
//! `en.json` and that its fallback equals the catalog value.
//!
//! A plain text search is not enough here: Rust string escapes and the
//! line-continuation `\` syntax change a literal's value, and `tr(...)` text
//! can appear inside a string literal or doc comment without being a call.
//! This gate parses with `syn` and inspects only real `tr` calls.

use std::collections::HashMap;

use anyhow::{Context as _, Result, bail};
use serde::Deserialize;
use syn::spanned::Spanned as _;
use syn::visit::{self, Visit};
use syn::{Expr, ExprCall, File, Lit};

const EN_CATALOG: &str = "assets/locales/en.json";

#[derive(Deserialize)]
struct LocaleDocument {
    entries: HashMap<String, String>,
}

pub fn check() -> Result<()> {
    check_at(&super::Repo::discover()?)
}

/// The gate, rooted at an injected repository.
pub fn check_at(repo: &super::Repo) -> Result<()> {
    let catalog = repo.read(EN_CATALOG)?;
    let document: LocaleDocument =
        serde_json::from_str(&catalog).with_context(|| format!("failed to parse {EN_CATALOG}"))?;

    let files = repo.tracked("*.rs")?;
    let mut findings = Vec::new();

    for path in &files {
        // Vendored sources are upstream snapshots, not code this gate governs.
        if path.starts_with("vendor/") || path.starts_with("crates/grammars/vendor/") {
            continue;
        }
        let source = repo.read(&path.to_string_lossy())?;
        if !might_contain_tr_call(&source) {
            continue;
        }
        // A parse failure must fail the gate, never silently skip a file.
        let file: File = syn::parse_file(&source)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        let label = path.display().to_string();
        findings.extend(scan_file(&file, &document.entries, &label));
    }

    if findings.is_empty() {
        return Ok(());
    }
    findings.sort();
    bail!(
        "in-code i18n calls disagree with {EN_CATALOG} (each key must exist in the catalog and each fallback must equal its value):\n{}",
        findings.join("\n")
    )
}

/// Returns `file:line: ...` for every `tr` call whose literal key is absent
/// from the catalog or whose literal fallback differs from the catalog value.
/// Non-literal arguments are skipped: a computed key or fallback cannot be
/// compared against a static catalog entry.
fn scan_file(file: &File, entries: &HashMap<String, String>, file_label: &str) -> Vec<String> {
    let mut visitor = FallbackVisitor {
        entries,
        file_label,
        findings: Vec::new(),
    };
    visitor.visit_file(file);
    visitor.findings
}

struct FallbackVisitor<'a> {
    entries: &'a HashMap<String, String>,
    file_label: &'a str,
    findings: Vec<String>,
}

impl<'ast> Visit<'ast> for FallbackVisitor<'_> {
    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        if is_tr_call(&call.func) {
            let arguments: Vec<&Expr> = call.args.iter().collect();
            // Every `tr` wrapper takes `(cx, key, fallback)`; tolerate a future
            // extra argument so the fallback check is not silently skipped.
            if arguments.len() >= 3
                && let Some(key) = string_literal(arguments[1])
            {
                let line = call.span().start().line;
                match self.entries.get(&key) {
                    None => self.findings.push(format!(
                        "{}:{}: key `{key}` is not present in {EN_CATALOG}",
                        self.file_label, line,
                    )),
                    Some(expected) => {
                        if let Some(fallback) = string_literal(arguments[2])
                            && fallback != *expected
                        {
                            self.findings.push(format!(
                                "{}:{}: fallback for `{key}` is {fallback:?}, but {EN_CATALOG} has {expected:?}",
                                self.file_label, line,
                            ));
                        }
                    }
                }
            }
        }
        visit::visit_expr_call(self, call);
    }
}

/// A necessary-condition prefilter: a file can only contain a `tr(...)` call
/// if the identifier text `tr` occurs in it. Narrower spellings such as
/// `tr(` or `tr (` are not sound, because Rust allows whitespace, newlines,
/// and comments between `tr` and `(`; a file skipped on one of those would
/// silently escape the fallback check even though it contains a real call.
fn might_contain_tr_call(source: &str) -> bool {
    source.contains("tr")
}

/// Matches `tr(...)`, `i18n::tr(...)`, `app_i18n::tr(...)`, and any other path
/// whose final segment is `tr`. All of these are the i18n wrappers or thin
/// re-exports of `i18n::tr`.
fn is_tr_call(func: &Expr) -> bool {
    matches!(func, Expr::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "tr"))
}

fn string_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(literal) => match &literal.lit {
            Lit::Str(value) => Some(value.value()),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{HashMap, might_contain_tr_call, scan_file};

    fn entries(values: &[(&str, &str)]) -> HashMap<String, String> {
        values
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn findings(source: &str, values: &[(&str, &str)]) -> Vec<String> {
        let file = syn::parse_file(source).expect("test source should parse");
        scan_file(&file, &entries(values), "test.rs")
    }

    #[test]
    fn prefilter_admits_every_call_spelling() {
        for source in [
            r#"fn f(cx: &App) { tr(cx, "k", "wrong"); }"#,
            r#"fn f(cx: &App) { tr (cx, "k", "wrong"); }"#,
            "fn f(cx: &App) { tr\n    (cx, \"k\", \"wrong\"); }",
            r#"fn f(cx: &App) { tr /* note */ (cx, "k", "wrong"); }"#,
        ] {
            assert!(might_contain_tr_call(source), "{source}");
            assert_eq!(
                findings(source, &[("k", "right")]).len(),
                1,
                "a drifted fallback must still be caught for {source}"
            );
        }
        assert!(!might_contain_tr_call("fn f() {}"));
    }

    #[test]
    fn reports_a_drifted_fallback() {
        let result = findings(
            r#"fn show(cx: &App) { tr(cx, "greeting", "Hi"); }"#,
            &[("greeting", "Hello")],
        );
        assert_eq!(result.len(), 1, "a drifted fallback must be reported");
        assert!(result[0].contains("test.rs:1:"), "{result:?}");
        assert!(result[0].contains("greeting"), "{result:?}");
    }

    #[test]
    fn accepts_a_matching_fallback() {
        assert!(
            findings(
                r#"fn show(cx: &App) { tr(cx, "greeting", "Hello"); }"#,
                &[("greeting", "Hello")],
            )
            .is_empty()
        );
    }

    #[test]
    fn honors_rust_line_continuation_in_a_fallback() {
        // A `\` followed by a newline and leading whitespace is stripped by
        // rustc; `LitStr::value()` reproduces that. A text-based comparison
        // would wrongly flag this valid, matching fallback.
        let source = "fn show(cx: &App) {\n    tr(\n        cx,\n        \"long.key\",\n        \"first half \\\n         second half.\",\n    );\n}";
        assert!(findings(source, &[("long.key", "first half second half.")]).is_empty());
    }

    #[test]
    fn checks_bare_and_qualified_tr_calls() {
        for source in [
            r#"fn f(cx: &App) { tr(cx, "k", "wrong"); }"#,
            r#"fn f(cx: &App) { i18n::tr(cx, "k", "wrong"); }"#,
            r#"fn f(cx: &App) { app_i18n::tr(cx, "k", "wrong"); }"#,
        ] {
            assert_eq!(findings(source, &[("k", "right")]).len(), 1, "{source}");
        }
    }

    #[test]
    fn reports_an_unknown_key() {
        let result = findings(
            r#"fn f(cx: &App) { tr(cx, "missing.key", "wrong"); }"#,
            &[("k", "right")],
        );
        assert_eq!(
            result.len(),
            1,
            "a key absent from en.json must be reported"
        );
        assert!(result[0].contains("test.rs:1:"), "{result:?}");
        assert!(result[0].contains("missing.key"), "{result:?}");
    }

    #[test]
    fn reports_an_unknown_key_even_with_a_computed_fallback() {
        // The existence check does not depend on a literal fallback: a typo in
        // the key must be caught even when the fallback is computed.
        let result = findings(
            r#"fn f(cx: &App) { tr(cx, "missing.key", DEFAULT_TITLE); }"#,
            &[("k", "right")],
        );
        assert_eq!(result.len(), 1, "{result:?}");
    }

    #[test]
    fn ignores_computed_keys_and_non_literal_fallbacks() {
        // A computed key cannot be compared statically against the catalog.
        assert!(
            findings(
                r#"fn f(cx: &App) { tr(cx, key, DEFAULT_TITLE); }"#,
                &[("k", "right")],
            )
            .is_empty()
        );
        // A key that exists but a computed fallback cannot be compared.
        assert!(
            findings(
                r#"fn f(cx: &App) { tr(cx, "k", DEFAULT_TITLE); }"#,
                &[("k", "right")],
            )
            .is_empty()
        );
    }

    #[test]
    fn ignores_tr_text_that_is_not_a_call() {
        // `tr(...)` inside a string literal or doc comment is not a call, so
        // an AST walk must not report it as a drifted fallback.
        assert!(
            findings(
                r#"
            /// Example: tr(cx, "k", "wrong").
            fn f() {
                let example = "tr(cx, \"k\", \"wrong\")";
                helper(cx, "k", "wrong");
            }
            "#,
                &[("k", "right")],
            )
            .is_empty()
        );
    }
}
