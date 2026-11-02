//! Turns a build request into an explicit task graph.
//!
//! The graph is the contract between the CLI, `--print-plan`, CI, and the
//! scheduler: nothing is discovered at execution time, so what a plan prints is
//! exactly what runs. Every task is a plain command with a working directory and
//! an environment, already wrapped for the container when the request asks for
//! one.

#![allow(
    clippy::disallowed_methods,
    reason = "planning reads git and tool versions to describe the build"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};

use crate::tasks::build::container::{ContainerPolicy, ContainerSpec, Engine, with_msvc};
use crate::tasks::build::model::{Channel, Format, Platform, Profile, Target};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub schema_version: u32,
    pub workspace: PathBuf,
    pub generated_by: String,
    pub repository: RepositoryState,
    pub channel: Channel,
    pub profile: Profile,
    pub jobs: u32,
    pub targets: Vec<TargetPlan>,
    pub tasks: Vec<Task>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RepositoryState {
    pub commit: String,
    pub dirty: bool,
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetPlan {
    pub target: Target,
    pub formats: Vec<Format>,
    pub container: Option<ContainerInfo>,
    pub pgo: Option<PgoPlan>,
    pub artifacts: Vec<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PgoPlan {
    pub mode: PgoMode,
    pub fingerprint: String,
    pub profile: PathBuf,
    pub min_functions: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum PgoMode {
    /// No profile is consumed.
    Off,
    /// Consume a profile when one exists for this build.
    Auto,
    /// Consume a profile; planning fails when none exists.
    Use,
    /// Produce an instrumented build and a profile. Not a `build` option: use
    /// `cargo xtask pgo train`.
    #[value(skip)]
    Train,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub engine: String,
    pub image: String,
    pub image_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub depends_on: Vec<String>,
    pub jobs: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guard: Option<Guard>,
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub environment: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<ContainerInfo>,
}

/// A condition that makes a task skippable instead of failing. Each variant
/// answers "is the work already done?".
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Guard {
    /// Skipped when the toolchain image is already present.
    ImageAlreadyBuilt { engine: String, image: String },
    /// Skipped when a complete remote-server matrix is already cached.
    RemoteServerCached { directory: PathBuf },
}

impl Guard {
    pub fn should_skip(&self) -> Result<bool> {
        match self {
            Guard::ImageAlreadyBuilt { engine, image } => {
                let engine = match engine.as_str() {
                    "podman" => Engine::Podman,
                    "docker" => Engine::Docker,
                    other => bail!("unknown container engine `{other}`"),
                };
                engine.image_exists(image)
            }
            Guard::RemoteServerCached { directory } => {
                crate::tasks::build::remote_server::is_complete_at(directory)
            }
        }
    }
}

/// Everything `cargo xtask build` and `cargo xtask dist` need.
#[derive(Clone, Debug)]
pub struct BuildRequest {
    pub targets: Vec<Target>,
    pub formats: Vec<Format>,
    pub profile: Profile,
    pub channel: Channel,
    pub container: ContainerPolicy,
    pub pgo: PgoMode,
    pub jobs: Option<u32>,
    pub offline: bool,
    pub sccache: bool,
    pub min_pgo_functions: u64,
    pub require_pgo: bool,
}

impl Plan {
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).context("failed to serialize the build plan")
    }

    /// Checks that every dependency names a task in the plan exactly once and
    /// that the graph has no cycle.
    pub fn validate(&self) -> Result<()> {
        let mut seen = BTreeSet::new();
        for task in &self.tasks {
            if !seen.insert(task.id.as_str()) {
                bail!("duplicate task id `{}`", task.id);
            }
        }
        for task in &self.tasks {
            for dependency in &task.depends_on {
                if !seen.contains(dependency.as_str()) {
                    bail!(
                        "task `{}` depends on `{dependency}`, which is not in the plan",
                        task.id
                    );
                }
            }
        }

        let mut resolved = BTreeSet::new();
        while resolved.len() < self.tasks.len() {
            let before = resolved.len();
            for task in &self.tasks {
                if resolved.contains(task.id.as_str()) {
                    continue;
                }
                if task
                    .depends_on
                    .iter()
                    .all(|dependency| resolved.contains(dependency.as_str()))
                {
                    resolved.insert(task.id.as_str());
                }
            }
            if resolved.len() == before {
                let remaining = self
                    .tasks
                    .iter()
                    .filter(|task| !resolved.contains(task.id.as_str()))
                    .map(|task| task.id.as_str())
                    .collect::<Vec<_>>();
                bail!("the build plan has a cycle among: {}", remaining.join(", "));
            }
        }

        Ok(())
    }

    pub fn summary(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!(
            "plan: {} task(s), profile {}, channel {}, {} job(s)",
            self.tasks.len(),
            self.profile.key(),
            self.channel.as_str(),
            self.jobs
        ));
        for target in &self.targets {
            let container = target
                .container
                .as_ref()
                .map_or_else(|| "host".to_owned(), |info| info.image.clone());
            let pgo = target.pgo.as_ref().map_or("off", |plan| {
                if plan.mode == PgoMode::Off {
                    "off"
                } else {
                    "on"
                }
            });
            lines.push(format!(
                "  {:<26} formats: {:<28} pgo: {:<4} {container}",
                target.target.triple(),
                target
                    .formats
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                pgo
            ));
        }
        for task in &self.tasks {
            lines.push(format!(
                "  [{}] {} (jobs {})",
                task.id,
                task.command.join(" "),
                task.jobs
            ));
        }
        lines.join("\n")
    }
}

