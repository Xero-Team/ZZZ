//! Keeps the crates allowed to contain `unsafe` an enumerable, reviewed set.
//!
//! `.rules` requires a `// SAFETY:` argument for every `unsafe` block, but the
//! workspace keeps no record of *which* crates may contain `unsafe` at all, so
//! a crate can take its first `unsafe` block and nothing notices except a
//! reviewer who happens to read that file. The 1,324 undocumented blocks
//! already in the tree make a blanket `#![forbid(unsafe_code)]` impossible, so
//! this gate draws the boundary around the crates instead of the blocks.
//!
//! The ledger is one row per crate that may contain `unsafe`, with the FFI
//! boundary in the reason column. A crate outside the ledger that gains its
//! first `unsafe` fails, and so does a ledger row whose crate no longer has
//! any: the list shrinks with the code instead of rotting.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, Result, bail};

/// The reviewed set of crates that may contain `unsafe`.
const LEDGER: &str = include_str!("unsafe_crates.tsv");

const LEDGER_PATH: &str = "tooling/xtask/src/tasks/check/unsafe_crates.tsv";

/// Path fragments that never count towards a crate's `unsafe` surface.
///
/// Vendored sources are not workspace members, but they can sit inside a
/// member's directory (`crates/grammars/vendor`), so the prefix match in
/// `member_of` alone is not enough to keep them out.
const EXCLUDED_FRAGMENTS: &[&str] = &["/vendor/", "/target/"];

pub fn check() -> Result<()> {
    check_at(&super::Repo::discover()?)
}

/// The gate, rooted at an injected repository.
pub fn check_at(repo: &super::Repo) -> Result<()> {
    let listed = parse_ledger(LEDGER)?;
    let members = super::lints_inheritance::workspace_members(repo)?;
    let files = repo.tracked("*.rs")?;

    let mut with_unsafe = BTreeSet::new();
    for path in &files {
        let relative = path.to_string_lossy().replace('\\', "/");
        if EXCLUDED_FRAGMENTS
            .iter()
            .any(|fragment| relative.contains(fragment))
        {
            continue;
        }
        let Some(member) = member_of(&relative, &members) else {
            continue;
        };
        let source = repo
            .read(&relative)
            .with_context(|| format!("failed to read {relative}"))?;
        if has_unsafe_item(&source) {
            with_unsafe.insert(member.to_owned());
        }
    }

    verify(&listed, &members, &with_unsafe)
}

/// The workspace member that contains `relative`, if any.
///
/// Members are matched longest first so a member nested inside another one
/// cannot be attributed to its parent.
fn member_of<'a>(relative: &str, members: &'a [String]) -> Option<&'a str> {
    members
        .iter()
        .filter(|member| {
            relative
                .strip_prefix(member.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
        })
        .max_by_key(|member| member.len())
        .map(String::as_str)
}

/// Parses the ledger into `crate -> reason`.
///
/// Blank lines and `#` comments are skipped. A row without a tab separator or
/// without a reason is rejected: the reason is the whole point of the row.
fn parse_ledger(source: &str) -> Result<BTreeMap<String, String>> {
    let mut listed = BTreeMap::new();
    for (index, line) in source.lines().enumerate() {
        let line_number = index + 1;
        // Only the line terminator is trimmed: trimming trailing whitespace
        // would also strip the tab that separates an empty reason from its
        // crate, turning "no reason" into "no separator".
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let Some((crate_name, reason)) = line.split_once('\t') else {
            bail!(
                "{LEDGER_PATH}:{line_number}: expected `<crate>` and `<reason>` separated by a tab"
            );
        };
        let crate_name = crate_name.trim();
        let reason = reason.trim();
        if crate_name.is_empty() {
            bail!("{LEDGER_PATH}:{line_number}: the crate column is empty");
        }
        if reason.is_empty() {
            bail!(
                "{LEDGER_PATH}:{line_number}: {crate_name} has no reason; name the FFI boundary it crosses"
            );
        }
        if listed
            .insert(crate_name.to_owned(), reason.to_owned())
            .is_some()
        {
            bail!("{LEDGER_PATH}:{line_number}: {crate_name} is listed twice");
        }
    }
    Ok(listed)
}

