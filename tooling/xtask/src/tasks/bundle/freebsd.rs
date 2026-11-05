//! FreeBSD packaging: the `tar.gz` artifact.
//!
//! FreeBSD shares the desktop layout with Linux (`.app` directory, `libexec`,
//! `bin`, `share/icons`, `share/applications`), so the staged tree mirrors
//! `bundle/linux.rs`. The difference is the toolchain: a native FreeBSD host
//! needs nothing special, while a Linux cross build takes its sysroot and
//! linker from `build::freebsd`.
//!
//! Dynamic library bundling, FFmpeg staging, deb/rpm, and the glibc floor are
//! Linux-only. A FreeBSD tarball carries the two binaries, the generated
//! license document, the icons, and the desktop entry; the system provides the
//! shared libraries (documented in `docs/src/development/freebsd.md`).

#![allow(
    clippy::disallowed_methods,
    reason = "packaging drives cargo, tar, and the strip tools"
)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};

use crate::tasks::build::freebsd::{self, FreeBSDEnvironment, SYSROOT_VARIABLE};
use crate::tasks::build::model::{Format, Platform};
use crate::tasks::bundle::{AppIdentity, Context, app_identity, licenses, util};

const DESKTOP_TEMPLATE: &str = "crates/zzz/resources/zzz.desktop.in";

/// Dispatches a FreeBSD target from the shared bundle entry point. The
/// sysroot and linker come from the environment (see `build::freebsd`), which
/// the planner's child processes inherit unchanged.
pub fn bundle(context: &Context) -> Result<()> {
    if context.target.platform() != Platform::FreeBSD {
        bail!("{} is not a FreeBSD target", context.target.triple());
    }
    if !context.profile.is_release() {
        bail!("FreeBSD bundles need a release-family profile; use `cargo run` for a dev build");
    }
    if context
        .formats
        .iter()
        .any(|format| *format != Format::TarGz)
    {
        bail!("FreeBSD only supports the tar.gz format");
    }

    let sysroot = std::env::var_os(SYSROOT_VARIABLE).map(PathBuf::from);
    let environment = FreeBSDEnvironment::detect(context.target, sysroot.as_deref())?;
    eprintln!(
        "freebsd: bundling {} (profile {}, channel {}, {})",
        context.target.triple(),
        context.profile.key(),
        context.channel.as_str(),
        if environment.is_cross() {
            "cross"
        } else {
            "native"
        }
    );
    environment.require_cross(context.target)?;
    freebsd::ensure_rust_std(context.target, &environment)?;
    licenses::generate(&context.workspace, None, context.offline)?;

    if !context.skip_build {
        build_binaries(context, &environment)?;
    }
    let staged = stage(context)?;
    let archive = write_archive(context, &staged)?;
    eprintln!("freebsd bundle: {}", archive.display());
    Ok(())
}

fn build_binaries(context: &Context, environment: &FreeBSDEnvironment) -> Result<()> {
    let mut rustflags = std::env::var("RUSTFLAGS").unwrap_or_default();
    // Keep the bundled libraries reachable if a sysroot adds transitive ones.
    rustflags.push_str(" -C link-args=-Wl,--disable-new-dtags,-rpath,$ORIGIN/../lib");

    let mut command = Command::new("cargo");
    command
        .current_dir(&context.workspace)
        .arg("build")
        .args(context.profile_flag())
        .args(["--target", context.target.triple(), "--target-dir"])
        .arg(&context.target_dir)
        .args(["--package", "zzz", "--package", "cli"])
        .env("ZZZ_BUNDLE", "true")
        .env("ZZZ_RELEASE_CHANNEL", context.channel.as_str())
        .env("RELEASE_VERSION", &context.release_version)
        .env("CARGO_BUILD_JOBS", context.jobs.to_string())
        .env("LIBZ_SYS_STATIC", "1")
        .env_remove("CARGO_ENCODED_RUSTFLAGS");
    if context.offline {
        command.env("CARGO_NET_OFFLINE", "true");
    }
    environment.apply(context.target, &mut command, &rustflags)?;
    util::run(&mut command, "cargo build")
}

struct StagedBundle {
    temporary: tempfile::TempDir,
    app_name: String,
}

