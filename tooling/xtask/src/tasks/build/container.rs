//! Container plumbing for release builds.
//!
//! The host's C library and linker end up in every artifact, so a release built
//! on a rolling distribution only starts there. Building inside a pinned
//! toolchain image turns "which machine built this" into a recorded input
//! instead of an accident.
//!
//! This module only decides *how* to wrap a command. Planning lives in
//! `build/plan.rs`, execution in `build/scheduler.rs`.

#![allow(
    clippy::disallowed_methods,
    reason = "xtask is a CLI with no UI thread to block"
)]

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

pub const IMAGE_REPOSITORY: &str = "localhost/zzz-toolchain";
pub const TOOLCHAIN_DOCKERFILE: &str = "Dockerfile-toolchain";
/// Must match the `BASE_IMAGE` default in `Dockerfile-toolchain`. The digest
/// pins the Ubuntu point release; bumping it is a deliberate change.
pub const DEFAULT_BASE_IMAGE: &str =
    "ubuntu:22.04@sha256:076e7bf82319619f28dd0aa9c02ab34b055ad37100395b4f49a2ffba04fb5e11";

/// Inputs that change the toolchain image. Hashing them into the tag means an
/// edited Dockerfile or dependency script produces a new tag instead of silently
/// reusing a stale image.
const IMAGE_INPUTS: [&str; 5] = [
    TOOLCHAIN_DOCKERFILE,
    "script/linux",
    "script/install-cmake",
    "script/lib/github-release.sh",
    "rust-toolchain.toml",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum ContainerPolicy {
    /// Containers for release builds, the host for everything else.
    Auto,
    /// Fail instead of falling back to the host when no engine is available.
    Always,
    /// Never use a container, even for a release build.
    Never,
}

impl ContainerPolicy {
    pub fn resolve(self, profile: crate::tasks::build::model::Profile) -> bool {
        match self {
            ContainerPolicy::Always => true,
            ContainerPolicy::Never => false,
            ContainerPolicy::Auto => profile.is_release(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Engine {
    Podman,
    Docker,
}

impl Engine {
    pub fn binary(self) -> &'static str {
        match self {
            Engine::Podman => "podman",
            Engine::Docker => "docker",
        }
    }

    /// Picks the engine from `ZZZ_CONTAINER_ENGINE`, then from `PATH`.
    pub fn detect() -> Result<Self> {
        if let Ok(configured) = std::env::var("ZZZ_CONTAINER_ENGINE") {
            return match configured.as_str() {
                "podman" => Ok(Engine::Podman),
                "docker" => Ok(Engine::Docker),
                other => {
                    bail!("ZZZ_CONTAINER_ENGINE must be `podman` or `docker`, found `{other}`")
                }
            };
        }

        for engine in [Engine::Podman, Engine::Docker] {
            if engine.is_available() {
                return Ok(engine);
            }
        }

        bail!(
            "no container engine found; install podman, or set --container never to build on the host"
        );
    }

    fn is_available(self) -> bool {
        Command::new(self.binary())
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    }

    /// Name that resolves to the machine running the container, which is where
    /// a proxy listening on the host loopback actually lives.
    fn host_alias(self) -> &'static str {
        match self {
            Engine::Podman => "host.containers.internal",
            Engine::Docker => "host.docker.internal",
        }
    }

    pub fn image_exists(self, image: &str) -> Result<bool> {
        let status = Command::new(self.binary())
            .args(["image", "exists", image])
            .status()
            .with_context(|| format!("failed to run `{} image exists`", self.binary()))?;
        Ok(status.success())
    }

    pub fn image_id(self, image: &str) -> Option<String> {
        let output = Command::new(self.binary())
            .args(["image", "inspect", "--format", "{{.Id}}", image])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let id = String::from_utf8(output.stdout).ok()?;
        Some(id.trim().to_owned())
    }
}

impl fmt::Display for Engine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.binary())
    }
}

