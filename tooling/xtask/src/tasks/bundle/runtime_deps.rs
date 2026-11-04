//! Runtime dependency contract for Linux bundles.
//!
//! The dynamic loader aborts before `main` when a single `DT_NEEDED` entry
//! cannot be resolved, so every library the editor links has to ship inside the
//! bundle. The exceptions are glibc, the compiler runtime, the GLib family, and
//! libxkbcommon: binaries that get `dlopen`ed are built against the system GLib,
//! and an older bundled copy fails with missing symbols; libxkbcommon parses the
//! keymap the compositor generates at runtime, so a copy from an older build
//! image cannot read keymaps from newer compositors.

#![allow(
    clippy::disallowed_methods,
    reason = "the dependency check drives readelf, ldd, ldconfig, and bwrap"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};

use crate::tasks::bundle::util;

pub fn check(app_dir: &Path, robustness: bool) -> Result<()> {
    if !app_dir.is_dir() {
        bail!("{} is not a directory", app_dir.display());
    }
    for required in ["libexec/zzz-editor", "bin/zzz", "lib"] {
        if !app_dir.join(required).exists() {
            bail!(
                "{} does not look like a ZZZ bundle; {required} is missing",
                app_dir.display()
            );
        }
    }
    if !util::command_exists("readelf") {
        bail!("readelf is required; install binutils");
    }

    let mut failures = Vec::new();
    for binary in [app_dir.join("libexec/zzz-editor"), app_dir.join("bin/zzz")] {
        failures.extend(check_dependencies(&binary, app_dir, false)?);
        failures.extend(check_runtime_search_path(&binary, app_dir)?);
    }

    if let Ok(entries) = std::fs::read_dir(app_dir.join("lib")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                failures.extend(check_dependencies(&path, app_dir, true)?);
            }
        }
    }

    if failures.is_empty() && robustness {
        failures.extend(check_robustness(app_dir)?);
    }

    if !failures.is_empty() {
        bail!("the bundle would resolve libraries from the host system");
    }
    println!(
        "runtime dependencies: only glibc, the compiler runtime, the GLib family, and the host libxkbcommon are resolved from the host"
    );
    Ok(())
}

fn check_dependencies(
    binary: &Path,
    app_dir: &Path,
    allow_host_family: bool,
) -> Result<Vec<String>> {
    let mut failures = Vec::new();
    for library in needed_libraries(binary)? {
        if is_glibc_library(&library) || is_host_keymap_library(&library) {
            continue;
        }
        if allow_host_family && is_host_provided_family(&library) {
            continue;
        }
        if app_dir.join("lib").join(&library).exists() {
            continue;
        }
        failures.push(format!(
            "error: {} needs '{library}', which is neither bundled in lib/ nor provided by glibc",
            relative(binary, app_dir)
        ));
    }
    Ok(failures)
}

fn check_runtime_search_path(binary: &Path, app_dir: &Path) -> Result<Vec<String>> {
    let output = util::stdout(
        Command::new("readelf").args(["-d"]).arg(binary),
        "readelf -d",
    )?;

    let mut tag = None;
    let mut search_path = None;
    for line in output.lines() {
        if let Some(rest) = line.split("(RPATH)").nth(1) {
            tag = Some("RPATH");
            search_path = bracket_contents(rest);
        } else if let Some(rest) = line.split("(RUNPATH)").nth(1) {
            tag = Some("RUNPATH");
            search_path = bracket_contents(rest);
        }
    }

    match tag {
        None => Ok(vec![format!(
            "error: {} has no RPATH, so bundled libraries cannot be found",
            relative(binary, app_dir)
        )]),
        Some("RUNPATH") => Ok(vec![format!(
            "error: {} uses RUNPATH, which bundled libraries do not inherit",
            relative(binary, app_dir)
        )]),
        Some(_)
            if !search_path
                .as_deref()
                .unwrap_or_default()
                .contains("$ORIGIN") =>
        {
            Ok(vec![format!(
                "error: {} does not search $ORIGIN (found '{}')",
                relative(binary, app_dir),
                search_path.unwrap_or_default()
            )])
        }
        Some(_) => Ok(Vec::new()),
    }
}

