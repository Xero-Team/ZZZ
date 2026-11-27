//! Requires every lint suppression to say why it exists.
//!
//! `.rules` asks for `#[expect(lint, reason = "...")]` rather than a bare
//! `#[allow]`, because an expectation that stops matching fails the build. The
//! reason is what makes the failure actionable: without it the next reader
//! knows a suppression exists but not what it protects. Nothing enforced that
//! half of the rule.
//!
//! The gate checks the reason, not the level. `#[cfg_attr(<cfg>, allow(...))]`
//! has to stay `allow` (an `expect` fails the build on the targets where the
//! lint never fires), and proc-macro crates emit attributes into crates where
//! `expect` would be checked against the wrong tree. Plain `#[allow]` sites are
//! counted and reported, so the remaining `allow` surface stays visible without
//! blocking a change.
//!
//! Suppressions that predate the rule and whose justification cannot be
//! recovered from the code are listed in `lint_suppression_baseline.tsv`. The
//! baseline is shrink-only: a row whose suppression disappears fails the gate,
//! so the file cannot rot. Anything not in the baseline needs a `reason`.

use std::collections::BTreeMap;

use anyhow::{Context as _, Result, bail};

/// Suppressions that predate the rule, as `path<TAB>lint<TAB>count` rows.
const BASELINE: &str = include_str!("lint_suppression_baseline.tsv");

const BASELINE_PATH: &str = "tooling/xtask/src/tasks/check/lint_suppression_baseline.tsv";

/// Only these trees are scanned. Vendored sources keep their upstream style.
const CHECKED_ROOTS: &[&str] = &["crates/", "tooling/"];

/// Path fragments that are never checked.
const EXCLUDED_FRAGMENTS: &[&str] = &["/vendor/", "/target/"];

pub fn check() -> Result<()> {
    check_at(&super::Repo::discover()?)
}

/// The gate, rooted at an injected repository.
pub fn check_at(repo: &super::Repo) -> Result<()> {
    let baseline = parse_baseline(BASELINE)?;
    let files = repo.tracked("*.rs")?;
    let mut failures = Vec::new();
    let mut unreasoned: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut unkeyed = Vec::new();
    let mut plain_allows = 0;

    for path in &files {
        let relative = path.to_string_lossy().replace('\\', "/");
        if !is_checked(&relative) {
            continue;
        }
        let source = repo
            .read(&relative)
            .with_context(|| format!("failed to read {relative}"))?;
        for attribute in suppressions(&source) {
            if attribute.has_reason {
                if attribute.kind == Kind::Allow && !attribute.is_conditional {
                    plain_allows += 1;
                }
                continue;
            }
            match attribute.baseline_key(&relative) {
                Some(key) => unreasoned.entry(key).or_default().push(attribute.line),
                None => unkeyed.push(format!(
                    "  {relative}:{}: {}",
                    attribute.line, attribute.text
                )),
            }
        }
    }

    failures.extend(unkeyed);

    for (key, lines) in &unreasoned {
        let (path, lint) = key
            .split_once('\t')
            .expect("baseline keys hold two columns");
        match baseline.get(key) {
            Some(expected) if *expected == lines.len() => {}
            Some(expected) => failures.push(format!(
                "  {path}: {lines:?} carry {lint} without a reason; {BASELINE_PATH} expects {expected}, found {}",
                lines.len()
            )),
            None => {
                for line in lines {
                    failures.push(format!("  {path}:{line}: {lint} has no reason"));
                }
            }
        }
    }

    for (key, expected) in &baseline {
        if !unreasoned.contains_key(key) {
            failures.push(format!(
                "  {BASELINE_PATH}: row `{}` expects {expected} suppressions and matches none; delete it",
                key.replace('\t', " ")
            ));
        }
    }

    if plain_allows > 0 {
        println!(
            "note: {plain_allows} plain `#[allow]` suppressions could be `#[expect]` once the lint is known to fire on every target"
        );
    }

    if failures.is_empty() {
        return Ok(());
    }
    failures.sort();
    bail!(
        "lint suppressions must carry `reason = \"...\"`, so the next reader knows what the suppression protects:\n{}",
        failures.join("\n")
    )
}

fn is_checked(relative: &str) -> bool {
    if EXCLUDED_FRAGMENTS
        .iter()
        .any(|fragment| relative.contains(fragment))
    {
        return false;
    }
    if is_proc_macro_crate(relative) {
        return false;
    }
    CHECKED_ROOTS.iter().any(|root| relative.starts_with(root))
}

