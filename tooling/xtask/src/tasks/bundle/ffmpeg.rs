//! FFmpeg development packages, resolved through the GitHub release API.
//!
//! `ffmpeg-the-third` links against the FFmpeg 9.0 ABI, so the newest build of
//! that series is used. BtbN republishes the `latest` release on every build,
//! which keeps the asset names stable while the contents move forward. The
//! checksum comes from the API, and a cached install is refreshed on an
//! interval so a new build is picked up without hand-editing a version.

#![allow(
    clippy::disallowed_methods,
    reason = "the FFmpeg setup shells out to curl, tar, and unzip"
)]

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, bail};

use crate::tasks::bundle::github_release;
use crate::tasks::bundle::util;

pub const REPOSITORY: &str = "BtbN/FFmpeg-Builds";
pub const FFMPEG_SERIES: &str = "9.0";
pub const REFRESH_INTERVAL_DAYS: u64 = 7;
pub const MARKER_NAME: &str = ".zzz-ffmpeg.json";

struct TargetSpec {
    target: &'static str,
    platform: &'static str,
    extension: &'static str,
}

const TARGETS: [TargetSpec; 4] = [
    TargetSpec {
        target: "x86_64-unknown-linux-gnu",
        platform: "linux64",
        extension: "tar.xz",
    },
    TargetSpec {
        target: "aarch64-unknown-linux-gnu",
        platform: "linuxarm64",
        extension: "tar.xz",
    },
    TargetSpec {
        target: "x86_64-pc-windows-msvc",
        platform: "win64",
        extension: "zip",
    },
    TargetSpec {
        target: "aarch64-pc-windows-msvc",
        platform: "winarm64",
        extension: "zip",
    },
];

/// The default dependency root, shared by the bundlers and `cargo xtask ffmpeg`.
pub fn dependency_root(workspace: &Path) -> PathBuf {
    workspace.join(".deps/ffmpeg")
}

fn asset_pattern(spec: &TargetSpec) -> String {
    format!(
        "ffmpeg-n*-{}-lgpl-shared-{FFMPEG_SERIES}.{}",
        spec.platform, spec.extension
    )
}

pub fn ensure(
    workspace: &Path,
    target: &str,
    force: bool,
    refresh: bool,
    offline: bool,
) -> Result<PathBuf> {
    ensure_in(&dependency_root(workspace), target, force, refresh, offline)
}

/// [`ensure`] with an explicit dependency root instead of `<workspace>/.deps/ffmpeg`.
pub fn ensure_in(
    root: &Path,
    target: &str,
    force: bool,
    refresh: bool,
    offline: bool,
) -> Result<PathBuf> {
    let spec = TARGETS
        .iter()
        .find(|spec| spec.target == target)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "unsupported FFmpeg target {target}; supported: {}",
                TARGETS
                    .iter()
                    .map(|spec| spec.target)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    util::create_dir(root)?;
    let install_dir = root.join(target);
    let cache = root.join("cache");
    let pattern = asset_pattern(spec);
    let refresh_interval = refresh_interval();

    let marker = if force {
        None
    } else {
        read_marker(&install_dir)
    };
    if marker.is_some() && offline {
        return Ok(install_dir);
    }
    if let Some(marker) = &marker
        && !refresh
        && marker_series_matches(marker, &pattern)
        && marker_is_fresh(marker, refresh_interval)
    {
        return Ok(install_dir);
    }

    let asset = match github_release::latest(REPOSITORY, &pattern) {
        Ok(asset) => asset,
        Err(error) => {
            if let Some(marker) = &marker {
                eprintln!(
                    "warning: keeping the installed FFmpeg package, could not check for a newer build: {error}"
                );
                let _ = marker;
                return Ok(install_dir);
            }
            return Err(error);
        }
    };
    let checked_at = now_unix();

    if let Some(marker) = &marker
        && marker
            .get("sha256")
            .and_then(|value| value.as_str())
            .is_some_and(|sha| sha == asset.sha256)
    {
        write_marker(&install_dir, marker, checked_at)?;
        return Ok(install_dir);
    }

    eprintln!(
        "installing FFmpeg {FFMPEG_SERIES} from {}/{}",
        asset.tag, asset.name
    );
    util::create_dir(&cache)?;
    let archive = cache.join(&asset.name);
    github_release::download(&asset, &archive, offline)?;
    prune_cache(&cache, &pattern, &archive)?;

    let extract_root = tempfile::Builder::new()
        .prefix("ffmpeg-extract-")
        .tempdir_in(root)
        .with_context(|| {
            format!(
                "failed to create a temporary directory in {}",
                root.display()
            )
        })?;
    util::extract(&archive, extract_root.path())?;
    let source = package_root(extract_root.path())?;
    let staging = root.join(format!(".{target}.staging"));
    util::remove(&staging)?;
    std::fs::rename(&source, &staging).with_context(|| {
        format!(
            "failed to move {} to {}",
            source.display(),
            staging.display()
        )
    })?;

    let marker = serde_json::json!({
        "target": target,
        "asset_pattern": pattern,
        "ffmpeg_series": FFMPEG_SERIES,
        "archive": asset.name,
        "sha256": asset.sha256,
        "release_tag": asset.tag,
        "published_at": asset.published_at,
        "source": asset.url,
        "checked_at_unix": checked_at,
    });
    std::fs::write(
        staging.join(MARKER_NAME),
        serde_json::to_string_pretty(&marker)? + "\n",
    )?;

    util::remove(&install_dir)?;
    std::fs::rename(&staging, &install_dir).with_context(|| {
        format!(
            "failed to move {} to {}",
            staging.display(),
            install_dir.display()
        )
    })?;
    Ok(install_dir)
}

