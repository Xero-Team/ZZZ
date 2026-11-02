//! Linux packaging: the tarball, Debian, and RPM artifacts.
//!
//! One implementation covers the tarball, the Debian and RPM packages, and the
//! runtime-dependency check. The tarball is the primary artifact: it carries
//! the editor, the CLI, every shared library the editor links, the FFmpeg
//! license, the desktop entry, the icons, and the generated license document.
//! The Debian and RPM packages are staged from the same directory, so a
//! distribution package cannot drift from the tarball next to it.
//!
//! AppImage, Flatpak, and Snap are host-only post-tasks: they start from the
//! tarball and are dispatched at the end of [`bundle`].

#![allow(
    clippy::disallowed_methods,
    reason = "packaging drives cargo, tar, ldd, and the strip tools"
)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};

use crate::tasks::build::model::{Format, Platform, Profile};
use crate::tasks::bundle::{
    AppIdentity, Context, app_identity, ffmpeg, licenses, remote_server, runtime_deps, util,
};

/// `cargo-deb` release the packaging metadata is written against.
pub const CARGO_DEB_VERSION: &str = "3.8.0";
/// `cargo-generate-rpm` release the packaging metadata is written against. The
/// toolchain image installs 0.21.1, which produces identical packages, so the
/// upstream 0.21.0 pin accepts either unless the version is overridden.
pub const CARGO_GENERATE_RPM_VERSION: &str = "0.21.0";
pub const CARGO_GENERATE_RPM_ALTERNATE_VERSION: &str = "0.21.1";

const GRAMMAR_MANIFEST: &str = "crates/grammars/vendor/manifest.toml";
const DESKTOP_TEMPLATE: &str = "crates/zzz/resources/zzz.desktop.in";

pub fn bundle(context: &Context) -> Result<()> {
    ensure_native_linux(context)?;

    let host_only = crate::tasks::bundle::host_only_formats(&context.formats);
    let mut container = crate::tasks::bundle::container_formats(&context.formats);

    if context.skip_build {
        // A host-only format starts from the tarball the container task wrote,
        // so `--skip-build` only has to reproduce the distribution packages.
        if container
            .iter()
            .any(|format| matches!(format, Format::Deb | Format::Rpm))
        {
            package_existing(context)?;
        } else if host_only.is_empty() {
            bail!(
                "--skip-build needs an existing tarball to package; request `--formats deb` \
                 and/or `rpm`, or run a full bundle first"
            );
        }
    } else {
        if !host_only.is_empty() && !container.contains(&Format::TarGz) {
            container.push(Format::TarGz);
        }
        if !container.is_empty() {
            build_and_bundle(context, &container)?;
        }
    }

    for format in host_only {
        match format {
            Format::AppImage => crate::tasks::bundle::appimage::bundle(context)?,
            Format::Flatpak => crate::tasks::bundle::flatpak::bundle(context)?,
            Format::Snap => crate::tasks::bundle::snap::bundle(context)?,
            _ => unreachable!("host_only_formats only returns host-only formats"),
        }
    }

    Ok(())
}

