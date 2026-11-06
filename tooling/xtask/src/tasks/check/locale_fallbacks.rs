//! Verifies that every `i18n::tr` fallback in Rust sources equals its key's
//! value in `assets/locales/en.json`.
//!
//! `i18n::tr` only uses the `fallback` argument when the key is missing from
//! both catalogs, so a drifted fallback is invisible at runtime and silently
//! rots. `check locale-keys` compares the en/zh *key sets* but never looks at
//! the in-code fallback strings, leaving the `Z-I18N-FB` rule unenforced.
//!
//! A plain text search is not enough here: Rust string escapes and the
//! line-continuation `\` syntax change a literal's value, and `tr(...)` text
//! can appear inside a string literal or doc comment without being a call.
//! This gate parses with `syn` and inspects only real `tr` calls.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

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
    let catalog = fs::read_to_string(Path::new(EN_CATALOG))
        .with_context(|| format!("failed to read {EN_CATALOG}"))?;
    let document: LocaleDocument =
        serde_json::from_str(&catalog).with_context(|| format!("failed to parse {EN_CATALOG}"))?;

    let files = super::tracked_files("*.rs")?;
    let mut findings = Vec::new();

    for path in &files {
        // Vendored sources are upstream snapshots, not code this gate governs.
        if path.starts_with("vendor/") || path.starts_with("crates/grammars/vendor/") {
            continue;
        }
        let source = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        // `tr (` (with a space) and `tr(` both occur; either spelling may
        // contain a real call, so don't skip a file on a narrow text match.
        if !source.contains("tr(") && !source.contains("tr (") {
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
        "in-code i18n fallbacks drift from {EN_CATALOG} (keep each fallback identical to the en.json value):\n{}",
        findings.join("\n")
    )
}

/// Returns `file:line: ...` for every `tr` call whose fallback differs from the
/// catalog value. Unknown keys and non-literal arguments are skipped: missing
/// keys are the `locale-keys` gate's concern, and a computed fallback cannot be
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
                && let (Some(key), Some(fallback)) =
                    (string_literal(arguments[1]), string_literal(arguments[2]))
                && let Some(expected) = self.entries.get(&key)
                && fallback != *expected
            {
                self.findings.push(format!(
                    "{}:{}: fallback for `{key}` is {fallback:?}, but {EN_CATALOG} has {expected:?}",
                    self.file_label,
                    call.span().start().line,
                ));
            }
        }
        visit::visit_expr_call(self, call);
    }
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
    use super::{HashMap, scan_file};

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
    fn ignores_non_literal_and_unknown_key_calls() {
        // A computed fallback cannot be compared statically.
        assert!(
            findings(
                r#"fn f(cx: &App) { tr(cx, "k", DEFAULT_TITLE); }"#,
                &[("k", "right")],
            )
            .is_empty()
        );
        // A key absent from the catalog is the locale-keys gate's concern.
        assert!(
            findings(
                r#"fn f(cx: &App) { tr(cx, "unknown", "wrong"); }"#,
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
