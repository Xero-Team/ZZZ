//! Flatpak packaging, a host-only step on top of the Linux tarball.
//!
//! The manifest is generated from `crates/zzz/resources/flatpak`, with the
//! channel's app id and branding substituted in. Sources are made absolute so
//! the manifest can live under `target/` instead of the repository root, which
//! keeps a flatpak build from leaving an untracked `dev.zzz.ZZZ*.json` behind.

#![allow(
    clippy::disallowed_methods,
    reason = "packaging drives flatpak-builder"
)]

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context as _, Result, bail};

use crate::tasks::build::model::{Channel, Platform};
use crate::tasks::bundle::linux;
use crate::tasks::bundle::{Context, app_identity, util};

pub fn bundle(context: &Context) -> Result<()> {
    if Platform::host() != Platform::Linux {
        bail!("Flatpaks can only be built on Linux");
    }
    if context.target.is_cross() {
        bail!(
            "cross-compiling a Flatpak is not supported; run it for {} on a matching host",
            context.target.triple()
        );
    }
    for tool in ["flatpak", "flatpak-builder"] {
        util::require_tool(tool, "Install flatpak and flatpak-builder.")?;
    }

    let archive = linux::find_archive(context)?;
    let archive_name = archive
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("{} has no file name", archive.display()))?;
    let identity = app_identity(context.channel);
    let (light, dark) = branding(context.channel);

    let template_path = context
        .workspace
        .join("crates/zzz/resources/flatpak/manifest-template.json");
    let template = fs::read_to_string(&template_path)
        .with_context(|| format!("failed to read {}", template_path.display()))?;

    let mut variables = BTreeMap::new();
    variables.insert("APP_ID", identity.id.clone());
    variables.insert("APP_NAME", identity.name.to_owned());
    variables.insert("BRANDING_LIGHT", light.to_owned());
    variables.insert("BRANDING_DARK", dark.to_owned());
    variables.insert("ICON_FILE", identity.icon.to_owned());
    variables.insert("ARCHIVE", archive_name.to_owned());
    variables.insert("CHANNEL", context.channel.as_str().to_owned());

    let resources = context.workspace.join("crates/zzz/resources");
    let manifest = util::envsubst(&template, &variables)
        // The template names the archive relative to the repository root; the
        // generated manifest lives under `target/`, so both sources become
        // absolute instead of depending on where the file is written.
        .replace(
            &format!("\"./target/release/{archive_name}\""),
            &format!("\"{}\"", archive.display()),
        )
        .replace(
            "\"./crates/zzz/resources\"",
            &format!("\"{}\"", resources.display()),
        );

    let directory = context.target_dir.join("flatpak");
    util::create_dir(&directory)?;
    let manifest_path = directory.join(format!("{}.json", identity.id));
    fs::write(&manifest_path, manifest)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;

    let build_directory = directory.join("build");
    let mut command = Command::new("flatpak-builder");
    command
        .arg("--user")
        .arg("--install")
        .arg("--force-clean")
        .arg(&build_directory)
        .arg(&manifest_path);
    util::run(&mut command, "flatpak-builder")?;

    let output = context
        .bundle_dir()
        .join(format!("{}.flatpak", identity.id));
    util::remove(&output)?;
    let repo = repository()?;
    util::run(
        Command::new("flatpak")
            .arg("build-bundle")
            .arg(&repo)
            .arg(&output)
            .arg(&identity.id),
        "flatpak build-bundle",
    )?;

    if !output.is_file() {
        bail!(
            "flatpak build-bundle reported success but {} does not exist",
            output.display()
        );
    }
    eprintln!("Flatpak: {}", output.display());
    Ok(())
}

fn repository() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("HOME is not set; cannot locate the Flatpak repository"))?;
    Ok(home.join(".local/share/flatpak/repo"))
}

/// Only the nightly build uses a warmer accent; the rest share the default
/// palette the product uses for the light theme.
fn branding(channel: Channel) -> (&'static str, &'static str) {
    match channel {
        Channel::Nightly => ("#e9aa6a", "#1a5fb4"),
        Channel::Stable | Channel::Preview | Channel::Dev => ("#99c1f1", "#1a5fb4"),
    }
}
