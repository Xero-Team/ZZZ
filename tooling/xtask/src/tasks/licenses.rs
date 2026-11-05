use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use clap::{Args, Subcommand};

use crate::workspace::load_workspace;

#[derive(Args)]
pub struct LicensesArgs {
    #[command(subcommand)]
    command: Option<LicensesCommand>,
}

#[derive(Subcommand)]
enum LicensesCommand {
    /// Require a `LICENSE-AGPL` symlink in every crate directory.
    Check,
    /// Generate `assets/licenses.md` (and optionally the CSV inventory).
    Generate(GenerateArgs),
}

#[derive(Args)]
struct GenerateArgs {
    /// Write the Markdown document here instead of `assets/licenses.md`.
    #[arg(long)]
    output: Option<PathBuf>,
    /// Also write the CSV dependency inventory to `assets/licenses.csv`.
    #[arg(long)]
    csv: bool,
    /// Do not download or install `cargo-about`.
    #[arg(long)]
    offline: bool,
}

pub fn run_licenses(args: LicensesArgs) -> Result<()> {
    match args.command.unwrap_or(LicensesCommand::Check) {
        LicensesCommand::Check => check_license_symlinks(),
        LicensesCommand::Generate(args) => generate(args),
    }
}

fn generate(args: GenerateArgs) -> Result<()> {
    let workspace = load_workspace()?;
    let root = workspace.workspace_root.as_std_path();
    crate::tasks::bundle::licenses::generate(root, args.output.as_deref(), args.offline)?;
    if args.csv {
        generate_csv(root, args.offline)?;
    }
    Ok(())
}

/// The CSV inventory uses a different `cargo-about` template and appends the
/// FFmpeg line. It lives next to the Markdown generator so one command owns
/// both shipped license artifacts.
fn generate_csv(workspace: &Path, offline: bool) -> Result<()> {
    use std::process::Command;

    let version = crate::tasks::bundle::licenses::CARGO_ABOUT_VERSION;
    let installed = crate::tasks::bundle::util::stdout(
        Command::new("cargo")
            .current_dir(workspace)
            .args(["about", "--version"]),
        "cargo about --version",
    )
    .unwrap_or_default();
    if !installed.contains(&format!("cargo-about {version}")) {
        if offline {
            bail!(
                "cargo-about {version} is required while --offline is set; install it with \
                 `cargo install cargo-about@{version} --features cli`"
            );
        }
        crate::tasks::bundle::util::run(
            Command::new("cargo").current_dir(workspace).args([
                "install",
                &format!("cargo-about@{version}"),
                "--features",
                "cli",
            ]),
            "cargo install cargo-about",
        )?;
    }

    let output = workspace.join("assets/licenses.csv");
    let header_and_rows = crate::tasks::bundle::util::output(
        Command::new("cargo").current_dir(workspace).args([
            "about",
            "generate",
            "--fail",
            "-c",
            "script/licenses/zzz-licenses.toml",
            "script/licenses/template.csv.hbs",
        ]),
        "cargo about generate (csv)",
    )?;
    let stdout = String::from_utf8_lossy(&header_and_rows.stdout);
    let mut lines = stdout.lines();
    let header = lines.next().unwrap_or_default().to_owned();
    let mut rows = lines
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    rows.sort();
    let (series, _, _) = crate::tasks::bundle::ffmpeg::dependency_info(workspace);
    rows.push(format!(
        "FFmpeg,{series},LGPL-3.0-or-later,https://github.com/FFmpeg/FFmpeg/tree/release/{series}"
    ));
    let document = std::iter::once(header)
        .chain(rows)
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    std::fs::write(&output, document)
        .with_context(|| format!("failed to write {}", output.display()))?;
    eprintln!("licenses: {}", output.display());
    Ok(())
}

fn check_license_symlinks() -> Result<()> {
    const LICENSE_FILES: &[&str] = &["LICENSE-APACHE", "LICENSE-GPL", "LICENSE-AGPL"];

    let workspace = load_workspace()?;

    for package in workspace.workspace_packages() {
        let crate_dir = package
            .manifest_path
            .parent()
            .with_context(|| format!("no crate directory for {}", package.name))?;

        if let Some(license_file) = first_license_file(crate_dir, LICENSE_FILES) {
            if !license_file.is_symlink() {
                println!("{} is not a symlink", license_file.display());
            }

            continue;
        }

        println!("Missing license: {}", package.name);
    }

    Ok(())
}

fn first_license_file(path: impl AsRef<Path>, license_files: &[&str]) -> Option<PathBuf> {
    for license_file in license_files {
        let path_to_license = path.as_ref().join(license_file);
        if path_to_license.exists() {
            return Some(path_to_license);
        }
    }

    None
}