/// Everything needed to turn a host command into a container command.
#[derive(Clone, Debug)]
pub struct ContainerSpec {
    pub engine: Engine,
    pub image: String,
    pub workspace: PathBuf,
    pub target_directory: PathBuf,
    /// Host directory holding the downloads Cargo would otherwise repeat.
    pub cargo_cache: PathBuf,
    pub jobs: u32,
    pub offline: bool,
    pub sccache: bool,
    /// Whether the image carries the msvc-wine layer. The tag encodes it, so an
    /// image built for Windows targets is never mistaken for the plain one.
    pub msvc: bool,
}

impl ContainerSpec {
    pub fn new(
        workspace: &Path,
        target_directory: &Path,
        jobs: u32,
        offline: bool,
        sccache: bool,
        msvc: bool,
    ) -> Result<Self> {
        Ok(Self {
            engine: Engine::detect()?,
            image: image_tag(workspace, msvc)?,
            workspace: workspace.to_path_buf(),
            target_directory: target_directory.to_path_buf(),
            cargo_cache: workspace.join(".cache/container-cargo"),
            jobs,
            offline,
            sccache,
            msvc,
        })
    }

    /// The command that builds the toolchain image. The tag already encodes
    /// every input, so an up-to-date image is reused by the scheduler's guard
    /// instead of being rebuilt here.
    pub fn image_build_command(&self) -> Vec<String> {
        let mut arguments = vec![
            self.engine.binary().to_owned(),
            "build".to_owned(),
            "--file".to_owned(),
            TOOLCHAIN_DOCKERFILE.to_owned(),
            "--tag".to_owned(),
            self.image.clone(),
        ];
        // The build recipes download toolchains and crates, so the image build
        // needs the same proxy the runtime does. These are predefined build
        // arguments, which the engines accept without an `ARG` declaration.
        let proxies = proxy_environment(self.engine);
        if !proxies.is_empty() && self.engine == Engine::Podman {
            arguments.push("--http-proxy=false".to_owned());
        }
        for (key, value) in proxies {
            arguments.push("--build-arg".to_owned());
            arguments.push(format!("{key}={value}"));
        }
        // Only forwarded when the operator overrides the pin; passing the
        // default would defeat the digest in the Dockerfile.
        if let Some(base_image) = base_image_override() {
            arguments.push("--build-arg".to_owned());
            arguments.push(format!("BASE_IMAGE={base_image}"));
        }
        if self.msvc {
            arguments.push("--build-arg".to_owned());
            arguments.push("WITH_MSVC=1".to_owned());
        }
        arguments.push(".".to_owned());
        arguments
    }

