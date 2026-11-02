//! Profile-guided optimization as first-class build steps.
//!
//! PGO is two builds: one instrumented, one that consumes the profile. The
//! profile is only valid for the exact target, profile, flags, and compiler that
//! produced it, so it lives under a fingerprint directory and is verified before
//! a release build consumes it.
//!
//! `train` and `build` are planners: each emits one scheduled task that runs
//! `pgo generate` or `pgo optimize` inside the toolchain container. The three
//! steps of a training round — build, training run, merge — now live in
//! [`run_generate`]; `run_optimize` is the profile-consuming build. Both call the
//! same Linux bundler as `cargo xtask bundle`, so a PGO build and a plain build
//! cannot drift apart.

#![allow(
    clippy::disallowed_methods,
    reason = "pgo drives xvfb-run, llvm-profdata, and the training harness"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};
use clap::{Args, Subcommand};

use crate::tasks::build::container::{ContainerPolicy, ContainerSpec};
use crate::tasks::build::model::{Channel, Format, Platform, Profile, Target};
use crate::tasks::build::plan::{
    BuildRequest, PgoMode, PgoPlan, TargetPlan, Task, assemble_tasks, container_info,
    container_spec, pgo_profile_path,
};
use crate::tasks::build::remote_server;
use crate::tasks::build::scheduler::{self, ExecuteOptions};
use crate::tasks::bundle::{self, linux, util};

/// A profile that covers fewer functions was not produced by a real training
/// run. Both `pgo verify` and the release gate use the same floor.
pub const MIN_FUNCTIONS: u64 = 100_000;

#[derive(Args)]
pub struct PgoArgs {
    #[command(subcommand)]
    command: PgoCommand,
}

#[derive(Subcommand)]
enum PgoCommand {
    /// Build an instrumented bundle, train it, and merge the profile.
    Train(PgoBuildArgs),
    /// Build the shipping bundle from an existing profile.
    Build(PgoBuildArgs),
    /// Internal: the instrumented build, the training run, and the merge.
    #[command(hide = true)]
    Generate(PgoStageArgs),
    /// Internal: the profile-consuming build.
    #[command(hide = true)]
    Optimize(PgoStageArgs),
    /// Print a profile summary and fail when it is empty or too small.
    Verify {
        /// Profile to inspect (defaults to the one for the host target).
        profile: Option<PathBuf>,
        #[arg(long, default_value_t = MIN_FUNCTIONS)]
        min_functions: u64,
        #[arg(long)]
        target: Option<Target>,
    },
    /// Show where the profile for a target lives and whether it is current.
    Status {
        #[arg(long)]
        target: Option<Target>,
    },
}

#[derive(Args, Clone)]
struct PgoBuildArgs {
    #[arg(long = "target", value_parser = parse_target)]
    targets: Vec<Target>,
    #[arg(long, value_enum, default_value = "auto")]
    container: ContainerPolicy,
    #[arg(long)]
    jobs: Option<u32>,
    #[arg(long)]
    offline: bool,
    #[arg(long)]
    no_sccache: bool,
    /// Artifacts `pgo build` produces. `pgo train` always builds only the
    /// tarball it trains from, so this does not change the profile.
    #[arg(long, value_enum, value_delimiter = ',')]
    formats: Vec<Format>,
    #[arg(long)]
    channel: Option<Channel>,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    keep_going: bool,
}

/// The arguments the internal steps need. The planner supplies the target
/// directory and channel it already resolved, so both stages build into the same
/// tree as the release bundle they belong to.
#[derive(Args)]
struct PgoStageArgs {
    #[arg(long, value_parser = parse_target)]
    target: Target,
    /// Cargo target directory (defaults to `target`).
    #[arg(long)]
    target_dir: Option<PathBuf>,
    #[arg(long, value_enum)]
    channel: Option<Channel>,
    #[arg(long)]
    jobs: Option<u32>,
    #[arg(long)]
    offline: bool,
    #[arg(long, value_enum, value_delimiter = ',')]
    formats: Vec<Format>,
}