fn ensure_native_linux(context: &Context) -> Result<()> {
    if Platform::host() != Platform::Linux {
        bail!("Linux bundles can only be created on Linux");
    }
    if context.target.is_musl() {
        bail!(
            "the musl targets exist so the editor can embed a static remote server; they are \
             not distributed bundles"
        );
    }
    if context.target.is_cross() {
        bail!(
            "cross-compiling a Linux bundle is not supported; run the bundle for {} on a \
             matching host",
            context.target.triple()
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Full build
// ---------------------------------------------------------------------------

fn build_and_bundle(context: &Context, formats: &[Format]) -> Result<()> {
    let package_deb = formats.contains(&Format::Deb);
    let package_rpm = formats.contains(&Format::Rpm);

    if package_deb {
        ensure_cargo_deb()?;
    }
    if package_rpm {
        ensure_cargo_generate_rpm()?;
    }
    grammar_gate(context)?;

    let optimization = Optimization::from_environment()?;
    let sentry = sentry_enabled();

    let ffmpeg_dir = ffmpeg::ensure(
        &context.workspace,
        context.target.triple(),
        false,
        false,
        context.offline,
    )?;
    ffmpeg::require_complete(&ffmpeg_dir)?;

    // Generate the licenses first: they are compiled into the binaries through
    // `assets/licenses.md` and copied into the archive.
    licenses::generate(&context.workspace, None, context.offline)?;

    build_host_binaries(context, &ffmpeg_dir, &optimization, sentry)?;
    apply_bolt(context, &optimization)?;
    upload_symbols(context)?;
    strip_binaries(&context.artifact_dir())?;

    let staged = stage(context, &ffmpeg_dir)?;
    let archive = write_archive(context, &staged)?;
    eprintln!("bundle: {}", archive.display());

    if package_deb || package_rpm {
        if !context.workspace.join("assets/licenses.md").is_file() {
            bail!("assets/licenses.md is missing; run a full bundle before packaging");
        }
        stage_package_assets(
            context,
            &staged.desktop,
            &staged.icon_512,
            &staged.icon_1024,
            &staged.ffmpeg_license,
        )?;
        run_packagers(context, package_deb, package_rpm)?;
    }

    Ok(())
}

fn build_host_binaries(
    context: &Context,
    ffmpeg_dir: &Path,
    optimization: &Optimization,
    sentry: bool,
) -> Result<()> {
    let mut rustflags = std::env::var("RUSTFLAGS").unwrap_or_default();
    rustflags.push_str(&link_flags(optimization));
    if context.profile == Profile::LinuxNative {
        rustflags.push_str(" -C target-cpu=native");
    }
    if let Some(directory) = &optimization.pgo_generate {
        util::create_dir(directory)?;
        remove_profraw(directory)?;
        rustflags.push_str(&format!(" -Cprofile-generate={}", directory.display()));
    }
    if let Some(profile) = &optimization.pgo_profile {
        rustflags.push_str(&format!(
            " -Cprofile-use={} -Cllvm-args=-pgo-warn-missing-function",
            profile.display()
        ));
    }

    let mut command = Command::new("cargo");
    command.current_dir(&context.workspace).arg("build");
    command.args(context.profile_flag());
    command
        .args(["--target", context.target.triple(), "--target-dir"])
        .arg(&context.target_dir)
        .args(["--package", "zzz", "--package", "cli"])
        .env("RUSTFLAGS", rustflags.trim())
        .env("LIBZ_SYS_STATIC", "1")
        .env("ZZZ_BUNDLE", "true")
        .env("FFMPEG_DIR", ffmpeg_dir)
        .env("RELEASE_VERSION", &context.release_version)
        .env("CARGO_BUILD_JOBS", context.jobs.to_string())
        .env_remove("CARGO_ENCODED_RUSTFLAGS");
    // Point the embed build script at the cached matrix so it copies archives
    // instead of compiling a second copy of every remote server.
    if let Some(directory) = remote_server::ensure(context)? {
        command.env("ZZZ_EMBED_REMOTE_SERVER_DIR", directory);
    }

    if std::env::var_os("CC").is_none()
        && let Some(clang) = util::which("clang")
    {
        command.env("CC", clang);
    }
    if context.offline {
        command.env("CARGO_NET_OFFLINE", "true");
    }
    if optimization.needs_symbols(sentry) {
        command.env(strip_variable(context.profile), "none");
    }

    util::run(&mut command, "cargo build")
}

/// `--disable-new-dtags` keeps `DT_RPATH` rather than `DT_RUNPATH`. The bundled
/// libraries have no search path of their own, and only `DT_RPATH` is inherited
/// by their dependencies, so `RUNPATH` would leave e.g. `libavcodec.so` unable
/// to find the bundled `libavutil.so`. `runtime_deps::check` enforces this.
fn link_flags(optimization: &Optimization) -> String {
    let mut flags = if std::env::consts::ARCH == "aarch64" {
        " -C link-arg=-fuse-ld=lld -C link-args=-Wl,--disable-new-dtags,-rpath,$ORIGIN/../lib"
            .to_owned()
    } else {
        " -C link-args=-Wl,--disable-new-dtags,-rpath,$ORIGIN/../lib".to_owned()
    };
    if optimization.bolt_enabled() {
        flags.push_str(",--emit-relocs,-znow");
    }
    flags
}

fn strip_variable(profile: Profile) -> String {
    format!(
        "CARGO_PROFILE_{}_STRIP",
        profile.key().to_ascii_uppercase().replace('-', "_")
    )
}

fn remove_profraw(directory: &Path) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "profraw")
        {
            util::remove(&path)?;
        }
    }
    Ok(())
}

struct Optimization {
    pgo_generate: Option<PathBuf>,
    pgo_profile: Option<PathBuf>,
    bolt_prepare: bool,
    bolt_instrument: Option<PathBuf>,
    bolt_perf: Option<PathBuf>,
    bolt_fdata: Option<PathBuf>,
}

