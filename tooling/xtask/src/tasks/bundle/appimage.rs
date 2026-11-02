//! AppImage packaging, a host-only step on top of the Linux tarball.
//!
//! `appimagetool` is the only distribution tool that is routinely missing, so
//! it is resolved rather than pinned: `APPIMAGETOOL`, then `PATH`, then the
//! newest release of `AppImage/appimagetool` verified against the digest the
//! release publishes. A downloaded AppImage is run with
//! `--appimage-extract-and-run` because containers and minimal hosts often have
//! no FUSE mount to execute it in place.

#![allow(
    clippy::disallowed_methods,
    reason = "packaging drives appimagetool and tar"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};

use crate::tasks::build::model::Platform;
use crate::tasks::bundle::github_release;
use crate::tasks::bundle::linux;
use crate::tasks::bundle::{Context, app_identity, util};

const REPOSITORY: &str = "AppImage/appimagetool";
const TOOL: &str = "APPIMAGETOOL";
const VERSION: &str = "ZZZ_APPIMAGETOOL_VERSION";
const NO_DOWNLOAD: &str = "ZZZ_APPIMAGE_NO_DOWNLOAD";
const NAME: &str = "ZZZ_APPIMAGE_NAME";

const APPRUN: &str = r#"#!/bin/sh
set -eu
HERE="$(dirname "$(readlink -f "$0")")"
export PATH="${HERE}/bin:${PATH}"
export LD_LIBRARY_PATH="${HERE}/lib${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"
exec "${HERE}/bin/zzz" "$@"
"#;

pub fn bundle(context: &Context) -> Result<()> {
    if Platform::host() != Platform::Linux {
        bail!("AppImages can only be built on Linux");
    }

    let archive = linux::find_archive(context)?;
    let identity = app_identity(context.channel);
    let suffix = identity.suffix;
    let (tool, extract_and_run) = resolve_tool(context)?;

    let temporary = tempfile::Builder::new()
        .prefix("zzz-appimage-")
        .tempdir()
        .context("failed to create an AppImage staging directory")?;
    util::extract(&archive, temporary.path())?;

    let extracted = linux::single_app_directory(temporary.path())?;
    let appdir = temporary.path().join(format!("zzz{suffix}.AppDir"));
    fs::rename(&extracted, &appdir).with_context(|| {
        format!(
            "failed to move {} to {}",
            extracted.display(),
            appdir.display()
        )
    })?;

    util::create_dir(&appdir)?;
    let apprun = appdir.join("AppRun");
    fs::write(&apprun, APPRUN).with_context(|| format!("failed to write {}", apprun.display()))?;
    util::make_executable(&apprun)?;

    let desktop = appdir
        .join("share/applications")
        .join(format!("{}.desktop", identity.id));
    if !desktop.is_file() {
        bail!(
            "the tarball is missing {}.desktop; rebuild the Linux bundle",
            identity.id
        );
    }
    util::copy(&desktop, &appdir.join(format!("zzz{suffix}.desktop")))?;

    let icon = appdir.join("share/icons/hicolor/512x512/apps/zzz.png");
    if !icon.is_file() {
        bail!(
            "the tarball is missing the 512x512 icon at {}",
            icon.display()
        );
    }
    symlink(
        Path::new("share/icons/hicolor/512x512/apps/zzz.png"),
        &appdir.join("zzz.png"),
    )?;
    symlink(Path::new("zzz.png"), &appdir.join(".DirIcon"))?;

    let output = context.bundle_dir().join(format!(
        "{}-linux-{}.AppImage",
        output_name(suffix),
        context.arch()
    ));
    util::remove(&output)?;

    let mut command = Command::new(&tool);
    if extract_and_run {
        command.arg("--appimage-extract-and-run");
    }
    command
        .env("ARCH", context.arch())
        .arg(&appdir)
        .arg(&output);
    util::run(&mut command, "appimagetool")?;

    if !output.is_file() {
        bail!(
            "appimagetool reported success but {} does not exist",
            output.display()
        );
    }
    eprintln!("AppImage: {}", output.display());
    Ok(())
}

fn output_name(suffix: &str) -> String {
    match std::env::var(NAME) {
        Ok(name) if !name.trim().is_empty() => name.trim().to_owned(),
        _ => format!("zzz{suffix}"),
    }
}

/// Returns the tool and whether it has to be run with
/// `--appimage-extract-and-run`. A downloaded AppImage is not guaranteed to be
/// executable in place on a host without FUSE.
fn resolve_tool(context: &Context) -> Result<(PathBuf, bool)> {
    if let Some(path) = std::env::var_os(TOOL) {
        let path = PathBuf::from(path);
        if !path.is_file() {
            bail!("{TOOL} points to {}, which does not exist", path.display());
        }
        return Ok((path, false));
    }
    if let Some(path) = util::which("appimagetool") {
        return Ok((path, false));
    }
    if std::env::var(NO_DOWNLOAD).is_ok_and(|value| value == "1") {
        bail!(
            "appimagetool was not found and {NO_DOWNLOAD}=1; install it from \
             https://github.com/AppImage/appimagetool/releases or set {TOOL}"
        );
    }

    let pattern = format!("appimagetool-{}.AppImage", context.arch());
    let asset = match std::env::var(VERSION) {
        Ok(tag) if !tag.trim().is_empty() => {
            github_release::tagged(REPOSITORY, &pattern, tag.trim())?
        }
        _ => github_release::latest(REPOSITORY, &pattern)?,
    };

    let directory = context.target_dir.join("appimage");
    util::create_dir(&directory)?;
    let key = asset.sha256.get(..12).unwrap_or(&asset.tag);
    let path = directory.join(format!("appimagetool-{key}-{}.AppImage", context.arch()));
    if !path.is_file() {
        eprintln!(
            "appimagetool not found on PATH; installing {} into {}",
            asset.tag,
            path.display()
        );
        github_release::download(&asset, &path, context.offline)?;
        prune(&directory, &path)?;
    }
    util::make_executable(&path)?;
    Ok((path, true))
}

fn prune(directory: &Path, keep: &Path) -> Result<()> {
    let entries = fs::read_dir(directory)
        .with_context(|| format!("failed to read {}", directory.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|name| name.to_str())
            && name.starts_with("appimagetool-")
            && name.ends_with(".AppImage")
        {
            util::remove(&path)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link)
        .with_context(|| format!("failed to link {} to {}", link.display(), target.display()))
}

#[cfg(not(unix))]
fn symlink(_target: &Path, _link: &Path) -> Result<()> {
    bail!("AppImage symlinks require a Unix host")
}
