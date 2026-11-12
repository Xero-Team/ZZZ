//! Repository gates that do not need a compiler.
//!
//! CI used to run these as one shell script per step. The orchestration lives
//! here now, so `cargo xtask check` is the single command contributors and CI
//! run, and the exclusion lists sit next to the code that consumes them.
//!
//! Two of the gates stay implemented as scripts: `script/check-philosophy` and
//! `script/check-upstream-ledger` are policy engines made of hundreds of
//! product-specific rules and ledger rows. They are data, not build logic, and
//! rewriting them in Rust would trade reviewable rules for code with no
//! behavioural gain. `cargo xtask check` invokes them, so they are no longer
//! part of the pipeline's surface.

#![allow(
    clippy::disallowed_methods,
    reason = "repository gates drive git and the policy scripts, and stream their output"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};
use clap::{Args, Subcommand, ValueEnum};

mod ci_wiring;
mod gpui_api;
mod keymaps;
mod lints_inheritance;
mod locale_catalog;
mod locale_fallbacks;
mod locale_placeholders;
mod module_layout;
mod msrv;
mod scroll_handlers;

#[derive(Args)]
pub struct CheckArgs {
    #[command(subcommand)]
    command: Option<CheckCommand>,
}

#[derive(Subcommand)]
enum CheckCommand {
    /// Every gate CI runs: markers, keymaps, scroll handlers, licenses,
    /// module layout, MSRV consistency, lint inheritance, locale catalog
    /// structure, keys, i18n call sites and catalog values, DCO, philosophy,
    /// ledger, workflows, CI gate wiring, the `gpui` public API snapshot,
    /// documentation formatting, and shell scripts.
    All,
    /// Reject leftover markers for unfinished work.
    Todos,
    /// Reject macOS-only modifiers in shared keymaps and duplicate bindings.
    Keys,
    /// Reject re-entrant entity/ListState access inside scroll handlers.
    ScrollHandlers,
    /// Require a `LICENSE-AGPL` symlink in every crate directory.
    Licenses,
    /// Reject new `mod.rs` module files outside vendored sources.
    ModuleLayout,
    /// Require the pinned Rust toolchain version to agree across its sources.
    Msrv,
    /// Require every workspace member to opt into `[workspace.lints]`.
    LintsInheritance,
    /// Reject duplicate keys, malformed keys, and blank values in the catalogs.
    LocaleCatalog,
    /// Require the locale catalogs to have identical key sets.
    LocaleKeys,
    /// Require in-code `i18n::tr` keys to exist and fallbacks to match the catalog.
    LocaleFallbacks,
    /// Require en/zh catalog values to agree on format placeholders.
    LocalePlaceholders,
    /// Require DCO sign-off on recent non-merge commits.
    Dco,
    /// Run ShellCheck over the scripts in the repository root's `script/`.
    Shellcheck {
        /// Minimum severity that fails the check.
        #[arg(long, value_enum, default_value = "warning")]
        severity: Severity,
    },
    /// Require `docs/` and the default settings file to match Prettier.
    Docs,
    /// Check that vendored third-party sources match their manifests.
    Vendor,
    /// Assert that rejected commercial and network surfaces stay absent.
    Philosophy,
    /// Validate the upstream rejection ledger and its allowlist.
    UpstreamLedger,
    /// Validate the `run:` patterns in the GitHub workflow files.
    Workflows,
    /// Require CI to keep invoking the repository gates.
    CiWiring,
    /// Compare the `gpui` crate's public item signatures against a snapshot.
    GpuiApi(gpui_api::GpuiApiArgs),
    /// Check documentation links with `lychee` (needs it on `PATH`).
    Links {
        /// Check remote links too instead of only local ones.
        #[arg(long)]
        all: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Severity {
    Error,
    Warning,
}

impl Severity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

pub fn run_check(args: CheckArgs) -> Result<()> {
    if !Path::new("script").is_dir() {
        bail!("cargo xtask check must be run from the workspace root");
    }

    match args.command.unwrap_or(CheckCommand::All) {
        CheckCommand::All => run_all(),
        CheckCommand::Todos => check_todos(),
        CheckCommand::Keys => keymaps::check(),
        CheckCommand::ScrollHandlers => scroll_handlers::check(),
        CheckCommand::Licenses => check_licenses(),
        CheckCommand::ModuleLayout => module_layout::check(),
        CheckCommand::Msrv => msrv::check(),
        CheckCommand::LintsInheritance => lints_inheritance::check(),
        CheckCommand::LocaleCatalog => locale_catalog::check(),
        CheckCommand::LocaleKeys => check_locale_keys(),
        CheckCommand::LocaleFallbacks => locale_fallbacks::check(),
        CheckCommand::LocalePlaceholders => locale_placeholders::check(),
        CheckCommand::Dco => check_dco(),
        CheckCommand::Shellcheck { severity } => check_shellcheck(severity),
        CheckCommand::Docs => check_docs(),
        CheckCommand::Vendor => run_script(&["script/vendor-sync", "check"], "vendor check"),
        CheckCommand::Philosophy => check_philosophy(),
        CheckCommand::UpstreamLedger => check_upstream_ledger(),
        CheckCommand::Workflows => check_workflows(),
        CheckCommand::CiWiring => ci_wiring::check(),
        CheckCommand::GpuiApi(args) => gpui_api::check(args),
        CheckCommand::Links { all } => check_links(all),
    }
}

/// Every gate `check all` runs, in reporting order.
///
/// `vendor` and `links` stay out: they need tooling the CI runner does not
/// install, and neither has a failure mode a clean Linux checkout can hide.
const GATES: &[(&str, fn() -> Result<()>)] = &[
    ("unfinished-work markers", check_todos),
    ("keymaps", keymaps::check),
    ("scroll handlers", scroll_handlers::check),
    ("crate licenses", check_licenses),
    ("module layout", module_layout::check),
    ("MSRV consistency", msrv::check),
    ("lint inheritance", lints_inheritance::check),
    ("locale catalog structure", locale_catalog::check),
    ("locale catalog keys", check_locale_keys),
    ("i18n call sites", locale_fallbacks::check),
    ("i18n catalog values", locale_placeholders::check),
    ("DCO sign-off", check_dco),
    ("philosophy boundary", check_philosophy),
    ("upstream rejection ledger", check_upstream_ledger),
    ("GitHub workflow files", check_workflows),
    ("CI gate wiring", ci_wiring::check),
    ("gpui public API snapshot", gpui_api::check_snapshot),
    ("documentation formatting", check_docs),
    ("shell scripts", check_shellcheck_default),
];

fn run_all() -> Result<()> {
    let mut failures = Vec::new();
    for (name, check) in GATES {
        println!("checking {name}");
        if let Err(error) = check() {
            eprintln!("error: {name}: {error:#}");
            failures.push(*name);
        }
    }

    if failures.is_empty() {
        println!("all {} repository checks passed", GATES.len());
        return Ok(());
    }

    bail!(
        "{} of {} checks failed: {}",
        failures.len(),
        GATES.len(),
        failures.join(", ")
    )
}

fn check_todos() -> Result<()> {
    // Brackets around a character keep the pattern out of plain-text searches.
    let matches = git_grep(&[
        "--no-color",
        "--ignore-case",
        "--line-number",
        "--extended-regexp",
        "-e",
        "tod[o]!|FIXM[E]",
        "--",
        ":(exclude).github/workflows/ci.yaml",
        ":(exclude)*criteria.md",
        ":(exclude)*prompt.md",
        ":(exclude)vendor/**",
        ":(exclude)crates/grammars/vendor/**",
    ])?;

    if matches.is_empty() {
        return Ok(());
    }
    bail!("found markers for unfinished work:\n{matches}");
}

fn check_licenses() -> Result<()> {
    let manifests = tracked_files("**/*/Cargo.toml")?;
    let mut failures = Vec::new();

    for manifest in manifests {
        let Some(directory) = manifest.parent() else {
            continue;
        };
        // Vendored third-party crates carry their own upstream licenses.
        if directory
            .components()
            .any(|component| component.as_os_str() == "vendor")
        {
            continue;
        }
        if let Err(error) = check_crate_license(directory) {
            failures.push(format!("{}: {error}", directory.display()));
        }
    }

    if failures.is_empty() {
        return Ok(());
    }
    bail!(
        "crate license files are not valid:\n{}",
        failures.join("\n")
    );
}

fn check_crate_license(directory: &Path) -> Result<()> {
    const LICENSE: &str = "LICENSE-AGPL";

    let license_path = directory.join(LICENSE);
    match fs::symlink_metadata(&license_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {}
        Ok(_) => bail!("{LICENSE} exists but is not a symlink"),
        Err(_) => bail!("missing a {LICENSE} symlink"),
    }

    let target = fs::read_link(&license_path)
        .with_context(|| format!("failed to read {}", license_path.display()))?;
    let depth = directory.components().count();
    let expected = PathBuf::from("../".repeat(depth)).join(LICENSE);
    if target != expected {
        bail!(
            "{LICENSE} points to {} but should point to {}",
            target.display(),
            expected.display()
        );
    }

    if !license_path.exists() {
        bail!("{LICENSE} is a broken symlink (its target does not exist)");
    }

    Ok(())
}

fn check_shellcheck(severity: Severity) -> Result<()> {
    let scripts = shell_scripts()?;
    if scripts.is_empty() {
        return Ok(());
    }

    let mut command = Command::new("shellcheck");
    command.args(["-x", "-S", severity.as_str(), "-C"]);
    command.args(&scripts);
    eprintln!(
        "+ shellcheck -x -S {} -C <{} files>",
        severity.as_str(),
        scripts.len()
    );
    let status = command
        .status()
        .context("failed to start shellcheck; install it with your package manager")?;
    if !status.success() {
        bail!("shellcheck failed with {status}");
    }
    Ok(())
}

/// The scripts ShellCheck covers: files directly in `script/` whose shebang
/// asks for a POSIX shell. Nested directories hold Python and JavaScript.
fn shell_scripts() -> Result<Vec<PathBuf>> {
    let mut scripts = Vec::new();
    let entries = fs::read_dir("script").context("failed to read the script directory")?;
    for entry in entries {
        let entry = entry.context("failed to read a directory entry in script/")?;
        if !entry.file_type().is_ok_and(|file_type| file_type.is_file()) {
            continue;
        }
        let path = entry.path();
        if has_shell_shebang(&path) {
            scripts.push(path);
        }
    }
    scripts.sort();
    Ok(scripts)
}

fn has_shell_shebang(path: &Path) -> bool {
    let Ok(contents) = fs::read(path) else {
        return false;
    };
    let first_line = contents.split(|byte| *byte == b'\n').next().unwrap_or(&[]);
    let line = String::from_utf8_lossy(first_line);
    let Some(shebang) = line.strip_prefix("#!") else {
        return false;
    };
    let interpreter = shebang
        .trim_start()
        .strip_prefix("/bin/")
        .or_else(|| shebang.trim_start().strip_prefix("/usr/bin/env "));
    interpreter.is_some_and(|interpreter| {
        ["sh", "bash", "dash"]
            .iter()
            .any(|shell| interpreter.starts_with(shell))
    })
}

fn check_philosophy() -> Result<()> {
    run_script(&["script/check-philosophy"], "philosophy check")
}

fn check_docs() -> Result<()> {
    run_script(&["script/prettier"], "documentation formatting check")
}

/// The severity `check all` enforces. Warnings are clean today, so the gate
/// holds that bar instead of only rejecting errors.
fn check_shellcheck_default() -> Result<()> {
    check_shellcheck(Severity::Warning)
}

fn check_locale_keys() -> Result<()> {
    run_script(&["script/check-locale-keys"], "locale catalog key check")
}

fn check_dco() -> Result<()> {
    run_script(&["script/check-dco"], "DCO sign-off check")
}

fn check_upstream_ledger() -> Result<()> {
    run_script(&["script/check-upstream-ledger"], "upstream ledger check")
}

fn check_links(all: bool) -> Result<()> {
    let mut command = Command::new("lychee");
    command.arg("--no-progress");
    if !all {
        command.args(["--exclude", "^http"]);
    }
    command.arg("./docs/src/**/*");

    eprintln!("+ lychee --no-progress ./docs/src/**/*");
    let status = command
        .status()
        .context("failed to start lychee; install it with `cargo install lychee`")?;
    if !status.success() {
        bail!("lychee failed with {status}");
    }
    Ok(())
}

fn check_workflows() -> Result<()> {
    crate::tasks::workflow_checks::validate(
        crate::tasks::workflow_checks::WorkflowValidationArgs {},
    )
}

fn run_script(command: &[&str], description: &str) -> Result<()> {
    let (program, arguments) = command
        .split_first()
        .context("a check must name the program to run")?;
    let program_path = Path::new(program);
    if !program_path.is_file() {
        bail!("missing {program}");
    }

    let mut command = Command::new(program_path);
    command.args(arguments);
    let mut printed = program_path.display().to_string();
    for argument in arguments {
        printed.push(' ');
        printed.push_str(argument);
    }
    eprintln!("+ {printed}");
    let status = command
        .status()
        .with_context(|| format!("failed to start the {description}"))?;
    if !status.success() {
        bail!("the {description} failed with {status}");
    }
    Ok(())
}

/// Returns the output of `git grep`, or an empty string when nothing matched.
fn git_grep(arguments: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("grep")
        .args(arguments)
        .output()
        .context("failed to run git grep")?;

    match output.status.code() {
        // `git grep` exits 1 when it finds nothing.
        Some(0) => Ok(String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_owned()),
        Some(1) => Ok(String::new()),
        _ => bail!(
            "git grep failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    }
}

/// Expands a `git ls-files` pathspec, so checks only look at tracked files.
///
/// `git ls-files` exits 0 with empty output when nothing matches, including
/// when the pathspec is misspelled or the index is empty/unreadable. Every
/// caller scans the returned files and treats "no findings" as success, so an
/// empty result must fail closed instead of silently disabling the gate.
fn tracked_files(pathspec: &str) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .args(["ls-files", pathspec])
        .output()
        .context("failed to run git ls-files")?;
    if !output.status.success() {
        bail!(
            "git ls-files failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    parse_tracked_files(&String::from_utf8_lossy(&output.stdout), pathspec)
}

fn parse_tracked_files(stdout: &str, pathspec: &str) -> Result<Vec<PathBuf>> {
    let files: Vec<PathBuf> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(PathBuf::from)
        .collect();
    if files.is_empty() {
        bail!(
            "git ls-files {pathspec:?} matched no tracked files; the pathspec may be wrong, or the git index empty or unreadable"
        );
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::parse_tracked_files;
    use std::path::PathBuf;

    /// Deleting a line from `GATES` silently disables a gate for both local
    /// runs and CI, so the list length is pinned; adding a gate bumps it.
    const GATE_COUNT: usize = 19;

    #[test]
    fn gate_list_keeps_every_gate() {
        assert_eq!(
            super::GATES.len(),
            GATE_COUNT,
            "the gate list changed; bump GATE_COUNT when adding, restore the entry when removing"
        );
    }

    #[test]
    fn gate_names_are_unique() {
        let mut names: Vec<&str> = super::GATES.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique, "duplicate gate names: {names:?}");
    }

    #[test]
    fn empty_git_ls_files_output_fails_closed() {
        let error = parse_tracked_files("", "*.rs").expect_err("empty output must fail");
        assert!(
            error.to_string().contains("matched no tracked files"),
            "unexpected error: {error}"
        );
        assert!(
            error.to_string().contains("*.rs"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn whitespace_only_output_fails_closed() {
        assert!(parse_tracked_files("\n", "*.rs").is_err());
    }

    #[test]
    fn collects_every_line_as_a_path() {
        let files = parse_tracked_files("a.rs\nb/c.rs\n", "*.rs").expect("paths should parse");
        assert_eq!(files, vec![PathBuf::from("a.rs"), PathBuf::from("b/c.rs")]);
    }
}