pub fn run_pgo(args: PgoArgs) -> Result<()> {
    let workspace = std::env::current_dir()?;
    match args.command {
        PgoCommand::Train(args) => run_training(&workspace, args),
        PgoCommand::Build(args) => run_build(&workspace, args),
        PgoCommand::Generate(args) => run_generate(&workspace, args),
        PgoCommand::Optimize(args) => run_optimize(&workspace, args),
        PgoCommand::Verify {
            profile,
            min_functions,
            target,
        } => {
            let profile = if let Some(profile) = profile {
                absolute(&workspace, &profile)
            } else {
                let target = target.unwrap_or_else(Target::host_default);
                pgo_profile_path(&workspace, target, Profile::Release)?.1
            };
            verify_profile(&profile, min_functions)?;
            Ok(())
        }
        PgoCommand::Status { target } => run_status(&workspace, target),
    }
}

// ---------------------------------------------------------------------------
// The planner-facing verbs
// ---------------------------------------------------------------------------

fn run_training(workspace: &Path, args: PgoBuildArgs) -> Result<()> {
    let target = single_target(&args)?;
    if target.platform() != Platform::Linux {
        bail!(
            "PGO training only exists for Linux targets; {} has no harness yet",
            target.triple()
        );
    }
    if Platform::host() != Platform::Linux {
        bail!("PGO training needs a Linux host with Xvfb");
    }

    let channel = args.channel.unwrap_or_else(|| Channel::detect(workspace));
    let jobs = args.jobs.unwrap_or_else(default_jobs);
    let request = BuildRequest {
        targets: vec![target],
        formats: args.formats.clone(),
        profile: Profile::Release,
        channel,
        container: args.container,
        pgo: PgoMode::Train,
        jobs: args.jobs,
        offline: args.offline,
        sccache: !args.no_sccache,
        min_pgo_functions: MIN_FUNCTIONS,
        require_pgo: false,
    };

    let (fingerprint, profile) = pgo_profile_path(workspace, target, Profile::Release)?;
    let directory = profile
        .parent()
        .ok_or_else(|| anyhow::anyhow!("the PGO profile path has no parent directory"))?;
    let raw = directory.join("raw");
    util::create_dir(&raw)?;

    let spec = container_spec(workspace, &request)?;
    // A containerized run brings its own Xvfb; only a host build needs the
    // tool installed here.
    if spec.is_none() {
        util::require_tool(
            "xvfb-run",
            "PGO training runs the editor under a virtual X server; install xvfb.",
        )?;
    }
    let target_dir = if spec.is_some() {
        "target/container"
    } else {
        "target"
    };
    let mut command = vec![
        "xvfb-run".to_owned(),
        "--auto-servernum".to_owned(),
        "--server-args=-screen 0 1600x1200x24".to_owned(),
    ];
    command.extend(stage_command(
        "generate",
        target,
        target_dir,
        channel,
        jobs,
        &args.formats,
        args.offline,
    ));

    let mut environment = BTreeMap::new();
    // The bundler reads this and switches to an instrumented build. Setting it
    // in the task environment (rather than mutating this process's variables)
    // is what lets the inner step stay a plain command.
    environment.insert(
        "ZZZ_PGO_GENERATE_DIR".to_owned(),
        map_path(spec.as_ref(), &raw),
    );
    // A virtual X server has no GPU, so the training run has to use llvmpipe.
    environment.insert("LIBGL_ALWAYS_SOFTWARE".to_owned(), "1".to_owned());
    environment.insert("CARGO_BUILD_JOBS".to_owned(), jobs.to_string());
    environment.insert("CARGO_INCREMENTAL".to_owned(), "0".to_owned());
    if args.offline {
        environment.insert("CARGO_NET_OFFLINE".to_owned(), "true".to_owned());
    }
    add_remote_server_environment(workspace, spec.as_ref(), &mut environment)?;

    let info = container_info(spec.as_ref());
    let command = match &spec {
        Some(spec) => spec.wrap(&command, &environment),
        None => command,
    };

    let mut tasks = Vec::new();
    if let Some(spec) = &spec {
        tasks.push(toolchain_task(spec));
    }
    tasks.push(Task {
        id: "pgo:train".to_owned(),
        description: format!("Train the PGO profile for {}", target.triple()),
        depends_on: spec
            .as_ref()
            .map_or_else(Vec::new, |_| vec!["toolchain".to_owned()]),
        jobs: jobs.max(1),
        guard: None,
        command,
        environment,
        container: info.clone(),
    });

    let plan = assemble_tasks(
        workspace,
        &request,
        tasks,
        vec![TargetPlan {
            target,
            formats: request.formats.clone(),
            container: info,
            pgo: Some(PgoPlan {
                mode: PgoMode::Train,
                fingerprint,
                profile,
                min_functions: MIN_FUNCTIONS,
            }),
            artifacts: Vec::new(),
        }],
    )?;
    scheduler::execute(
        &plan,
        ExecuteOptions {
            dry_run: args.dry_run,
            keep_going: args.keep_going,
            verbose: false,
        },
    )
}

