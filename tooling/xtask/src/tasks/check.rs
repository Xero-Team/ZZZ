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

mod gpui_api;
mod keymaps;
mod scroll_handlers;

#[derive(Args)]
pub struct CheckArgs {
    #[command(subcommand)]
    command: Option<CheckCommand>,
}

#[derive(Subcommand)]
enum CheckCommand {
    /// Every gate CI runs: markers, keymaps, scroll handlers, licenses,
    /// locale catalog keys, DCO, philosophy, ledger, workflows, and the
    /// `gpui` public API snapshot.
    All,
    /// Reject leftover markers for unfinished work.
    Todos,
    /// Reject macOS-only modifiers in shared keymaps and duplicate bindings.
    Keys,
    /// Reject re-entrant entity/ListState access inside scroll handlers.
    ScrollHandlers,
    /// Require a `LICENSE-AGPL` symlink in every crate directory.
    Licenses,
    /// Require the locale catalogs to have identical key sets.
    LocaleKeys,
    /// Require DCO sign-off on recent non-merge commits.
    Dco,
    /// Run ShellCheck over the scripts in the repository root's `script/`.
    Shellcheck {
        /// Minimum severity that fails the check.
        #[arg(long, value_enum, default_value = "error")]
        severity: Severity,
    },
    /// Check that vendored third-party sources match their manifests.
    Vendor,
    /// Assert that rejected commercial and network surfaces stay absent.
    Philosophy,
    /// Validate the upstream rejection ledger and its allowlist.
    UpstreamLedger,
    /// Validate the `run:` patterns in the GitHub workflow files.
    Workflows,
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
        CheckCommand::LocaleKeys => check_locale_keys(),
        CheckCommand::Dco => check_dco(),
        CheckCommand::Shellcheck { severity } => check_shellcheck(severity),
        CheckCommand::Vendor => run_script(&["script/vendor-sync", "check"], "vendor check"),
        CheckCommand::Philosophy => check_philosophy(),
        CheckCommand::UpstreamLedger => check_upstream_ledger(),
        CheckCommand::Workflows => check_workflows(),
        CheckCommand::GpuiApi(args) => gpui_api::check(args),
        CheckCommand::Links { all } => check_links(all),
    }
}

/// Runs the gates CI enforces. `shellcheck` and `vendor` are left out: they
/// need tools the default runner does not install, and neither has a failure
/// mode that a clean Linux checkout can hide.
fn run_all() -> Result<()> {
    let checks: &[(&str, fn() -> Result<()>)] = &[
        ("unfinished-work markers", check_todos),
        ("keymaps", keymaps::check),
        ("scroll handlers", scroll_handlers::check),
        ("crate licenses", check_licenses),
        ("locale catalog keys", check_locale_keys),
        ("DCO sign-off", check_dco),
        ("philosophy boundary", check_philosophy),
        ("upstream rejection ledger", check_upstream_ledger),
        ("GitHub workflow files", check_workflows),
        ("gpui public API snapshot", gpui_api::check_snapshot),
    ];

    let mut failures = Vec::new();
    for (name, check) in checks {
        println!("checking {name}");
        if let Err(error) = check() {
            eprintln!("error: {name}: {error:#}");
            failures.push(*name);
        }
    }

    if failures.is_empty() {
        println!("all {} repository checks passed", checks.len());
        return Ok(());
    }

    bail!(
        "{} of {} checks failed: {}",
        failures.len(),
        checks.len(),
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

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(PathBuf::from)
        .collect())
}
