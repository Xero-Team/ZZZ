//! Post-build verification.
//!
//! A bundle that builds is not necessarily a bundle that starts. These checks
//! cover the failures that only show up on somebody else's machine: a C library
//! floor raised by the build host, a binary that cannot report its version, and
//! checksums that no longer match the files.

#![allow(
    clippy::disallowed_methods,
    reason = "verification shells out to objdump and the produced binaries"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};
use clap::{Args, Subcommand};
use regex::Regex;

use crate::tasks::build::model::{Platform, Profile, Target};

#[derive(Args)]
pub struct VerifyArgs {
    #[command(subcommand)]
    command: VerifyCommand,
}

#[derive(Subcommand)]
enum VerifyCommand {
    /// Fail when a binary needs a newer glibc than the supported floor.
    GlibcFloor {
        binary: PathBuf,
        #[arg(long, default_value = "2.35")]
        max: String,
    },
    /// Run a binary with `--version` and print what it reports.
    Version { binary: PathBuf },
    /// Re-check every checksum in a `SHA256SUMS` file.
    Checksums { directory: PathBuf },
    /// Verify the binaries and packages produced for a target.
    Artifacts {
        #[arg(long, value_parser = parse_target)]
        target: Target,
        #[arg(long, default_value = "2.35")]
        max_glibc: String,
        #[arg(long, value_enum, default_value = "release")]
        profile: Profile,
        /// Directory Cargo wrote to. Defaults to `CARGO_TARGET_DIR`, then
        /// `target`; relative paths resolve against the workspace.
        #[arg(long)]
        target_dir: Option<PathBuf>,
    },
}

pub fn run_verify(args: VerifyArgs) -> Result<()> {
    let workspace = std::env::current_dir()?;
    match args.command {
        VerifyCommand::GlibcFloor { binary, max } => {
            check_glibc_floor(&binary, &max)?;
            Ok(())
        }
        VerifyCommand::Version { binary } => {
            print_version(&binary)?;
            Ok(())
        }
        VerifyCommand::Checksums { directory } => verify_checksums(&directory),
        VerifyCommand::Artifacts {
            target,
            max_glibc,
            profile,
            target_dir,
        } => {
            // A plan that runs inside the toolchain container leaves the
            // location to `CARGO_TARGET_DIR`, exactly like the bundler does.
            let target_dir = target_dir
                .or_else(|| std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from))
                .unwrap_or_else(|| PathBuf::from("target"));
            verify_artifacts(&workspace, target, profile, &target_dir, &max_glibc)
        }
    }
}

fn verify_artifacts(
    workspace: &Path,
    target: Target,
    profile: Profile,
    target_dir: &Path,
    max_glibc: &str,
) -> Result<()> {
    let root = if target_dir.is_absolute() {
        target_dir.to_path_buf()
    } else {
        workspace.join(target_dir)
    };
    let artifact_directory = profile.artifact_directory();
    let target_directory = root.join(target.triple()).join(artifact_directory);
    let host_directory = root.join(artifact_directory);

    // Each packaging path puts its binaries somewhere different: the Linux
    // bundle builds in `target/release`, the cross scripts build under the
    // triple, and the macOS bundle nests them in the app. Listing every
    // plausible location keeps the check working whether the caller ran the
    // plan or the platform script by hand.
    let candidates: Vec<(&str, PathBuf)> = match target.platform() {
        Platform::Linux | Platform::FreeBSD => {
            let mut candidates = Vec::new();
            for name in ["zzz", "cli"] {
                candidates.push((name, target_directory.join(name)));
                candidates.push((name, host_directory.join(name)));
            }
            candidates
        }
        Platform::Mac => {
            let mut candidates = Vec::new();
            for name in ["zzz", "cli"] {
                candidates.push((name, target_directory.join(name)));
            }
            candidates.extend(app_binaries(&target_directory));
            candidates
        }
        Platform::Windows => ["zzz.exe", "cli.exe"]
            .into_iter()
            .map(|name| (name, target_directory.join(name)))
            .collect(),
    };

    let mut verified = Vec::new();
    for (name, candidate) in candidates {
        if !candidate.is_file() {
            continue;
        }
        if target.platform() == Platform::Linux {
            check_glibc_floor(&candidate, max_glibc)?;
        }
        println!("verified {}", candidate.display());
        verified.push((name, candidate));
    }

    if verified.is_empty() {
        bail!(
            "no built binaries for {} under {}",
            target.triple(),
            target_directory.display()
        );
    }

    // A binary that cannot report its version is not a shippable binary, and a
    // broken dynamic link shows up before `main`. Only the CLI can answer
    // without opening a window, and only a binary for this host can run here.
    if target.platform() == Platform::host() {
        if let Some((_, cli)) = verified
            .iter()
            .find(|(name, path)| name.starts_with("cli") && path.is_file())
        {
            print_version(cli)?;
        }
    }

    Ok(())
}

