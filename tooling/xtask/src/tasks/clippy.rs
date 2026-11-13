//! The lint gate.
//!
//! `cargo xtask clippy` is the only supported way to lint the workspace. It
//! replaces the shell script that used to hold this logic, so the deny set, the
//! package selection rules, and the philosophy preflight live in one place.
//!
//! The base lint levels are `[workspace.lints]` in `Cargo.toml`. What remains
//! here covers what a lint table cannot: denying the `warnings` group, opt-in
//! lints that are too noisy to gate workspace-wide, and the non-Rust checks.

#![allow(
    clippy::disallowed_methods,
    reason = "a lint gate drives cargo and streams its output to the terminal"
)]

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context as _, Result, bail};
use clap::Parser;

/// Packages that are background, transport, or otherwise await on thread-safe
/// state, so `clippy::future_not_send` is high-signal there. GPUI crates await
/// main-thread local state on purpose and are far noisier, which is why this is
/// a curated list rather than a workspace lint.
const FUTURE_NOT_SEND_PACKAGES: &[&str] = &[
    "cli",
    "compliance",
    "git",
    "language_model",
    "lsp",
    "sqlez",
    "theme",
    "watch",
    "xtask",
];

#[derive(Parser)]
pub struct ClippyArgs {
    /// Automatically apply lint suggestions (`clippy --fix`).
    #[arg(long)]
    fix: bool,

    /// The package to run Clippy against. Repeat to lint several packages;
    /// omit it to lint the whole workspace.
    #[arg(long, short)]
    package: Vec<String>,

    /// Lint the whole workspace. This is the default; the flag exists so older
    /// invocations of `script/clippy --workspace` keep working.
    #[arg(long)]
    workspace: bool,

    /// Lint for this target triple instead of the host. Cross-target linting
    /// only compiles; it does not link.
    #[arg(long)]
    target: Option<String>,
}

pub fn run_clippy(args: ClippyArgs) -> Result<()> {
    let workspace = env::current_dir().context("failed to locate the workspace")?;

    run_philosophy_check(&workspace)?;
    let ccache_dir = writable_ccache_dir(&workspace);

    let scope = if args.package.is_empty() || args.workspace {
        vec!["--workspace".to_owned()]
    } else {
        args.package
            .iter()
            .flat_map(|package| ["--package".to_owned(), package.clone()])
            .collect()
    };
    let targeted = !args.package.is_empty() && !args.workspace;

    // Tests, benches, and examples are held to the same bar as library and
    // binary targets.
    run_clippy_pass(
        &args,
        &scope,
        ccache_dir.as_deref(),
        &["--all-targets"],
        &["--deny", "warnings"],
        "workspace lints",
    )?;

    // Manifest lints stay on the command line: they are target-independent, and
    // path dependencies without `[lints] workspace = true` would miss them in
    // the lint table.
    run_clippy_pass(
        &args,
        &scope,
        ccache_dir.as_deref(),
        &[],
        &[
            "--deny",
            "warnings",
            "--deny",
            "clippy::negative_feature_names",
            "--deny",
            "clippy::wildcard_dependencies",
        ],
        "manifest lints",
    )?;

    run_future_not_send_pass(&args, &scope, ccache_dir.as_deref(), targeted)?;
    run_auxiliary_checks(&workspace)
}

/// Runs one `cargo clippy` invocation.
fn run_clippy_pass(
    args: &ClippyArgs,
    scope: &[String],
    ccache_dir: Option<&Path>,
    extra_cargo_args: &[&str],
    lint_args: &[&str],
    description: &str,
) -> Result<()> {
    let mut command = clippy_command(args, ccache_dir);
    command.args(scope);
    command.args(["--release", "--all-features"]);
    command.args(extra_cargo_args);
    command.arg("--");
    command.args(lint_args);

    println!("running the {description} pass");
    run(command, "cargo clippy")
}

/// `future_not_send` is enforced for the curated packages. An explicit `-p`
/// selection allows it instead: a targeted run on a GPUI crate would otherwise
/// fail on futures that intentionally await main-thread state.
fn run_future_not_send_pass(
    args: &ClippyArgs,
    scope: &[String],
    ccache_dir: Option<&Path>,
    targeted: bool,
) -> Result<()> {
    let mut command = clippy_command(args, ccache_dir);

    if targeted {
        command.args(scope);
        command.args(["--release", "--all-features"]);
        command.args(["--", "--deny", "warnings"]);
        // `--allow` rather than `--warn`, because `--deny warnings` promotes
        // every warning to an error.
        command.args(["--allow", "clippy::future_not_send"]);
    } else {
        command.args(["--release", "--all-features"]);
        for package in FUTURE_NOT_SEND_PACKAGES {
            command.arg("--package").arg(package);
        }
        command.args(["--", "--deny", "warnings"]);
        command.args(["--deny", "clippy::future_not_send"]);
    }

    println!("running the future_not_send pass");
    run(command, "cargo clippy")
}