/// Compares the ledger against what the tree actually contains.
fn verify(
    listed: &BTreeMap<String, String>,
    members: &[String],
    with_unsafe: &BTreeSet<String>,
) -> Result<()> {
    let mut failures = Vec::new();

    for member in with_unsafe {
        if !listed.contains_key(member) {
            failures.push(format!(
                "  - {member} contains `unsafe` but has no row in {LEDGER_PATH}; \
                 either remove the `unsafe`, or add a row naming the FFI boundary it crosses"
            ));
        }
    }

    for crate_name in listed.keys() {
        if !members.contains(crate_name) {
            failures.push(format!(
                "  - {crate_name} has a row in {LEDGER_PATH} but is not a workspace member"
            ));
        } else if !with_unsafe.contains(crate_name) {
            failures.push(format!(
                "  - {crate_name} has a row in {LEDGER_PATH} but contains no `unsafe`; delete the row"
            ));
        }
    }

    if failures.is_empty() {
        return Ok(());
    }
    failures.sort();
    bail!(
        "the `unsafe` surface ledger and the tree disagree:\n{}",
        failures.join("\n")
    )
}

/// Whether the source contains an `unsafe` block, function, impl, trait, or
/// extern block.
///
/// Comment lines and string literals are skipped so prose and query strings
/// that spell out `unsafe fn` do not demand a ledger row, and `unsafe` is
/// matched as a word so `unsafely` and `#[unsafe(...)]` attributes do not
/// count: neither introduces an unchecked memory operation.
///
/// A raw string that spans several lines is masked one line at a time, so it
/// still counts. That direction is deliberate: a false positive costs one
/// ledger row that names the fixture, while a false negative would hide real
/// `unsafe`.
fn has_unsafe_item(source: &str) -> bool {
    source.lines().any(|line| {
        if line.trim_start().starts_with("//") {
            return false;
        }
        let masked = mask_string_literals(line);
        let mut rest = masked.as_str();
        while let Some(index) = rest.find("unsafe") {
            let before_is_boundary = rest[..index]
                .chars()
                .next_back()
                .is_none_or(|character| !is_identifier_character(character));
            let after = &rest[index + "unsafe".len()..];
            if before_is_boundary && ends_word(after) && starts_unsafe_item(after) {
                return true;
            }
            rest = after;
        }
        false
    })
}

/// Replaces the contents of string literals on one line with spaces.
///
/// Handles `"..."` with escapes, `r"..."`, `r#"..."#` with any number of
/// hashes, and the byte-string forms of both. A `'` is only treated as a char
/// literal delimiter when it closes within a few characters, so lifetimes like
/// `&'a str` are left alone.
fn mask_string_literals(line: &str) -> String {
    let characters: Vec<char> = line.chars().collect();
    let mut masked = String::with_capacity(line.len());
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        if character == '"' {
            masked.push('"');
            index = mask_quoted(&characters, index + 1, &mut masked);
            continue;
        }
        if character == 'r'
            && let Some(hashes) = raw_string_hashes(&characters, index + 1)
        {
            masked.push('r');
            for _ in 0..hashes {
                masked.push('#');
            }
            masked.push('"');
            index = mask_raw(&characters, index + 1 + hashes + 1, hashes, &mut masked);
            continue;
        }
        if character == '\'' && is_char_literal(&characters, index) {
            masked.push('\'');
            masked.push(' ');
            let closing = char_literal_end(&characters, index);
            index = closing;
            continue;
        }
        masked.push(character);
        index += 1;
    }
    masked
}

fn raw_string_hashes(characters: &[char], index: usize) -> Option<usize> {
    let mut hashes = 0;
    let mut cursor = index;
    while characters.get(cursor) == Some(&'#') {
        hashes += 1;
        cursor += 1;
    }
    (characters.get(cursor) == Some(&'"')).then_some(hashes)
}

