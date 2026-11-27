//! Caps how large a hand-written source file may grow.
//!
//! `.rules` asks for a new logical component rather than another module inside a
//! file that already holds too many, and review alone could not hold that line:
//! the workspace carries hundreds of files past the size where a file stops
//! being one idea. A soft cap on new files plus a baseline that only goes down
//! turns the rule into something a machine enforces, and makes growing an
//! already-large file a deliberate, reviewed edit instead of a side effect.
//!
//! The cap counts every line, including doc comments, because those are what
//! makes a large file expensive to read. Test modules are out of scope: they
//! live in `*_tests.rs`, `*_test.rs`, and `tests/`, which this gate skips
//! entirely, so a fixture-heavy test file never competes for the same budget as
//! the code it covers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

/// The most lines a file may hold without a baseline entry.
///
/// A file at this size is still one idea; past it, a reader scanning for a
/// symbol has to keep two hierarchies in their head, and two changes to
/// different halves of the file conflict in review for no reason.
const SOFT_LIMIT: usize = 800;

/// The baseline: one `path<TAB>ceiling` row per file that is already over
/// [`SOFT_LIMIT`]. A ceiling is the file's line count when it was recorded, and
/// raising one is a reviewed decision recorded in the diff.
const BASELINE: &str = "tooling/xtask/src/tasks/check/file_size_baseline.tsv";

/// The tracked directories whose `.rs` files the gate counts.
const ROOTS: &[&str] = &["crates/", "tooling/", "extensions/"];

pub fn check() -> Result<()> {
    check_at(&super::Repo::discover()?)
}

/// The gate, rooted at an injected repository.
pub fn check_at(repo: &super::Repo) -> Result<()> {
    let ceilings = parse_baseline(&repo.read(BASELINE)?)?;
    let mut files = sourced_files(repo)?;
    files.sort();

    let mut findings = Vec::new();

    for path in &files {
        let relative = path.to_string_lossy().replace('\\', "/");
        let lines = count_lines(&repo.path(&relative))?;
        let ceiling = ceilings.get(&relative).copied();

        match ceiling {
            Some(ceiling) if lines > ceiling => findings.push(format!(
                "{relative} grew to {lines} lines, past its recorded ceiling of {ceiling}; \
                 split it, or raise the ceiling in {BASELINE} as its own reviewed change"
            )),
            Some(_) => {}
            None if lines > SOFT_LIMIT => findings.push(format!(
                "{relative} is {lines} lines and has no baseline entry; the limit for a file \
                 without one is {SOFT_LIMIT}, so split it"
            )),
            None => {}
        }
    }

    for path in ceilings.keys() {
        if !files
            .iter()
            .any(|file| file.to_string_lossy() == path.as_str())
        {
            findings.push(format!(
                "{BASELINE} lists {path}, which is not a tracked source file; remove the entry"
            ));
        }
    }

    if findings.is_empty() {
        return Ok(());
    }
    findings.sort();
    bail!(
        "source files are larger than the repository allows:\n{}",
        findings.join("\n")
    );
}

/// The `.rs` files the gate counts: hand-written sources under [`ROOTS`].
fn sourced_files(repo: &super::Repo) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for root in ROOTS {
        for path in repo.tracked(root)? {
            if is_counted_source(&path) {
                files.push(path);
            }
        }
    }
    Ok(files)
}

/// Whether a tracked path is a hand-written source file rather than a vendored
/// snapshot, a test, or something that is not Rust at all.
fn is_counted_source(path: &Path) -> bool {
    if path.extension().is_none_or(|extension| extension != "rs") {
        return false;
    }
    let normalized = path.to_string_lossy().replace('\\', "/");
    if normalized.starts_with("vendor/") || normalized.contains("/vendor/") {
        return false;
    }
    if normalized.contains("/tests/") {
        return false;
    }
    !(normalized.ends_with("_tests.rs") || normalized.ends_with("_test.rs"))
}

