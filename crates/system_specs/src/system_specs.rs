use client::platform_info;
pub use gpui::GpuSpecs;
use gpui::{App, AppContext as _, Task, Window, actions};
use human_bytes::human_bytes;
use release_channel::{AppCommitSha, AppVersion, ReleaseChannel};
use semver::Version;
use serde::Serialize;
use std::{env, fmt::Display};
use sysinfo::{MemoryRefreshKind, RefreshKind, System};

actions!(
    zzz,
    [
        /// Copies system specifications to the clipboard for bug reports.
        CopySystemSpecsIntoClipboard,
    ]
);

#[derive(Clone, Debug, Serialize)]
pub struct SystemSpecs {
    app_version: String,
    release_channel: &'static str,
    os_name: String,
    os_version: String,
    memory: u64,
    architecture: &'static str,
    commit_sha: Option<String>,
    bundle_type: Option<String>,
    gpu_specs: Option<String>,
}

impl SystemSpecs {
    pub fn new(window: &mut Window, cx: &mut App) -> Task<Self> {
        let app_version = AppVersion::global(cx).to_string();
        let release_channel = ReleaseChannel::global(cx);
        let os_name = platform_info::os_name();
        let system = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()),
        );
        let memory = system.total_memory();
        let architecture = env::consts::ARCH;
        let commit_sha = match release_channel {
            ReleaseChannel::Dev => AppCommitSha::try_global(cx).map(|sha| sha.full()),
            ReleaseChannel::Stable => None,
        };
        let bundle_type = bundle_type();

        let gpu_specs = window.gpu_specs().map(|specs| {
            format!(
                "{} || {} || {}",
                specs.device_name, specs.driver_name, specs.driver_info
            )
        });

        cx.background_spawn(async move {
            let os_version = platform_info::os_version();
            SystemSpecs {
                app_version,
                release_channel: release_channel.display_name(),
                bundle_type,
                os_name,
                os_version,
                memory,
                architecture,
                commit_sha,
                gpu_specs,
            }
        })
    }

    pub fn new_stateless(
        app_version: Version,
        app_commit_sha: Option<AppCommitSha>,
        release_channel: ReleaseChannel,
    ) -> Self {
        let os_name = platform_info::os_name();
        let os_version = platform_info::os_version();
        let system = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()),
        );
        let memory = system.total_memory();
        let architecture = env::consts::ARCH;
        let commit_sha = match release_channel {
            ReleaseChannel::Dev => app_commit_sha.map(|sha| sha.full()),
            ReleaseChannel::Stable => None,
        };
        let bundle_type = bundle_type();

        Self {
            app_version: app_version.to_string(),
            release_channel: release_channel.display_name(),
            os_name,
            os_version,
            memory,
            architecture,
            commit_sha,
            bundle_type,
            gpu_specs: try_determine_available_gpus(),
        }
    }
}

impl Display for SystemSpecs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let os_information = format!("OS: {} {}", self.os_name, self.os_version);
        let app_version_information = format!(
            "ZZZ: v{} ({}) {}{}",
            self.app_version,
            match &self.commit_sha {
                Some(commit_sha) => format!("{} {}", self.release_channel, commit_sha),
                None => self.release_channel.to_owned(),
            },
            if let Some(bundle_type) = &self.bundle_type {
                format!("({bundle_type})")
            } else {
                "".to_owned()
            },
            if cfg!(debug_assertions) {
                "(Taylor's Version)"
            } else {
                ""
            },
        );
        let system_specs = [
            app_version_information,
            os_information,
            format!("Memory: {}", human_bytes(self.memory as f64)),
            format!("Architecture: {}", self.architecture),
        ]
        .into_iter()
        .chain(
            self.gpu_specs
                .as_ref()
                .map(|specs| format!("GPU: {}", specs)),
        )
        .collect::<Vec<String>>()
        .join("\n");

        write!(f, "{system_specs}")
    }
}

fn try_determine_available_gpus() -> Option<String> {
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        #[allow(
            clippy::disallowed_methods,
            reason = "we are not running in an executor"
        )]
        std::process::Command::new("vulkaninfo")
            .args(&["--summary"])
            .output()
            .ok()
            .map(|output| {
                [
                    "<details><summary>`vulkaninfo --summary` output</summary>",
                    "",
                    "```",
                    String::from_utf8_lossy(&output.stdout).as_ref(),
                    "```",
                    "</details>",
                ]
                .join("\n")
            })
            .or(Some("Failed to run `vulkaninfo --summary`".to_owned()))
    }
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    {
        None
    }
}