fn clippy_command(args: &ClippyArgs, ccache_dir: Option<&Path>) -> Command {
    let mut command = Command::new(cargo_program());
    command.arg("clippy");
    if args.fix {
        command.arg("--fix");
    }
    if let Some(target) = &args.target {
        command.args(["--target", target]);
    }
    if let Some(ccache_dir) = ccache_dir {
        command.env("CCACHE_DIR", ccache_dir);
    }
    command
}

fn cargo_program() -> OsString {
    env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

fn run(mut command: Command, description: &str) -> Result<()> {
    eprintln!("+ {}", display(&command));
    let status = command
        .status()
        .with_context(|| format!("failed to start {description}"))?;
    if !status.success() {
        bail!("{description} failed with {status}");
    }
    Ok(())
}

fn display(command: &Command) -> String {
    let mut text = command.get_program().to_string_lossy().into_owned();
    for argument in command.get_args() {
        text.push(' ');
        text.push_str(&argument.to_string_lossy());
    }
    text
}

/// Returns a writable `ccache` directory when the configured one cannot be
/// written, so a build-script compile does not fail on a read-only home.
fn writable_ccache_dir(workspace: &Path) -> Option<PathBuf> {
    if env::var_os("CCACHE_DIR").is_some() {
        return None;
    }

    let cache_dir = ccache_cache_dir()?;
    if is_writable(&cache_dir) {
        return None;
    }

    let fallback = workspace.join("target/ccache");
    if let Err(error) = std::fs::create_dir_all(&fallback) {
        eprintln!(
            "warning: could not create {}: {error}; using the configured ccache directory",
            fallback.display()
        );
        return None;
    }
    Some(fallback)
}

fn ccache_cache_dir() -> Option<PathBuf> {
    let output = Command::new("ccache")
        .args(["get-config", "cache_dir"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Probes by creating a file: the permission bits do not account for a
/// read-only mount or a full disk.
fn is_writable(path: &Path) -> bool {
    tempfile::NamedTempFile::new_in(path).is_ok()
}

/// Runs the philosophy gate before the compiler, so a rejected commercial or
/// network surface fails in seconds instead of after a release build.
fn run_philosophy_check(workspace: &Path) -> Result<()> {
    if !cfg!(unix) {
        eprintln!(
            "skipping the philosophy check: script/check-philosophy needs a POSIX shell; CI runs it"
        );
        return Ok(());
    }

    let script = workspace.join("script/check-philosophy");
    if !script.is_file() {
        bail!("missing {}", script.display());
    }

    let mut command = Command::new(&script);
    command.current_dir(workspace);
    println!("running the philosophy check");
    run(command, "philosophy check")
}

/// The tools below are optional and developer-facing, so each one is skipped
/// when it is not installed. CI installs them where they are required and sets
/// `GITHUB_ACTIONS`, which skips this block entirely.
fn run_auxiliary_checks(workspace: &Path) -> Result<()> {
    if env::var_os("GITHUB_ACTIONS").is_some() {
        return Ok(());
    }

    if is_installed("cargo-machete") {
        // Run the binary directly: through `cargo`, 0.9 receives the bare
        // subcommand name as an extra path and fails walking it.
        run_tool(
            OsString::from("cargo-machete"),
            &["."],
            workspace,
            "cargo machete",
        )?;
    }

    if is_installed("typos") {
        run_tool(
            OsString::from("typos"),
            &["--config", "typos.toml"],
            workspace,
            "typos",
        )?;
    }

    if is_installed("buf") {
        run_tool(
            OsString::from("buf"),
            &["lint", "crates/proto/proto"],
            workspace,
            "buf lint",
        )?;
        run_tool(
            OsString::from("buf"),
            &["format", "--diff", "--exit-code", "crates/proto/proto"],
            workspace,
            "buf format",
        )?;
    }

    Ok(())
}

fn run_tool(
    program: OsString,
    arguments: &[&str],
    workspace: &Path,
    description: &str,
) -> Result<()> {
    let mut command = Command::new(program);
    command.args(arguments).current_dir(workspace);
    run(command, description)
}

fn is_installed(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
