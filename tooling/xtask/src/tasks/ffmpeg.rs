//! Pinned FFmpeg development packages.
//!
//! The Linux and Windows bundlers link a prebuilt FFmpeg, and the release job
//! fetches that package before it drops into a network namespace. `ensure`
//! exposes the same resolution the bundlers use, so the fetch can happen
//! outside the closed build, and `info` reports which build is installed for
//! the generated license document.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use clap::{Args, Subcommand};

use crate::tasks::build::model::Target;
use crate::tasks::bundle::ffmpeg;

#[derive(Args)]
pub struct FfmpegArgs {
    #[command(subcommand)]
    command: FfmpegCommand,
}

#[derive(Subcommand)]
enum FfmpegCommand {
    /// Download, or reuse, the newest development package for a target.
    Ensure {
        /// Rust target triple. Defaults to the host target.
        #[arg(long)]
        target: Option<String>,
        /// Reinstall even when the installed package is current.
        #[arg(long)]
        force: bool,
        /// Look for a newer build now instead of after the refresh interval.
        #[arg(long)]
        refresh: bool,
        /// Do not download anything; fail when no package is installed.
        #[arg(long)]
        offline: bool,
        /// Dependency root. Defaults to `$ZZZ_FFMPEG_ROOT`, then
        /// `<workspace>/.deps/ffmpeg`.
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Print `<series>|<archive>|<release tag>` for the installed package.
    Info {
        /// Dependency root. Defaults to `$ZZZ_FFMPEG_ROOT`, then
        /// `<workspace>/.deps/ffmpeg`.
        #[arg(long)]
        root: Option<PathBuf>,
    },
}

pub fn run_ffmpeg(args: FfmpegArgs) -> Result<()> {
    let workspace = std::env::current_dir().context("failed to locate the workspace")?;

    match args.command {
        FfmpegCommand::Ensure {
            target,
            force,
            refresh,
            offline,
            root,
        } => {
            let target = match target {
                Some(target) => target,
                None => default_target()?,
            };
            let root = dependency_root(&workspace, root);
            let install_dir = ffmpeg::ensure_in(&root, &target, force, refresh, offline)?;
            println!("{}", install_dir.display());
            Ok(())
        }
        FfmpegCommand::Info { root } => {
            let root = dependency_root(&workspace, root);
            let (series, archive, tag) = ffmpeg::dependency_info_in(&root);
            println!("{series}|{archive}|{tag}");
            Ok(())
        }
    }
}

fn dependency_root(workspace: &Path, root: Option<PathBuf>) -> PathBuf {
    root.or_else(|| std::env::var_os("ZZZ_FFMPEG_ROOT").map(PathBuf::from))
        .unwrap_or_else(|| ffmpeg::dependency_root(workspace))
}

/// macOS links AVFoundation instead of FFmpeg, so there is nothing to fetch
/// there and the error says so rather than listing unrelated triples.
fn default_target() -> Result<String> {
    let triple = Target::host_default().triple();
    if triple.ends_with("apple-darwin") {
        bail!("macOS uses the AVFoundation video backend and does not need FFmpeg");
    }
    Ok(triple.to_owned())
}