/// `<series>|<archive>|<release tag>` for the installed package, used by the
/// generated license document so it names the bundled build.
pub fn dependency_info(workspace: &Path) -> (String, String, String) {
    dependency_info_in(&dependency_root(workspace))
}

/// [`dependency_info`] with an explicit dependency root.
pub fn dependency_info_in(root: &Path) -> (String, String, String) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return default_dependency_info();
    };
    for entry in entries.flatten() {
        let path = entry.path().join(MARKER_NAME);
        let Ok(contents) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(marker) = serde_json::from_str::<serde_json::Value>(&contents) else {
            continue;
        };
        return (
            marker
                .get("ffmpeg_series")
                .and_then(|value| value.as_str())
                .unwrap_or(FFMPEG_SERIES)
                .to_owned(),
            marker
                .get("archive")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown build")
                .to_owned(),
            marker
                .get("release_tag")
                .and_then(|value| value.as_str())
                .unwrap_or("latest")
                .to_owned(),
        );
    }
    default_dependency_info()
}

fn default_dependency_info() -> (String, String, String) {
    (
        FFMPEG_SERIES.to_owned(),
        "unknown build".to_owned(),
        "latest".to_owned(),
    )
}

fn package_root(extracted: &Path) -> Result<PathBuf> {
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(extracted)? {
        let path = entry?.path();
        if path.is_dir()
            && path.join("include/libavcodec/avcodec.h").is_file()
            && path.join("lib").is_dir()
        {
            candidates.push(path);
        }
    }
    match candidates.len() {
        1 => Ok(candidates.remove(0)),
        _ => bail!("the FFmpeg archive does not contain exactly one development package"),
    }
}

fn read_marker(install_dir: &Path) -> Option<serde_json::Value> {
    let contents = std::fs::read_to_string(install_dir.join(MARKER_NAME)).ok()?;
    let marker: serde_json::Value = serde_json::from_str(&contents).ok()?;
    if !install_dir.join("include/libavcodec/avcodec.h").is_file()
        || !install_dir.join("lib").is_dir()
    {
        return None;
    }
    Some(marker)
}

fn marker_series_matches(marker: &serde_json::Value, pattern: &str) -> bool {
    marker.get("asset_pattern").and_then(|value| value.as_str()) == Some(pattern)
}

fn marker_is_fresh(marker: &serde_json::Value, interval: Duration) -> bool {
    let Some(checked) = marker
        .get("checked_at_unix")
        .and_then(serde_json::Value::as_u64)
    else {
        return false;
    };
    let now = now_unix();
    now.saturating_sub(checked) < interval.as_secs()
}

fn write_marker(install_dir: &Path, marker: &serde_json::Value, checked_at: u64) -> Result<()> {
    let mut marker = marker.clone();
    marker["checked_at_unix"] = serde_json::Value::from(checked_at);
    std::fs::write(
        install_dir.join(MARKER_NAME),
        serde_json::to_string_pretty(&marker)? + "\n",
    )?;
    Ok(())
}

fn prune_cache(cache: &Path, pattern: &str, keep: &Path) -> Result<()> {
    for entry in std::fs::read_dir(cache)?.flatten() {
        let path = entry.path();
        if path == keep || !path.is_file() {
            continue;
        }
        if let Some(name) = path.file_name().and_then(|name| name.to_str())
            && github_release::glob_matches(pattern, name)
        {
            util::remove(&path)?;
        }
    }
    Ok(())
}

fn refresh_interval() -> Duration {
    let days = std::env::var("ZZZ_FFMPEG_REFRESH_DAYS")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(REFRESH_INTERVAL_DAYS as f64);
    Duration::from_secs((days.max(0.0) * 86_400.0) as u64)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

/// Validates that a directory looks like a usable development package.
pub fn require_complete(directory: &Path) -> Result<()> {
    if !directory.join("include/libavcodec/avcodec.h").is_file() || !directory.join("lib").is_dir()
    {
        bail!(
            "the FFmpeg development package is incomplete: {}",
            directory.display()
        );
    }
    Ok(())
}
