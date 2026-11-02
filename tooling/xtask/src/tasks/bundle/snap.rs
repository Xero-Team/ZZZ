//! Snap packaging, a host-only step on top of the Linux tarball.
//!
//! `snapcraft` only reads `snap/snapcraft.yaml` from the project root, so unlike
//! the other packagers this module has to write its generated files there. The
//! directory is already ignored by git.

#![allow(clippy::disallowed_methods, reason = "packaging drives snapcraft")]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};

use crate::tasks::build::model::Platform;
use crate::tasks::bundle::linux;
use crate::tasks::bundle::{Context, util};

const DESKTOP_TEMPLATE: &str = "crates/zzz/resources/zzz.desktop.in";
const SNAPCRAFT_TEMPLATE: &str = "crates/zzz/resources/snap/snapcraft.yaml.in";
const ICON: &str = "crates/zzz/resources/app-icon.png";

pub fn bundle(context: &Context) -> Result<()> {
    if Platform::host() != Platform::Linux {
        bail!("Snaps can only be built on Linux");
    }
    if context.target.is_cross() {
        bail!(
            "cross-compiling a Snap is not supported; run it for {} on a matching host",
            context.target.triple()
        );
    }
    util::require_tool(
        "snapcraft",
        "Install it with `sudo snap install snapcraft --classic`.",
    )?;

    let archive = linux::find_archive(context)?;
    let gui = context.workspace.join("snap/gui");
    util::create_dir(&gui)?;

    write_desktop(context, &gui)?;
    util::copy(&context.workspace.join(ICON), &gui.join("zzz.png"))?;
    write_manifest(context, &archive)?;

    // `snapcraft clean` is required for a stale `stage-packages` entry to
    // disappear; snapcraft otherwise reuses the previous build directory.
    util::run(
        Command::new("snapcraft")
            .arg("clean")
            .current_dir(&context.workspace),
        "snapcraft clean",
    )?;
    util::run(
        Command::new("snapcraft").current_dir(&context.workspace),
        "snapcraft",
    )?;

    let produced = newest_snap(&context.workspace)?;
    let name = produced
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("{} has no file name", produced.display()))?;
    let output = context.bundle_dir().join(name);
    util::create_dir(&context.bundle_dir())?;
    util::remove(&output)?;
    fs::rename(&produced, &output).with_context(|| {
        format!(
            "failed to move {} to {}",
            produced.display(),
            output.display()
        )
    })?;

    eprintln!("Snap: {}", output.display());
    Ok(())
}

fn write_desktop(context: &Context, gui: &Path) -> Result<()> {
    let template = read(&context.workspace.join(DESKTOP_TEMPLATE))?;
    let mut variables = BTreeMap::new();
    variables.insert("APP_NAME", "ZZZ".to_owned());
    variables.insert("APP_CLI", "zzz".to_owned());
    variables.insert("APP_ICON", "${SNAP}/meta/gui/zzz.png".to_owned());
    variables.insert("APP_ARGS", "%U".to_owned());
    variables.insert("DO_STARTUP_NOTIFY", "true".to_owned());

    let desktop = gui.join("zzz.desktop");
    fs::write(&desktop, util::envsubst(&template, &variables))
        .with_context(|| format!("failed to write {}", desktop.display()))
}

fn write_manifest(context: &Context, archive: &Path) -> Result<()> {
    let template = read(&context.workspace.join(SNAPCRAFT_TEMPLATE))?;
    let mut variables = BTreeMap::new();
    variables.insert("RELEASE_VERSION", context.release_version.clone());
    variables.insert("SNAP_ARCHIVE", archive.display().to_string());

    let manifest = context.workspace.join("snap/snapcraft.yaml");
    fs::write(&manifest, util::envsubst(&template, &variables))
        .with_context(|| format!("failed to write {}", manifest.display()))
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}

/// Snapcraft names the file `<name>_<version>_<arch>.snap` in the working
/// directory. Pick the newest so a stale artifact cannot be shipped.
fn newest_snap(workspace: &Path) -> Result<PathBuf> {
    let mut candidates = Vec::new();
    let entries = fs::read_dir(workspace)
        .with_context(|| format!("failed to read {}", workspace.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let matches = path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "snap")
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("zzz_"));
        if matches {
            candidates.push(path);
        }
    }
    candidates.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    candidates
        .pop()
        .ok_or_else(|| anyhow::anyhow!("snapcraft reported success but produced no .snap file"))
}
