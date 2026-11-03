//! Small process and filesystem helpers shared by the bundling modules.
//!
//! The bundlers shell out to the platform tools that cannot be reimplemented
//! (linkers, codesign, `cargo-bundle`, `snapcraft`). Everything here keeps those
//! invocations uniform: one log line per command, one place that reads the
//! expected version of a helper, one checksummed download path.

#![allow(
    clippy::disallowed_methods,
    reason = "bundling is a CLI workflow; there is no UI thread to block"
)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{Context as _, Result, bail};
use sha2::{Digest, Sha256};

/// Runs a command, logging it and failing when it does.
pub fn run(command: &mut Command, description: &str) -> Result<()> {
    eprintln!("+ {}", display(command));
    let status = command
        .status()
        .with_context(|| format!("failed to start {description}"))?;
    if !status.success() {
        bail!("{description} failed with {status}");
    }
    Ok(())
}

/// Runs a command and returns its captured output.
pub fn output(command: &mut Command, description: &str) -> Result<Output> {
    eprintln!("+ {}", display(command));
    let output = command
        .output()
        .with_context(|| format!("failed to start {description}"))?;
    if !output.status.success() {
        bail!(
            "{description} failed with {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output)
}

/// Runs a command and returns trimmed standard output.
pub fn stdout(command: &mut Command, description: &str) -> Result<String> {
    let output = output(command, description)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub fn display(command: &Command) -> String {
    let mut text = command.get_program().to_string_lossy().into_owned();
    for argument in command.get_args() {
        text.push(' ');
        let argument = argument.to_string_lossy();
        if argument.contains(' ') || argument.contains('$') {
            text.push('\'');
            text.push_str(&argument);
            text.push('\'');
        } else {
            text.push_str(&argument);
        }
    }
    text
}

pub fn require_tool(tool: &str, hint: &str) -> Result<()> {
    require_tool_with_args(tool, &["--version"], hint)
}

/// `require_tool` for a tool whose version flag is not `--version`; `zig`
/// answers the `version` subcommand and rejects the flag.
pub fn require_tool_with_args(tool: &str, arguments: &[&str], hint: &str) -> Result<()> {
    let status = Command::new(tool)
        .args(arguments)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let available = status.is_ok_and(|status| status.success());
    if !available {
        bail!("{tool} is required but was not found on PATH.\n{hint}");
    }
    Ok(())
}

pub fn create_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))
}

pub fn remove(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path).with_context(|| format!("failed to remove {}", path.display()))
        }
        Ok(_) => {
            fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to inspect {}", path.display())),
    }
}

pub fn copy(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        create_dir(parent)?;
    }
    fs::copy(source, destination).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

/// Adds the executable bit on Unix. Windows has no such permission bit, so the
/// call is a no-op there.
#[cfg(unix)]
pub fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let mut permissions = fs::metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?
        .permissions();
    permissions.set_mode(permissions.mode() | 0o111);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to make {} executable", path.display()))
}

#[cfg(not(unix))]
pub fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

pub fn sha256_file(path: &Path) -> Result<String> {
    use std::io::Read as _;

    let mut file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Downloads `url` to `destination`, reusing a file whose checksum already
/// matches. `offline` turns a missing download into an actionable error instead
/// of a network attempt.
pub fn download(url: &str, destination: &Path, expected_sha256: &str, offline: bool) -> Result<()> {
    if destination.is_file() {
        if sha256_file(destination)? == expected_sha256 {
            return Ok(());
        }
        remove(destination)?;
    }
    if offline {
        bail!(
            "cannot download {url} while --offline is set; provide {} manually",
            destination.display()
        );
    }

    if let Some(parent) = destination.parent() {
        create_dir(parent)?;
    }
    let partial = destination.with_extension(format!(
        "{}part",
        destination
            .extension()
            .map(|extension| format!("{}.", extension.to_string_lossy()))
            .unwrap_or_default()
    ));
    remove(&partial)?;

    eprintln!("downloading {url}");
    run(
        Command::new("curl")
            .args([
                "--fail",
                "--location",
                "--retry",
                "3",
                "--retry-all-errors",
                "--silent",
                "--show-error",
                "--output",
            ])
            .arg(&partial)
            .arg(url),
        "download",
    )?;

    let actual = sha256_file(&partial)?;
    if actual != expected_sha256 {
        remove(&partial)?;
        bail!("checksum mismatch for {url}: expected {expected_sha256}, got {actual}");
    }
    fs::rename(&partial, destination).with_context(|| {
        format!(
            "failed to move {} to {}",
            partial.display(),
            destination.display()
        )
    })?;
    Ok(())
}

/// Fetches a small text document (for example a release's `sha256sum.txt`).
pub fn download_text(url: &str, offline: bool) -> Result<String> {
    if offline {
        bail!("cannot fetch {url} while --offline is set");
    }
    let output = output(
        Command::new("curl")
            .args([
                "--fail",
                "--location",
                "--retry",
                "3",
                "--retry-all-errors",
                "--silent",
                "--show-error",
            ])
            .arg(url),
        "download",
    )?;
    String::from_utf8(output.stdout).with_context(|| format!("{url} did not return UTF-8 text"))
}

/// Extracts a `.tar.gz`, `.tar.xz`, or `.zip` archive into `destination`.
pub fn extract(archive: &Path, destination: &Path) -> Result<()> {
    create_dir(destination)?;
    let name = archive.to_string_lossy();
    if name.ends_with(".zip") {
        run(
            Command::new("unzip")
                .arg("-q")
                .arg(archive)
                .arg("-d")
                .arg(destination),
            "unzip",
        )
    } else if name.ends_with(".tar.xz") {
        run(
            Command::new("tar")
                .arg("--extract")
                .arg("--xz")
                .arg("--file")
                .arg(archive)
                .arg("--directory")
                .arg(destination),
            "untar",
        )
    } else {
        run(
            Command::new("tar")
                .arg("--extract")
                .arg("--gzip")
                .arg("--file")
                .arg(archive)
                .arg("--directory")
                .arg(destination),
            "untar",
        )
    }
}

pub fn rust_host_triple() -> Result<String> {
    let output = stdout(
        Command::new("rustc").args(["--version", "--verbose"]),
        "rustc",
    )?;
    output
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_owned)
        .ok_or_else(|| anyhow::anyhow!("rustc did not report a host triple"))
}

