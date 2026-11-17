//! Builds the whole remote-server matrix once and caches it by content.
//!
//! `crates/remote_server_embed/build.rs` used to build every remote-server
//! target while compiling the editor. That meant a fresh matrix build on every
//! clean checkout, a `target/` lock shared with the editor build, and no way to
//! reuse the result for a second bundle. This module hoists the matrix into one
//! explicit task: it builds all six targets, names each archive `<os>-<arch>`,
//! and stores them under a fingerprint of the inputs that produced them.
//!
//! Bundlers then point `ZZZ_EMBED_REMOTE_SERVER_DIR` at the cache, so
//! `remote_server_embed` only copies files instead of compiling.

#![allow(
    clippy::disallowed_methods,
    reason = "the matrix build drives cargo, zig, and the platform toolchains"
)]

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, bail};
use clap::{Args, Subcommand};
use flate2::Compression;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};

use crate::tasks::build::model::{Platform, Target};

pub const SCHEMA_VERSION: u32 = 1;
pub const CACHE_DIRECTORY: &str = "target/remote-server";
const ARCHIVE_PREFIX: &str = "zzz-remote-server";
const DEFAULT_KEEP: usize = 3;

/// Every triple the editor embeds. The order is stable so plans and logs are
/// readable and the same build always writes the same manifest.
pub const MATRIX: [Target; 8] = [
    Target::LinuxX86_64Musl,
    Target::LinuxAarch64Musl,
    Target::MacX86_64,
    Target::MacAarch64,
    Target::WindowsX86_64,
    Target::WindowsAarch64,
    Target::FreeBSDX86_64,
    Target::FreeBSDAarch64,
];

#[derive(Subcommand)]
pub enum RemoteServerCommand {
    /// Build every remote-server target and store the archives in the cache.
    Build(BuildArgs),
    /// Print the cache location and whether it is usable.
    Status,
    /// Remove cached matrices, keeping the newest one.
    Clean(CleanArgs),
}

#[derive(Args)]
pub struct RemoteServerArgs {
    #[command(subcommand)]
    pub command: RemoteServerCommand,
}

#[derive(Args, Clone, Debug)]
pub struct BuildArgs {
    /// Parallel Cargo jobs across the whole matrix.
    #[arg(long)]
    pub jobs: Option<u32>,
    /// Build only these triples (repeatable). Defaults to the whole matrix.
    #[arg(long = "target", value_parser = parse_target)]
    pub targets: Vec<Target>,
    /// Fail instead of skipping targets whose cross toolchain is unavailable.
    #[arg(long)]
    pub require_all: bool,
    /// Rebuild even when a complete cache already exists.
    #[arg(long)]
    pub force: bool,
    /// Do not download a macOS SDK or Cargo dependencies.
    #[arg(long)]
    pub offline: bool,
    /// Keep this many matrices, including the current one.
    #[arg(long, default_value_t = DEFAULT_KEEP)]
    pub keep: usize,
    /// Print the cache directory without building.
    #[arg(long)]
    pub print_cache: bool,
    /// Do not print the per-target summary on success.
    #[arg(long)]
    pub quiet: bool,
}

#[derive(Args)]
pub struct CleanArgs {
    /// Remove every cached matrix instead of keeping the newest one.
    #[arg(long)]
    pub all: bool,
}

pub fn run(args: RemoteServerArgs) -> Result<()> {
    let workspace = std::env::current_dir()?;
    match args.command {
        RemoteServerCommand::Build(args) => {
            let print_cache = args.print_cache;
            let built = build(&workspace, &args)?;
            if !args.quiet && !print_cache {
                let state = if built.reused { "reused" } else { "built" };
                println!(
                    "remote-server cache ({state}): {}",
                    built.directory.display()
                );
                println!("fingerprint: {}", built.fingerprint);
            }
            Ok(())
        }
        RemoteServerCommand::Status => status(&workspace),
        RemoteServerCommand::Clean(args) => clean(&workspace, args.all),
    }
}

/// A matrix that has been built (or reused).
#[derive(Clone, Debug)]
pub struct Built {
    pub directory: PathBuf,
    pub fingerprint: String,
    pub reused: bool,
}

/// Location and identity of the cache for the current checkout.
#[derive(Clone, Debug)]
pub struct Layout {
    pub fingerprint: String,
    pub directory: PathBuf,
}

/// The toolchains a matrix needs on top of `rustup target add`, as seen by the
/// environment that runs it.
///
/// They belong in the cache key. A matrix built without them records the
/// targets as skipped so `status` can still call the run complete; without this
/// in the fingerprint, enabling them later would silently reuse that cache.
#[derive(Clone, Copy, Debug)]
pub struct Capabilities {
    /// zig and cargo-zigbuild, which every cross target needs.
    pub cross: bool,
    /// The msvc-wine layer, which the Windows targets need.
    pub msvc: bool,
}