/// Whether the path belongs to a crate whose name ends in `_macros`.
fn is_proc_macro_crate(relative: &str) -> bool {
    let mut parts = relative.split('/');
    let (Some(_root), Some(name), Some(_module)) = (parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    name.ends_with("_macros")
}

fn parse_baseline(source: &str) -> Result<BTreeMap<String, usize>> {
    let mut rows = BTreeMap::new();
    for (index, line) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let mut columns = line.split('\t');
        let (Some(path), Some(lint), Some(count), None) = (
            columns.next(),
            columns.next(),
            columns.next(),
            columns.next(),
        ) else {
            bail!(
                "{BASELINE_PATH}:{line_number}: expected `<path>`, `<lint>`, and `<count>` separated by tabs"
            );
        };
        let (path, lint) = (path.trim(), lint.trim());
        if path.is_empty() || lint.is_empty() {
            bail!("{BASELINE_PATH}:{line_number}: the path and lint columns must be filled in");
        }
        let count: usize = count.trim().parse().with_context(|| {
            format!("{BASELINE_PATH}:{line_number}: the count column must be a number")
        })?;
        if rows.insert(format!("{path}\t{lint}"), count).is_some() {
            bail!("{BASELINE_PATH}:{line_number}: {path} {lint} is listed twice");
        }
    }
    Ok(rows)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Allow,
    Expect,
    /// `#[cfg_attr(<cfg>, allow(...))]` and its `expect` variant.
    Conditional,
}

struct Suppression {
    line: usize,
    kind: Kind,
    /// Whether the suppression sits behind a `cfg_attr`, which forces `allow`.
    is_conditional: bool,
    has_reason: bool,
    /// The lint the suppression names, when it could be read.
    lint: Option<String>,
    text: String,
}

impl Suppression {
    /// The baseline row that excuses this suppression, if it can be keyed.
    fn baseline_key(&self, relative: &str) -> Option<String> {
        self.lint.as_ref().map(|lint| format!("{relative}\t{lint}"))
    }
}

/// Every lint suppression in the source, in source order.
///
/// The scan runs over a copy of the source with string literals masked, because
/// this crate's error message and its test fixtures spell out attributes as
/// text. Offsets are identical between the two, so the reported text still comes
/// from the original.
fn suppressions(source: &str) -> Vec<Suppression> {
    let masked = super::literals::mask(source);
    let mut found = Vec::new();
    let mut offset = 0;
    while let Some(start) = masked[offset..].find("#[") {
        let attribute_start = offset + start;
        let after_bracket = attribute_start + "#[".len();
        let Some(name) = identifier_at(&masked, after_bracket) else {
            offset = attribute_start + 1;
            continue;
        };
        let name_end = after_bracket + name.len();
        let Some(open) = masked[name_end..].find('(') else {
            offset = name_end;
            continue;
        };
        let open = name_end + open;
        let Some(close) = closing_paren(&masked, open) else {
            offset = name_end;
            continue;
        };
        let arguments = &masked[open..=close];
        let kind = match name {
            "allow" => Some(Kind::Allow),
            "expect" => Some(Kind::Expect),
            "cfg_attr" if contains_lint_level(arguments) => Some(Kind::Conditional),
            _ => None,
        };
        if let Some(kind) = kind
            && !is_in_comment(&masked, attribute_start)
        {
            found.push(Suppression {
                line: masked[..attribute_start].matches('\n').count() + 1,
                kind,
                is_conditional: kind == Kind::Conditional,
                has_reason: has_reason(arguments),
                lint: lint_name(kind, arguments),
                text: compact(&source[attribute_start..=close]),
            });
        }
        offset = attribute_start + 1;
    }
    found
}

fn identifier_at(source: &str, offset: usize) -> Option<&str> {
    let rest = source.get(offset..)?;
    let length = rest
        .chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .map(char::len_utf8)
        .sum::<usize>();
    (length > 0).then(|| &rest[..length])
}