impl Optimization {
    fn from_environment() -> Result<Self> {
        let pgo_generate = path_variable("ZZZ_PGO_GENERATE_DIR")?;
        let pgo_profile = path_variable("ZZZ_PGO_PROFILE")?;
        if pgo_generate.is_some() && pgo_profile.is_some() {
            bail!("ZZZ_PGO_GENERATE_DIR and ZZZ_PGO_PROFILE are mutually exclusive");
        }
        if pgo_generate.as_deref() == Some(Path::new("/")) {
            bail!("ZZZ_PGO_GENERATE_DIR cannot be /");
        }
        if let Some(profile) = &pgo_profile
            && !profile.is_file()
        {
            bail!("ZZZ_PGO_PROFILE does not exist: {}", profile.display());
        }

        let bolt_instrument = path_variable("ZZZ_BOLT_INSTRUMENT_FILE")?;
        let bolt_perf = path_variable("ZZZ_BOLT_PERF")?;
        let bolt_fdata = path_variable("ZZZ_BOLT_FDATA")?;
        let modes = [&bolt_instrument, &bolt_perf, &bolt_fdata]
            .into_iter()
            .filter(|mode| mode.is_some())
            .count();
        if modes > 1 {
            bail!("choose only one of ZZZ_BOLT_INSTRUMENT_FILE, ZZZ_BOLT_PERF, or ZZZ_BOLT_FDATA");
        }
        if let Some(perf) = &bolt_perf
            && !perf.is_file()
        {
            bail!("ZZZ_BOLT_PERF does not exist: {}", perf.display());
        }
        if let Some(fdata) = &bolt_fdata
            && !fdata.is_file()
        {
            bail!("ZZZ_BOLT_FDATA does not exist: {}", fdata.display());
        }

        let optimization = Self {
            pgo_generate,
            pgo_profile,
            bolt_prepare: std::env::var_os("ZZZ_BOLT_PREPARE")
                .is_some_and(|value| !value.is_empty()),
            bolt_instrument,
            bolt_perf,
            bolt_fdata,
        };

        if optimization.bolt_enabled() && !util::command_exists("llvm-bolt") {
            bail!("llvm-bolt is required for BOLT workflows");
        }
        if optimization.bolt_perf.is_some() && !util::command_exists("perf2bolt") {
            bail!("perf2bolt is required when ZZZ_BOLT_PERF is set");
        }

        Ok(optimization)
    }

    fn bolt_enabled(&self) -> bool {
        self.bolt_prepare
            || self.bolt_instrument.is_some()
            || self.bolt_perf.is_some()
            || self.bolt_fdata.is_some()
    }

    /// `[profile.release] strip = true` keeps ordinary builds small, but BOLT
    /// needs the symbol table and sentry-cli cannot symbolicate a stripped
    /// binary. Keep symbols for the host build only; the artifacts are stripped
    /// again before they are staged.
    fn needs_symbols(&self, sentry: bool) -> bool {
        self.pgo_generate.is_some() || self.pgo_profile.is_some() || self.bolt_enabled() || sentry
    }
}

fn apply_bolt(context: &Context, optimization: &Optimization) -> Result<()> {
    let binary = context.artifact_dir().join("zzz");
    if let Some(file) = &optimization.bolt_instrument {
        if let Some(parent) = file.parent() {
            util::create_dir(parent)?;
        }
        let instrumented = binary_file_with_suffix(&binary, ".bolt-instrumented");
        util::run(
            Command::new("llvm-bolt")
                .arg(&binary)
                .arg("-o")
                .arg(&instrumented)
                .arg("--instrument")
                .arg(format!("--instrumentation-file={}", file.display()))
                .arg("--instrumentation-file-append-pid")
                .arg("--update-debug-sections")
                .arg("--use-gnu-stack"),
            "llvm-bolt --instrument",
        )?;
        fs::rename(&instrumented, &binary)?;
    } else if optimization.bolt_perf.is_some() || optimization.bolt_fdata.is_some() {
        let profile = match (&optimization.bolt_perf, &optimization.bolt_fdata) {
            (_, Some(fdata)) => fdata.clone(),
            (Some(perf), None) => {
                let output = context.artifact_dir().join("zzz.perf.fdata");
                util::run(
                    Command::new("perf2bolt")
                        .arg(&binary)
                        .arg(format!("--perfdata={}", perf.display()))
                        .arg("-o")
                        .arg(&output),
                    "perf2bolt",
                )?;
                output
            }
            (None, None) => unreachable!("guarded above"),
        };
        let optimized = binary_file_with_suffix(&binary, ".bolt");
        util::run(
            Command::new("llvm-bolt")
                .arg(&binary)
                .arg("-o")
                .arg(&optimized)
                .arg(format!("--data={}", profile.display()))
                .arg("--reorder-blocks=ext-tsp")
                .arg("--reorder-functions=hfsort+")
                .arg("--split-functions")
                .arg("--split-all-cold")
                .arg("--icf=safe")
                .arg("--plt=hot")
                .arg("--dyno-stats")
                .arg("--update-debug-sections")
                .arg("--use-gnu-stack"),
            "llvm-bolt",
        )?;
        fs::rename(&optimized, &binary)?;
    }
    Ok(())
}