impl Capabilities {
    /// What this process can do.
    pub fn detect() -> Self {
        Self {
            cross: zig_available(),
            msvc: crate::tasks::build::msvc::available(),
        }
    }

    /// What the environment that builds the matrix can do.
    ///
    /// The planner assembles its task graph on the host while the matrix is
    /// built inside the toolchain container, and that image always ships zig
    /// and gains the msvc-wine layer when a Windows target is planned. Hashing
    /// this host's version would point every bundler at a directory the
    /// container never writes.
    pub fn for_container(spec: Option<&crate::tasks::build::container::ContainerSpec>) -> Self {
        match spec {
            // `ContainerSpec::msvc` already reflects `ZZZ_WITH_MSVC` and any
            // Windows target in the request.
            Some(spec) => Self {
                cross: true,
                msvc: spec.msvc,
            },
            None => Self::detect(),
        }
    }
}

pub fn layout(workspace: &Path) -> Result<Layout> {
    layout_for(workspace, Capabilities::detect())
}

/// Like [`layout`], but with the capabilities supplied by the caller.
pub fn layout_for(workspace: &Path, capabilities: Capabilities) -> Result<Layout> {
    let fingerprint = fingerprint(workspace, capabilities)?;
    Ok(Layout {
        directory: workspace.join(CACHE_DIRECTORY).join(&fingerprint),
        fingerprint,
    })
}

/// Ensures a usable matrix exists without rebuilding one that is complete.
///
/// Bundlers call this so a standalone `cargo xtask bundle` still works when the
/// planner did not run the `remote-server` task first.
pub fn ensure(workspace: &Path, offline: bool, jobs: Option<u32>) -> Result<PathBuf> {
    let layout = layout(workspace)?;
    if is_complete(&layout.directory, &MATRIX)? {
        return Ok(layout.directory);
    }
    let built = build(
        workspace,
        &BuildArgs {
            jobs,
            targets: Vec::new(),
            require_all: false,
            force: false,
            offline,
            keep: DEFAULT_KEEP,
            print_cache: false,
            quiet: true,
        },
    )?;
    Ok(built.directory)
}