/// The offset of the `)` that closes the `(` at `open`.
fn closing_paren(source: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    for (offset, character) in source[open..].char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether a `cfg_attr` body carries an `allow` or `expect` argument.
fn contains_lint_level(text: &str) -> bool {
    text.contains("allow(") || text.contains("expect(")
}

/// Whether the attribute text carries `reason = "..."`.
fn has_reason(text: &str) -> bool {
    let mut offset = 0;
    while let Some(found) = text[offset..].find("reason") {
        let at = offset + found;
        let after = &text[at + "reason".len()..];
        let before_is_boundary = text[..at]
            .chars()
            .next_back()
            .is_none_or(|character| !is_identifier_char(character));
        if before_is_boundary && after.trim_start().starts_with('=') {
            return true;
        }
        offset = at + "reason".len();
    }
    false
}

/// The first lint a suppression names.
///
/// A `cfg_attr` body starts with its condition, so the lint is read from the
/// inner `allow(...)` or `expect(...)` instead of the outer argument list.
fn lint_name(kind: Kind, arguments: &str) -> Option<String> {
    let after_level = if kind == Kind::Conditional {
        let open = arguments
            .find("allow(")
            .or_else(|| arguments.find("expect("))?;
        &arguments[open + "allow(".len()..]
    } else {
        &arguments[1..]
    };
    let name: String = after_level
        .trim_start()
        .chars()
        .take_while(|character| {
            character.is_alphanumeric() || *character == ':' || *character == '_'
        })
        .collect();
    (!name.is_empty()).then_some(name)
}

fn is_identifier_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Whether the match is inside a `//` comment on its own line.
fn is_in_comment(source: &str, offset: usize) -> bool {
    let line_start = source[..offset]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    source[line_start..offset].contains("//")
}

fn compact(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{Kind, is_checked, lint_name, parse_baseline, suppressions};

    fn texts(source: &str) -> Vec<(usize, Kind, bool, String)> {
        suppressions(source)
            .into_iter()
            .map(|found| (found.line, found.kind, found.has_reason, found.text))
            .collect()
    }

    #[test]
    fn a_bare_allow_is_reported_without_a_reason() {
        let found = texts("#[allow(dead_code)]\nfn helper() {}\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, 1);
        assert_eq!(found[0].1, Kind::Allow);
        assert!(!found[0].2);
    }

    #[test]
    fn a_reason_satisfies_the_gate() {
        let found = texts("#[expect(dead_code, reason = \"kept for tests\")]\nfn helper() {}\n");
        assert_eq!(found.len(), 1);
        assert!(found[0].2);
    }

    #[test]
    fn a_multi_line_attribute_is_read_to_its_closing_paren() {
        let source = "#[expect(\n    clippy::type_complexity,\n    reason = \"nested lookup\",\n)]\nstruct Lookup;\n";
        let found = texts(source);
        assert_eq!(found.len(), 1);
        assert!(found[0].2);
        assert_eq!(found[0].0, 1);
    }

    #[test]
    fn a_reason_word_without_an_equals_sign_does_not_count() {
        let found = texts("#[allow(dead_code, reason_json)]\n");
        assert!(!found[0].2);
    }

    #[test]
    fn a_conditional_suppression_is_found() {
        let found =
            texts("#[cfg_attr(not(target_os = \"linux\"), allow(unused))]\nfn helper() {}\n");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1, Kind::Conditional);
        assert!(!found[0].2);
    }

    #[test]
    fn a_cfg_attr_without_a_lint_level_is_ignored() {
        assert!(texts("#[cfg_attr(test, derive(Debug))]\nstruct Value;\n").is_empty());
    }

    #[test]
    fn a_suppression_named_in_a_line_comment_is_ignored() {
        assert!(texts("let value = 1; // use #[allow(dead_code)] instead\n").is_empty());
        assert!(texts("//! Suppress with #[allow(dead_code)].\n").is_empty());
    }

    #[test]
    fn a_suppression_named_in_a_string_literal_is_ignored() {
        assert!(texts("let note = \"#[allow(dead_code)] is banned here\";\n").is_empty());
        assert!(texts("let query = r#\"#[expect(unused)]\"#;\n").is_empty());
    }

    #[test]
    fn a_line_number_counts_from_the_attribute() {
        let found = texts("fn a() {}\n\n#[allow(unused)]\nfn b() {}\n");
        assert_eq!(found[0].0, 3);
    }

    #[test]
    fn the_lint_name_is_read_from_a_plain_and_a_conditional_attribute() {
        assert_eq!(
            lint_name(Kind::Allow, "(clippy::empty_enums)").as_deref(),
            Some("clippy::empty_enums")
        );
        assert_eq!(
            lint_name(Kind::Conditional, "(not(test), allow(dead_code))").as_deref(),
            Some("dead_code")
        );
        assert_eq!(lint_name(Kind::Expect, "()"), None);
    }

    #[test]
    fn the_baseline_parses_rows_and_skips_comments() {
        let rows = parse_baseline("# note\n\ncrates/gpui/src/scene.rs\tmissing_docs\t25\n")
            .expect("valid baseline");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows["crates/gpui/src/scene.rs\tmissing_docs"], 25);
    }

    #[test]
    fn a_baseline_row_without_a_count_is_rejected() {
        let error = parse_baseline("crates/gpui/src/scene.rs\tmissing_docs\n")
            .expect_err("missing count must fail");
        assert!(error.to_string().contains("separated by tabs"), "{error}");
    }

    #[test]
    fn a_duplicate_baseline_row_is_rejected() {
        let error = parse_baseline("a.rs\tdead_code\t1\na.rs\tdead_code\t2\n")
            .expect_err("duplicate must fail");
        assert!(error.to_string().contains("twice"), "{error}");
    }

    #[test]
    fn vendored_and_proc_macro_paths_are_skipped() {
        assert!(is_checked("crates/rope/src/rope.rs"));
        assert!(!is_checked("vendor/any_vec/src/lib.rs"));
        assert!(!is_checked(
            "crates/grammars/vendor/tree-sitter-x/src/parser.rs"
        ));
        assert!(!is_checked("crates/util_macros/src/util_macros.rs"));
        assert!(!is_checked(
            "crates/gpui_macros/tests/derive_inspector_reflection.rs"
        ));
    }
}