fn binary_file_with_suffix(binary: &Path, suffix: &str) -> PathBuf {
    let mut name = binary.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Uploads the unstripped host binary so stack traces can be symbolized.
///
/// Explicit opt-in only: an upload failure must never fail a release, and the
/// matrix builds the remote server, so its debug info is not available here.
fn upload_symbols(context: &Context) -> Result<()> {
    if !sentry_enabled() {
        eprintln!("ZZZ_ENABLE_SENTRY_UPLOAD is not set to 1; skipped sentry upload.");
        return Ok(());
    }
    if std::env::var_os("SENTRY_AUTH_TOKEN").is_none() {
        eprintln!("SENTRY_AUTH_TOKEN is missing; skipped sentry upload.");
        return Ok(());
    }
    if !util::command_exists("sentry-cli") {
        eprintln!("sentry-cli not found; skipping sentry upload.");
        return Ok(());
    }

    let binary = context.artifact_dir().join("zzz");
    for attempt in 1..=3 {
        eprintln!("uploading ZZZ debug symbols to Sentry (attempt {attempt}/3)");
        let status = Command::new("sentry-cli")
            .args([
                "debug-files",
                "upload",
                "--include-sources",
                "--wait",
                "-p",
                "zed",
                "-o",
                "zed-dev",
            ])
            .arg(&binary)
            .status();
        if status.is_ok_and(|status| status.success()) {
            eprintln!("sentry upload succeeded");
            return Ok(());
        }
        if attempt == 3 {
            eprintln!("warning: every sentry upload attempt failed");
        }
    }
    Ok(())
}

fn strip_binaries(artifact_dir: &Path) -> Result<()> {
    let tool = strip_tool()?;
    for binary in ["zzz", "cli"] {
        util::run(
            Command::new(&tool)
                .arg("--strip-all")
                .arg(artifact_dir.join(binary)),
            "strip",
        )?;
    }
    Ok(())
}

/// GNU objcopy on older distributions cannot read CREL sections produced by
/// newer LLVM, so prefer LLVM's tool and fall back to `rust-objcopy`.
fn strip_tool() -> Result<OsString> {
    if let Some(path) = util::which("llvm-objcopy") {
        return Ok(path.into_os_string());
    }
    if let Some(path) = util::which("rust-objcopy") {
        return Ok(path.into_os_string());
    }
    if util::which("rustc").is_some() {
        let sysroot = util::stdout(
            Command::new("rustc").args(["--print", "sysroot"]),
            "rustc --print sysroot",
        )?;
        let host = util::rust_host_triple()?;
        let candidate = PathBuf::from(sysroot)
            .join("lib/rustlib")
            .join(host)
            .join("bin/rust-objcopy");
        if candidate.is_file() {
            return Ok(candidate.into_os_string());
        }
    }
    bail!("llvm-objcopy or rust-objcopy is required to strip the Linux binaries")
}

// ---------------------------------------------------------------------------
// Staging the `.app` directory
// ---------------------------------------------------------------------------

struct StagedBundle {
    temporary: tempfile::TempDir,
    app_name: String,
    desktop: PathBuf,
    icon_512: PathBuf,
    icon_1024: PathBuf,
    ffmpeg_license: PathBuf,
}

fn stage(context: &Context, ffmpeg_dir: &Path) -> Result<StagedBundle> {
    let identity = app_identity(context.channel);
    let temporary = tempfile::Builder::new()
        .prefix("zzz-linux-bundle-")
        .tempdir()
        .context("failed to create a staging directory")?;
    let app_name = format!("zzz{}.app", identity.suffix);
    let app_dir = temporary.path().join(&app_name);

    let artifact_dir = context.artifact_dir();
    util::copy(
        &artifact_dir.join("zzz"),
        &app_dir.join("libexec/zzz-editor"),
    )?;
    util::copy(&artifact_dir.join("cli"), &app_dir.join("bin/zzz"))?;

    // Everything the editor links has to come from the bundle except glibc and
    // the compiler runtime: a missing or mismatched host copy of any other
    // library makes the dynamic loader refuse to start the binary at all.
    let libraries = direct_libraries(&artifact_dir.join("zzz"), &ffmpeg_dir.join("lib"))?;
    if libraries.is_empty() {
        bail!(
            "no runtime libraries were found for {}",
            artifact_dir.join("zzz").display()
        );
    }
    let library_dir = app_dir.join("lib");
    util::create_dir(&library_dir)?;
    for library in &libraries {
        copy_library(library, &library_dir)?;
    }
    bundle_transitive_libraries(&library_dir)?;

    let ffmpeg_license = app_dir.join("FFmpeg-LICENSE.txt");
    util::copy(&ffmpeg_dir.join("LICENSE.txt"), &ffmpeg_license)?;

    let (desktop, icon_512, icon_1024) = stage_desktop_assets(context, &identity, &app_dir)?;
    util::copy(
        &context.workspace.join("assets/licenses.md"),
        &app_dir.join("licenses.md"),
    )?;

    // Refuse to ship a bundle that would fall back to the host for a library.
    runtime_deps::check(&app_dir, context.robustness)?;

    Ok(StagedBundle {
        temporary,
        app_name,
        desktop,
        icon_512,
        icon_1024,
        ffmpeg_license,
    })
}

fn direct_libraries(binary: &Path, ffmpeg_lib: &Path) -> Result<Vec<PathBuf>> {
    let mut libraries = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (_, path) in resolved_libraries(binary, Some(ffmpeg_lib))? {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_direct_excluded(&name) {
            continue;
        }
        if seen.insert(name) {
            libraries.push(path);
        }
    }
    Ok(libraries)
}

fn copy_library(library: &Path, library_dir: &Path) -> Result<()> {
    let name = library.file_name().unwrap_or(library.as_os_str());
    util::copy(library, &library_dir.join(name))
}

/// Never bundle the GLib family or its private dependencies: host libraries
/// that get `dlopen`ed are built against the system GLib, and resolving them
/// against an older bundled copy fails with missing symbols. `libz` is excluded
/// here because the editor links its own vendored zlib, but the transitive pass
/// keeps it for libraries that expect the shared ABI.
fn is_direct_excluded(name: &str) -> bool {
    runtime_deps::is_glibc_library(name)
        || runtime_deps::is_host_provided_family(name)
        || name.starts_with("libz.so")
}

fn bundle_transitive_libraries(library_dir: &Path) -> Result<()> {
    let mut queue: Vec<PathBuf> = fs::read_dir(library_dir)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().contains(".so"))
        })
        .collect();
    queue.sort();

    let mut index = 0;
    while index < queue.len() {
        let library = queue[index].clone();
        index += 1;
        for (_, dependency) in resolved_libraries_lenient(&library) {
            let name = dependency
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            if runtime_deps::is_glibc_library(&name) || runtime_deps::is_host_provided_family(&name)
            {
                continue;
            }
            let destination = library_dir.join(&name);
            if destination.exists() {
                continue;
            }
            util::copy(&dependency, &destination)?;
            queue.push(destination);
        }
    }
    Ok(())
}