/// The number of lines in `path`, counted the way an editor's gutter does.
fn count_lines(path: &Path) -> Result<usize> {
    let contents = std::fs::read_to_string(path)
        .map_err(|error| anyhow::anyhow!("failed to read {}: {error}", path.display()))?;
    Ok(contents.lines().count())
}

/// Parses the baseline, rejecting rows that cannot mean anything.
fn parse_baseline(source: &str) -> Result<BTreeMap<String, usize>> {
    let mut ceilings = BTreeMap::new();
    for (index, line) in source.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let number = index + 1;
        let Some((path, ceiling)) = line.split_once('\t') else {
            bail!("{BASELINE}:{number} is not `path<TAB>ceiling`: {line:?}");
        };
        if path.is_empty() || path.starts_with('/') || path.contains("..") {
            bail!("{BASELINE}:{number} does not name a repository-relative path: {path:?}");
        }
        let ceiling: usize = ceiling.parse().map_err(|error| {
            anyhow::anyhow!("{BASELINE}:{number} has an unparsable ceiling {ceiling:?}: {error}")
        })?;
        if ceiling <= SOFT_LIMIT {
            bail!(
                "{BASELINE}:{number} records {path} at {ceiling} lines, which is not over the \
                 {SOFT_LIMIT}-line limit; remove the entry"
            );
        }
        if ceilings.insert(path.to_owned(), ceiling).is_some() {
            bail!("{BASELINE}:{number} repeats {path}");
        }
    }
    Ok(ceilings)
}

#[cfg(test)]
mod tests {
    use super::{BASELINE, SOFT_LIMIT, is_counted_source, parse_baseline};
    use std::path::Path;

    #[test]
    fn a_baseline_row_needs_a_tab_separator() {
        let error = parse_baseline("crates/a/src/a.rs 900\n").expect_err("no tab must fail");
        assert!(error.to_string().contains("path<TAB>ceiling"), "{error}");
    }

    #[test]
    fn a_ceiling_at_or_below_the_limit_is_stale() {
        let row = format!("crates/a/src/a.rs\t{SOFT_LIMIT}\n");
        let error = parse_baseline(&row).expect_err("a ceiling under the limit must fail");
        assert!(error.to_string().contains("not over the"), "{error}");
    }

    #[test]
    fn a_repeated_path_is_rejected() {
        let rows = "crates/a/src/a.rs\t900\ncrates/a/src/a.rs\t950\n";
        let error = parse_baseline(rows).expect_err("a duplicate must fail");
        assert!(error.to_string().contains("repeats"), "{error}");
    }

    #[test]
    fn paths_escaping_the_repository_are_rejected() {
        for path in ["/etc/passwd", "../outside.rs"] {
            let row = format!("{path}\t900\n");
            assert!(parse_baseline(&row).is_err(), "{path} must not be accepted");
        }
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let rows = "# a comment\n\ncrates/a/src/a.rs\t900\n";
        let ceilings = parse_baseline(rows).expect("the baseline should parse");
        assert_eq!(ceilings.get("crates/a/src/a.rs"), Some(&900));
    }

    #[test]
    fn the_baseline_path_stays_inside_the_check_module() {
        assert!(BASELINE.starts_with("tooling/xtask/src/tasks/check/"));
        assert!(
            Path::new(BASELINE)
                .extension()
                .is_some_and(|ext| ext == "tsv")
        );
    }

    #[test]
    fn tests_and_vendored_sources_are_not_counted() {
        for path in [
            "crates/editor/src/editor_tests.rs",
            "crates/editor/src/editor_test.rs",
            "crates/project/tests/integration/project_tests.rs",
            "vendor/font-kit/src/font.rs",
            "crates/grammars/vendor/tree-sitter-rust/src/parser.c",
            "docs/src/index.md",
        ] {
            assert!(
                !is_counted_source(Path::new(path)),
                "{path} must be skipped"
            );
        }
        for path in [
            "crates/editor/src/editor.rs",
            "tooling/xtask/src/tasks/check.rs",
            "extensions/foo/src/lib.rs",
        ] {
            assert!(is_counted_source(Path::new(path)), "{path} must be counted");
        }
    }
}