fn run_build(workspace: &Path, args: PgoBuildArgs) -> Result<()> {
    let target = single_target(&args)?;
    let channel = args.channel.unwrap_or_else(|| Channel::detect(workspace));
    let jobs = args.jobs.unwrap_or_else(default_jobs);
    let request = BuildRequest {
        targets: vec![target],
        formats: args.formats.clone(),
        profile: Profile::Release,
        channel,
        container: args.container,
        pgo: PgoMode::Use,
        jobs: args.jobs,
        offline: args.offline,
        sccache: !args.no_sccache,
        min_pgo_functions: MIN_FUNCTIONS,
        require_pgo: true,
    };

    let (fingerprint, profile) = pgo_profile_path(workspace, target, Profile::Release)?;
    if !profile.is_file() {
        bail!(
            "no PGO profile at {}; run `cargo xtask pgo train` first",
            profile.display()
        );
    }
    // Fail here, on the host, instead of after a container has compiled for
    // twenty minutes and only then rejected the profile.
    verify_profile(&profile, MIN_FUNCTIONS)?;

    let spec = container_spec(workspace, &request)?;
    let target_dir = if spec.is_some() {
        "target/container"
    } else {
        "target"
    };
    let command = stage_command(
        "optimize",
        target,
        target_dir,
        channel,
        jobs,
        &args.formats,
        args.offline,
    );

    let mut environment = BTreeMap::new();
    environment.insert(
        "ZZZ_PGO_PROFILE".to_owned(),
        map_path(spec.as_ref(), &profile),
    );
    environment.insert("CARGO_BUILD_JOBS".to_owned(), jobs.to_string());
    environment.insert("CARGO_INCREMENTAL".to_owned(), "0".to_owned());
    if args.offline {
        environment.insert("CARGO_NET_OFFLINE".to_owned(), "true".to_owned());
    }
    add_remote_server_environment(workspace, spec.as_ref(), &mut environment)?;

    let info = container_info(spec.as_ref());
    let command = match &spec {
        Some(spec) => spec.wrap(&command, &environment),
        None => command,
    };

    let mut tasks = Vec::new();
    if let Some(spec) = &spec {
        tasks.push(toolchain_task(spec));
    }
    tasks.push(Task {
        id: "pgo:build".to_owned(),
        description: format!("Build {} with the PGO profile", target.triple()),
        depends_on: spec
            .as_ref()
            .map_or_else(Vec::new, |_| vec!["toolchain".to_owned()]),
        jobs: jobs.max(1),
        guard: None,
        command,
        environment,
        container: info.clone(),
    });

    let plan = assemble_tasks(
        workspace,
        &request,
        tasks,
        vec![TargetPlan {
            target,
            formats: request.formats.clone(),
            container: info,
            pgo: Some(PgoPlan {
                mode: PgoMode::Use,
                fingerprint,
                profile,
                min_functions: MIN_FUNCTIONS,
            }),
            artifacts: Vec::new(),
        }],
    )?;
    scheduler::execute(
        &plan,
        ExecuteOptions {
            dry_run: args.dry_run,
            keep_going: args.keep_going,
            verbose: false,
        },
    )
}