pub fn assemble(workspace: &Path, request: &BuildRequest) -> Result<Plan> {
    if request.targets.is_empty() {
        bail!("no targets requested");
    }
    if request.pgo == PgoMode::Train {
        bail!("`build --pgo train` does not train; run `cargo xtask pgo train` instead");
    }
    // The packaging scripts only know how to turn a release-family profile into
    // an artifact, and pretending otherwise would produce a bundle from a
    // different profile than the one the plan claims.
    if !request.profile.is_release() {
        bail!(
            "`{}` builds no artifacts; use `cargo build` or `cargo run` for development builds",
            request.profile.key()
        );
    }
    if request.profile == Profile::LinuxNative
        && request
            .targets
            .iter()
            .any(|target| target.platform() != Platform::Linux)
    {
        bail!("linux-native only applies to Linux targets");
    }

    let mut planner = Planner::new(workspace, request)?;
    planner.add_prerequisites()?;

    let mut targets = Vec::new();
    for target in &request.targets {
        targets.push(planner.add_target(*target)?);
    }

    finish(workspace, request, planner.tasks, targets)
}

/// Builds a plan from tasks that a caller already assembled. `pgo train` and
/// `pgo build` use it so their single-step plans go through the same scheduler
/// and container wrapper as a full build.
pub fn assemble_tasks(
    workspace: &Path,
    request: &BuildRequest,
    tasks: Vec<Task>,
    targets: Vec<TargetPlan>,
) -> Result<Plan> {
    finish(workspace, request, tasks, targets)
}

fn finish(
    workspace: &Path,
    request: &BuildRequest,
    tasks: Vec<Task>,
    targets: Vec<TargetPlan>,
) -> Result<Plan> {
    let jobs = request.jobs.unwrap_or_else(default_jobs);
    if jobs == 0 {
        bail!("--jobs must be at least 1");
    }

    let version = crate_version(workspace)?;
    let commit =
        git_output(workspace, &["rev-parse", "HEAD"]).unwrap_or_else(|_| "unknown".to_owned());
    let dirty =
        git_output(workspace, &["status", "--porcelain"]).is_ok_and(|output| !output.is_empty());

    let plan = Plan {
        schema_version: SCHEMA_VERSION,
        workspace: workspace.to_path_buf(),
        generated_by: format!("cargo xtask build (zzz {version})"),
        repository: RepositoryState {
            commit,
            dirty,
            version,
        },
        channel: request.channel,
        profile: request.profile,
        jobs,
        targets,
        tasks,
    };
    plan.validate()?;
    Ok(plan)
}

pub fn container_spec(workspace: &Path, request: &BuildRequest) -> Result<Option<ContainerSpec>> {
    if !request.container.resolve(request.profile) {
        return Ok(None);
    }
    let jobs = request.jobs.unwrap_or_else(default_jobs).max(1);
    Ok(Some(ContainerSpec::new(
        workspace,
        &workspace.join("target"),
        jobs,
        request.offline,
        request.sccache,
        needs_msvc(&request.targets),
    )?))
}