/// Binaries inside the `.app` that `cargo bundle` produced. The bundle name and
/// the profile directory are both decisions of the packaging script, so the
/// check walks what is actually there instead of guessing the name.
fn app_binaries(directory: &Path) -> Vec<(&'static str, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut binaries = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "app") {
            continue;
        }
        let macos = path.join("Contents/MacOS");
        for name in ["zzz", "cli"] {
            binaries.push((name, macos.join(name)));
        }
    }
    binaries
}

fn check_glibc_floor(binary: &Path, maximum: &str) -> Result<()> {
    let maximum = parse_version(maximum)?;
    let floor = glibc_floor(binary)?;
    if floor > maximum {
        bail!(
            "{} requires GLIBC {}.{}, above the supported {}.{}",
            binary.display(),
            floor.0,
            floor.1,
            maximum.0,
            maximum.1
        );
    }
    println!(
        "{}: GLIBC {}.{} (supported: {}.{})",
        binary.display(),
        floor.0,
        floor.1,
        maximum.0,
        maximum.1
    );
    Ok(())
}

fn glibc_floor(binary: &Path) -> Result<(u32, u32)> {
    let pattern = Regex::new(r"GLIBC_(\d+)\.(\d+)").context("invalid version pattern")?;
    let output = objdump_symbols(binary)?;
    let text = String::from_utf8_lossy(&output);

    let mut highest: Option<(u32, u32)> = None;
    for captures in pattern.captures_iter(&text) {
        let major = captures[1].parse::<u32>()?;
        let minor = captures[2].parse::<u32>()?;
        let version = (major, minor);
        if highest.is_none_or(|current| version > current) {
            highest = Some(version);
        }
    }

    highest.ok_or_else(|| {
        anyhow::anyhow!(
            "{} links no glibc symbols; it may be statically linked or not an ELF binary",
            binary.display()
        )
    })
}

fn objdump_symbols(binary: &Path) -> Result<Vec<u8>> {
    for tool in ["objdump", "llvm-objdump"] {
        let Ok(output) = Command::new(tool).args(["-T"]).arg(binary).output() else {
            continue;
        };
        if output.status.success() {
            return Ok(output.stdout);
        }
    }
    bail!(
        "neither objdump nor llvm-objdump could read {}",
        binary.display()
    )
}

fn print_version(binary: &Path) -> Result<()> {
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .with_context(|| format!("failed to run {}", binary.display()))?;
    if !output.status.success() {
        bail!(
            "{} --version exited with {}",
            binary.display(),
            output.status
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let version = text.trim();
    if version.is_empty() {
        bail!("{} --version printed nothing", binary.display());
    }
    println!("{version}");
    Ok(())
}

fn verify_checksums(directory: &Path) -> Result<()> {
    let path = directory.join("SHA256SUMS");
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    let mut checked = 0;
    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
        let Some((expected, file)) = line.split_once("  ") else {
            bail!("malformed checksum line: {line}");
        };
        let file_path = directory.join(file);
        let actual = crate::tasks::build::manifest::sha256_file(&file_path)?;
        if actual != expected {
            bail!(
                "checksum mismatch for {}: {actual} != {expected}",
                file_path.display()
            );
        }
        checked += 1;
    }
    println!("verified {checked} checksum(s) in {}", directory.display());
    Ok(())
}

fn parse_version(value: &str) -> Result<(u32, u32)> {
    let (major, minor) = value
        .split_once('.')
        .ok_or_else(|| anyhow::anyhow!("expected a version like 2.35, found `{value}`"))?;
    Ok((major.parse()?, minor.parse()?))
}

fn parse_target(value: &str) -> Result<Target, String> {
    value.parse::<Target>().map_err(|error| error.to_string())
}