fn needed_libraries(binary: &Path) -> Result<Vec<String>> {
    let output = util::stdout(
        Command::new("readelf").args(["-d"]).arg(binary),
        "readelf -d",
    )?;
    let mut libraries = Vec::new();
    for line in output.lines() {
        if let Some(rest) = line.split("(NEEDED)").nth(1)
            && let Some(name) = bracket_contents(rest)
        {
            libraries.push(name);
        }
    }
    Ok(libraries)
}

fn bracket_contents(text: &str) -> Option<String> {
    let start = text.find('[')? + 1;
    let end = text.rfind(']')?;
    (end >= start).then(|| text[start..end].to_owned())
}

fn relative(binary: &Path, app_dir: &Path) -> String {
    binary
        .strip_prefix(app_dir)
        .unwrap_or(binary)
        .display()
        .to_string()
}

pub(crate) fn is_glibc_library(name: &str) -> bool {
    const PREFIXES: [&str; 9] = [
        "ld-linux",
        "libc.so.",
        "libm.so.",
        "libmvec.so.",
        "libpthread.so.",
        "libdl.so.",
        "librt.so.",
        "libresolv.so.",
        "libgcc_s.so.",
    ];
    PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

pub(crate) fn is_host_provided_family(name: &str) -> bool {
    // zlib is not listed here: the bundler skips it while collecting the
    // editor's direct dependencies (libz-sys links its own copy), but a bundled
    // library may still resolve the shared ABI from the host, so the transitive
    // pass and this check keep allowing it.
    const PREFIXES: [&str; 10] = [
        "libglib-2.0.so",
        "libgobject-2.0.so",
        "libgio-2.0.so",
        "libgmodule-2.0.so",
        "libpcre.so",
        "libpcre2-8.so",
        "libmount.so",
        "libblkid.so",
        "libselinux.so",
        "libffi.so",
    ];
    PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

/// libxkbcommon parses the keymap the Wayland compositor sends at runtime. The
/// keymap follows the compositor's copy, so a version from an older build image
/// fails on newer compositors; a graphical host always provides its own copy.
pub(crate) fn is_host_keymap_library(name: &str) -> bool {
    const PREFIXES: [&str; 2] = ["libxkbcommon.so", "libxkbcommon-x11.so"];
    PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

fn check_robustness(app_dir: &Path) -> Result<Vec<String>> {
    let editor = app_dir.join("libexec/zzz-editor");
    if !util::command_exists("bwrap") {
        eprintln!("warning: skipping robustness check; bwrap is not installed");
        return Ok(Vec::new());
    }

    let empty = tempfile::NamedTempFile::new().context("failed to create a temporary file")?;
    let mut arguments: Vec<std::ffi::OsString> = vec!["--dev-bind".into(), "/".into(), "/".into()];
    let mut hidden = 0u32;

    if let Ok(entries) = std::fs::read_dir(app_dir.join("lib")) {
        for entry in entries.flatten() {
            let bundled = entry.path();
            let Some(name) = bundled.file_name() else {
                continue;
            };
            let Some(host_library) = host_library_for(&name.to_string_lossy())? else {
                continue;
            };
            arguments.push("--ro-bind".into());
            arguments.push(empty.path().as_os_str().to_owned());
            arguments.push(host_library.into_os_string());
            hidden += 1;
        }
    }

    if hidden == 0 {
        eprintln!("warning: skipping robustness check; no bundled library is present on this host");
        return Ok(Vec::new());
    }

    let status = Command::new("bwrap")
        .args(&arguments)
        .arg(&editor)
        .arg("--help")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("failed to run bwrap")?;
    if status.success() {
        println!("robustness: the editor starts with {hidden} host libraries made unusable");
        Ok(Vec::new())
    } else {
        Ok(vec![
            "error: the editor does not start when the host copies of the bundled libraries are unusable"
                .to_owned(),
        ])
    }
}

fn host_library_for(name: &str) -> Result<Option<PathBuf>> {
    let output = Command::new("ldconfig").arg("-p").output();
    let Ok(output) = output else {
        return Ok(None);
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let Some(path) = text.lines().find_map(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        (fields.first() == Some(&name))
            .then(|| fields.last().copied())
            .flatten()
    }) else {
        return Ok(None);
    };
    // `ldconfig` reports the soname, usually a symlink; bwrap can only shadow
    // the file the symlink points at.
    match std::fs::canonicalize(path) {
        Ok(path) => Ok(Some(path)),
        Err(_) => Ok(Some(PathBuf::from(path))),
    }
}
