//! `cargo xtask build`, `dist`, `pgo`, `doctor`, and `verify`.
//!
//! This module is the single entry point for producing artifacts. The planner
//! in [`plan`] turns one option grammar into a task graph, the scheduler runs
//! it inside the toolchain container where that is needed, and the bundlers in
//! `tasks::bundle` implement each platform's packaging.

pub mod container;
pub mod doctor;
pub mod freebsd;
pub mod manifest;
pub mod model;
pub mod msvc;
pub mod pgo;
pub mod plan;
pub mod remote_server;
pub mod scheduler;
pub mod verify;

use std::path::PathBuf;
use std::time::SystemTime;

use anyhow::{Result, bail};
use clap::Args;

use crate::tasks::build::container::ContainerPolicy;
use crate::tasks::build::model::{Channel, Format, Platform, Profile, Target};
use crate::tasks::build::plan::{BuildRequest, PgoMode};
use crate::tasks::build::scheduler::ExecuteOptions;

#[derive(Args)]
pub struct BuildArgs {
    /// Target triple (repeatable). Defaults to the host target.
    #[arg(long = "target", value_parser = parse_target)]
    targets: Vec<Target>,
    /// Cargo profile to produce. Only release-family profiles ship artifacts.
    #[arg(long, value_enum, default_value = "release")]
    profile: Profile,
    /// `auto` containerizes release builds, `always` and `never` override that.
    #[arg(long, value_enum, default_value = "auto")]
    container: ContainerPolicy,
    /// Consume an existing PGO profile. `auto` uses one when it exists.
    #[arg(long, value_enum, default_value = "auto")]
    pgo: PgoMode,
    /// Never use a PGO profile, even when one exists.
    #[arg(long, conflicts_with = "require_pgo")]
    no_pgo: bool,
    /// Fail when no PGO profile is available.
    #[arg(long)]
    require_pgo: bool,
    /// Artifact formats (repeatable, comma-separated). Defaults per target.
    #[arg(long, value_enum, value_delimiter = ',')]
    formats: Vec<Format>,
    /// Total CPU budget for the whole plan, split across targets.
    #[arg(long)]
    jobs: Option<u32>,
    /// Release channel baked into the binary and the bundle name. Defaults to
    /// the shipping channel, so packaging produces release artifacts while
    /// `cargo run` and `cargo build` stay on `dev`.
    #[arg(long, value_enum, default_value = "stable")]
    channel: Option<Channel>,
    /// Fetch dependencies first and build without network access.
    #[arg(long)]
    offline: bool,
    /// Do not wrap the compiler in sccache.
    #[arg(long)]
    no_sccache: bool,
    /// Print the task graph as JSON instead of running it.
    #[arg(long)]
    print_plan: bool,
    /// Print the commands that would run without running them.
    #[arg(long)]
    dry_run: bool,
    /// Keep running independent tasks after one fails.
    #[arg(long)]
    keep_going: bool,
    /// Echo every command before it runs.
    #[arg(long)]
    verbose: bool,
    /// Minimum function count a PGO profile must cover to be accepted.
    #[arg(long, default_value_t = 100_000)]
    min_pgo_functions: u64,
}