    /// Wraps `command` so it runs inside the toolchain image.
    ///
    /// Environment variables are forwarded explicitly. There is no allowlist to
    /// maintain: callers put the variables they mean in the task, and this
    /// function passes exactly those through.
    pub fn wrap(&self, command: &[String], environment: &BTreeMap<String, String>) -> Vec<String> {
        let mut wrapped = vec![
            self.engine.binary().to_owned(),
            "run".to_owned(),
            "--rm".to_owned(),
            "--init".to_owned(),
            "--shm-size=1g".to_owned(),
        ];

        // SELinux cannot relabel a workspace that is also used on the host, and
        // both engines accept `label=disable` for exactly this case.
        wrapped.push("--security-opt".to_owned());
        wrapped.push("label=disable".to_owned());

        wrapped.push("--volume".to_owned());
        wrapped.push(format!("{}:/src", self.workspace.display()));
        wrapped.push("--volume".to_owned());
        wrapped.push(format!(
            "{}:/src/target/container",
            self.target_directory.join("container").display()
        ));
        // Only the download caches are mounted over the image's cargo home: the
        // toolchain binaries live in the same directory, and mounting the whole
        // tree would hide cargo, rustc, and the installed helper tools.
        for subdirectory in ["registry", "git"] {
            wrapped.push("--volume".to_owned());
            wrapped.push(format!(
                "{}:/usr/local/cargo/{subdirectory}",
                self.cargo_cache.join(subdirectory).display()
            ));
        }
        wrapped.push("--workdir".to_owned());
        wrapped.push("/src".to_owned());

        if self.offline {
            wrapped.push("--network=none".to_owned());
        } else {
            let proxies = proxy_environment(self.engine);
            if !proxies.is_empty() {
                // Both engines copy the host's proxy variables into the
                // container verbatim, which leaves a proxy bound to the host
                // loopback pointing at the container itself. Take over the
                // forwarding so the rewritten values are the only ones set.
                match self.engine {
                    Engine::Podman => wrapped.push("--http-proxy=false".to_owned()),
                    Engine::Docker => {
                        wrapped.push("--add-host=host.docker.internal:host-gateway".to_owned())
                    }
                }
                for (key, value) in proxies {
                    wrapped.push("--env".to_owned());
                    wrapped.push(format!("{key}={value}"));
                }
            }
        }

        wrapped.push("--env".to_owned());
        wrapped.push("CARGO_TARGET_DIR=/src/target/container".to_owned());
        // A task that has its own share of the job budget sets this itself;
        // passing both would leave the container with two values for one key.
        if !environment.contains_key("CARGO_BUILD_JOBS") {
            wrapped.push("--env".to_owned());
            wrapped.push(format!("CARGO_BUILD_JOBS={}", self.jobs));
        }

        if self.sccache {
            wrapped.push("--env".to_owned());
            // `cargo install` puts the binary in $CARGO_HOME/bin, not
            // /usr/local/bin.
            wrapped.push("RUSTC_WRAPPER=/usr/local/cargo/bin/sccache".to_owned());
            wrapped.push("--env".to_owned());
            wrapped.push("SCCACHE_DIR=/src/target/sccache".to_owned());
        }

        for (key, value) in environment {
            wrapped.push("--env".to_owned());
            wrapped.push(format!("{key}={value}"));
        }

        wrapped.push(self.image.clone());
        wrapped.extend(command.iter().cloned());
        wrapped
    }

    /// Maps a host path into the container namespace when the mount makes that
    /// necessary. The workspace is mounted at `/src`, so a path under it has an
    /// obvious translation; anything else is left alone and will fail loudly
    /// when the script cannot find it.
    pub fn path_for(&self, host: &Path) -> String {
        match host.strip_prefix(&self.workspace) {
            Ok(relative) => format!("/src/{}", relative.display()),
            Err(_) => host.display().to_string(),
        }
    }
}

/// Tag derived from the files that define the image. Two checkouts with the
/// same toolchain inputs share an image; any edit produces a new tag.
pub fn image_tag(workspace: &Path, targets_need_msvc: bool) -> Result<String> {
    let msvc = targets_need_msvc || with_msvc();
    let mut hash = Fnv1a::new();
    hash.write(
        base_image_override()
            .unwrap_or_else(|| DEFAULT_BASE_IMAGE.to_owned())
            .as_bytes(),
    );
    for input in IMAGE_INPUTS {
        hash.write(input.as_bytes());
        let path = workspace.join(input);
        let contents = std::fs::read(&path)
            .with_context(|| format!("failed to read toolchain input {}", path.display()))?;
        hash.write(&contents);
    }
    // The MSVC layer adds roughly a gigabyte, so a plan that does not ask for a
    // Windows target does not build it. Keeping it in the tag stops a Windows
    // image from being mistaken for the plain one.
    if msvc {
        hash.write(b"with-msvc");
    }
    let suffix = if msvc { "-msvc" } else { "" };
    Ok(format!("{IMAGE_REPOSITORY}:{:016x}{suffix}", hash.finish()))
}

/// Proxy variables a container build may need. The upper-case spellings matter
/// to some downloaders, so both forms are forwarded.
const PROXY_VARIABLES: [&str; 8] = [
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "no_proxy",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
];