fn resolved_libraries(
    binary: &Path,
    library_path: Option<&Path>,
) -> Result<Vec<(String, PathBuf)>> {
    let mut command = Command::new("ldd");
    command.arg(binary);
    if let Some(path) = library_path {
        let mut search = path.as_os_str().to_owned();
        if let Some(existing) = std::env::var_os("LD_LIBRARY_PATH") {
            search.push(":");
            search.push(existing);
        }
        command.env("LD_LIBRARY_PATH", search);
    }
    let output = util::stdout(&mut command, "ldd")?;
    Ok(parse_ldd(&output))
}

/// Bundled libraries may need a library the host does not have, in which case
/// `ldd` exits non-zero but still prints everything it resolved.
fn resolved_libraries_lenient(binary: &Path) -> Vec<(String, PathBuf)> {
    match Command::new("ldd").arg(binary).output() {
        Ok(output) => parse_ldd(&String::from_utf8_lossy(&output.stdout)),
        Err(_) => Vec::new(),
    }
}

fn parse_ldd(text: &str) -> Vec<(String, PathBuf)> {
    let mut libraries = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some((left, right)) = line.split_once("=> ") else {
            continue;
        };
        let path = match right.find(" (0x") {
            Some(index) => &right[..index],
            None => right,
        };
        let path = path.trim();
        if path.is_empty() || !path.starts_with('/') {
            continue;
        }
        libraries.push((left.trim().to_owned(), PathBuf::from(path)));
    }
    libraries
}

