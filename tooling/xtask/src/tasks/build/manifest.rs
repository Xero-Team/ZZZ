//! Artifact manifest and checksums.
//!
//! A release is a set of files that somebody else has to trust. The manifest
//! records what produced each one — commit, profile, toolchain image, flags — so
//! a file can be traced back without rebuilding it.

#![allow(
    clippy::disallowed_methods,
    reason = "manifest writes checksums over artifacts produced by this run"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::tasks::build::model::{Format, Target};
use crate::tasks::build::plan::{Plan, RepositoryState};

pub const MANIFEST_FILE: &str = "manifest.json";
pub const CHECKSUMS_FILE: &str = "SHA256SUMS";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub target: String,
    pub format: String,
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub created_at: u64,
    pub source: RepositoryState,
    pub channel: String,
    pub profile: String,
    pub jobs: u32,
    pub containers: Vec<String>,
    pub pgo: Vec<String>,
    pub artifacts: Vec<ArtifactRecord>,
}

/// Files produced by this run, discovered by extension and modification time
/// rather than by hard-coded names. Packaging scripts own their output names;
/// the manifest should not have to know all of them.
pub fn collect(workspace: &Path, plan: &Plan, since: SystemTime) -> Result<Vec<ArtifactRecord>> {
    // One task can be containerized while another is not, so both roots are
    // searched and the records say where each file came from.
    let roots = [workspace.join("target"), workspace.join("target/container")];
    // A per-target directory can only hold that target's output, while the two
    // `release` directories are shared by every target the plan builds.
    let mut directories: BTreeMap<PathBuf, Option<Target>> = BTreeMap::new();
    for root in roots {
        directories.insert(root.clone(), None);
        directories.insert(root.join("release"), None);
        for target in &plan.targets {
            directories.insert(
                root.join(target.target.triple()).join("release"),
                Some(target.target),
            );
        }
    }

    let wanted: BTreeSet<(Target, Format)> = plan
        .targets
        .iter()
        .flat_map(|target| {
            target
                .formats
                .iter()
                .map(move |format| (target.target, *format))
        })
        .collect();

    let mut records = Vec::new();
    for (directory, owner) in directories {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !name.to_ascii_lowercase().starts_with("zzz") || name.contains("remote-server") {
                continue;
            }
            let Some(format) = format_for_name(name) else {
                continue;
            };
            let Some(target) = artifact_target(owner, &wanted, format, name) else {
                continue;
            };
            if !modified_since(&path, since)? {
                continue;
            }

            records.push(ArtifactRecord {
                target: target.triple().to_owned(),
                format: format.to_string(),
                path: relative(workspace, &path),
                size: entry.metadata()?.len(),
                sha256: sha256_file(&path)?,
            });
        }
    }

    records.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(records)
}