pub fn build(workspace: &Path, args: &BuildArgs) -> Result<Built> {
    let layout = layout(workspace)?;
    let targets = if args.targets.is_empty() {
        MATRIX.to_vec()
    } else {
        args.targets.clone()
    };
    if targets.is_empty() {
        bail!("no remote-server targets requested");
    }

    if args.print_cache {
        // A script needs the path and nothing else; do not build a matrix just
        // to report where one would live.
        println!("{}", layout.directory.display());
        return Ok(Built {
            directory: layout.directory,
            fingerprint: layout.fingerprint,
            reused: true,
        });
    }

    if !args.force && is_complete(&layout.directory, &targets)? {
        eprintln!(
            "remote-server: reusing {} ({})",
            layout.fingerprint,
            layout.directory.display()
        );
        return Ok(Built {
            directory: layout.directory,
            fingerprint: layout.fingerprint,
            reused: true,
        });
    }

    let jobs = args.jobs.unwrap_or_else(default_jobs).max(1);
    crate::tasks::bundle::util::create_dir(&layout.directory)?;
    let work_root = workspace.join(CACHE_DIRECTORY).join("build");
    crate::tasks::bundle::util::create_dir(&work_root)?;
    let logs = layout.directory.join("logs");
    crate::tasks::bundle::util::create_dir(&logs)?;

    let sdk = if targets
        .iter()
        .any(|target| target.platform() == Platform::Mac)
    {
        match crate::tasks::bundle::macos_sdk::ensure(workspace, None, args.offline) {
            Ok(path) => Some(path),
            Err(error) => {
                if args.require_all {
                    return Err(error)
                        .context("the macOS SDK is required for the remote-server matrix");
                }
                eprintln!("remote-server: skipping macOS targets: {error}");
                None
            }
        }
    } else {
        None
    };

    // Decide up front which targets this environment can build, so a missing
    // cross toolchain is a recorded skip rather than a failure three minutes in.
    // The record also answers `remote-server status`: a matrix is complete when
    // every target is either built or skipped for a reason it can name.
    let mut plan = Vec::new();
    let mut skipped = Vec::new();
    for target in targets {
        match capability(target, sdk.as_deref()) {
            Ok(()) => plan.push(target),
            Err(reason) if args.require_all => {
                bail!(
                    "cannot build remote_server for {}: {reason}",
                    target.triple()
                )
            }
            Err(reason) => {
                eprintln!("remote-server: skipping {}: {reason}", target.triple());
                skipped.push(Skipped {
                    triple: target.triple().to_owned(),
                    reason,
                });
            }
        }
    }

    eprintln!(
        "remote-server: building {} target(s) with {jobs} job(s) into {}",
        plan.len(),
        layout.directory.display()
    );

    let concurrency = ((jobs / 4).clamp(1, 6) as usize).min(plan.len().max(1));
    let per_build_jobs = (jobs / u32::try_from(concurrency).unwrap_or(1)).max(1);

    let mut archives = Vec::new();
    let mut failures: Vec<(String, String)> = Vec::new();
    for group in plan.chunks(concurrency) {
        let results = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for target in group {
                let sdk = sdk.clone();
                let work_root = work_root.clone();
                let logs = logs.clone();
                let directory = layout.directory.clone();
                handles.push((
                    *target,
                    scope.spawn(move || {
                        build_one(
                            workspace,
                            *target,
                            &work_root,
                            &directory,
                            &logs,
                            per_build_jobs,
                            sdk.as_deref(),
                            args.offline,
                        )
                    }),
                ));
            }
            handles
                .into_iter()
                .map(|(target, handle)| (target, handle.join()))
                .collect::<Vec<_>>()
        });

        for (target, joined) in results {
            match joined {
                Ok(Ok(archive)) => archives.push(archive),
                Ok(Err(error)) => {
                    if let Some(archive) = linux_gnu_fallback(
                        target,
                        &error,
                        workspace,
                        &layout.directory,
                        &logs,
                        per_build_jobs,
                    )? {
                        archives.push(archive);
                    } else if args.require_all {
                        return Err(error);
                    } else {
                        // A compile or verification failure is not a capability
                        // skip. Recording it as one would make `is_complete`
                        // reuse a matrix with no archive for this target, and
                        // the next run would report success without building it.
                        eprintln!("remote-server: failed {}: {error}", target.triple());
                        failures.push((target.triple().to_owned(), format!("{error:#}")));
                    }
                }
                Err(_) => bail!("a remote-server build thread panicked"),
            }
        }
    }

    if archives.is_empty() {
        if failures.is_empty() {
            bail!(
                "no remote-server archive could be built; the editor would embed none. \
                 Install zig/cargo-zigbuild for the cross targets, or run this inside the \
                 toolchain container"
            );
        }
        bail!(
            "no remote-server archive could be built: {}",
            describe_failures(&failures)
        );
    }

    let manifest = Manifest {
        schema_version: SCHEMA_VERSION,
        fingerprint: layout.fingerprint.clone(),
        commit: git(workspace, &["rev-parse", "HEAD"]).unwrap_or_else(|_| "unknown".to_owned()),
        dirty: git(workspace, &["status", "--porcelain"]).is_ok_and(|output| !output.is_empty()),
        rustc: rustc_version(),
        host: host_triple().unwrap_or_else(|_| "unknown".to_owned()),
        created_at_unix: now_unix(),
        requested: targets_requested(args),
        archives: archives.clone(),
        skipped,
    };
    write_manifest(&layout.directory, &manifest)?;

    if !args.quiet {
        for archive in &archives {
            eprintln!(
                "  {} ({:.1} MiB)",
                archive.file,
                archive.bytes as f64 / (1024.0 * 1024.0)
            );
        }
    }
    prune(workspace, args.keep, &layout.fingerprint)?;
    warn_if_tree_changed(workspace, &layout.fingerprint);

    if !failures.is_empty() {
        bail!(
            "{} remote-server target(s) failed to build; the archives above are usable, \
             but the matrix is incomplete and will be rebuilt: {}",
            failures.len(),
            describe_failures(&failures)
        );
    }

    Ok(Built {
        directory: layout.directory,
        fingerprint: layout.fingerprint,
        reused: false,
    })
}