fn mask_quoted(characters: &[char], from: usize, masked: &mut String) -> usize {
    let mut index = from;
    while index < characters.len() {
        match characters[index] {
            '\\' => {
                masked.push(' ');
                index += 1;
                if index < characters.len() {
                    masked.push(' ');
                    index += 1;
                }
            }
            '"' => {
                masked.push('"');
                return index + 1;
            }
            _ => {
                masked.push(' ');
                index += 1;
            }
        }
    }
    index
}

fn mask_raw(characters: &[char], from: usize, hashes: usize, masked: &mut String) -> usize {
    let mut index = from;
    while index < characters.len() {
        if characters[index] == '"' {
            let closes = (1..=hashes).all(|offset| characters.get(index + offset) == Some(&'#'));
            if closes {
                masked.push('"');
                for _ in 0..hashes {
                    masked.push('#');
                }
                return index + hashes + 1;
            }
        }
        masked.push(' ');
        index += 1;
    }
    index
}

/// Whether the `'` at `index` opens a char literal rather than a lifetime.
fn is_char_literal(characters: &[char], index: usize) -> bool {
    char_literal_end(characters, index) > index + 1
}

fn char_literal_end(characters: &[char], index: usize) -> usize {
    match (characters.get(index + 1), characters.get(index + 2)) {
        (Some('\\'), _) => {
            if characters.get(index + 3) == Some(&'\'') {
                index + 4
            } else {
                index + 1
            }
        }
        (Some(_), Some('\'')) => index + 3,
        _ => index + 1,
    }
}

/// Whether `after` begins at a word boundary, so `unsafely` and
/// `unsafe_guard` are not read as the keyword.
fn ends_word(after: &str) -> bool {
    after
        .chars()
        .next()
        .is_none_or(|character| !is_identifier_character(character))
}

fn starts_unsafe_item(after: &str) -> bool {
    let after = after.trim_start();
    if after.starts_with('(') {
        return false;
    }
    ["{", "fn", "impl", "trait", "extern"]
        .iter()
        .any(|keyword| match after.strip_prefix(keyword) {
            Some(remaining) => ends_word(remaining),
            None => false,
        })
}