/// Proxy variables from the host, with loopback hosts pointed at the machine
/// running the container. A proxy on `127.0.0.1` is not reachable from the
/// container's network namespace, so copying its address verbatim makes every
/// download fail; the engine's host alias is where that proxy actually listens.
fn proxy_environment(engine: Engine) -> BTreeMap<String, String> {
    let host = std::env::vars().collect::<BTreeMap<_, _>>();
    forwarded_proxy_environment(&host, engine.host_alias())
}

fn forwarded_proxy_environment(
    host: &BTreeMap<String, String>,
    alias: &str,
) -> BTreeMap<String, String> {
    PROXY_VARIABLES
        .iter()
        .filter_map(|name| {
            let value = host.get(*name)?;
            if value.is_empty() {
                return None;
            }
            let forwarded = if name.eq_ignore_ascii_case("no_proxy") {
                value.clone()
            } else {
                rewrite_proxy_host(value, alias)
            };
            Some((name.to_string(), forwarded))
        })
        .collect()
}

/// Replaces a loopback host in a proxy URL with `alias`, keeping the scheme,
/// any credentials, and the port. Values that already name a reachable host are
/// returned unchanged.
fn rewrite_proxy_host(value: &str, alias: &str) -> String {
    let Some((scheme, rest)) = value.split_once("://") else {
        return value.to_owned();
    };
    let (authority, tail) = match rest.find('/') {
        Some(index) => rest.split_at(index),
        None => (rest, ""),
    };
    let (credentials, host) = match authority.rsplit_once('@') {
        Some((credentials, host)) => (format!("{credentials}@"), host),
        None => (String::new(), authority),
    };
    let loopback = ["127.0.0.1", "localhost", "[::1]"]
        .into_iter()
        .find_map(|needle| {
            host.strip_prefix(needle)
                .filter(|remainder| remainder.is_empty() || remainder.starts_with(':'))
                .map(|remainder| format!("{alias}{remainder}"))
        });
    match loopback {
        Some(host) => format!("{scheme}://{credentials}{host}{tail}"),
        None => value.to_owned(),
    }
}

/// Base image override for a one-off build. Unset means the Dockerfile's
/// digest-pinned default.
fn base_image_override() -> Option<String> {
    std::env::var("ZZZ_BASE_IMAGE")
        .ok()
        .filter(|value| !value.is_empty())
}

/// Whether the operator forced the Windows cross layer regardless of targets.
/// A plan that contains a Windows target asks for it on its own; this override
/// is how an image gets built ahead of time.
pub fn with_msvc() -> bool {
    std::env::var("ZZZ_WITH_MSVC").is_ok_and(|value| value == "1" || value == "true")
}