fn stage_desktop_assets(
    context: &Context,
    identity: &AppIdentity,
    app_dir: &Path,
) -> Result<(PathBuf, PathBuf, PathBuf)> {
    let resources = context.workspace.join("crates/zzz/resources");
    let icon_512 = app_dir.join("share/icons/hicolor/512x512/apps/zzz.png");
    let icon_1024 = app_dir.join("share/icons/hicolor/1024x1024/apps/zzz.png");
    util::copy(&resources.join(format!("{}.png", identity.icon)), &icon_512)?;
    util::copy(
        &resources.join(format!("{}@2x.png", identity.icon)),
        &icon_1024,
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

    Ok((desktop, icon_512, icon_1024))
}

// ---------------------------------------------------------------------------
// Archive and distribution packages
// ---------------------------------------------------------------------------

fn archive_path(context: &Context) -> PathBuf {
    context
        .bundle_dir()
        .join(format!("zzz-linux-{}.tar.gz", context.arch()))
}

fn write_archive(context: &Context, staged: &StagedBundle) -> Result<PathBuf> {
    util::create_dir(&context.bundle_dir())?;
    remove_old_archives(context)?;
    let archive = archive_path(context);
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

fn remove_old_archives(context: &Context) -> Result<()> {
    let directory = context.bundle_dir();
    let suffix = format!("-linux-{}.tar.gz", context.arch());
    let Ok(entries) = fs::read_dir(&directory) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("zzz") && name.ends_with(&suffix) {
            util::remove(&entry.path())?;
        }
    }
    Ok(())
}

fn stage_package_assets(
    context: &Context,
    desktop: &Path,
    icon_512: &Path,
    icon_1024: &Path,
    ffmpeg_license: &Path,
) -> Result<PathBuf> {
    // cargo-deb and cargo-generate-rpm treat `target/release` as a logical path
    // and rewrite it for custom profiles and target directories. Keep the
    // metadata stable by staging channel-specific generated assets here.
    let directory = context.artifact_dir().join("package-assets");
    util::remove(&directory)?;
    util::create_dir(&directory)?;
    util::copy(desktop, &directory.join("zzz.desktop"))?;
    util::copy(icon_512, &directory.join("zzz.png"))?;
    util::copy(icon_1024, &directory.join("zzz@2x.png"))?;
    util::copy(
        &context.workspace.join("assets/licenses.md"),
        &directory.join("licenses.md"),
    )?;
    util::copy(ffmpeg_license, &directory.join("FFmpeg-LICENSE.txt"))?;
    Ok(directory)
}

fn run_packagers(context: &Context, package_deb: bool, package_rpm: bool) -> Result<()> {
    let output_directory = context.bundle_dir();
    util::create_dir(&output_directory)?;
    let variant = context.channel.as_str();
    let arch = context.arch();

    if package_deb {
        let mut command = Command::new("cargo");
        command
            .current_dir(&context.workspace)
            .args([
                "deb",
                "--locked",
                "--no-build",
                "--package",
                "zzz",
                "--profile",
            ])
            .arg(context.profile.key())
            .args([
                "--target",
                context.target.triple(),
                "--variant",
                variant,
                "--output",
            ])
            .arg(output_directory.join(format!("zzz-{variant}-linux-{arch}.deb")))
            .env("CARGO_TARGET_DIR", &context.target_dir);
        if context.offline {
            command.env("CARGO_NET_OFFLINE", "true");
        }
        util::run(&mut command, "cargo deb")?;
    }

    if package_rpm {
        let mut command = Command::new("cargo");
        command
            .current_dir(&context.workspace)
            .args(["generate-rpm", "--package", "crates/zzz", "--profile"])
            .arg(context.profile.key())
            .args(["--target", context.target.triple(), "--target-dir"])
            .arg(&context.target_dir)
            .args(["--variant", variant, "--output"])
            .arg(output_directory.join(format!("zzz-{variant}-linux-{arch}.rpm")))
            .env("CARGO_TARGET_DIR", &context.target_dir);
        if context.offline {
            command.env("CARGO_NET_OFFLINE", "true");
        }
        util::run(&mut command, "cargo generate-rpm")?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// `--skip-build`: distribution packages from an existing tarball
// ---------------------------------------------------------------------------

fn package_existing(context: &Context) -> Result<()> {
    let package_deb = context.formats.contains(&Format::Deb);
    let package_rpm = context.formats.contains(&Format::Rpm);
    if package_deb {
        ensure_cargo_deb()?;
    }
    if package_rpm {
        ensure_cargo_generate_rpm()?;
    }

    if !context.workspace.join("assets/licenses.md").is_file() {
        bail!("assets/licenses.md is missing; run a full bundle before packaging");
    }

    let archive = find_archive(context)?;
    let temporary = tempfile::Builder::new()
        .prefix("zzz-linux-package-")
        .tempdir()
        .context("failed to create a staging directory")?;
    util::extract(&archive, temporary.path())?;
    let app_dir = single_app_directory(temporary.path())?;

    let desktop = single_desktop_file(&app_dir)?;
    let icon_512 = app_dir.join("share/icons/hicolor/512x512/apps/zzz.png");
    let icon_1024 = app_dir.join("share/icons/hicolor/1024x1024/apps/zzz.png");
    let ffmpeg_license = app_dir.join("FFmpeg-LICENSE.txt");
    for required in [&icon_512, &icon_1024, &ffmpeg_license] {
        if !required.is_file() {
            bail!(
                "{} is missing package assets; {} does not exist",
                archive.display(),
                required.display()
            );
        }
    }

    stage_package_assets(context, &desktop, &icon_512, &icon_1024, &ffmpeg_license)?;
    run_packagers(context, package_deb, package_rpm)?;
    eprintln!("packages: {}", context.bundle_dir().display());
    Ok(())
}

/// Locates the tarball that AppImage, Flatpak, and Snap wrap.
pub(crate) fn find_archive(context: &Context) -> Result<PathBuf> {
    let directory = context.bundle_dir();
    let exact = directory.join(format!("zzz-linux-{}.tar.gz", context.arch()));
    if exact.is_file() {
        return Ok(exact);
    }

    let suffix = format!("-linux-{}.tar.gz", context.arch());
    let mut candidates = Vec::new();
    if let Ok(entries) = fs::read_dir(&directory) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("zzz") && name.ends_with(&suffix) && entry.path().is_file() {
                candidates.push(entry.path());
            }
        }
    }
    candidates.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    candidates.pop().ok_or_else(|| {
        anyhow::anyhow!(
            "no Linux tarball in {}; run `cargo xtask bundle --target {}` first",
            directory.display(),
            context.target.triple()
        )
    })
}

/// Returns the one `.app` directory inside an extracted tarball.
pub(crate) fn single_app_directory(directory: &Path) -> Result<PathBuf> {
    let mut applications = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() && path.extension().is_some_and(|extension| extension == "app") {
            applications.push(path);
        }
    }
    match applications.len() {
        1 => Ok(applications.remove(0)),
        count => bail!(
            "expected one .app directory in {}, found {count}",
            directory.display()
        ),
    }
}