fn describe_failures(failures: &[(String, String)]) -> String {
    failures
        .iter()
        .map(|(triple, reason)| format!("{triple}: {reason}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Warns when the checkout moved on while the matrix was compiling.
///
/// A matrix build outlives many edits. The archives stay consistent with each
/// other, but the fingerprint no longer describes the tree, so the next run
/// would rebuild. Saying so beats leaving a cache that never matches.
fn warn_if_tree_changed(workspace: &Path, fingerprint: &str) {
    let Ok(current) = layout(workspace) else {
        return;
    };
    if current.fingerprint != fingerprint {
        eprintln!(
            "remote-server: the working tree changed while building, so this cache is keyed \
             {fingerprint} rather than {}. Run it again to key the matrix to the current tree.",
            current.fingerprint
        );
    }
}

fn targets_requested(args: &BuildArgs) -> Vec<String> {
    if args.targets.is_empty() {
        MATRIX
            .iter()
            .map(|target| target.triple().to_owned())
            .collect()
    } else {
        args.targets
            .iter()
            .map(|target| target.triple().to_owned())
            .collect()
    }
}

fn status(workspace: &Path) -> Result<()> {
    let layout = layout(workspace)?;
    println!("fingerprint: {}", layout.fingerprint);
    println!("cache:       {}", layout.directory.display());
    match read_manifest(&layout.directory)? {
        Some(manifest) => {
            println!(
                "manifest:    {} archive(s), {} skipped, built {}",
                manifest.archives.len(),
                manifest.skipped.len(),
                manifest.created_at_unix
            );
            for archive in &manifest.archives {
                let missing = !layout.directory.join(&archive.file).is_file();
                println!(
                    "  {}{}",
                    archive.file,
                    if missing { "  (missing)" } else { "" }
                );
            }
            for skipped in &manifest.skipped {
                println!("  skipped {}: {}", skipped.triple, skipped.reason);
            }
            println!(
                "complete:    {}",
                if is_complete(&layout.directory, &MATRIX)? {
                    "yes"
                } else {
                    "no"
                }
            );
        }
        None => println!("manifest:    none (the matrix has not been built)"),
    }
    Ok(())
}

fn clean(workspace: &Path, all: bool) -> Result<()> {
    let root = workspace.join(CACHE_DIRECTORY);
    if !root.is_dir() {
        println!("nothing to clean: {}", root.display());
        return Ok(());
    }
    let mut entries = matrices(&root)?;
    if all {
        for (_, path) in entries {
            crate::tasks::bundle::util::remove(&path)?;
        }
        crate::tasks::bundle::util::remove(&root.join("build"))?;
        println!("removed every remote-server matrix in {}", root.display());
    } else {
        entries.sort_by_key(|(modified, _)| *modified);
        entries.reverse();
        for (_, path) in entries.into_iter().skip(1) {
            crate::tasks::bundle::util::remove(&path)?;
        }
        println!("kept the newest remote-server matrix in {}", root.display());
    }
    Ok(())
}

fn capability(target: Target, sdk: Option<&Path>) -> std::result::Result<(), String> {
    match target.platform() {
        Platform::Linux => {
            if target.is_cross() {
                if !zig_available() && musl_cc(target.triple()).is_none() {
                    return Err(
                        "zig, cargo-zigbuild, and a cross musl C compiler are all missing"
                            .to_owned(),
                    );
                }
            } else if target.is_musl() && musl_cc(target.triple()).is_none() && !zig_available() {
                return Err("a musl C compiler is not installed".to_owned());
            }
            Ok(())
        }
        Platform::Mac => {
            if Platform::host() != Platform::Mac {
                if !zig_available() {
                    return Err("zig and cargo-zigbuild are not installed".to_owned());
                }
                if sdk.is_none() {
                    return Err("the macOS SDK is unavailable".to_owned());
                }
            } else if target.is_cross() && !zig_available() {
                return Err("zig and cargo-zigbuild are not installed".to_owned());
            }
            Ok(())
        }
        Platform::Windows => {
            if Platform::host() == Platform::Linux
                && crate::tasks::build::msvc::MsvcEnvironment::detect(target.arch().as_str())
                    .is_err()
            {
                return Err("msvc-wine is not installed".to_owned());
            }
            Ok(())
        }
        Platform::FreeBSD => {
            let environment = crate::tasks::build::freebsd::FreeBSDEnvironment::detect(
                target,
                std::env::var_os(crate::tasks::build::freebsd::SYSROOT_VARIABLE)
                    .map(std::path::PathBuf::from)
                    .as_deref(),
            )
            .map_err(|error| error.to_string())?;
            environment
                .require_cross(target)
                .map_err(|error| error.to_string())
        }
    }
}

/// Builds one matrix target and stores its archive.
///
/// `directory` is the cache directory resolved when the run started. It is
/// passed in rather than re-derived because the fingerprint covers the working
/// tree, and a tree that changes while a ten-minute compile runs would otherwise
/// make each thread invent a different, possibly nonexistent destination.
fn build_one(
    workspace: &Path,
    target: Target,
    work_root: &Path,
    directory: &Path,
    logs: &Path,
    jobs: u32,
    sdk: Option<&Path>,
    offline: bool,
) -> Result<Archive> {
    let triple = target.triple();
    let work = work_root.join(triple);
    crate::tasks::bundle::util::create_dir(&work)?;
    crate::tasks::bundle::util::rustup_add_target(triple)?;

    invalidate_corrupt_aws_lc_sys(&work, triple);

    let zig = uses_zig(target);
    let mut rustflags = std::env::var("ZZZ_REMOTE_SERVER_RUSTFLAGS").unwrap_or_default();
    if target.is_musl() && !rustflags.contains("target-feature=+crt-static") {
        rustflags.push_str(" -C target-feature=+crt-static");
    }
    if zig {
        // rustc passes `-O1` to the linker on optimized builds; Zig's cc
        // wrapper reports it as a deprecated setting (rust-lang/rust#158192).
        rustflags.push_str(" -A linker_messages");
    }

    let mut command = Command::new("cargo");
    command
        .current_dir(workspace)
        .arg(if zig { "zigbuild" } else { "build" })
        .args([
            "--release",
            "--package",
            "remote_server",
            "--target",
            triple,
            "--target-dir",
        ])
        .arg(&work)
        .env("ZZZ_BUILDING_REMOTE_SERVER", "1")
        .env("ZZZ_SKIP_EMBED_REMOTE_SERVER", "1")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env("RUSTFLAGS", rustflags.trim())
        .env("CARGO_BUILD_JOBS", jobs.to_string());
    if offline {
        command.env("CARGO_NET_OFFLINE", "true");
    }
    if zig {
        crate::tasks::bundle::util::configure_zig(&mut command, workspace, triple);
    }
    if let Some(cc) = musl_cc(triple).filter(|_| target.is_musl()) {
        command.env(format!("CC_{}", triple.replace('-', "_")), cc);
    }
    if target.platform() == Platform::Mac
        && let Some(sdk) = sdk
    {
        command.env("SDKROOT", sdk).env_remove("CXXFLAGS");
    }
    if target.platform() == Platform::Windows && Platform::host() == Platform::Linux {
        let environment =
            crate::tasks::build::msvc::MsvcEnvironment::detect(target.arch().as_str())?;
        environment.apply(&mut command);
    }
    if target.platform() == Platform::FreeBSD {
        let environment = crate::tasks::build::freebsd::FreeBSDEnvironment::detect(
            target,
            std::env::var_os(crate::tasks::build::freebsd::SYSROOT_VARIABLE)
                .map(std::path::PathBuf::from)
                .as_deref(),
        )?;
        environment.apply(target, &mut command, &rustflags)?;
    }

    eprintln!("remote-server: compiling {triple}");
    let output = command
        .output()
        .with_context(|| format!("failed to start cargo for {triple}"))?;
    let log_path = logs.join(format!("{triple}.log"));
    fs::write(&log_path, &output.stderr)
        .with_context(|| format!("failed to write {}", log_path.display()))?;
    if !output.status.success() {
        let log = String::from_utf8_lossy(&output.stderr);
        let tail = log.lines().rev().take(40).collect::<Vec<_>>();
        bail!(
            "cargo failed for {triple} ({}); see {}\n{}",
            output.status,
            log_path.display(),
            tail.into_iter().rev().collect::<Vec<_>>().join("\n")
        );
    }

    let binary_name = if target.platform() == Platform::Windows {
        "remote_server.exe"
    } else {
        "remote_server"
    };
    let binary = work.join(triple).join("release").join(binary_name);
    if !binary.is_file() {
        bail!(
            "the build produced no {binary_name} at {}",
            binary.display()
        );
    }
    ensure_no_openssl(&binary, target)?;
    archive_binary(directory, target, &binary)
}

/// The embedded remote server must not link OpenSSL.
///
/// The editor ships this archive to every SSH remote host; a `libssl`
/// dependency would make it unstartable where the library is missing or a
/// different ABI. Only the musl build is fatal when the dependency shows up —
/// the glibc fallback is an explicit compatibility path.
fn ensure_no_openssl(binary: &Path, target: Target) -> Result<()> {
    if target.platform() != Platform::Linux || target.is_cross() {
        return Ok(());
    }
    let Ok(output) = Command::new("ldd").arg(binary).output() else {
        return Ok(());
    };
    let text = String::from_utf8_lossy(&output.stdout);
    if !text.contains("libssl") && !text.contains("libcrypto") {
        return Ok(());
    }
    if target.is_musl() {
        bail!("remote_server still depends on libssl or libcrypto");
    }
    eprintln!("remote-server: using the non-musl build, which links OpenSSL");
    Ok(())
}

/// The matrix used to fall back to a glibc Linux build when musl was missing.
/// Keep that fallback for the host architecture so a source checkout still
/// produces an embeddable archive.
fn linux_gnu_fallback(
    target: Target,
    error: &anyhow::Error,
    workspace: &Path,
    directory: &Path,
    logs: &Path,
    jobs: u32,
) -> Result<Option<Archive>> {
    if target != Target::LinuxX86_64Musl
        || Platform::host() != Platform::Linux
        || std::env::consts::ARCH != "x86_64"
    {
        return Ok(None);
    }
    eprintln!("remote-server: musl build failed ({error}); trying the glibc archive");
    let work = workspace
        .join(CACHE_DIRECTORY)
        .join("build/x86_64-unknown-linux-gnu");
    crate::tasks::bundle::util::create_dir(&work)?;
    crate::tasks::bundle::util::rustup_add_target("x86_64-unknown-linux-gnu")?;
    let mut command = Command::new("cargo");
    command
        .current_dir(workspace)
        .args([
            "build",
            "--release",
            "--package",
            "remote_server",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--target-dir",
        ])
        .arg(&work)
        .env("ZZZ_BUILDING_REMOTE_SERVER", "1")
        .env("ZZZ_SKIP_EMBED_REMOTE_SERVER", "1")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .env("CARGO_BUILD_JOBS", jobs.to_string());
    let output = command.output().context("failed to start cargo")?;
    let log_path = logs.join("x86_64-unknown-linux-gnu.log");
    fs::write(&log_path, &output.stderr)?;
    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "the glibc fallback also failed; see {}",
            log_path.display()
        ));
    }
    let binary = work.join("x86_64-unknown-linux-gnu/release/remote_server");
    archive_binary(directory, target, &binary).map(Some)
}