/// Whether the plan needs the msvc-wine layer.
///
/// A Windows target needs Microsoft's headers, libraries, and `fxc.exe`, which
/// only that layer provides, so asking for the target is enough. `ZZZ_WITH_MSVC`
/// stays as an override for building the image before a plan needs it.
fn needs_msvc(targets: &[Target]) -> bool {
    with_msvc()
        || targets
            .iter()
            .any(|target| target.platform() == Platform::Windows)
}

pub fn container_info(spec: Option<&ContainerSpec>) -> Option<ContainerInfo> {
    spec.map(|spec| ContainerInfo {
        engine: spec.engine.binary().to_owned(),
        image: spec.image.clone(),
        image_id: spec.engine.image_id(&spec.image),
    })
}

/// Profile directory keyed by the inputs that make a profile applicable.
pub fn pgo_profile_path(
    workspace: &Path,
    target: Target,
    profile: Profile,
) -> Result<(String, PathBuf)> {
    let fingerprint = pgo_fingerprint(workspace, target, profile)?;
    let directory = workspace.join("target/pgo").join(&fingerprint);
    Ok((fingerprint, directory.join("merged.profdata")))
}

fn default_jobs() -> u32 {
    std::thread::available_parallelism().map_or(1, |count| u32::try_from(count.get()).unwrap_or(1))
}

fn crate_version(workspace: &Path) -> Result<String> {
    let path = workspace.join("crates/zzz/Cargo.toml");
    let source = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let manifest: toml::Value =
        toml::from_str(&source).with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(manifest
        .get("package")
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .unwrap_or("0.0.0")
        .to_owned())
}