fn is_identifier_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::{has_unsafe_item, member_of, parse_ledger, verify};

    fn members(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn a_block_counts() {
        assert!(has_unsafe_item("let value = unsafe { libc::getuid() };\n"));
        assert!(has_unsafe_item("    unsafe {\n"));
    }

    #[test]
    fn a_function_impl_trait_and_extern_block_count() {
        assert!(has_unsafe_item("unsafe fn get_window_state() {}\n"));
        assert!(has_unsafe_item("unsafe impl Send for Connection {}\n"));
        assert!(has_unsafe_item("unsafe trait Marker {}\n"));
        assert!(has_unsafe_item("unsafe extern \"C\" {\n"));
    }

    #[test]
    fn prose_and_attributes_do_not_count() {
        assert!(!has_unsafe_item("// this used to be unsafe {\n"));
        assert!(!has_unsafe_item("/// Safety: never unsafe fn\n"));
        assert!(!has_unsafe_item("#[unsafe(no_mangle)]\n"));
        assert!(!has_unsafe_item("let unsafely_named = 1;\n"));
        assert!(!has_unsafe_item("fn do_unsafe() {}\n"));
    }

    #[test]
    fn a_string_literal_does_not_count() {
        assert!(!has_unsafe_item("let label: &str = \"async unsafe fn\";\n"));
        assert!(!has_unsafe_item("let label = \"unsafe impl Send\";\n"));
    }

    #[test]
    fn a_raw_string_does_not_count() {
        assert!(!has_unsafe_item("let query = r#\"unsafe { x }\"#;\n"));
        assert!(!has_unsafe_item("let query = r##\"unsafe fn\"##;\n"));
    }

    #[test]
    fn an_escaped_quote_does_not_end_the_string() {
        assert!(!has_unsafe_item("let text = \"a \\\" unsafe {\";\n"));
    }

    #[test]
    fn a_lifetime_is_not_a_char_literal() {
        assert!(has_unsafe_item(
            "fn find<'a>(value: &'a str) { let _ = unsafe { value }; }\n"
        ));
    }

    #[test]
    fn a_char_literal_containing_a_quote_does_not_open_a_string() {
        assert!(has_unsafe_item(
            "let quote = '\"'; let value = unsafe { libc::getuid() };\n"
        ));
    }

    #[test]
    fn a_suffix_identifier_does_not_count() {
        assert!(!has_unsafe_item("let unsafe_guard = 1;\n"));
        assert!(!has_unsafe_item("fn unsafe_fn_name() {}\n"));
    }

    #[test]
    fn the_ledger_parses_rows_and_skips_comments() {
        let listed = parse_ledger("# comment\n\nsqlez\tSQLite C API\nvfs-\tobjc\n").expect("valid");
        assert_eq!(listed.len(), 2);
        assert_eq!(listed["sqlez"], "SQLite C API");
    }

    #[test]
    fn a_row_without_a_reason_is_rejected() {
        let error = parse_ledger("sqlez\t\n").expect_err("empty reason must fail");
        assert!(error.to_string().contains("no reason"), "{error}");
    }

    #[test]
    fn a_row_without_a_separator_is_rejected() {
        let error = parse_ledger("sqlez SQLite\n").expect_err("missing tab must fail");
        assert!(error.to_string().contains("tab"), "{error}");
    }

    #[test]
    fn a_duplicate_row_is_rejected() {
        let error = parse_ledger("sqlez\tone\nsqlez\ttwo\n").expect_err("duplicate must fail");
        assert!(error.to_string().contains("twice"), "{error}");
    }

    #[test]
    fn an_unlisted_crate_with_unsafe_is_reported() {
        let listed = BTreeMap::new();
        let error = verify(
            &listed,
            &members(&["rope"]),
            &BTreeSet::from(["rope".to_owned()]),
        )
        .expect_err("unlisted crate must fail");
        assert!(error.to_string().contains("rope"), "{error}");
    }

    #[test]
    fn a_stale_row_is_reported() {
        let listed = BTreeMap::from([("rope".to_owned(), "rope internals".to_owned())]);
        let error = verify(&listed, &members(&["rope"]), &BTreeSet::new())
            .expect_err("stale row must fail");
        assert!(error.to_string().contains("no `unsafe`"), "{error}");
    }

    #[test]
    fn a_row_for_a_removed_member_is_reported() {
        let listed = BTreeMap::from([("gone".to_owned(), "ffi".to_owned())]);
        let error = verify(&listed, &members(&["rope"]), &BTreeSet::new())
            .expect_err("row for a non-member must fail");
        assert!(
            error.to_string().contains("not a workspace member"),
            "{error}"
        );
    }

    #[test]
    fn a_matching_ledger_passes() {
        let listed = BTreeMap::from([("sqlez".to_owned(), "SQLite C API".to_owned())]);
        verify(
            &listed,
            &members(&["sqlez", "rope"]),
            &BTreeSet::from(["sqlez".to_owned()]),
        )
        .expect("matching ledger should pass");
    }

    #[test]
    fn member_lookup_matches_whole_path_components() {
        let all = members(&["crates/rope", "crates/rope_text"]);
        assert_eq!(
            member_of("crates/rope/src/rope.rs", &all),
            Some("crates/rope")
        );
        assert_eq!(
            member_of("crates/rope_text/src/x.rs", &all),
            Some("crates/rope_text")
        );
        assert_eq!(
            member_of("crates/rope_text/src/x.rs", &members(&["crates/rope"])),
            None
        );
        assert_eq!(member_of("vendor/any_vec/src/lib.rs", &all), None);
    }

    #[test]
    fn member_lookup_prefers_the_longest_prefix() {
        let all = members(&["crates/grammars", "crates/grammars/vendor/tree-sitter-x"]);
        assert_eq!(
            member_of("crates/grammars/vendor/tree-sitter-x/src/parser.rs", &all),
            Some("crates/grammars/vendor/tree-sitter-x")
        );
    }
}