#[derive(Args)]
pub struct DistArgs {
    /// Build every target this host can produce.
    #[arg(long)]
    all: bool,
    /// Target triple (repeatable). Defaults to the host target without `--all`.
    #[arg(long = "target", value_parser = parse_target)]
    targets: Vec<Target>,
    /// Artifact formats (repeatable, comma-separated). Defaults per target.
    #[arg(long, value_enum, value_delimiter = ',')]
    formats: Vec<Format>,
    /// Release channel baked into the binary and the bundle name. Defaults to
    /// the shipping channel, so packaging produces release artifacts while
    /// `cargo run` and `cargo build` stay on `dev`.
    #[arg(long, value_enum, default_value = "stable")]
    channel: Option<Channel>,
    /// `auto` containerizes release builds, `always` and `never` override that.
    #[arg(long, value_enum, default_value = "auto")]
    container: ContainerPolicy,
    /// Consume an existing PGO profile. `auto` uses one when it exists.
    #[arg(long, value_enum, default_value = "auto")]
    pgo: PgoMode,
    /// Never use a PGO profile, even when one exists.
    #[arg(long)]
    no_pgo: bool,
    /// Total CPU budget for the whole plan, split across targets.
    #[arg(long)]
    jobs: Option<u32>,
    /// Fetch dependencies first and build without network access.
    #[arg(long)]
    offline: bool,
    /// Do not wrap the compiler in sccache.
    #[arg(long)]
    no_sccache: bool,
    /// Directory for the manifest, checksums, and copied artifacts.
    #[arg(long, default_value = "dist")]
    out: PathBuf,
    /// Print the task graph as JSON instead of running it.
    #[arg(long)]
    print_plan: bool,
    /// Print the commands that would run without running them.
    #[arg(long)]
    dry_run: bool,
    /// Keep running independent tasks after one fails.
    #[arg(long)]
    keep_going: bool,
    /// Echo every command before it runs.
    #[arg(long)]
    verbose: bool,
    /// Minimum function count a PGO profile must cover to be accepted.
    #[arg(long, default_value_t = 100_000)]
    min_pgo_functions: u64,
}

pub fn run_build(args: BuildArgs) -> Result<()> {
    let workspace = std::env::current_dir()?;
    let channel = args
        .channel
        .unwrap_or_else(|| Channel::detect_shipping(&workspace));
    let pgo = resolve_pgo(args.pgo, args.no_pgo);
    let request = BuildRequest {
        targets: default_targets(args.targets),
        formats: args.formats,
        profile: args.profile,
        channel,
        container: args.container,
        pgo,
        jobs: args.jobs,
        offline: args.offline,
        sccache: !args.no_sccache,
        min_pgo_functions: args.min_pgo_functions,
        require_pgo: require_pgo(args.require_pgo, pgo, channel),
    };

    let plan = plan::assemble(&workspace, &request)?;
    if args.print_plan {
        println!("{}", plan.to_json()?);
        return Ok(());
    }
    if !args.dry_run {
        eprintln!("{}", plan.summary());
    }

    scheduler::execute(
        &plan,
        ExecuteOptions {
            dry_run: args.dry_run,
            keep_going: args.keep_going,
            verbose: args.verbose,
        },
    )
}

pub fn run_dist(args: DistArgs) -> Result<()> {
    let workspace = std::env::current_dir()?;
    let channel = args
        .channel
        .unwrap_or_else(|| Channel::detect_shipping(&workspace));
    let pgo = resolve_pgo(args.pgo, args.no_pgo);
    let targets = if args.all {
        distributable_targets()?
    } else {
        default_targets(args.targets)
    };
    let request = BuildRequest {
        targets,
        formats: args.formats,
        profile: Profile::Release,
        channel,
        container: args.container,
        pgo,
        jobs: args.jobs,
        offline: args.offline,
        sccache: !args.no_sccache,
        min_pgo_functions: args.min_pgo_functions,
        require_pgo: require_pgo(false, pgo, channel),
    };

    let plan = plan::assemble(&workspace, &request)?;
    if args.print_plan {
        println!("{}", plan.to_json()?);
        return Ok(());
    }
    if !args.dry_run {
        eprintln!("{}", plan.summary());
    }

    let started = SystemTime::now();
    scheduler::execute(
        &plan,
        ExecuteOptions {
            dry_run: args.dry_run,
            keep_going: args.keep_going,
            verbose: args.verbose,
        },
    )?;
    if args.dry_run {
        return Ok(());
    }

    let artifacts = manifest::collect(&workspace, &plan, started)?;
    if artifacts.is_empty() {
        bail!("the build produced no recognizable artifacts");
    }
    // The plan names the file each format is supposed to produce, so a format
    // that silently wrote nothing (or wrote under a name the bundler no longer
    // uses) fails here instead of shipping an incomplete `dist/`.
    let missing: Vec<&PathBuf> = plan
        .targets
        .iter()
        .flat_map(|target| target.artifacts.iter())
        .filter(|path| !path.exists())
        .collect();
    if !missing.is_empty() {
        let names: Vec<String> = missing
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        bail!("the build did not produce:\n  {}", names.join("\n  "));
    }
    let out = absolutize(&workspace, &args.out);
    manifest::write(&workspace, &plan, artifacts, &out)?;
    eprintln!("manifest: {}", out.join(manifest::MANIFEST_FILE).display());
    Ok(())
}