fn git_output(workspace: &Path, arguments: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .current_dir(workspace)
        .args(arguments)
        .output()
        .context("failed to run git")?;
    if !output.status.success() {
        bail!("git {} failed", arguments.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

struct Planner<'a> {
    workspace: PathBuf,
    request: &'a BuildRequest,
    tasks: Vec<Task>,
    ids: BTreeSet<String>,
    container: Option<ContainerSpec>,
    container_info: Option<ContainerInfo>,
    version: String,
    channel: Channel,
    total_jobs: u32,
    remote_server_dir: PathBuf,
}

impl<'a> Planner<'a> {
    fn new(workspace: &Path, request: &'a BuildRequest) -> Result<Self> {
        let container = container_spec(workspace, request)?;
        let container_info = container_info(container.as_ref());
        // The matrix task runs in the container whenever one is in play, and
        // that image ships zig and gains the msvc-wine layer when a Windows
        // target is planned. Hash the capability the task will see, not the one
        // this host happens to have.
        let capabilities =
            crate::tasks::build::remote_server::Capabilities::for_container(container.as_ref());
        let remote_server_dir =
            crate::tasks::build::remote_server::layout_for(workspace, capabilities)?.directory;
        Ok(Self {
            workspace: workspace.to_path_buf(),
            request,
            tasks: Vec::new(),
            ids: BTreeSet::new(),
            container,
            container_info,
            version: crate_version(workspace)?,
            channel: request.channel,
            total_jobs: request.jobs.unwrap_or_else(default_jobs).max(1),
            remote_server_dir,
        })
    }
}

impl Planner<'_> {
    fn add_prerequisites(&mut self) -> Result<()> {
        if let Some(spec) = self.container.clone() {
            let command = spec.image_build_command();
            self.push(Task {
                id: "toolchain".to_owned(),
                description: "Build the pinned toolchain image".to_owned(),
                depends_on: Vec::new(),
                jobs: 1,
                guard: Some(Guard::ImageAlreadyBuilt {
                    engine: spec.engine.binary().to_owned(),
                    image: spec.image,
                }),
                command,
                environment: BTreeMap::new(),
                container: None,
            })?;
        }

        self.push(Task {
            id: "grammars".to_owned(),
            description: "Verify vendored Tree-sitter grammars".to_owned(),
            depends_on: Vec::new(),
            jobs: 1,
            guard: None,
            command: vec![
                "cargo".to_owned(),
                "xtask".to_owned(),
                "grammars".to_owned(),
                "check".to_owned(),
            ],
            environment: BTreeMap::new(),
            container: None,
        })?;

        if self.container.is_some() && self.request.offline {
            let command = vec![
                "cargo".to_owned(),
                "fetch".to_owned(),
                "--locked".to_owned(),
            ];
            let environment = BTreeMap::new();
            // Fetching is the one step that may use the network: the builds
            // after it run offline, which is the point of passing `--offline`.
            let wrapped = match &self.container {
                Some(spec) => ContainerSpec {
                    offline: false,
                    ..spec.clone()
                }
                .wrap(&command, &environment),
                None => command,
            };
            self.push(Task {
                id: "fetch".to_owned(),
                description: "Fetch locked Cargo dependencies".to_owned(),
                depends_on: vec!["toolchain".to_owned()],
                jobs: 1,
                guard: None,
                command: wrapped,
                environment,
                container: self.container_info.clone(),
            })?;
        }

        // The editor embeds one remote-server archive per platform, so the
        // whole matrix is built once, before any target, and cached by content.
        // Bundling then only points `ZZZ_EMBED_REMOTE_SERVER_DIR` at the cache.
        if !self.request.targets.is_empty() {
            let mut dependencies = vec!["grammars".to_owned()];
            if self.container.is_some() {
                dependencies.push("toolchain".to_owned());
            }
            if self.container.is_some() && self.request.offline {
                dependencies.push("fetch".to_owned());
            }
            let mut command = vec![
                "cargo".to_owned(),
                "xtask".to_owned(),
                "remote-server".to_owned(),
                "build".to_owned(),
                "--jobs".to_owned(),
                self.total_jobs.to_string(),
                "--quiet".to_owned(),
            ];
            if self.request.offline {
                command.push("--offline".to_owned());
            }
            let environment = BTreeMap::new();
            let wrapped = match &self.container {
                Some(spec) => spec.wrap(&command, &environment),
                None => command,
            };
            self.push(Task {
                id: "remote-server".to_owned(),
                description: "Build the remote-server matrix".to_owned(),
                depends_on: dependencies,
                jobs: self.total_jobs,
                guard: Some(Guard::RemoteServerCached {
                    directory: self.remote_server_dir.clone(),
                }),
                command: wrapped,
                environment,
                container: self.container_info.clone(),
            })?;
        }

        Ok(())
    }

    fn add_target(&mut self, target: Target) -> Result<TargetPlan> {
        // A containerized Windows build gets the msvc-wine layer automatically:
        // the image tag carries it. Without the container, the host has to
        // provide wine and the Microsoft toolchain itself, so say so here
        // instead of letting the compiler fail on a missing `windows.h`.
        if target.platform() == Platform::Windows
            && !self.should_containerize(target)
            && Platform::host() == Platform::Linux
            && crate::tasks::build::msvc::MsvcEnvironment::detect(target.arch().as_str()).is_err()
        {
            bail!(
                "{} needs the msvc-wine layer, which the toolchain image provides: \
                 drop `--container never` to build it there, or install msvc-wine on this host",
                target.triple()
            );
        }

        let formats = self.formats_for(target)?;
        // The MSI and Inno installers need dotnet, ISCC, and PowerShell, so a
        // plan that asks for them from Linux can never succeed. Reject it here
        // so `--print-plan` and CI fail before compiling anything.
        if target.platform() == Platform::Windows
            && Platform::host() != Platform::Windows
            && formats
                .iter()
                .any(|format| matches!(format, Format::Msi | Format::Inno))
        {
            bail!(
                "the MSI and Inno installers need a Windows host; from Linux build the \
                 portable binaries with --formats exe"
            );
        }
        let pgo = self.resolve_pgo(target)?;
        let pgo_task = pgo.as_ref().and_then(|plan| {
            (plan.mode == PgoMode::Use).then(|| Task {
                id: format!("pgo:verify:{}", target.triple()),
                description: format!("Verify the PGO profile for {}", target.triple()),
                depends_on: Vec::new(),
                jobs: 1,
                guard: None,
                command: vec![
                    "cargo".to_owned(),
                    "xtask".to_owned(),
                    "pgo".to_owned(),
                    "verify".to_owned(),
                    plan.profile.display().to_string(),
                    "--min-functions".to_owned(),
                    plan.min_functions.to_string(),
                ],
                environment: BTreeMap::new(),
                container: None,
            })
        });

        let mut dependencies = vec!["grammars".to_owned(), "remote-server".to_owned()];
        if self.container.is_some() {
            dependencies.push("toolchain".to_owned());
        }
        if self.container.is_some() && self.request.offline {
            dependencies.push("fetch".to_owned());
        }
        if pgo_task.is_some() {
            dependencies.push(format!("pgo:verify:{}", target.triple()));
        }

        if let Some(task) = pgo_task {
            self.push(task)?;
        }

        let jobs = self.jobs_for_targets();
        let (container_formats, host_formats) = self.split_formats(target, &formats);

        let last_task = if container_formats.is_empty() {
            None
        } else {
            let bundle_id = format!("bundle:{}", target.triple());
            let (command, environment) =
                self.bundle_command(target, &container_formats, pgo.as_ref(), false, jobs);
            let wrapped = match &self.container {
                Some(spec) if self.should_containerize(target) => spec.wrap(&command, &environment),
                _ => command,
            };
            self.push(Task {
                id: bundle_id.clone(),
                description: format!("Build and bundle {}", target.triple()),
                depends_on: dependencies.clone(),
                jobs,
                guard: None,
                command: wrapped,
                environment,
                container: if self.should_containerize(target) {
                    self.container_info.clone()
                } else {
                    None
                },
            })?;
            Some(bundle_id)
        };

        let last_task = if host_formats.is_empty() {
            last_task
        } else {
            let host_id = format!("bundle-host:{}", target.triple());
            let (command, environment) =
                self.bundle_command(target, &host_formats, pgo.as_ref(), true, jobs);
            let mut host_dependencies = dependencies;
            if let Some(previous) = &last_task {
                host_dependencies.push(previous.clone());
            }
            self.push(Task {
                id: host_id.clone(),
                description: format!("Build host-only formats for {}", target.triple()),
                depends_on: host_dependencies,
                jobs,
                guard: None,
                command,
                environment,
                container: None,
            })?;
            Some(host_id)
        };

        let bundle_id = last_task
            .ok_or_else(|| anyhow::anyhow!("{} has no format to build", target.triple()))?;

        self.push(Task {
            id: format!("verify:{}", target.triple()),
            description: format!("Verify artifacts for {}", target.triple()),
            depends_on: vec![bundle_id],
            jobs: 1,
            guard: None,
            command: self.verify_command(target),
            environment: BTreeMap::new(),
            container: None,
        })?;

        let artifacts = expected_artifacts(
            &self.workspace,
            target,
            self.request.profile,
            self.should_containerize(target),
            &formats,
            self.channel,
            &self.version,
        );

        Ok(TargetPlan {
            target,
            formats,
            container: if self.should_containerize(target) {
                self.container_info.clone()
            } else {
                None
            },
            pgo,
            artifacts,
        })
    }

    /// Linux splits its formats: `tar.gz`, `deb`, and `rpm` run inside the
    /// toolchain image, while AppImage, Flatpak, and Snap need host tools and
    /// network access. Every other platform is one task.
    fn split_formats(&self, target: Target, formats: &[Format]) -> (Vec<Format>, Vec<Format>) {
        if target.platform() != Platform::Linux {
            return (formats.to_vec(), Vec::new());
        }
        let formats: BTreeSet<Format> = formats.iter().copied().collect();
        let mut container = crate::tasks::bundle::container_formats(&formats);
        let host = crate::tasks::bundle::host_only_formats(&formats);
        // A host-only format starts from the tarball, so the container has to
        // produce it even when the caller named only `appimage`, `flatpak`, or
        // `snap`.
        if !host.is_empty() && !container.contains(&Format::TarGz) {
            container.push(Format::TarGz);
        }
        container.sort();
        (container, host)
    }

    fn formats_for(&self, target: Target) -> Result<Vec<Format>> {
        let formats = if self.request.formats.is_empty() {
            target.default_formats()
        } else {
            self.request
                .formats
                .iter()
                .copied()
                .filter(|format| target.supports(*format))
                .collect()
        };

        if formats.is_empty() {
            bail!(
                "none of the requested formats are supported for {}",
                target.triple()
            );
        }
        Ok(formats)
    }

    fn resolve_pgo(&self, target: Target) -> Result<Option<PgoPlan>> {
        let mode = match self.request.pgo {
            PgoMode::Off | PgoMode::Train => return Ok(None),
            PgoMode::Use => PgoMode::Use,
            PgoMode::Auto => PgoMode::Auto,
        };

        if target.platform() != Platform::Linux {
            // Profiles do not transfer across ABIs, and there is no training
            // harness for macOS or Windows yet. Say so instead of silently
            // shipping an unoptimized build.
            if mode == PgoMode::Use {
                bail!(
                    "{} has no PGO training harness; use --pgo off for this target",
                    target.triple()
                );
            }
            return Ok(None);
        }

        let (fingerprint, profile) =
            pgo_profile_path(&self.workspace, target, self.request.profile)?;
        let exists = profile.is_file();

        let resolved = match mode {
            PgoMode::Use if !exists => bail!(
                "no PGO profile at {}; run `cargo xtask pgo train --target {}` first",
                profile.display(),
                target.triple()
            ),
            PgoMode::Use => PgoMode::Use,
            PgoMode::Auto if exists => PgoMode::Use,
            PgoMode::Auto => PgoMode::Off,
            PgoMode::Off | PgoMode::Train => PgoMode::Off,
        };

        if resolved == PgoMode::Off {
            if self.request.require_pgo {
                bail!(
                    "the {} channel requires PGO for {}, but no profile exists at {}",
                    self.channel.as_str(),
                    target.triple(),
                    profile.display()
                );
            }
            return Ok(None);
        }

        Ok(Some(PgoPlan {
            mode: resolved,
            fingerprint,
            profile,
            min_functions: self.request.min_pgo_functions,
        }))
    }

    fn should_containerize(&self, target: Target) -> bool {
        if self.container.is_none() {
            return false;
        }
        // Native macOS and Windows bundles need Apple and Microsoft tooling
        // that only runs on those hosts. The cross paths are the ones a Linux
        // container can own.
        match target.platform() {
            Platform::Linux => true,
            Platform::Mac | Platform::Windows => Platform::host() == Platform::Linux,
        }
    }

    fn jobs_for_targets(&self) -> u32 {
        let count = u32::try_from(self.request.targets.len())
            .unwrap_or(1)
            .max(1);
        (self.total_jobs / count).max(1)
    }

    /// One command for every platform. `skip_build` is the host-only follow-up
    /// that packages AppImage, Flatpak, and Snap from the tarball the container
    /// already produced; it must not compile again.
    fn bundle_command(
        &self,
        target: Target,
        formats: &[Format],
        pgo: Option<&PgoPlan>,
        skip_build: bool,
        jobs: u32,
    ) -> (Vec<String>, BTreeMap<String, String>) {
        let mut environment = BTreeMap::new();
        environment.insert("CARGO_BUILD_JOBS".to_owned(), jobs.to_string());
        environment.insert("CARGO_INCREMENTAL".to_owned(), "0".to_owned());
        environment.insert("RELEASE_VERSION".to_owned(), self.version.clone());
        // The matrix is built once, ahead of every target. Pointing the embed
        // build script at the cache means it copies archives instead of
        // compiling them again inside each bundle.
        environment.insert(
            "ZZZ_EMBED_REMOTE_SERVER_DIR".to_owned(),
            self.path_for(&self.remote_server_dir),
        );
        if self.request.offline {
            // In a container the network namespace is the lock; on a host there
            // is no namespace to use, so cargo itself has to refuse. Either
            // way `--offline` means "this build does not download".
            environment.insert("CARGO_NET_OFFLINE".to_owned(), "true".to_owned());
        }
        // The plan already ran `grammars check` as a prerequisite. Repeating it
        // inside the bundler would install and run the Tree-sitter CLI a second
        // time for a tree the host just verified.
        environment.insert("ZZZ_SKIP_GRAMMAR_CHECK".to_owned(), "1".to_owned());
        if let Some(pgo) = pgo {
            environment.insert("ZZZ_PGO_PROFILE".to_owned(), self.path_for(&pgo.profile));
        }
        // The container already sets `CARGO_TARGET_DIR`, and the host-only
        // follow-up gets the same tree through `--target-dir`, so the bundler
        // resolves one directory without a second environment value.

        let mut command = vec![
            "cargo".to_owned(),
            "xtask".to_owned(),
            "bundle".to_owned(),
            "--target".to_owned(),
            target.triple().to_owned(),
            "--profile".to_owned(),
            self.request.profile.key().to_owned(),
            "--channel".to_owned(),
            self.channel.as_str().to_owned(),
            "--jobs".to_owned(),
            jobs.to_string(),
            "--formats".to_owned(),
            formats
                .iter()
                .map(|format| format.as_str())
                .collect::<Vec<_>>()
                .join(","),
        ];
        if skip_build {
            command.push("--skip-build".to_owned());
        }
        if self.request.offline {
            command.push("--offline".to_owned());
        }
        if self.should_containerize(target) {
            command.push("--target-dir".to_owned());
            command.push("target/container".to_owned());
        }
        (command, environment)
    }

    fn verify_command(&self, target: Target) -> Vec<String> {
        let mut command = vec![
            "cargo".to_owned(),
            "xtask".to_owned(),
            "verify".to_owned(),
            "artifacts".to_owned(),
            "--target".to_owned(),
            target.triple().to_owned(),
            "--profile".to_owned(),
            self.request.profile.key().to_owned(),
        ];
        if target.platform() == Platform::Linux {
            command.push("--max-glibc".to_owned());
            command.push(self.request.max_glibc().to_owned());
        }
        if self.should_containerize(target) {
            command.push("--target-dir".to_owned());
            command.push("target/container".to_owned());
        }
        command
    }

    /// Maps a host path into the container namespace when the mount makes that
    /// necessary. The workspace is mounted at `/src`, so a path under it has an
    /// obvious translation; anything else is left alone and will fail loudly
    /// when the script cannot find it.
    fn path_for(&self, path: &Path) -> String {
        match &self.container {
            Some(spec) => spec.path_for(path),
            None => path.display().to_string(),
        }
    }

    fn push(&mut self, task: Task) -> Result<()> {
        if !self.ids.insert(task.id.clone()) {
            bail!("duplicate task id `{}`", task.id);
        }
        self.tasks.push(task);
        Ok(())
    }
}

fn expected_artifacts(
    workspace: &Path,
    target: Target,
    profile: Profile,
    containerized: bool,
    formats: &[Format],
    channel: Channel,
    version: &str,
) -> Vec<PathBuf> {
    let arch = target.arch().as_str();
    // A container build keeps its whole target tree under `target/container` so
    // it never shares fingerprints with a host build. Linux packages next to
    // the profile directory at the root of that tree; macOS and Windows nest
    // them under the target triple.
    let mut root = workspace.join("target");
    if containerized {
        root = root.join("container");
    }
    let artifact_directory = match target.platform() {
        Platform::Linux => root.join(profile.artifact_directory()),
        Platform::Mac | Platform::Windows => root
            .join(target.triple())
            .join(profile.artifact_directory()),
    };
    let variant = channel.as_str();
    let suffix = if variant == "stable" {
        String::new()
    } else {
        format!("-{variant}")
    };
    let identity = crate::tasks::bundle::app_identity(channel);

    let mut artifacts = Vec::new();
    for format in formats {
        match format {
            Format::TarGz => {
                artifacts.push(artifact_directory.join(format!("zzz-linux-{arch}.tar.gz")));
            }
            Format::Deb => {
                artifacts.push(artifact_directory.join(format!("zzz-{variant}-linux-{arch}.deb")));
            }
            Format::Rpm => {
                artifacts.push(artifact_directory.join(format!("zzz-{variant}-linux-{arch}.rpm")));
            }
            Format::AppImage => {
                artifacts
                    .push(artifact_directory.join(format!("zzz{suffix}-linux-{arch}.AppImage")));
            }
            Format::Flatpak => {
                artifacts.push(artifact_directory.join(format!("{}.flatpak", identity.id)));
            }
            Format::Snap => {
                // `snapcraft` names the file itself, with its own arch names.
                artifacts.push(
                    artifact_directory.join(format!("zzz_{version}_{}.snap", snap_arch(arch))),
                );
            }
            Format::App => {
                artifacts.push(artifact_directory.join(format!("{}.app", identity.name)));
            }
            Format::Dmg => {
                artifacts.push(artifact_directory.join(format!("ZZZ-{arch}.dmg")));
            }
            Format::Exe => {
                artifacts.push(artifact_directory.join("zzz.exe"));
                artifacts.push(artifact_directory.join("cli.exe"));
            }
            Format::Msi => {
                // `tooling/windows_msi_builder` writes next to `target/`.
                artifacts.push(workspace.join(format!(
                    "target/{}-{arch}-{version}.msi",
                    windows_base_name(channel)
                )));
            }
            Format::Inno => {
                artifacts.push(workspace.join(format!("target/ZZZ-{arch}.exe")));
            }
        }
    }
    artifacts
}

/// The channel's executable base name, matching `ChannelConfig` in
/// `tooling/windows_msi_builder`.
fn windows_base_name(channel: Channel) -> &'static str {
    match channel {
        Channel::Stable => "ZZZ",
        Channel::Preview => "ZZZPreview",
        Channel::Nightly => "ZZZNightly",
        Channel::Dev => "ZZZDev",
    }
}

