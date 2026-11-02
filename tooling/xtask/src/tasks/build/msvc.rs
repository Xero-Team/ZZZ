//! The Microsoft C toolchain used by the Windows cross build.
//!
//! The old cross-build shell had to source `msvcenv.sh` and export a dozen
//! variables. This module does the same work in Rust so the cross build has one
//! entry point and the environment it needs is visible in the plan.
//!
//! Native Windows builds inherit the Visual Studio developer environment from
//! the shell that invoked `cargo xtask`; only the Linux cross path configures
//! anything here.

#![allow(
    clippy::disallowed_methods,
    reason = "the Windows cross build shells out to clang-cl and lld-link"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};

pub const DEFAULT_MSVC_ROOT: &str = "/opt/msvc";

/// The architecture names msvc-wine and Wine's `fxc.exe` use.
pub fn msvc_arch(rust_arch: &str) -> Result<&'static str> {
    match rust_arch {
        "x86_64" => Ok("x64"),
        "aarch64" => Ok("arm64"),
        _ => bail!("unsupported MSVC architecture `{rust_arch}`; use x86_64 or aarch64"),
    }
}

fn host_msvc_arch() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("x64"),
        "aarch64" => Ok("arm64"),
        "x86" => Ok("x86"),
        other => bail!("unsupported host architecture for fxc.exe: {other}"),
    }
}

pub fn msvc_root() -> PathBuf {
    std::env::var_os("MSVC_ROOT").map_or_else(|| PathBuf::from(DEFAULT_MSVC_ROOT), PathBuf::from)
}

/// Whether an msvc-wine install is usable from this process.
///
/// The matrix records a target as skipped when its toolchain is missing, so the
/// cache key has to include this: reusing a cache built without the layer would
/// report a complete matrix that has no Windows archives. Either architecture
/// counts as present; per-target completeness is still checked in [`MsvcEnvironment::detect`].
pub fn available() -> bool {
    ["x86_64", "aarch64"]
        .iter()
        .any(|arch| MsvcEnvironment::detect(arch).is_ok())
}

/// Everything the compiler needs for one Rust target. Applying it twice is
/// harmless, which matters because the archive builder and the editor build
/// both call it.
pub struct MsvcEnvironment {
    variables: Vec<(String, String)>,
    path_prefix: Option<PathBuf>,
    /// `x86_64_pc_windows_msvc`, the suffix `cmake` and `cc` expect on their
    /// per-target variables.
    env_triple: String,
    /// Rendered by [`render_cmake_toolchain`]; written by
    /// [`MsvcEnvironment::with_cmake_toolchain`].
    cmake_toolchain: String,
    cmake_toolchain_path: Option<PathBuf>,
}

impl MsvcEnvironment {
    /// Configures the cross environment from `MSVC_ROOT`.
    pub fn detect(rust_arch: &str) -> Result<Self> {
        let msvc_arch = msvc_arch(rust_arch)?;
        let root = msvc_root();
        let bindir = root.join("bin").join(msvc_arch);
        if !bindir.join("cl").is_file() {
            bail!(
                "msvc-wine `cl` not found at {} (MSVC_ROOT={})",
                bindir.join("cl").display(),
                root.display()
            );
        }

        let msvcenv = bindir.join("msvcenv.sh");
        let contents = std::fs::read_to_string(&msvcenv).with_context(|| {
            format!(
                "failed to read {}; the msvc-wine install is incomplete",
                msvcenv.display()
            )
        })?;
        let variables = parse_msvcenv(&contents);

        let msvc_version = variables
            .iter()
            .find(|(key, _)| key == "MSVCVER")
            .map(|(_, value)| value.clone())
            .or_else(|| std::env::var("MSVCVER").ok())
            .ok_or_else(|| anyhow::anyhow!("MSVCVER is not defined by {}", msvcenv.display()))?;
        let sdk_version = variables
            .iter()
            .find(|(key, _)| key == "SDKVER")
            .map(|(_, value)| value.clone())
            .or_else(|| std::env::var("SDKVER").ok())
            .ok_or_else(|| anyhow::anyhow!("SDKVER is not defined by {}", msvcenv.display()))?;

        let msvc_unix = root.join("vc/tools/msvc").join(&msvc_version);
        let (sdk_include, sdk_lib) = find_sdk(&root, &sdk_version)?;
        if !msvc_unix.join("include").is_dir() {
            bail!(
                "msvc-wine headers not found under {} (MSVCVER={msvc_version})",
                msvc_unix.display()
            );
        }

        let triple = format!("{rust_arch}-pc-windows-msvc");
        let env_triple = triple.replace('-', "_");
        let upper = env_triple.to_ascii_uppercase();
        let include_dirs = [
            msvc_unix.join("include"),
            msvc_unix.join("atlmfc/include"),
            sdk_include.join("shared"),
            sdk_include.join("ucrt"),
            sdk_include.join("um"),
            sdk_include.join("winrt"),
        ];
        let include = join_paths(&include_dirs);
        let library = join_paths(&[
            msvc_unix.join("lib").join(msvc_arch),
            msvc_unix.join("atlmfc/lib").join(msvc_arch),
            sdk_lib.join("ucrt").join(msvc_arch),
            sdk_lib.join("um").join(msvc_arch),
        ]);

        let mut variables = vec![
            ("INCLUDE".to_owned(), include),
            ("LIB".to_owned(), library),
            (format!("CC_{env_triple}"), "clang-cl".to_owned()),
            (format!("CXX_{env_triple}"), "clang-cl".to_owned()),
            (format!("AR_{env_triple}"), "llvm-lib".to_owned()),
            (
                format!("CARGO_TARGET_{upper}_LINKER"),
                "lld-link".to_owned(),
            ),
            (
                format!("CFLAGS_{env_triple}"),
                "-Wno-unused-but-set-variable".to_owned(),
            ),
        ];
        // MSVC's own `rc` wrapper cannot be used by embed-resource; forcing the
        // variable out of the environment is part of the contract.
        variables.push(("RC".to_owned(), String::new()));

        let path_prefix = Some(bindir);

        Ok(Self {
            variables,
            path_prefix,
            env_triple,
            cmake_toolchain: render_cmake_toolchain(rust_arch, &include_dirs),
            cmake_toolchain_path: None,
        })
    }

