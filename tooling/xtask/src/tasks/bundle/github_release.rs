//! Resolves third-party build inputs through the GitHub release API.
//!
//! Hand-pinned versions rot: a maintainer deletes an `autobuild-*` release and
//! every build script that named it starts failing with HTTP 404. Resolving the
//! newest release and verifying the published `sha256` digest keeps the build
//! working without a human noticing, and still refuses an unverified download.

#![allow(
    clippy::disallowed_methods,
    reason = "release discovery drives curl and the GitHub API"
)]

use std::process::Command;

use anyhow::{Context as _, Result, bail};
use serde::Deserialize;

use crate::tasks::bundle::util;

#[derive(Clone, Debug)]
pub struct Asset {
    pub tag: String,
    pub name: String,
    /// Empty when the release predates GitHub's digest field.
    pub sha256: String,
    pub url: String,
    pub published_at: String,
}

#[derive(Deserialize)]
struct Release {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    #[serde(default)]
    name: String,
    #[serde(default)]
    digest: Option<String>,
    #[serde(default)]
    browser_download_url: String,
}

/// The newest release asset whose name matches `pattern`.
pub fn latest(repository: &str, pattern: &str) -> Result<Asset> {
    let releases = fetch(&format!("/repos/{repository}/releases?per_page=40"))?;
    newest(repository, pattern, &releases)
}

/// An asset from a specific release tag.
pub fn tagged(repository: &str, pattern: &str, tag: &str) -> Result<Asset> {
    let release = fetch(&format!("/repos/{repository}/releases/tags/{tag}"))?;
    newest(repository, pattern, &release)
}

fn newest(repository: &str, pattern: &str, releases: &[Release]) -> Result<Asset> {
    let mut candidates = Vec::new();
    for release in releases {
        for asset in &release.assets {
            if !glob_matches(pattern, &asset.name) {
                continue;
            }
            let digest = asset
                .digest
                .as_deref()
                .and_then(|digest| digest.strip_prefix("sha256:"))
                .unwrap_or_default()
                .to_owned();
            candidates.push(Asset {
                tag: release.tag_name.clone(),
                name: asset.name.clone(),
                sha256: digest,
                url: asset.browser_download_url.clone(),
                published_at: release
                    .published_at
                    .clone()
                    .or_else(|| release.created_at.clone())
                    .unwrap_or_default(),
            });
        }
    }
    candidates.sort_by(|left, right| right.published_at.cmp(&left.published_at));
    candidates.into_iter().next().ok_or_else(|| {
        anyhow::anyhow!("{repository} publishes no release asset matching `{pattern}`")
    })
}

fn fetch(path: &str) -> Result<Vec<Release>> {
    let token = std::env::var("GITHUB_TOKEN")
        .ok()
        .or_else(|| std::env::var("GH_TOKEN").ok());
    let mut command = Command::new("curl");
    command.args([
        "--silent",
        "--show-error",
        "--fail",
        "--location",
        "--retry",
        "3",
        "--retry-all-errors",
        "--header",
        "User-Agent: ZZZ-Setup",
        "--header",
        "Accept: application/vnd.github+json",
    ]);
    if let Some(token) = token {
        command.args(["--header", &format!("Authorization: Bearer {token}")]);
    }
    command.arg(format!("https://api.github.com{path}"));

    let output = util::output(&mut command, "GitHub release lookup")?;
    let text = String::from_utf8(output.stdout).context("GitHub returned non-UTF-8 JSON")?;
    let value: serde_json::Value =
        serde_json::from_str(&text).context("failed to parse the GitHub release response")?;
    // The API returns an object for a single release and an array for a list.
    if value.is_array() {
        serde_json::from_value(value).context("failed to parse GitHub releases")
    } else {
        let release: Release =
            serde_json::from_value(value).context("failed to parse a GitHub release")?;
        Ok(vec![release])
    }
}

/// Downloads an asset and verifies the digest the API published. A release
/// without a digest is refused: the whole point is that the bytes were not
/// chosen by hand.
pub fn download(asset: &Asset, destination: &std::path::Path, offline: bool) -> Result<()> {
    if asset.sha256.is_empty() {
        bail!(
            "{} publishes no sha256 digest for {}; refusing an unverified download",
            asset.tag,
            asset.name
        );
    }
    util::download(&asset.url, destination, &asset.sha256, offline)
}

/// Shell-style glob matching: `*` matches any run, `?` one character.
pub fn glob_matches(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let mut pattern_index = 0;
    let mut name_index = 0;
    let mut star: Option<(usize, usize)> = None;

    while name_index < name.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == '?' || pattern[pattern_index] == name[name_index])
        {
            pattern_index += 1;
            name_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == '*' {
            star = Some((pattern_index, name_index));
            pattern_index += 1;
        } else if let Some((star_index, star_name)) = star {
            pattern_index = star_index + 1;
            name_index = star_name + 1;
            star = Some((star_index, star_name + 1));
        } else {
            return false;
        }
    }

    pattern[pattern_index.min(pattern.len())..]
        .iter()
        .all(|character| *character == '*')
        && pattern_index <= pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs_match_ffmpeg_asset_names() {
        assert!(glob_matches(
            "ffmpeg-n*-linux64-lgpl-shared-9.0.tar.xz",
            "ffmpeg-n9.0.2-22-g46d8f462ee-linux64-lgpl-shared-9.0.tar.xz"
        ));
        assert!(!glob_matches(
            "ffmpeg-n*-win64-lgpl-shared-9.0.zip",
            "ffmpeg-n9.0.2-22-g46d8f462ee-linux64-lgpl-shared-9.0.tar.xz"
        ));
        assert!(glob_matches(
            "appimagetool-?.AppImage",
            "appimagetool-x.AppImage"
        ));
        assert!(glob_matches("*", "anything"));
    }
}