fn single_desktop_file(app_dir: &Path) -> Result<PathBuf> {
    let directory = app_dir.join("share/applications");
    let mut files = Vec::new();
    for entry in fs::read_dir(&directory)? {
        let path = entry?.path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "desktop")
        {
            files.push(path);
        }
    }
    match files.len() {
        1 => Ok(files.remove(0)),
        count => bail!(
            "expected one .desktop file in {}, found {count}",
            directory.display()
        ),
    }
}

// ---------------------------------------------------------------------------
// Tool gating
// ---------------------------------------------------------------------------

fn ensure_cargo_deb() -> Result<()> {
    let expected =
        std::env::var("ZZZ_CARGO_DEB_VERSION").unwrap_or_else(|_| CARGO_DEB_VERSION.to_owned());
    let actual = tool_version("cargo-deb").ok_or_else(|| {
        anyhow::anyhow!(
            "cargo-deb {expected} is required for Debian packages; install it with \
             `cargo install cargo-deb --version {expected} --locked`"
        )
    })?;
    if actual != expected {
        bail!("cargo-deb {expected} is required; found {actual}");
    }
    Ok(())
}

fn ensure_cargo_generate_rpm() -> Result<()> {
    let expected = std::env::var("ZZZ_CARGO_GENERATE_RPM_VERSION").ok();
    let actual = tool_version("cargo-generate-rpm").ok_or_else(|| {
        anyhow::anyhow!(
            "cargo-generate-rpm {CARGO_GENERATE_RPM_VERSION} is required for RPM packages; \
             install it with `cargo install cargo-generate-rpm --version \
             {CARGO_GENERATE_RPM_VERSION} --locked`"
        )
    })?;
    let accepted = match &expected {
        Some(expected) => actual == *expected,
        None => {
            actual == CARGO_GENERATE_RPM_VERSION || actual == CARGO_GENERATE_RPM_ALTERNATE_VERSION
        }
    };
    if !accepted {
        let expected = expected.as_deref().unwrap_or(CARGO_GENERATE_RPM_VERSION);
        bail!("cargo-generate-rpm {expected} is required; found {actual}");
    }
    Ok(())
}