/// FNV-1a, chosen because the tag only needs to change when inputs change and a
/// 64-bit non-cryptographic hash keeps the dependency list small.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policies_resolve_against_release_profiles() {
        use crate::tasks::build::model::Profile;
        assert!(ContainerPolicy::Auto.resolve(Profile::Release));
        assert!(!ContainerPolicy::Auto.resolve(Profile::Dev));
        assert!(ContainerPolicy::Always.resolve(Profile::Dev));
        assert!(!ContainerPolicy::Never.resolve(Profile::Release));
    }

    #[test]
    fn wrapping_forwards_only_the_named_environment() {
        let spec = ContainerSpec {
            engine: Engine::Podman,
            image: "localhost/zzz-toolchain:test".to_owned(),
            workspace: PathBuf::from("/workspace"),
            target_directory: PathBuf::from("/workspace/target"),
            cargo_cache: PathBuf::from("/workspace/.cache/container-cargo"),
            jobs: 8,
            offline: true,
            sccache: true,
            msvc: false,
        };
        let mut environment = BTreeMap::new();
        environment.insert("ZZZ_PGO_PROFILE".to_owned(), "/profile".to_owned());

        let wrapped = spec.wrap(&["cargo".to_owned(), "build".to_owned()], &environment);

        assert!(wrapped.contains(&"--network=none".to_owned()));
        assert!(wrapped.contains(&"ZZZ_PGO_PROFILE=/profile".to_owned()));
        assert!(wrapped.contains(&"RUSTC_WRAPPER=/usr/local/cargo/bin/sccache".to_owned()));
        // The task did not set a job count, so the container-wide one applies.
        assert!(wrapped.contains(&"CARGO_BUILD_JOBS=8".to_owned()));
        assert_eq!(wrapped.last().map(String::as_str), Some("build"));
    }

    #[test]
    fn a_task_job_budget_replaces_the_container_wide_one() {
        let spec = ContainerSpec {
            engine: Engine::Podman,
            image: "localhost/zzz-toolchain:test".to_owned(),
            workspace: PathBuf::from("/workspace"),
            target_directory: PathBuf::from("/workspace/target"),
            cargo_cache: PathBuf::from("/workspace/.cache/container-cargo"),
            jobs: 8,
            offline: false,
            sccache: false,
            msvc: false,
        };
        let mut environment = BTreeMap::new();
        environment.insert("CARGO_BUILD_JOBS".to_owned(), "3".to_owned());

        let wrapped = spec.wrap(&["cargo".to_owned(), "build".to_owned()], &environment);

        let job_values = wrapped
            .iter()
            .filter(|argument| argument.starts_with("CARGO_BUILD_JOBS="))
            .collect::<Vec<_>>();
        assert_eq!(job_values, vec!["CARGO_BUILD_JOBS=3"]);
    }

    #[test]
    fn loopback_proxies_are_rewritten_to_the_host_alias() {
        let alias = "host.containers.internal";

        assert_eq!(
            rewrite_proxy_host("http://127.0.0.1:7897", alias),
            "http://host.containers.internal:7897"
        );
        assert_eq!(
            rewrite_proxy_host("socks5://user:secret@localhost:1080", alias),
            "socks5://user:secret@host.containers.internal:1080"
        );
        assert_eq!(
            rewrite_proxy_host("http://[::1]:3128/", alias),
            "http://host.containers.internal:3128/"
        );
        // A proxy that is already reachable keeps its address.
        assert_eq!(
            rewrite_proxy_host("http://proxy.example.com:8080", alias),
            "http://proxy.example.com:8080"
        );
        // Not a URL, and not a loopback prefix.
        assert_eq!(
            rewrite_proxy_host("127.0.0.1:7897", alias),
            "127.0.0.1:7897"
        );
        assert_eq!(
            rewrite_proxy_host("http://127.0.0.10:7897", alias),
            "http://127.0.0.10:7897"
        );
    }

    #[test]
    fn proxy_variables_are_forwarded_with_the_host_alias() {
        let host = BTreeMap::from([
            ("http_proxy".to_owned(), "http://127.0.0.1:7897".to_owned()),
            ("HTTPS_PROXY".to_owned(), "http://127.0.0.1:7897".to_owned()),
            ("no_proxy".to_owned(), "localhost,127.0.0.1".to_owned()),
            ("UNRELATED".to_owned(), "value".to_owned()),
        ]);

        let forwarded = forwarded_proxy_environment(&host, "host.containers.internal");

        assert_eq!(
            forwarded.get("http_proxy").map(String::as_str),
            Some("http://host.containers.internal:7897")
        );
        assert_eq!(
            forwarded.get("HTTPS_PROXY").map(String::as_str),
            Some("http://host.containers.internal:7897")
        );
        // Bypass lists are about names, not reachable addresses, so they pass
        // through untouched.
        assert_eq!(
            forwarded.get("no_proxy").map(String::as_str),
            Some("localhost,127.0.0.1")
        );
        assert_eq!(forwarded.len(), 3);
    }

    #[test]
    fn image_tags_change_with_inputs() {
        let first = Fnv1a::new();
        assert_eq!(first.finish(), 0xcbf2_9ce4_8422_2325);
    }
}