    /// Writes the CMake toolchain file under `directory` and returns the
    /// environment that points CMake at it.
    ///
    /// CMake only accepts `CMAKE_SYSTEM_NAME` from a toolchain file, and on a
    /// Linux host it cannot find a Visual Studio installation to configure an
    /// MSVC target for. Build scripts receive the file through
    /// `CMAKE_TOOLCHAIN_FILE_<triple>`; the generator is set explicitly so the
    /// `cmake` crate does not ask for a Visual Studio generator, and because
    /// `Unix Makefiles` rejects the platform flag it passes for an MSVC target.
    pub fn with_cmake_toolchain(mut self, directory: &Path) -> Result<Self> {
        std::fs::create_dir_all(directory)
            .with_context(|| format!("failed to create {}", directory.display()))?;
        let path = directory.join(format!("cmake-{}.cmake", self.env_triple));
        std::fs::write(&path, &self.cmake_toolchain)
            .with_context(|| format!("failed to write {}", path.display()))?;
        self.cmake_toolchain_path = Some(path);
        Ok(self)
    }

    /// Adds the environment to `command`.
    pub fn apply(&self, command: &mut Command) {
        if let Some(prefix) = &self.path_prefix {
            let mut paths = vec![prefix.clone()];
            if let Some(existing) = std::env::var_os("PATH") {
                paths.extend(std::env::split_paths(&existing));
            }
            if let Ok(joined) = std::env::join_paths(paths) {
                command.env("PATH", joined);
            }
        }
        for (key, value) in &self.variables {
            if key == "RC" && value.is_empty() {
                command.env_remove(key);
            } else {
                command.env(key, value);
            }
        }
        if let Some(toolchain) = &self.cmake_toolchain_path {
            // `cmake` looks the variable up under both spellings; the
            // underscored one is also settable from a shell.
            command.env(
                format!("CMAKE_TOOLCHAIN_FILE_{}", self.env_triple),
                toolchain,
            );
            command.env(format!("CMAKE_GENERATOR_{}", self.env_triple), "Ninja");
        }
        if std::env::var_os("GPUI_FXC_PATH").is_none()
            && let Some(fxc) = find_fxc()
        {
            // The raw `fxc.exe`: `gpui_windows` recognises a `.exe` and runs it
            // through msvc-wine's `wine-msvc.sh`, which rewrites the absolute
            // paths the shader step passes into drives Wine can see. A plain
            // `wine` call would hand fxc `/src/...` and it would read that as
            // an unknown option.
            command.env("GPUI_FXC_PATH", fxc);
        }
    }
}

/// Joins directories the way both `msvcenv.sh` and MSVC spell a path list.
fn join_paths(directories: &[PathBuf]) -> String {
    directories
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(";")
}