fn tool_version(tool: &str) -> Option<String> {
    let description = format!("{tool} --version");
    let output = util::stdout(Command::new(tool).arg("--version"), &description).ok()?;
    output.split_whitespace().last().map(str::to_owned)
}

fn grammar_gate(context: &Context) -> Result<()> {
    if std::env::var("ZZZ_SKIP_GRAMMAR_CHECK").is_ok_and(|value| value == "1" || value == "true") {
        return Ok(());
    }

    let expected = expected_tree_sitter_version(&context.workspace)?;
    let actual = tool_version("tree-sitter");
    if actual.as_deref() != Some(expected.as_str()) {
        bail!(
            "tree-sitter {expected} is required; found {}. Install it with \
             `cargo install --locked --version {expected} tree-sitter-cli`",
            actual.as_deref().unwrap_or("nothing")
        );
    }

    let mut command = Command::new("cargo");
    command
        .current_dir(&context.workspace)
        .args(["xtask", "grammars", "check"]);
    if context.offline {
        command.env("CARGO_NET_OFFLINE", "true");
    }
    util::run(&mut command, "cargo xtask grammars check")
}

fn expected_tree_sitter_version(workspace: &Path) -> Result<String> {
    let path = workspace.join(GRAMMAR_MANIFEST);
    let source =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let version = source
        .lines()
        .find_map(|line| line.strip_prefix("tree-sitter-cli-version = "))
        .map(str::trim)
        .and_then(|value| value.strip_prefix('"'))
        .and_then(|value| value.strip_suffix('"'))
        .filter(|value| !value.is_empty());
    version.map(str::to_owned).ok_or_else(|| {
        anyhow::anyhow!(
            "{} does not declare a tree-sitter-cli-version",
            path.display()
        )
    })
}

fn path_variable(name: &str) -> Result<Option<PathBuf>> {
    match std::env::var_os(name) {
        None => Ok(None),
        Some(value) if value.is_empty() => Ok(None),
        Some(value) => {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                bail!("{name} must be an absolute path");
            }
            Ok(Some(path))
        }
    }
}

fn sentry_enabled() -> bool {
    std::env::var("ZZZ_ENABLE_SENTRY_UPLOAD").is_ok_and(|value| value == "1")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ldd_output_only_keeps_resolved_paths() {
        let output = "\tlinux-vdso.so.1 (0x00007ffe)\n\
                      \tlibavcodec.so.63 => /opt/ffmpeg/lib/libavcodec.so.63 (0x00007f00)\n\
                      \tlibmissing.so.1 => not found\n\
                      \t/lib64/ld-linux-x86-64.so.2 (0x00007f01)\n";
        let resolved = parse_ldd(output);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].0, "libavcodec.so.63");
        assert_eq!(
            resolved[0].1,
            PathBuf::from("/opt/ffmpeg/lib/libavcodec.so.63")
        );
    }

    #[test]
    fn direct_pass_drops_glibc_and_glib_but_keeps_zlib_for_transitives() {
        assert!(is_direct_excluded("libc.so.6"));
        assert!(is_direct_excluded("libglib-2.0.so.0"));
        assert!(is_direct_excluded("libz.so.1"));
        assert!(!is_direct_excluded("libssl.so.3"));
        assert!(!runtime_deps::is_host_provided_family("libz.so.1"));
        assert!(runtime_deps::is_host_provided_family("libffi.so.8"));
    }

    #[test]
    fn strip_variable_names_the_profile() {
        assert_eq!(
            strip_variable(Profile::Release),
            "CARGO_PROFILE_RELEASE_STRIP"
        );
        assert_eq!(
            strip_variable(Profile::ReleaseFast),
            "CARGO_PROFILE_RELEASE_FAST_STRIP"
        );
        assert_eq!(
            strip_variable(Profile::LinuxNative),
            "CARGO_PROFILE_LINUX_NATIVE_STRIP"
        );
    }

    #[test]
    fn link_flags_disable_new_dtags_and_emit_relocations_for_bolt() {
        let optimization = Optimization {
            pgo_generate: None,
            pgo_profile: None,
            bolt_prepare: false,
            bolt_instrument: None,
            bolt_perf: None,
            bolt_fdata: None,
        };
        let flags = link_flags(&optimization);
        assert!(flags.contains("--disable-new-dtags"));
        assert!(flags.contains("-rpath,$ORIGIN/../lib"));
        assert!(!flags.contains("--emit-relocs"));

        let optimization = Optimization {
            bolt_prepare: true,
            ..optimization
        };
        let flags = link_flags(&optimization);
        assert!(flags.ends_with("$ORIGIN/../lib,--emit-relocs,-znow"));
    }
}