pub fn write(
    workspace: &Path,
    plan: &Plan,
    artifacts: Vec<ArtifactRecord>,
    out_directory: &Path,
) -> Result<PathBuf> {
    std::fs::create_dir_all(out_directory)
        .with_context(|| format!("failed to create {}", out_directory.display()))?;

    let containers = plan
        .targets
        .iter()
        .filter_map(|target| target.container.as_ref())
        .map(|container| {
            format!(
                "{}/{}",
                container.image,
                container.image_id.as_deref().unwrap_or("unknown")
            )
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let pgo = plan
        .targets
        .iter()
        .filter_map(|target| {
            target
                .pgo
                .as_ref()
                .map(|pgo| format!("{}={}", target.target.triple(), pgo.fingerprint))
        })
        .collect();

    let manifest = Manifest {
        schema_version: 1,
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs()),
        source: plan.repository.clone(),
        channel: plan.channel.as_str().to_owned(),
        profile: plan.profile.key().to_owned(),
        jobs: plan.jobs,
        containers,
        pgo,
        artifacts: artifacts.clone(),
    };

    let manifest_path = out_directory.join(MANIFEST_FILE);
    let json = serde_json::to_string_pretty(&manifest)
        .context("failed to serialize the artifact manifest")?;
    std::fs::write(&manifest_path, json)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    let checksums = checksums_document(&artifacts);
    let checksums_path = out_directory.join(CHECKSUMS_FILE);
    std::fs::write(&checksums_path, checksums)
        .with_context(|| format!("failed to write {}", checksums_path.display()))?;

    copy_artifacts(workspace, &artifacts, out_directory)?;
    Ok(manifest_path)
}

/// `SHA256SUMS` names the copies in the output directory, because that is what
/// a consumer downloads and re-hashes. The manifest keeps the path inside the
/// workspace, where the file was actually produced.
fn checksums_document(artifacts: &[ArtifactRecord]) -> String {
    let mut document = String::new();
    for artifact in artifacts {
        let name = Path::new(&artifact.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&artifact.path);
        document.push_str(&format!("{}  {}\n", artifact.sha256, name));
    }
    document
}

fn copy_artifacts(
    workspace: &Path,
    artifacts: &[ArtifactRecord],
    out_directory: &Path,
) -> Result<()> {
    for artifact in artifacts {
        let source = workspace.join(&artifact.path);
        let Some(name) = source.file_name() else {
            bail!("artifact {} has no file name", source.display());
        };
        let destination = out_directory.join(name);
        if source == destination {
            continue;
        }
        std::fs::copy(&source, &destination).with_context(|| {
            format!(
                "failed to copy {} to {}",
                source.display(),
                destination.display()
            )
        })?;
    }
    Ok(())
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
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

fn modified_since(path: &Path, since: SystemTime) -> Result<bool> {
    let modified = path
        .metadata()
        .with_context(|| format!("failed to stat {}", path.display()))?
        .modified()
        .context("the filesystem does not report modification times")?;
    Ok(modified >= since)
}

fn relative(workspace: &Path, path: &Path) -> String {
    path.strip_prefix(workspace)
        .unwrap_or(path)
        .display()
        .to_string()
}

/// The target a discovered file belongs to. A per-target directory names it
/// directly; a shared `release` directory needs the arch in the file name,
/// because both architectures of a format can land there.
fn artifact_target(
    owner: Option<Target>,
    wanted: &BTreeSet<(Target, Format)>,
    format: Format,
    name: &str,
) -> Option<Target> {
    match owner {
        Some(target) => wanted.contains(&(target, format)).then_some(target),
        None => wanted
            .iter()
            .find(|(target, wanted_format)| {
                *wanted_format == format && name.contains(target.arch().as_str())
            })
            .map(|(target, _)| *target),
    }
}

fn format_for_name(name: &str) -> Option<Format> {
    let lower = name.to_ascii_lowercase();
    // `.tar.gz` is the only two-part suffix here; match it before the single
    // suffix so a tarball is not mistaken for a lone archive.
    if lower.ends_with("tar.gz") {
        return Some(Format::TarGz);
    }
    match lower.rsplit_once('.').map(|(_, extension)| extension)? {
        "appimage" => Some(Format::AppImage),
        "flatpak" => Some(Format::Flatpak),
        "snap" => Some(Format::Snap),
        "deb" => Some(Format::Deb),
        "rpm" => Some(Format::Rpm),
        "dmg" => Some(Format::Dmg),
        "msi" => Some(Format::Msi),
        "exe" => Some(Format::Exe),
        _ => None,
    }
}

/// Keeps the artifact manifest readable: every shipped file is discovered by
/// extension, so a new format only needs an entry here and in `Format`.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_are_recognized_by_extension() {
        assert_eq!(
            format_for_name("zzz-linux-x86_64.tar.gz"),
            Some(Format::TarGz)
        );
        assert_eq!(format_for_name("ZZZ-x86_64.dmg"), Some(Format::Dmg));
        assert_eq!(format_for_name("zzz.AppImage"), Some(Format::AppImage));
        assert_eq!(format_for_name("zzz-x86_64.deb"), Some(Format::Deb));
        assert_eq!(format_for_name("random.txt"), None);
    }

    #[test]
    fn a_per_target_directory_names_its_target() {
        let wanted = BTreeSet::from([(Target::WindowsX86_64, Format::Exe)]);

        // `zzz.exe` carries no arch, and only the Windows directory holds it.
        assert_eq!(
            artifact_target(Some(Target::WindowsX86_64), &wanted, Format::Exe, "zzz.exe"),
            Some(Target::WindowsX86_64)
        );
        // In a shared directory the arch in the name is what disambiguates.
        assert_eq!(artifact_target(None, &wanted, Format::Exe, "zzz.exe"), None);
        assert_eq!(
            artifact_target(None, &wanted, Format::Exe, "zzz-x86_64.exe"),
            Some(Target::WindowsX86_64)
        );
        // A format the plan did not request still stays out.
        assert_eq!(
            artifact_target(Some(Target::WindowsX86_64), &wanted, Format::Dmg, "zzz.exe"),
            None
        );
    }

    #[test]
    fn checksums_are_stable() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("artifact");
        std::fs::write(&path, b"hello").expect("write");
        assert_eq!(
            sha256_file(&path).expect("hash"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn checksums_name_the_copied_file() {
        let document = checksums_document(&[ArtifactRecord {
            target: "x86_64-unknown-linux-gnu".to_owned(),
            format: "tar-gz".to_owned(),
            path: "target/release/zzz-linux-x86_64.tar.gz".to_owned(),
            size: 1,
            sha256: "00".repeat(32),
        }]);
        assert_eq!(
            document,
            format!("{}  zzz-linux-x86_64.tar.gz\n", "00".repeat(32))
        );
    }
}