/// `snapcraft` spells architectures differently from Cargo.
fn snap_arch(arch: &str) -> &str {
    match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    }
}

fn pgo_fingerprint(workspace: &Path, target: Target, profile: Profile) -> Result<String> {
    let mut hash = Fnv1a::new();
    hash.write(b"pgo-v1");
    hash.write(target.triple().as_bytes());
    hash.write(profile.key().as_bytes());
    for input in ["rust-toolchain.toml", "Cargo.toml"] {
        let path = workspace.join(input);
        let contents =
            std::fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
        hash.write(&contents);
    }
    Ok(format!("{:016x}", hash.finish()))
}

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

impl BuildRequest {
    fn max_glibc(&self) -> &'static str {
        // Both bases the toolchain image supports stay at or below this floor.
        "2.35"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(targets: Vec<Target>) -> BuildRequest {
        BuildRequest {
            targets,
            formats: Vec::new(),
            profile: Profile::Release,
            channel: Channel::Dev,
            container: ContainerPolicy::Never,
            pgo: PgoMode::Off,
            jobs: Some(4),
            offline: false,
            sccache: false,
            min_pgo_functions: 100_000,
            require_pgo: false,
        }
    }

    #[test]
    fn plan_is_topologically_valid() {
        let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let plan = assemble(&workspace, &request(vec![Target::LinuxX86_64])).expect("plan builds");
        plan.validate().expect("plan is valid");
        assert!(
            plan.tasks
                .iter()
                .any(|task| task.id == "bundle:x86_64-unknown-linux-gnu")
        );
        assert!(
            plan.tasks
                .iter()
                .any(|task| task.id == "verify:x86_64-unknown-linux-gnu")
        );
    }