fn resolve_pgo(pgo: PgoMode, no_pgo: bool) -> PgoMode {
    if no_pgo { PgoMode::Off } else { pgo }
}

/// PGO is mandatory when the caller asks to consume a profile. A shipping
/// channel alone does not require one: `--no-pgo` is the documented escape
/// hatch and is recorded in the manifest as an absent profile.
fn require_pgo(explicit: bool, pgo: PgoMode, channel: Channel) -> bool {
    let _ = channel;
    pgo == PgoMode::Use && explicit
}

fn default_targets(targets: Vec<Target>) -> Vec<Target> {
    if targets.is_empty() {
        vec![Target::host_default()]
    } else {
        targets
    }
}

/// Targets this host can produce. macOS and Windows need their own toolchains
/// for signed installers, so from Linux the cross paths are the only extras.
///
/// `dist --all` means "everything the current settings can build". A Windows
/// target makes the planner request the msvc-wine layer, so no target is
/// silently dropped here: an environment that cannot build Windows fails while
/// planning instead of producing a set that looks complete. The toolchain
/// image supplies the FreeBSD cross sysroots and wrappers, so those targets
/// are included on a Linux host too.
fn distributable_targets() -> Result<Vec<Target>> {
    let host = Platform::host();
    // On a Linux host the toolchain image supplies the cross toolchains for
    // Mac, Windows, and FreeBSD, so every distributable target is buildable.
    // Other hosts produce only their own platform.
    let targets = Target::DISTRIBUTABLE
        .iter()
        .copied()
        .filter(|target| target.platform() == host || host == Platform::Linux)
        .collect::<Vec<_>>();
    if targets.is_empty() {
        bail!("this host cannot produce any distributable targets");
    }
    Ok(targets)
}

fn absolutize(workspace: &std::path::Path, path: &std::path::Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace.join(path)
    }
}

fn parse_target(value: &str) -> Result<Target, String> {
    value.parse::<Target>().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pgo_is_only_required_when_explicitly_requested() {
        assert!(!require_pgo(false, PgoMode::Auto, Channel::Stable));
        assert!(!require_pgo(false, PgoMode::Use, Channel::Preview));
        assert!(!require_pgo(false, PgoMode::Off, Channel::Stable));
        assert!(require_pgo(true, PgoMode::Use, Channel::Stable));
        assert!(!require_pgo(true, PgoMode::Off, Channel::Stable));
    }

    #[test]
    fn no_pgo_overrides_the_mode() {
        assert_eq!(resolve_pgo(PgoMode::Auto, true), PgoMode::Off);
        assert_eq!(resolve_pgo(PgoMode::Use, false), PgoMode::Use);
    }

    #[test]
    fn empty_target_list_falls_back_to_the_host() {
        assert_eq!(default_targets(Vec::new()), vec![Target::host_default()]);
    }

    #[test]
    fn targets_round_trip_through_the_cli_parser() {
        assert_eq!(
            parse_target("aarch64-apple-darwin").expect("parses"),
            Target::MacAarch64
        );
        assert!(parse_target("not-a-target").is_err());
    }
}
