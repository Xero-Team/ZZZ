//! Locates or downloads the macOS SDK used by the cross build.
//!
//! The full GUI app references CoreAudio process-tap APIs that older SDKs do
//! not declare, so the SDK is resolved rather than inherited from the host: the
//! newest release of `joseluisq/macosx-sdks` is used, or a named tag through
//! `ZZZ_MACOS_SDK_VERSION` / `--macos-sdk`. Like every other third-party input,
//! the download is verified against the `sha256` the release publishes, with the
//! release's `sha256sum.txt` as the fallback for older tags.

#![allow(clippy::disallowed_methods, reason = "SDK setup drives curl and tar")]

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};

use crate::tasks::bundle::github_release;
use crate::tasks::bundle::util;

pub const REPOSITORY: &str = "joseluisq/macosx-sdks";
const PATTERN: &str = "MacOSX*.sdk.tar.xz";
const CHECKSUM_FILE: &str = "sha256sum.txt";

pub fn ensure(workspace: &Path, version: Option<&str>, offline: bool) -> Result<PathBuf> {
    if let Some(sdkroot) = std::env::var_os("SDKROOT") {
        let sdkroot = PathBuf::from(sdkroot);
        if is_usable(&sdkroot) {
            return Ok(sdkroot);
        }
    }

    if cfg!(target_os = "macos")
        && let Ok(path) = util::stdout(
            Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-path"]),
            "xcrun --show-sdk-path",
        )
    {
        let path = PathBuf::from(path);
        if is_usable(&path) {
            return Ok(path);
        }
    }

    let version = version
        .map(str::to_owned)
        .or_else(|| std::env::var("ZZZ_MACOS_SDK_VERSION").ok())
        .filter(|value| !value.is_empty());

    let asset = match version {
        Some(version) => {
            github_release::tagged(REPOSITORY, &format!("MacOSX{version}.sdk.tar.xz"), &version)
                .with_context(|| format!("unsupported macOS SDK version {version}"))?
        }
        None => github_release::latest(REPOSITORY, PATTERN)?,
    };

    let Some(sdk_name) = asset.name.strip_suffix(".tar.xz") else {
        bail!("unexpected macOS SDK asset name: {}", asset.name);
    };
    let root = workspace.join(".deps/macos-sdk");
    util::create_dir(&root)?;
    let sdk_path = root.join(sdk_name);
    if is_usable(&sdk_path) {
        return Ok(sdk_path);
    }

    let sha256 = if asset.sha256.is_empty() {
        checksum_from_release(&asset, offline)?
    } else {
        asset.sha256.clone()
    };
    let archive = root.join(&asset.name);
    eprintln!("installing macOS SDK {} ({})", asset.tag, asset.name);
    util::download(&asset.url, &archive, &sha256, offline)?;
    let extract_root = tempfile::Builder::new()
        .prefix("macos-sdk-")
        .tempdir_in(&root)
        .with_context(|| {
            format!(
                "failed to create a temporary directory in {}",
                root.display()
            )
        })?;
    util::extract(&archive, extract_root.path())?;
    let downloaded = find_sdk(extract_root.path())
        .ok_or_else(|| anyhow::anyhow!("{} contains no usable SDK", asset.name))?;
    util::remove(&sdk_path)?;
    std::fs::rename(&downloaded, &sdk_path).with_context(|| {
        format!(
            "failed to move {} to {}",
            downloaded.display(),
            sdk_path.display()
        )
    })?;
    util::remove(&archive)?;
    if !is_usable(&sdk_path) {
        bail!(
            "the downloaded macOS SDK does not look usable: {}",
            sdk_path.display()
        );
    }
    Ok(sdk_path)
}

/// Older releases predate GitHub's per-asset digest and ship `sha256sum.txt`
/// instead. Fetching the checksum is the one unverified request; the archive
/// itself is still verified with it.
fn checksum_from_release(asset: &github_release::Asset, offline: bool) -> Result<String> {
    let url = format!(
        "https://github.com/{REPOSITORY}/releases/download/{}/{CHECKSUM_FILE}",
        asset.tag
    );
    let text = util::download_text(&url, offline).with_context(|| {
        format!(
            "{} publishes no digest for {} and {CHECKSUM_FILE} is unavailable",
            asset.tag, asset.name
        )
    })?;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(checksum) = fields.next() else {
            continue;
        };
        let Some(name) = fields.next() else {
            continue;
        };
        if name.trim_start_matches('*') == asset.name {
            return Ok(checksum.to_ascii_lowercase());
        }
    }
    bail!(
        "{} does not list {} in {CHECKSUM_FILE}",
        asset.tag,
        asset.name
    )
}

fn find_sdk(root: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() && is_usable(&path) {
            candidates.push(path);
        }
    }
    candidates.sort();
    candidates.into_iter().next()
}

fn is_usable(path: &Path) -> bool {
    path.join("System/Library/Frameworks/CoreServices.framework/Headers/CoreServices.h")
        .is_file()
}