/// Renders the CMake toolchain file for one Windows target.
///
/// `clang-cl` and `lld-link` are the compilers the `cc` crate already drives, so
/// a CMake-built dependency lands in the same ABI as the Rust code around it.
fn render_cmake_toolchain(rust_arch: &str, include_dirs: &[PathBuf]) -> String {
    let processor = match rust_arch {
        "aarch64" => "ARM64",
        _ => "AMD64",
    };
    let target = format!("{rust_arch}-pc-windows-msvc");
    let includes = include_dirs
        .iter()
        .map(|path| format!("  \"{}\"\n", path.display()))
        .collect::<String>();

    format!(
        "\
# Generated by `cargo xtask` for the Windows cross build; do not edit.
set(CMAKE_SYSTEM_NAME Windows)
set(CMAKE_SYSTEM_VERSION 10.0)
set(CMAKE_SYSTEM_PROCESSOR {processor})

set(CMAKE_C_COMPILER clang-cl)
set(CMAKE_C_COMPILER_TARGET {target})
set(CMAKE_CXX_COMPILER clang-cl)
set(CMAKE_CXX_COMPILER_TARGET {target})
set(CMAKE_AR llvm-lib)
set(CMAKE_RC_COMPILER llvm-rc)
set(CMAKE_LINKER lld-link)

# The build produces archives, never an executable that runs on this host.
set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)

# `clang-cl` splits `INCLUDE` on the host's path separator, so msvc-wine's
# Windows-style list never resolves. Name the same directories here; CMake then
# passes them as `-imsvc` on every compile.
set(CMAKE_C_IMPLICIT_INCLUDE_DIRECTORIES \"\")
set(CMAKE_CXX_IMPLICIT_INCLUDE_DIRECTORIES \"\")
set(CMAKE_C_STANDARD_INCLUDE_DIRECTORIES
{includes})
set(CMAKE_CXX_STANDARD_INCLUDE_DIRECTORIES
{includes})
"
    )
}

/// Parses the `KEY=VALUE` assignments msvc-wine writes into `msvcenv.sh`.
fn parse_msvcenv(contents: &str) -> Vec<(String, String)> {
    let mut variables = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        // The generated script writes `export FOO=bar`, so strip the prefix
        // before validating the name.
        let key = key.trim().strip_prefix("export ").unwrap_or(key).trim();
        if key.starts_with('#')
            || !key
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            continue;
        }
        let value = value.trim().trim_matches('"').trim_matches('\'');
        variables.push((key.to_owned(), value.to_owned()));
    }
    variables
}

fn find_sdk(root: &Path, sdk_version: &str) -> Result<(PathBuf, PathBuf)> {
    for base in [root.join("kits/10"), root.join("Windows Kits/10")] {
        let include = base.join("include").join(sdk_version);
        if include.is_dir() {
            return Ok((include, base.join("lib").join(sdk_version)));
        }
    }
    bail!(
        "the Windows SDK {sdk_version} is not installed under {}",
        root.display()
    )
}

fn find_fxc() -> Option<PathBuf> {
    let root = msvc_root();
    let host = host_msvc_arch().ok()?;
    for kit in [root.join("Windows Kits/10/bin"), root.join("kits/10/bin")] {
        if !kit.is_dir() {
            continue;
        }
        let mut found = None;
        for entry in std::fs::read_dir(&kit).ok()?.flatten() {
            let candidate = entry.path().join(host).join("fxc.exe");
            if candidate.is_file() {
                found = Some(candidate);
            }
        }
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Whether the current process already runs inside a Visual Studio developer
/// shell. Native Windows builds rely on the caller having set it up.
pub fn native_environment_ready() -> bool {
    std::env::var_os("VSCMD_VER").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msvcenv_assignments_are_parsed() {
        let variables =
            parse_msvcenv("export MSVCVER=14.44.35207\nSDKVER=\"10.0.26100.0\"\n# comment\n");
        assert!(variables.contains(&("MSVCVER".to_owned(), "14.44.35207".to_owned())));
        assert!(variables.contains(&("SDKVER".to_owned(), "10.0.26100.0".to_owned())));
    }

    #[test]
    fn architecture_names_match_msvc_wine() {
        assert_eq!(msvc_arch("x86_64").expect("known"), "x64");
        assert_eq!(msvc_arch("aarch64").expect("known"), "arm64");
        assert!(msvc_arch("riscv64").is_err());
    }

    #[test]
    fn the_cmake_toolchain_names_the_target_and_its_headers() {
        let includes = [
            PathBuf::from("/opt/msvc/vc/tools/msvc/14.50.35717/include"),
            PathBuf::from("/opt/msvc/kits/10/include/10.0.26100.0/ucrt"),
        ];
        let toolchain = render_cmake_toolchain("aarch64", &includes);

        assert!(toolchain.contains("set(CMAKE_SYSTEM_NAME Windows)"));
        assert!(toolchain.contains("set(CMAKE_SYSTEM_PROCESSOR ARM64)"));
        assert!(toolchain.contains("set(CMAKE_C_COMPILER_TARGET aarch64-pc-windows-msvc)"));
        for include in includes {
            assert!(toolchain.contains(&format!("\"{}\"", include.display())));
        }
    }
}