fn uses_zig(target: Target) -> bool {
    if !zig_available() || matches!(target.platform(), Platform::Windows | Platform::FreeBSD) {
        // FreeBSD uses its own clang/lld driver and sysroot, not Zig's.
        return false;
    }
    if target.is_cross() {
        return true;
    }
    // A native musl target still needs Zig when there is no musl C compiler.
    target.is_musl() && musl_cc(target.triple()).is_none()
}

fn archive_binary(directory: &Path, target: Target, binary: &Path) -> Result<Archive> {
    let os = embed_os(target.platform());
    let arch = target.arch().as_str();
    let file_name = if target.platform() == Platform::Windows {
        format!("{ARCHIVE_PREFIX}-{os}-{arch}.zip")
    } else {
        format!("{ARCHIVE_PREFIX}-{os}-{arch}.gz")
    };
    let destination = directory.join(&file_name);
    if target.platform() == Platform::Windows {
        zip_file(binary, &destination)?;
    } else {
        gzip_file(binary, &destination)?;
    }
    let bytes = fs::metadata(&destination)?.len();
    Ok(Archive {
        triple: target.triple().to_owned(),
        os: os.to_owned(),
        arch: arch.to_owned(),
        file: file_name,
        sha256: crate::tasks::bundle::util::sha256_file(&destination)?,
        bytes,
    })
}