#[derive(Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize, Clone)]
pub struct GpuInfo {
    pub device_name: Option<String>,
    pub device_pci_id: u16,
    pub vendor_name: Option<String>,
    pub vendor_pci_id: u16,
    pub driver_version: Option<String>,
    pub driver_name: Option<String>,
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub fn read_gpu_info_from_sys_class_drm() -> anyhow::Result<Vec<GpuInfo>> {
    use anyhow::Context as _;
    use pciid_parser;
    let dir_iter = std::fs::read_dir("/sys/class/drm").context("Failed to read /sys/class/drm")?;
    let mut pci_addresses = vec![];
    let mut gpus = Vec::<GpuInfo>::new();
    let pci_db = pciid_parser::Database::read().ok();
    for entry in dir_iter {
        let Ok(entry) = entry else {
            continue;
        };

        let device_path = entry.path().join("device");
        let Some(pci_address) = device_path.read_link().ok().and_then(|pci_address| {
            pci_address
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .map(str::trim)
                .map(str::to_owned)
        }) else {
            continue;
        };
        let Ok(device_pci_id) = read_pci_id_from_path(device_path.join("device")) else {
            continue;
        };
        let Ok(vendor_pci_id) = read_pci_id_from_path(device_path.join("vendor")) else {
            continue;
        };
        let driver_name = std::fs::read_link(device_path.join("driver"))
            .ok()
            .and_then(|driver_link| {
                driver_link
                    .file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .map(str::trim)
                    .map(str::to_owned)
            });
        let driver_version = driver_name
            .as_ref()
            .and_then(|driver_name| {
                std::fs::read_to_string(format!("/sys/module/{driver_name}/version")).ok()
            })
            .as_deref()
            .map(str::trim)
            .map(str::to_owned);

        let already_found = gpus
            .iter()
            .zip(&pci_addresses)
            .any(|(gpu, gpu_pci_address)| {
                gpu_pci_address == &pci_address
                    && gpu.driver_version == driver_version
                    && gpu.driver_name == driver_name
            });

        if already_found {
            continue;
        }

        let vendor = pci_db
            .as_ref()
            .and_then(|db| db.vendors.get(&vendor_pci_id));
        let vendor_name = vendor.map(|vendor| vendor.name.clone());
        let device_name = vendor
            .and_then(|vendor| vendor.devices.get(&device_pci_id))
            .map(|device| device.name.clone());

        gpus.push(GpuInfo {
            device_name,
            device_pci_id,
            vendor_name,
            vendor_pci_id,
            driver_version,
            driver_name,
        });
        pci_addresses.push(pci_address);
    }

    Ok(gpus)
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn read_pci_id_from_path(path: impl AsRef<std::path::Path>) -> anyhow::Result<u16> {
    use anyhow::Context as _;
    let id = std::fs::read_to_string(path)?;
    let id = id
        .trim()
        .strip_prefix("0x")
        .context("Not a device ID")
        .context(id.clone())?;
    anyhow::ensure!(
        id.len() == 4,
        "Not a device id, expected 4 digits, found {}",
        id.len()
    );
    u16::from_str_radix(id, 16).context("Failed to parse device ID")
}

/// Returns value of `ZZZ_BUNDLE_TYPE` set at compiletime or else at runtime.
///
/// The compiletime value is used by flatpak since it doesn't seem to have a way to provide a
/// runtime environment variable.
///
/// The runtime value is used by snap since the ZZZ snaps use release binaries directly, and so
/// cannot have this baked in.
fn bundle_type() -> Option<String> {
    option_env!("ZZZ_BUNDLE_TYPE")
        .map(|bundle_type| bundle_type.to_owned())
        .or_else(|| env::var("ZZZ_BUNDLE_TYPE").ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvRestore {
        name: String,
        original: Option<String>,
    }

    impl EnvRestore {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                original: env::var(name).ok(),
            }
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            match self.original.as_deref() {
                Some(value) => unsafe { env::set_var(&self.name, value) },
                None => unsafe { env::remove_var(&self.name) },
            }
        }
    }

    #[test]
    fn bundle_type_uses_runtime_env_when_compile_time_value_is_absent() {
        let _lock = ENV_LOCK.lock().expect("env lock poisoned");
        let _restore = EnvRestore::new("ZZZ_BUNDLE_TYPE");
        unsafe { env::set_var("ZZZ_BUNDLE_TYPE", "portable-test") };

        let expected = option_env!("ZZZ_BUNDLE_TYPE")
            .map(str::to_string)
            .unwrap_or_else(|| "portable-test".to_string());

        assert_eq!(bundle_type(), Some(expected));
    }

    #[test]
    fn new_stateless_includes_commit_sha_only_for_dev_channel() {
        let dev_specs = SystemSpecs::new_stateless(
            Version::new(1, 2, 3),
            Some(AppCommitSha::new("abcdef0".to_string())),
            ReleaseChannel::Dev,
        );
        let stable_specs = SystemSpecs::new_stateless(
            Version::new(1, 2, 3),
            Some(AppCommitSha::new("abcdef0".to_string())),
            ReleaseChannel::Stable,
        );

        assert_eq!(dev_specs.app_version, "1.2.3");
        assert_eq!(dev_specs.release_channel, "ZZZ Dev");
        assert_eq!(dev_specs.commit_sha.as_deref(), Some("abcdef0"));

        assert_eq!(stable_specs.release_channel, "ZZZ");
        assert_eq!(stable_specs.commit_sha, None);
    }

    #[test]
    fn display_renders_core_fields_and_optional_gpu_line() {
        let specs = SystemSpecs {
            app_version: "1.2.3".to_string(),
            release_channel: "ZZZ Dev",
            os_name: "TestOS".to_string(),
            os_version: "9.9".to_string(),
            memory: 1024,
            architecture: "x86_64",
            commit_sha: Some("abcdef0".to_string()),
            bundle_type: Some("portable".to_string()),
            gpu_specs: Some("GPU A || Driver B || Info C".to_string()),
        };

        let rendered = specs.to_string();

        assert!(rendered.contains("ZZZ: v1.2.3 (ZZZ Dev abcdef0) (portable)"));
        assert!(rendered.contains("OS: TestOS 9.9"));
        assert!(rendered.contains("Memory:"));
        assert!(rendered.contains("Architecture: x86_64"));
        assert!(rendered.contains("GPU: GPU A || Driver B || Info C"));
    }
}