pub fn crate_version(workspace: &Path) -> Result<String> {
    let path = workspace.join("crates/zzz/Cargo.toml");
    let source =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let manifest: toml::Value =
        toml::from_str(&source).with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(manifest
        .get("package")
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .unwrap_or("0.0.0")
        .to_owned())
}

pub fn rustup_target_installed(triple: &str) -> Result<bool> {
    let output = stdout(
        Command::new("rustup").args(["target", "list", "--installed"]),
        "rustup target list",
    )?;
    Ok(output.lines().any(|line| line.trim() == triple))
}

pub fn rustup_add_target(triple: &str) -> Result<()> {
    if rustup_target_installed(triple)? {
        return Ok(());
    }
    if rustup_available() {
        run(
            Command::new("rustup").args(["target", "add", triple]),
            "rustup target add",
        )?;
    }
    Ok(())
}

/// Points `cargo zigbuild` at caches inside the workspace, one per target.
///
/// `cargo-zigbuild` keeps its linker wrappers in a cache directory keyed only
/// by its own version, and it creates the shared `ar`, `lib`, and
/// `install_name_tool` links by testing for the path and then calling
/// `symlink`. Two targets building at once trip over that gap, and the loser
/// dies with `File exists (os error 17)` before `cargo` runs at all. Giving
/// every target its own cache keeps the matrix and `dist --all` builds apart.
pub fn configure_zig(command: &mut Command, workspace: &Path, triple: &str) {
    // Zig's global cache is content addressed and safe to share, so it dedupes
    // work across targets; the local cache and the linker wrappers are not.
    let root = workspace.join("target").join("zig-cache");
    command
        .env("ZIG_GLOBAL_CACHE_DIR", root.join("global"))
        .env("ZIG_LOCAL_CACHE_DIR", root.join("local").join(triple))
        .env(
            "CARGO_ZIGBUILD_CACHE_DIR",
            root.join("cargo-zigbuild").join(triple),
        );
}

pub fn rustup_available() -> bool {
    Command::new("rustup")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn command_exists(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Finds an executable on `PATH` without running it. Unlike `command_exists`
/// this also works for tools that do not implement `--version`.
pub fn which(tool: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|directory| directory.join(tool))
        .find(|candidate| candidate.is_file())
}

/// Substitutes `$NAME` and `${NAME}` placeholders. Unknown names are left in
/// place so a template that gains a variable fails visibly instead of silently
/// producing an empty value.
pub fn envsubst(template: &str, variables: &BTreeMap<&str, String>) -> String {
    let mut output = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(index) = rest.find('$') {
        output.push_str(&rest[..index]);
        let reference = &rest[index + 1..];
        let (name, consumed) = if let Some(stripped) = reference.strip_prefix('{') {
            let Some(end) = stripped.find('}') else {
                output.push('$');
                rest = reference;
                continue;
            };
            (&stripped[..end], end + 2)
        } else {
            let end = reference
                .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
                .unwrap_or(reference.len());
            (&reference[..end], end)
        };
        // Keep the original reference, braces included, so this behaves like
        // GNU envsubst: a variable the template expects from its own runtime
        // (for example `${SNAP}` in a desktop entry) survives untouched.
        let original = &rest[index..index + 1 + consumed];
        match variables.get(name) {
            Some(value) if !name.is_empty() => output.push_str(value),
            _ => output.push_str(original),
        }
        rest = &rest[index + 1 + consumed..];
    }
    output.push_str(rest);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envsubst_replaces_both_forms() {
        let mut variables = BTreeMap::new();
        variables.insert("NAME", "ZZZ".to_owned());
        assert_eq!(
            envsubst("name=$NAME id=${NAME}-Dev", &variables),
            "name=ZZZ id=ZZZ-Dev"
        );
    }

    #[test]
    fn envsubst_leaves_unknown_variables_visible() {
        let variables = BTreeMap::new();
        assert_eq!(envsubst("$UNKNOWN", &variables), "$UNKNOWN");
        assert_eq!(
            envsubst("Icon=${SNAP}/meta/gui/zzz.png", &variables),
            "Icon=${SNAP}/meta/gui/zzz.png"
        );
        assert_eq!(envsubst("cost: $5", &variables), "cost: $5");
    }
}
