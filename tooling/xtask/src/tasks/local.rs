//! `cargo xtask install` and `cargo xtask build-docker`.
//!
//! These replace `script/install-linux` and `script/build-docker`, which were
//! the last two build entrypoints outside xtask. The install command drives
//! `cargo xtask bundle` and then unpacks the tarball through the same local
//! `script/install.sh` that the philosophy gate checks; no hosted binary is
//! ever downloaded.

#![allow(
    clippy::disallowed_methods,
    reason = "install drives cargo, tar, and the local installer"
)]

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Result, bail};
use clap::{Args, Subcommand};

use crate::tasks::build::model::{Channel, Profile, Target};
use crate::tasks::bundle::util;

#[derive(Args)]
pub struct InstallArgs {
    #[command(subcommand)]
    command: Option<InstallCommand>,
}

#[derive(Subcommand)]
enum InstallCommand {
    /// Build a local Linux tarball and install it into `~/.local`.
    Local(LocalInstallArgs),
}

#[derive(Args)]
struct LocalInstallArgs {
    /// Build with the CPU-native profile instead of `release`.
    #[arg(long)]
    native: bool,
    /// Install an existing bundle instead of rebuilding it.
    #[arg(long)]
    skip_build: bool,
    /// Release channel baked into the binary (defaults to the shipping one).
    #[arg(long, value_enum)]
    channel: Option<Channel>,
}

#[derive(Args)]
pub struct BuildDockerArgs {
    /// Base image to build the distribution smoke test against.
    base_image: String,
    /// Container engine to use. Defaults to podman, then docker.
    #[arg(long)]
    engine: Option<String>,
}

pub fn run_install(args: InstallArgs) -> Result<()> {
    let command = args
        .command
        .unwrap_or(InstallCommand::Local(LocalInstallArgs {
            native: false,
            skip_build: false,
            channel: None,
        }));
    match command {
        InstallCommand::Local(args) => install_local(args),
    }
}

fn install_local(args: LocalInstallArgs) -> Result<()> {
    let workspace = std::env::current_dir()?;
    if crate::tasks::build::model::Platform::host() != crate::tasks::build::model::Platform::Linux {
        bail!("local installation is only implemented for Linux");
    }
    let profile = if args.native {
        Profile::LinuxNative
    } else {
        Profile::Release
    };
    let channel = args.channel.unwrap_or_else(Channel::detect_shipping);
    let target = Target::host_default();

    let update_explanation = format!(
        "You need to fetch and rebuild zzz in {}",
        workspace.display()
    );
    if !args.skip_build {
        let mut command = Command::new("cargo");
        command
            .current_dir(&workspace)
            .args(["xtask", "bundle", "--target", target.triple()])
            .args(["--profile", profile.key()])
            .args(["--channel", channel.as_str()])
            .args(["--formats", "tar.gz"])
            // crates/cli/build.rs reads this at compile time to bake the local
            // update explanation into the binary, so it belongs on the build
            // step rather than on the installer that only unpacks the tarball.
            .env("ZZZ_UPDATE_EXPLANATION", &update_explanation);
        util::run(&mut command, "cargo xtask bundle")?;
    }

    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| workspace.join("target"), PathBuf::from);
    let archive = target_dir
        .join(profile.artifact_directory())
        .join(format!("zzz-linux-{}.tar.gz", target.arch().as_str()));
    if !archive.is_file() {
        bail!(
            "{} is missing; run `cargo xtask install local` without --skip-build",
            archive.display()
        );
    }

    let installer = workspace.join("script/install.sh");
    if !installer.is_file() {
        bail!("{} is missing", installer.display());
    }
    util::run(
        Command::new(&installer)
            .current_dir(&workspace)
            .env("ZZZ_BUNDLE_PATH", &archive)
            .env("ZZZ_CHANNEL", channel.as_str())
            .env("ZZZ_UPDATE_EXPLANATION", &update_explanation),
        "script/install.sh",
    )
}

pub fn run_build_docker(args: BuildDockerArgs) -> Result<()> {
    let workspace = std::env::current_dir()?;
    let dockerfile = workspace.join("Dockerfile-distros");
    if !dockerfile.is_file() {
        bail!("{} is missing", dockerfile.display());
    }
    let engine = match args.engine {
        Some(engine) => engine,
        None => ["podman", "docker"]
            .into_iter()
            .find(|binary| util::which(binary).is_some())
            .ok_or_else(|| anyhow::anyhow!("neither podman nor docker is installed"))?
            .to_owned(),
    };
    util::run(
        Command::new(&engine)
            .current_dir(&workspace)
            .arg("build")
            .arg(".")
            .args(["--file", "Dockerfile-distros"])
            .args(["--tag", "many"])
            .args(["--build-arg", &format!("BASE_IMAGE={}", args.base_image)])
            .env("DOCKER_BUILDKIT", "1"),
        &format!("{engine} build"),
    )
}