fn stage(context: &Context) -> Result<StagedBundle> {
    let identity = app_identity(context.channel);
    let temporary = tempfile::Builder::new()
        .prefix("zzz-freebsd-bundle-")
        .tempdir()
        .context("failed to create a staging directory")?;
    let app_name = format!("zzz{}.app", identity.suffix);
    let app_dir = temporary.path().join(&app_name);

    let artifact_dir = context.artifact_dir();
    for (source, destination) in [("zzz", "libexec/zzz-editor"), ("cli", "bin/zzz")] {
        let binary = artifact_dir.join(source);
        if !binary.is_file() {
            bail!(
                "{} is missing; run without --skip-build first",
                binary.display()
            );
        }
        util::copy(&binary, &app_dir.join(destination))?;
    }

    stage_desktop_assets(context, &identity, &app_dir)?;
    util::copy(
        &context.workspace.join("assets/licenses.md"),
        &app_dir.join("licenses.md"),
    )?;

    Ok(StagedBundle {
        temporary,
        app_name,
    })
}

fn stage_desktop_assets(context: &Context, identity: &AppIdentity, app_dir: &Path) -> Result<()> {
    let resources = context.workspace.join("crates/zzz/resources");
    util::copy(
        &resources.join(format!("{}.png", identity.icon)),
        &app_dir.join("share/icons/hicolor/512x512/apps/zzz.png"),
    )?;
    util::copy(
        &resources.join(format!("{}@2x.png", identity.icon)),
        &app_dir.join("share/icons/hicolor/1024x1024/apps/zzz.png"),
    )?;

    let template_path = context.workspace.join(DESKTOP_TEMPLATE);
    let template = fs::read_to_string(&template_path)
        .with_context(|| format!("failed to read {}", template_path.display()))?;
    let mut variables = BTreeMap::new();
    variables.insert("APP_NAME", identity.name.to_owned());
    variables.insert("APP_CLI", "zzz".to_owned());
    variables.insert("APP_ICON", "zzz".to_owned());
    variables.insert("APP_ARGS", "%U".to_owned());
    variables.insert("DO_STARTUP_NOTIFY", "true".to_owned());

    let desktop = app_dir
        .join("share/applications")
        .join(format!("{}.desktop", identity.id));
    util::create_dir(desktop.parent().unwrap_or(app_dir))?;
    fs::write(&desktop, util::envsubst(&template, &variables))
        .with_context(|| format!("failed to write {}", desktop.display()))?;
    util::make_executable(&desktop)?;
    Ok(())
}

fn archive_path(context: &Context) -> PathBuf {
    context
        .bundle_dir()
        .join(format!("zzz-freebsd-{}.tar.gz", context.arch()))
}

fn write_archive(context: &Context, staged: &StagedBundle) -> Result<PathBuf> {
    util::create_dir(&context.bundle_dir())?;
    let archive = archive_path(context);
    util::remove(&archive)?;
    util::run(
        Command::new("tar")
            .arg("--create")
            .arg("--gzip")
            .arg("--file")
            .arg(&archive)
            .arg("--directory")
            .arg(staged.temporary.path())
            .arg(&staged.app_name),
        "tar",
    )?;
    Ok(archive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::build::model::Profile;

    #[test]
    fn archive_names_include_the_architecture() {
        assert!(
            archive_path(&test_context(
                crate::tasks::build::model::Target::FreeBSDX86_64
            ))
            .to_string_lossy()
            .ends_with("zzz-freebsd-x86_64.tar.gz")
        );
        assert!(
            archive_path(&test_context(
                crate::tasks::build::model::Target::FreeBSDAarch64
            ))
            .to_string_lossy()
            .ends_with("zzz-freebsd-aarch64.tar.gz")
        );
    }

    fn test_context(target: crate::tasks::build::model::Target) -> Context {
        Context {
            workspace: PathBuf::from("/workspace"),
            target,
            profile: Profile::Release,
            channel: crate::tasks::build::model::Channel::Stable,
            formats: [Format::TarGz].into_iter().collect(),
            target_dir: PathBuf::from("/workspace/target"),
            skip_build: false,
            jobs: 1,
            offline: false,
            install: false,
            open: false,
            robustness: false,
            sign: false,
            release_version: "0.0.0".to_owned(),
        }
    }
}