fn gzip_file(source: &Path, destination: &Path) -> Result<()> {
    let input =
        fs::File::open(source).with_context(|| format!("failed to open {}", source.display()))?;
    let output = fs::File::create(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    let mut encoder = GzEncoder::new(output, Compression::best());
    let mut reader = std::io::BufReader::new(input);
    std::io::copy(&mut reader, &mut encoder)
        .with_context(|| format!("failed to compress {}", source.display()))?;
    encoder
        .finish()
        .context("failed to finish the gzip stream")?;
    Ok(())
}

fn zip_file(source: &Path, destination: &Path) -> Result<()> {
    let name = source
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("{} has no file name", source.display()))?
        .to_string_lossy()
        .into_owned();
    let output = fs::File::create(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    let mut archive = zip::ZipWriter::new(output);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    archive
        .start_file(name, options)
        .context("failed to start the zip entry")?;
    let mut input =
        fs::File::open(source).with_context(|| format!("failed to open {}", source.display()))?;
    std::io::copy(&mut input, &mut archive).context("failed to write the zip entry")?;
    archive
        .finish()
        .context("failed to finish the zip archive")?;
    Ok(())
}

fn is_complete(directory: &Path, targets: &[Target]) -> Result<bool> {
    let Some(manifest) = read_manifest(directory)? else {
        return Ok(false);
    };
    if manifest.schema_version != SCHEMA_VERSION {
        return Ok(false);
    }
    for target in targets {
        let file = archive_file(*target);
        let found = manifest
            .archives
            .iter()
            .find(|archive| archive.file == file && archive.triple == target.triple());
        if let Some(archive) = found {
            let path = directory.join(&archive.file);
            if !path.is_file() {
                return Ok(false);
            }
            if crate::tasks::bundle::util::sha256_file(&path)? != archive.sha256 {
                return Ok(false);
            }
        } else {
            let skipped = manifest
                .skipped
                .iter()
                .any(|skipped| skipped.triple == target.triple());
            if !skipped {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn archive_file(target: Target) -> String {
    format!(
        "{}-{}-{}.{}",
        ARCHIVE_PREFIX,
        embed_os(target.platform()),
        target.arch().as_str(),
        if target.platform() == Platform::Windows {
            "zip"
        } else {
            "gz"
        }
    )
}

/// The operating-system name used by the embed contract. It says `macos`, not
/// the shorter `mac` the plan uses for task ids.
fn embed_os(platform: Platform) -> &'static str {
    match platform {
        Platform::Linux => "linux",
        Platform::Mac => "macos",
        Platform::Windows => "windows",
        Platform::FreeBSD => "freebsd",
    }
}

fn read_manifest(directory: &Path) -> Result<Option<Manifest>> {
    let path = directory.join("manifest.json");
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    match serde_json::from_str::<Manifest>(&contents) {
        Ok(manifest) => Ok(Some(manifest)),
        Err(error) => {
            eprintln!(
                "warning: ignoring an unreadable remote-server manifest at {}: {error}",
                path.display()
            );
            Ok(None)
        }
    }
}

fn write_manifest(directory: &Path, manifest: &Manifest) -> Result<()> {
    let path = directory.join("manifest.json");
    let temporary = directory.join("manifest.json.part");
    let mut file = fs::File::create(&temporary)
        .with_context(|| format!("failed to create {}", temporary.display()))?;
    file.write_all(serde_json::to_string_pretty(manifest)?.as_bytes())?;
    file.write_all(b"\n")?;
    fs::rename(&temporary, &path).with_context(|| {
        format!(
            "failed to move {} to {}",
            temporary.display(),
            path.display()
        )
    })?;
    Ok(())
}

fn prune(workspace: &Path, keep: usize, current: &str) -> Result<()> {
    let root = workspace.join(CACHE_DIRECTORY);
    let mut entries = matrices(&root)?;
    entries.retain(|(_, path)| path.file_name().and_then(|name| name.to_str()) != Some(current));
    entries.sort_by_key(|(modified, _)| *modified);
    entries.reverse();
    for (_, path) in entries.into_iter().skip(keep.saturating_sub(1)) {
        crate::tasks::bundle::util::remove(&path)?;
    }
    Ok(())
}

fn matrices(root: &Path) -> Result<Vec<(SystemTime, PathBuf)>> {
    let mut matrices = Vec::new();
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(matrices),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", root.display()));
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.len() != 16 || !name.chars().all(|character| character.is_ascii_hexdigit()) {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH);
        matrices.push((modified, path));
    }
    Ok(matrices)
}

fn fingerprint(workspace: &Path, capabilities: Capabilities) -> Result<String> {
    let mut hash = Fnv1a::new();
    hash.write(b"remote-server-v5");
    hash.write(
        git(workspace, &["rev-parse", "HEAD"])
            .unwrap_or_default()
            .as_bytes(),
    );
    hash.write(
        git(
            workspace,
            &["-c", "core.quotePath=false", "status", "--porcelain"],
        )
        .unwrap_or_default()
        .as_bytes(),
    );
    // The plan computes this fingerprint on the host while the matrix is built
    // inside the container, so the git output has to be identical in both. A
    // plain `git diff` abbreviation depends on the git version (`core.abbrev`),
    // which the image and the host need not share; pinning the abbreviation and
    // disabling external filters keeps the hash content-based instead.
    if let Ok(diff) = git(
        workspace,
        &[
            "-c",
            "diff.noprefix=false",
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--abbrev=40",
            "HEAD",
        ],
    ) {
        hash.write(diff.as_bytes());
    }
    // `git diff HEAD` covers tracked files only. The tooling that decides how
    // the matrix is produced is new code while it is being developed, and an
    // untracked file's contents never reach the diff, so a cache built by older
    // tooling would be reused for archives it no longer produces. Hashing the
    // untracked files keeps that honest; a clean checkout has none, so this
    // costs nothing once the work is committed.
    if let Ok(listing) = git(workspace, &["ls-files", "--others", "--exclude-standard"]) {
        let mut untracked = listing
            .lines()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();
        untracked.sort_unstable();
        for relative in untracked {
            hash.write(relative.as_bytes());
            if let Ok(contents) = fs::read(workspace.join(relative)) {
                hash.write(&contents);
            }
        }
    }
    hash.write(rustc_version().as_bytes());
    hash.write(Platform::host().as_str().as_bytes());
    hash.write(std::env::consts::ARCH.as_bytes());
    for input in ["rust-toolchain.toml", "Cargo.lock"] {
        let path = workspace.join(input);
        if let Ok(contents) = fs::read(&path) {
            hash.write(input.as_bytes());
            hash.write(&contents);
        }
    }
    hash.write(if capabilities.cross {
        b"zig"
    } else {
        b"no-zig"
    });
    // The skip that a missing msvc-wine records is only correct for that
    // environment, so an image that gains the layer keys its own cache.
    hash.write(if capabilities.msvc {
        b"msvc"
    } else {
        b"no-msvc"
    });
    Ok(format!("{:016x}", hash.finish()))
}

fn rustc_version() -> String {
    Command::new("rustc")
        .args(["--version", "--verbose"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map_or_default(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn host_triple() -> Result<String> {
    rustc_version()
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!("rustc did not report a host triple"))
}

fn git(workspace: &Path, arguments: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .current_dir(workspace)
        .args(arguments)
        .stdout(Stdio::piped())
        .output()
        .context("failed to run git")?;
    if !output.status.success() {
        bail!("git {} failed", arguments.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

/// Whether this process can build the cross targets of the matrix.
pub fn zig_available() -> bool {
    crate::tasks::bundle::util::which("zig").is_some()
        && crate::tasks::bundle::util::which("cargo-zigbuild").is_some()
}

fn musl_cc(triple: &str) -> Option<PathBuf> {
    let arch = triple.split('-').next().unwrap_or("");
    crate::tasks::bundle::util::which(&format!("{arch}-linux-musl-gcc")).or_else(|| {
        let host = host_triple().unwrap_or_default();
        if host.starts_with(arch) {
            crate::tasks::bundle::util::which("musl-gcc")
        } else {
            None
        }
    })
}

fn invalidate_corrupt_aws_lc_sys(target_dir: &Path, triple: &str) {
    let profile_dir = target_dir.join(triple).join("release");
    let build_dir = profile_dir.join("build");
    let Ok(entries) = fs::read_dir(&build_dir) else {
        return;
    };
    let mut corrupt = false;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("aws-lc-sys-") {
            continue;
        }
        let out = entry.path().join("out");
        let Ok(objects) = fs::read_dir(out) else {
            continue;
        };
        if objects.flatten().any(|object| {
            object
                .file_name()
                .to_string_lossy()
                .ends_with("bignum_sqr.o")
                && object_missing_named_symbols(&object.path())
        }) {
            corrupt = true;
            let _ = fs::remove_dir_all(entry.path());
        }
    }
    if !corrupt {
        return;
    }
    eprintln!("remote-server: removed corrupt aws-lc-sys objects for {triple}");
    for dir_name in [".fingerprint", "deps"] {
        let Ok(entries) = fs::read_dir(profile_dir.join(dir_name)) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("aws-lc-sys-") || name.starts_with("libaws_lc") {
                let path = entry.path();
                let _ = if path.is_dir() {
                    fs::remove_dir_all(&path)
                } else {
                    fs::remove_file(&path)
                };
            }
        }
    }
}

fn object_missing_named_symbols(path: &Path) -> bool {
    let Ok(output) = Command::new("nm").arg(path).output() else {
        return false;
    };
    if !output.status.success() {
        return true;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .all(|line| !line.contains(" T ") && !line.contains(" t "))
}

fn parse_target(value: &str) -> std::result::Result<Target, String> {
    let target: Target = value
        .parse()
        .map_err(|error: anyhow::Error| error.to_string())?;
    if !MATRIX.contains(&target) {
        return Err(format!(
            "{value} is not a remote-server target; expected one of: {}",
            MATRIX
                .iter()
                .map(|target| target.triple())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(target)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub fingerprint: String,
    pub commit: String,
    pub dirty: bool,
    pub rustc: String,
    pub host: String,
    pub created_at_unix: u64,
    pub requested: Vec<String>,
    pub archives: Vec<Archive>,
    pub skipped: Vec<Skipped>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Archive {
    pub triple: String,
    pub os: String,
    pub arch: String,
    pub file: String,
    pub sha256: String,
    pub bytes: u64,
}

/// A target this environment cannot build, with the toolchain or SDK it lacks.
///
/// Only capability skips are recorded here. A build that ran and failed is not
/// a skip: the matrix stays incomplete so the next run retries it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Skipped {
    pub triple: String,
    pub reason: String,
}

/// FNV-1a: the fingerprint only has to change when an input changes.
struct Fnv1a(u64);

impl Fnv1a {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn finish(self) -> u64 {
        self.0
    }
}

/// The `--jobs` value the planner should pass when the request has none.
pub fn default_jobs() -> u32 {
    std::thread::available_parallelism()
        .map_or(1, |count| u32::try_from(count.get()).unwrap_or(1))
        .max(1)
}

/// Whether `directory` holds every archive of the matrix.
pub fn is_complete_at(directory: &Path) -> Result<bool> {
    is_complete(directory, &MATRIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_matrix_is_exactly_the_embedded_targets() {
        assert_eq!(MATRIX.len(), 8);
        for target in MATRIX {
            if target.platform() == Platform::Linux {
                assert!(target.is_musl());
            }
        }
    }

    #[test]
    fn archive_names_match_the_embed_contract() {
        assert_eq!(
            archive_file(Target::LinuxX86_64Musl),
            "zzz-remote-server-linux-x86_64.gz"
        );
        assert_eq!(
            archive_file(Target::WindowsAarch64),
            "zzz-remote-server-windows-aarch64.zip"
        );
        assert_eq!(
            archive_file(Target::MacAarch64),
            "zzz-remote-server-macos-aarch64.gz"
        );
    }

    #[test]
    fn only_matrix_targets_parse() {
        assert!(parse_target("x86_64-unknown-linux-musl").is_ok());
        assert!(parse_target("aarch64-apple-darwin").is_ok());
        assert!(parse_target("x86_64-unknown-linux-gnu").is_err());
    }

    #[test]
    fn fingerprint_changes_with_the_schema_tag() {
        let mut first = Fnv1a::new();
        first.write(b"remote-server-v4");
        let mut second = Fnv1a::new();
        second.write(b"remote-server-v5");
        assert_ne!(first.finish(), second.finish());
    }

    #[test]
    fn a_host_without_the_cross_toolchain_gets_a_different_cache() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("the xtask crate lives two levels below the workspace");
        let full = Capabilities {
            cross: true,
            msvc: true,
        };
        let without_cross = Capabilities {
            cross: false,
            ..full
        };
        let without_msvc = Capabilities {
            msvc: false,
            ..full
        };
        let with_cross = layout_for(workspace, full).expect("layout").fingerprint;
        let cross_missing = layout_for(workspace, without_cross)
            .expect("layout")
            .fingerprint;
        let msvc_missing = layout_for(workspace, without_msvc)
            .expect("layout")
            .fingerprint;
        assert_ne!(with_cross, cross_missing);
        assert_ne!(with_cross, msvc_missing);
        assert_ne!(cross_missing, msvc_missing);
    }
}