    #[test]
    fn duplicate_task_ids_are_rejected() {
        let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let plan = assemble(&workspace, &request(vec![Target::LinuxX86_64])).expect("plan builds");
        let mut duplicated = plan.clone();
        duplicated.tasks.push(plan.tasks[0].clone());
        assert!(duplicated.validate().is_err());
    }

    #[test]
    fn jobs_are_split_across_targets() {
        let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let plan = assemble(
            &workspace,
            &request(vec![Target::LinuxX86_64, Target::MacAarch64]),
        )
        .expect("plan builds");
        for task in &plan.tasks {
            if task.id.starts_with("bundle:") {
                assert_eq!(task.jobs, 2);
            }
        }
    }

    #[test]
    fn development_profiles_do_not_package_artifacts() {
        let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut request = request(vec![Target::LinuxX86_64]);
        request.profile = Profile::Dev;
        assert!(assemble(&workspace, &request).is_err());
        request.profile = Profile::Dbg;
        assert!(assemble(&workspace, &request).is_err());
    }

    #[test]
    fn linux_native_only_applies_to_linux() {
        let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut request = request(vec![Target::MacAarch64]);
        request.profile = Profile::LinuxNative;
        assert!(assemble(&workspace, &request).is_err());

        request.targets = vec![Target::LinuxX86_64];
        assert!(assemble(&workspace, &request).is_ok());
    }

    #[test]
    fn unsupported_formats_are_rejected() {
        let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut request = request(vec![Target::MacAarch64]);
        request.formats = vec![Format::Deb];
        assert!(assemble(&workspace, &request).is_err());
    }
}