/// One command grammar for the two stages, so the inner process is always
/// started with the arguments it needs to reproduce the planner's decisions.
fn stage_command(
    stage: &str,
    target: Target,
    target_dir: &str,
    channel: Channel,
    jobs: u32,
    formats: &[Format],
    offline: bool,
) -> Vec<String> {
    let mut command = vec![
        "cargo".to_owned(),
        "xtask".to_owned(),
        "pgo".to_owned(),
        stage.to_owned(),
        "--target".to_owned(),
        target.triple().to_owned(),
        "--target-dir".to_owned(),
        target_dir.to_owned(),
        "--channel".to_owned(),
        channel.as_str().to_owned(),
        "--jobs".to_owned(),
        jobs.to_string(),
    ];
    if !formats.is_empty() {
        command.push("--formats".to_owned());
        command.push(
            formats
                .iter()
                .map(|format| format.as_str())
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    if offline {
        command.push("--offline".to_owned());
    }
    command
}

fn toolchain_task(spec: &ContainerSpec) -> Task {
    Task {
        id: "toolchain".to_owned(),
        description: "Build the pinned toolchain image".to_owned(),
        depends_on: Vec::new(),
        jobs: 1,
        guard: Some(crate::tasks::build::plan::Guard::ImageAlreadyBuilt {
            engine: spec.engine.binary().to_owned(),
            image: spec.image.clone(),
        }),
        command: spec.image_build_command(),
        environment: BTreeMap::new(),
        container: None,
    }
}

/// Points the embed build script at the cached matrix when one is complete. A
/// PGO build is otherwise a third full compile of every remote server.
///
/// This runs on the host while the PGO stages run in the container, so the
/// capabilities have to describe the stage's environment, not this one.
fn add_remote_server_environment(
    workspace: &Path,
    spec: Option<&ContainerSpec>,
    environment: &mut BTreeMap<String, String>,
) -> Result<()> {
    let capabilities = remote_server::Capabilities::for_container(spec);
    let directory = remote_server::layout_for(workspace, capabilities)?.directory;
    if remote_server::is_complete_at(&directory)? {
        environment.insert(
            "ZZZ_EMBED_REMOTE_SERVER_DIR".to_owned(),
            map_path(spec, &directory),
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The internal steps, executed inside the planned environment
// ---------------------------------------------------------------------------

fn run_generate(workspace: &Path, args: PgoStageArgs) -> Result<()> {
    if std::env::var_os("ZZZ_PGO_GENERATE_DIR").is_none() {
        bail!("`pgo generate` is an internal step; run `cargo xtask pgo train` instead");
    }

    let mut context = stage_context(workspace, &args)?;
    // Training only needs the tarball it extracts. Packaging the instrumented
    // binary into debs or rpms would leave slow artifacts next to the shipping
    // ones, and only the extracted bundle is used.
    context.formats = std::iter::once(Format::TarGz).collect();
    let (_, profile) = pgo_profile_path(workspace, context.target, context.profile)?;
    let directory = profile
        .parent()
        .ok_or_else(|| anyhow::anyhow!("the PGO profile path has no parent directory"))?
        .to_path_buf();
    let raw = directory.join("raw");

    eprintln!("PGO: instrumenting the host build into {}", raw.display());
    linux::bundle(&context)?;

    let archive = linux::find_archive(&context)?;
    eprintln!("PGO: extracting {}", archive.display());
    let bundle_dir = directory.join("bundle");
    util::remove(&bundle_dir)?;
    util::create_dir(&bundle_dir)?;
    util::extract(&archive, &bundle_dir)?;
    let app = linux::single_app_directory(&bundle_dir)?;

    // The editor resolves its bundled libraries through `$ORIGIN/../lib`, so
    // training has to use the packaged layout, not the raw cargo artifact.
    let trainer = workspace.join("script/pgo-train");
    let mut command = Command::new(&trainer);
    command.current_dir(workspace).arg(&app);
    util::run(&mut command, "script/pgo-train")?;

    let raw_profiles = raw_profiles(&raw)?;
    if raw_profiles.is_empty() {
        bail!(
            "the training session wrote no .profraw files to {}.\nThe editor has to shut \
             down normally for the profile to be flushed; quit it with the quit command \
             instead of killing it.",
            raw.display()
        );
    }

    let profdata = find_llvm_profdata()?;
    verify_llvm_major(&profdata)?;
    eprintln!(
        "PGO: merging {} raw profile(s) with {}",
        raw_profiles.len(),
        profdata.display()
    );
    util::remove(&profile)?;
    let mut command = Command::new(&profdata);
    command
        .arg("merge")
        .arg("-o")
        .arg(&profile)
        .args(&raw_profiles);
    util::run(&mut command, "llvm-profdata merge")?;
    verify_profile(&profile, MIN_FUNCTIONS)?;

    // The instrumented archive shares its name with the release artifact, and
    // shipping it by accident is hard to notice: it is several times slower.
    util::remove(&archive)?;

    eprintln!();
    eprintln!("PGO profile ready: {}", profile.display());
    eprintln!("Build the optimized bundle with: cargo xtask pgo build");
    Ok(())
}

fn run_optimize(workspace: &Path, args: PgoStageArgs) -> Result<()> {
    if std::env::var_os("ZZZ_PGO_PROFILE").is_none() {
        bail!("`pgo optimize` is an internal step; run `cargo xtask pgo build` instead");
    }

    let context = stage_context(workspace, &args)?;
    let (_, profile) = pgo_profile_path(workspace, context.target, context.profile)?;
    verify_profile(&profile, MIN_FUNCTIONS)?;
    eprintln!("PGO: building with {}", profile.display());
    linux::bundle(&context)
}

/// Builds the bundler's view of the world from the planner's decisions. The
/// instrument/profile switch itself still arrives through `ZZZ_PGO_*`, which is
/// how `cargo xtask bundle` consumes it too.
fn stage_context(workspace: &Path, args: &PgoStageArgs) -> Result<bundle::Context> {
    let channel = args.channel.unwrap_or_else(|| Channel::detect(workspace));
    let target_dir = match &args.target_dir {
        Some(path) if path.is_absolute() => path.clone(),
        Some(path) => workspace.join(path),
        None => workspace.join("target"),
    };
    let formats = if args.formats.is_empty() {
        args.target.default_formats()
    } else {
        args.formats.clone()
    };

    Ok(bundle::Context {
        workspace: workspace.to_path_buf(),
        target: args.target,
        profile: Profile::Release,
        channel,
        formats: formats.into_iter().collect(),
        target_dir,
        skip_build: false,
        jobs: args.jobs.unwrap_or_else(default_jobs),
        offline: args.offline,
        install: false,
        open: false,
        robustness: false,
        sign: false,
        release_version: util::crate_version(workspace)?,
    })
}

fn raw_profiles(directory: &Path) -> Result<Vec<PathBuf>> {
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(directory)
        .with_context(|| format!("failed to read {}", directory.display()))?;
    let mut profiles = Vec::new();
    for entry in entries {
        let path = entry
            .with_context(|| format!("failed to read {}", directory.display()))?
            .path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "profraw")
        {
            profiles.push(path);
        }
    }
    profiles.sort();
    Ok(profiles)
}

// ---------------------------------------------------------------------------
// Profile inspection
// ---------------------------------------------------------------------------

fn run_status(workspace: &Path, target: Option<Target>) -> Result<()> {
    let targets: Vec<Target> = match target {
        Some(target) => vec![target],
        None => vec![Target::host_default()],
    };

    for target in targets {
        let (fingerprint, profile) = pgo_profile_path(workspace, target, Profile::Release)?;
        println!("{}", target.triple());
        println!("  fingerprint: {fingerprint}");
        println!("  profile:     {}", profile.display());
        if !profile.is_file() {
            println!("  status:      missing (run `cargo xtask pgo train`)");
            continue;
        }
        let size = std::fs::metadata(&profile).map_or(0, |metadata| metadata.len());
        println!("  size:        {} MiB", size / (1024 * 1024));
        match profile_function_count(&profile) {
            Ok(count) => println!("  functions:   {count}"),
            Err(error) => println!("  functions:   unknown ({error})"),
        }
    }
    Ok(())
}

fn verify_profile(profile: &Path, min_functions: u64) -> Result<u64> {
    if !profile.is_file() {
        bail!("no profile at {}", profile.display());
    }
    let count = profile_function_count(profile)?;
    if count < min_functions {
        bail!(
            "{} covers {count} functions, below the required {min_functions}",
            profile.display()
        );
    }
    println!(
        "{} covers {count} functions (minimum {min_functions})",
        profile.display()
    );
    Ok(count)
}

fn profile_function_count(profile: &Path) -> Result<u64> {
    let profdata = find_llvm_profdata()?;
    verify_llvm_major(&profdata)?;
    let output = Command::new(&profdata)
        .args(["show", "--topn=0"])
        .arg(profile)
        .output()
        .with_context(|| format!("failed to run {}", profdata.display()))?;
    if !output.status.success() {
        bail!(
            "{} rejected {}: {}",
            profdata.display(),
            profile.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .find_map(|line| line.strip_prefix("Total functions:"))
        .and_then(|value| value.trim().parse().ok())
        .ok_or_else(|| anyhow::anyhow!("{} did not report a function count", profdata.display()))
}

/// Prefers the copy shipped with the active toolchain: a merger from a different
/// LLVM major version rejects the raw profiles.
pub(crate) fn find_llvm_profdata() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("LLVM_PROFDATA") {
        let path = PathBuf::from(path);
        if !path.is_file() {
            bail!("LLVM_PROFDATA does not exist: {}", path.display());
        }
        return Ok(path);
    }

    let sysroot = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    let host = Command::new("rustc")
        .args(["-vV"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .find_map(|line| line.strip_prefix("host: ").map(str::to_owned))
        });

    if let (Some(sysroot), Some(host)) = (sysroot, host) {
        let candidate = PathBuf::from(sysroot)
            .join("lib/rustlib")
            .join(host)
            .join("bin/llvm-profdata");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join("llvm-profdata");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    bail!(
        "llvm-profdata not found; run `rustup component add llvm-tools-preview` or set LLVM_PROFDATA"
    )
}

/// A merger from a different LLVM major version rejects the raw profiles, and
/// the failure would otherwise surface as an opaque error from `merge`.
fn verify_llvm_major(profdata: &Path) -> Result<()> {
    let Some(profdata_major) = command_llvm_major(Command::new(profdata).arg("--version")) else {
        return Ok(());
    };
    let Some(rustc_major) = command_llvm_major(Command::new("rustc").arg("-vV")) else {
        return Ok(());
    };
    if profdata_major != rustc_major {
        bail!(
            "{} is LLVM {profdata_major}.x, but rustc uses LLVM {rustc_major}.x.\nThe profile \
             has to be merged with the LLVM that produced it. Install the toolchain's copy \
             with `rustup component add llvm-tools-preview`, or point LLVM_PROFDATA at a \
             matching llvm-profdata.",
            profdata.display()
        );
    }
    Ok(())
}

fn command_llvm_major(command: &mut Command) -> Option<String> {
    let output = command
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    llvm_major(&String::from_utf8_lossy(&output.stdout))
}

/// `rustc -vV` prints `LLVM version: 23.1.1`; `llvm-profdata --version` prints
/// `LLVM version 23.1.4` under its banner. Only the major version matters.
fn llvm_major(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("LLVM version")?;
        let version = rest.trim_start_matches([':', ' ']).trim();
        let major = version.split('.').next()?;
        major
            .chars()
            .all(|character| character.is_ascii_digit())
            .then(|| major.to_owned())
    })
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn single_target(args: &PgoBuildArgs) -> Result<Target> {
    match args.targets.as_slice() {
        [] => Ok(Target::host_default()),
        [target] => Ok(*target),
        _ => bail!("PGO operates on one target at a time"),
    }
}

fn map_path(spec: Option<&ContainerSpec>, path: &Path) -> String {
    match spec {
        Some(spec) => spec.path_for(path),
        None => path.display().to_string(),
    }
}

fn absolute(workspace: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace.join(path)
    }
}

fn default_jobs() -> u32 {
    std::thread::available_parallelism().map_or(1, |count| u32::try_from(count.get()).unwrap_or(1))
}

fn parse_target(value: &str) -> Result<Target, String> {
    value.parse::<Target>().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_llvm_banner_formats_both_report_the_same_major() {
        assert_eq!(
            llvm_major("rustc 1.95.0\nLLVM version: 23.1.1\n"),
            Some("23".to_owned())
        );
        assert_eq!(
            llvm_major("LLVM version 23.1.4\n  Optimized build.\n"),
            Some("23".to_owned())
        );
    }

    #[test]
    fn an_unrelated_banner_reports_no_version() {
        assert_eq!(llvm_major("llvm-profdata 23\n"), None);
    }
}
