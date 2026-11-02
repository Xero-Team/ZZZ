//! Windows packaging: the portable binaries, the MSI, and the Inno installer.
//!
//! One implementation covers the Linux cross path and a native Windows host.
//! The difference is only how the binaries are produced: the cross path runs
//! `cargo build` with the msvc-wine environment from [`MsvcEnvironment`], while
//! a native build relies on the caller having entered a Visual Studio developer
//! shell. Everything after that — FFmpeg runtime DLLs, the explorer command
//! injector, the MSI, the Inno staging directory — is shared.
//!
//! Only `exe` is shippable from a non-Windows host. The installers need
//! `dotnet`, `ISCC.exe`, `makeAppx.exe`, and PowerShell, none of which exist in
//! the Linux container, so the planner rejects them before reaching this module.

#![allow(
    clippy::disallowed_methods,
    reason = "packaging drives cargo, dotnet, ISCC, makeAppx, and PowerShell"
)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};

use crate::tasks::build::model::{Channel, Format, Platform};
use crate::tasks::build::msvc::{self, MsvcEnvironment};
use crate::tasks::bundle::{Context, ffmpeg, github_release, licenses, remote_server, util};

/// The AGS SDK is pinned: the shim in `gpui_windows` is compiled against this
/// exact `amd_ags_x64.dll`, so a newer release is not a drop-in replacement.
const AGS_URL: &str =
    "https://codeload.github.com/GPUOpen-LibrariesAndSDKs/AGS_SDK/zip/refs/tags/v6.3.0";
const AGS_SHA256: &str = "292c5ce0c7f51d44a1e56f2a73d82f1e7304bb14d2b8e39a05708877210de884";
const AGS_DIRECTORY: &str = "AGS_SDK-6.3.0";

const CONPTY_REPOSITORY: &str = "microsoft/terminal";
const CONPTY_PATTERN: &str = "Microsoft.Windows.Console.ConPTY.*.nupkg";

const INNO_SETUP: &str = "Inno Setup 6/ISCC.exe";
const EXPLORER_INJECTOR: &str = "explorer_command_injector";

pub fn bundle(context: &Context) -> Result<()> {
    if context.target.platform() != Platform::Windows {
        bail!("the Windows bundler only handles pc-windows-msvc targets");
    }

    let native = Platform::host() == Platform::Windows;
    if !native
        && context
            .formats
            .iter()
            .any(|format| matches!(format, Format::Msi | Format::Inno))
    {
        bail!(
            "the MSI and Inno installers need a Windows host; from Linux build the portable \
             binaries with --formats exe"
        );
    }
    if native && !context.skip_build && !msvc::native_environment_ready() {
        bail!(
            "a native Windows build must run inside a Visual Studio developer shell; start \
             \"x64 Native Tools Command Prompt for VS\" or call Launch-VsDevShell.ps1 first"
        );
    }

    let wants_installer = context
        .formats
        .iter()
        .any(|format| matches!(format, Format::Msi | Format::Inno));
    let needs_injector = wants_installer;

    if context.skip_build {
        require_binaries(context)?;
        if needs_injector {
            require_artifact(context, "explorer_command_injector.dll")?;
        }
    } else {
        grammar_gate(context)?;
        licenses::generate(&context.workspace, None, context.offline)?;
        util::rustup_add_target(context.target.triple())?;
        build_binaries(context, native, needs_injector)?;
    }

    // The FFmpeg runtime DLLs belong to the portable artifact, not just to the
    // installers, so they are staged whether or not the binaries were rebuilt.
    stage_ffmpeg_runtime(context)?;

    if context.formats.contains(&Format::Msi) {
        build_msi(context)?;
    }
    if context.formats.contains(&Format::Inno) {
        build_inno(context)?;
    }

    if !context.skip_build {
        zip_debug_symbols(context)?;
    }
    upload_symbols(context)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Building the binaries
// ---------------------------------------------------------------------------

fn build_binaries(context: &Context, native: bool, needs_injector: bool) -> Result<()> {
    let environment = if native {
        None
    } else {
        // CMake-based dependencies (`libopus` through `rodio`) need a toolchain
        // file to build for Windows from this host.
        Some(
            MsvcEnvironment::detect(context.arch())?
                .with_cmake_toolchain(&context.target_dir.join("msvc"))?,
        )
    };

    let ffmpeg_dir = ffmpeg::ensure(
        &context.workspace,
        context.target.triple(),
        false,
        false,
        context.offline,
    )?;
    ffmpeg::require_complete(&ffmpeg_dir)?;

    let remote_server_dir = remote_server::ensure(context)?;
    let mut command = Command::new("cargo");
    command.current_dir(&context.workspace).arg("build");
    command.args(context.profile_flag());
    command
        .args(["--target", context.target.triple(), "--target-dir"])
        .arg(&context.target_dir)
        .args(["--package", "zzz", "--package", "cli"])
        .env("ZZZ_BUNDLE", "true")
        .env("ZZZ_RELEASE_CHANNEL", context.channel.as_str())
        .env("RELEASE_CHANNEL", context.channel.as_str())
        .env("RELEASE_VERSION", &context.release_version)
        .env("FFMPEG_DIR", &ffmpeg_dir)
        .env("CARGO_BUILD_JOBS", context.jobs.to_string());
    // Point the embed build script at the cached matrix. Without this the
    // script would compile a second copy of every remote server inside this
    // one bundle.
    if let Some(directory) = remote_server_dir {
        command.env("ZZZ_EMBED_REMOTE_SERVER_DIR", directory);
    }

    if let Some(environment) = &environment {
        environment.apply(&mut command);
    }
    if context.offline {
        command.env("CARGO_NET_OFFLINE", "true");
    }

    util::run(
        &mut command,
        &format!("cargo build for {}", context.target.triple()),
    )?;

    if needs_injector {
        build_explorer_injector(context, &environment)?;
    }

    Ok(())
}

fn build_explorer_injector(context: &Context, environment: &Option<MsvcEnvironment>) -> Result<()> {
    let mut command = Command::new("cargo");
    command.current_dir(&context.workspace).arg("build");
    command.args(context.profile_flag());
    command
        .args(["--target", context.target.triple(), "--target-dir"])
        .arg(&context.target_dir)
        .arg("--package")
        .arg(EXPLORER_INJECTOR)
        .env("CARGO_BUILD_JOBS", context.jobs.to_string());

    // Each channel links a different menu command id, so the feature selects
    // which one is compiled in. Nightly and dev share the default feature.
    match context.channel {
        Channel::Stable => {
            command.args(["--features", "stable", "--no-default-features"]);
        }
        Channel::Preview => {
            command.args(["--features", "preview", "--no-default-features"]);
        }
        Channel::Nightly | Channel::Dev => {}
    }

    if let Some(environment) = environment {
        environment.apply(&mut command);
    }
    if context.offline {
        command.env("CARGO_NET_OFFLINE", "true");
    }

    util::run(&mut command, "cargo build (explorer_command_injector)")
}

fn require_binaries(context: &Context) -> Result<()> {
    require_artifact(context, "zzz.exe")?;
    require_artifact(context, "cli.exe")?;
    Ok(())
}

fn require_artifact(context: &Context, name: &str) -> Result<()> {
    let path = context.artifact_dir().join(name);
    if !path.is_file() {
        bail!(
            "--skip-build was requested but {} is missing; run a full bundle first",
            path.display()
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// FFmpeg runtime DLLs
// ---------------------------------------------------------------------------

fn stage_ffmpeg_runtime(context: &Context) -> Result<()> {
    let Some(directory) = resolve_ffmpeg(context)? else {
        eprintln!(
            "warning: no FFmpeg package is installed; the portable artifact will be missing \
             its runtime DLLs"
        );
        return Ok(());
    };

    let artifact_dir = context.artifact_dir();
    util::create_dir(&artifact_dir)?;

    let mut copied = 0usize;
    let bin = directory.join("bin");
    if let Ok(entries) = fs::read_dir(&bin) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if Path::new(name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
                && is_ffmpeg_runtime(name)
            {
                util::copy(&path, &artifact_dir.join(name))?;
                copied += 1;
            }
        }
    }
    if copied == 0 {
        bail!("no FFmpeg runtime DLLs were found under {}", bin.display());
    }

    let license = directory.join("LICENSE.txt");
    if license.is_file() {
        util::copy(&license, &artifact_dir.join("FFmpeg-LICENSE.txt"))?;
    }
    Ok(())
}

fn is_ffmpeg_runtime(name: &str) -> bool {
    ["avcodec", "avformat", "avutil", "swscale", "swresample"]
        .iter()
        .any(|prefix| name.starts_with(&format!("{prefix}-")))
}

/// Finds the FFmpeg package without downloading when `--skip-build` is set.
fn resolve_ffmpeg(context: &Context) -> Result<Option<PathBuf>> {
    if let Some(directory) = std::env::var_os("FFMPEG_DIR") {
        let directory = PathBuf::from(directory);
        if directory.is_dir() {
            return Ok(Some(directory));
        }
    }
    let installed = context
        .workspace
        .join(".deps/ffmpeg")
        .join(context.target.triple());
    if installed.join(ffmpeg::MARKER_NAME).is_file() {
        return Ok(Some(installed));
    }
    if context.skip_build {
        return Ok(None);
    }
    Ok(Some(ffmpeg::ensure(
        &context.workspace,
        context.target.triple(),
        false,
        false,
        context.offline,
    )?))
}

// ---------------------------------------------------------------------------
// MSI
// ---------------------------------------------------------------------------

fn build_msi(context: &Context) -> Result<()> {
    util::require_tool("dotnet", "Install the .NET SDK to build the MSI installer.")?;
    let project = context
        .workspace
        .join("tooling/windows_msi_builder/WindowsMsiBuilder.csproj");
    if !project.is_file() {
        bail!("the MSI builder project is missing: {}", project.display());
    }

    let output = context.workspace.join("target").join(format!(
        "{}-{}-{}.msi",
        windows_base_name(context.channel),
        context.arch(),
        context.release_version
    ));
    if let Some(parent) = output.parent() {
        util::create_dir(parent)?;
    }
    util::remove(&output)?;

    let mut command = Command::new("dotnet");
    command
        .current_dir(&context.workspace)
        .arg("run")
        .arg("--project")
        .arg(&project)
        .arg("--")
        .arg("--arch")
        .arg(context.arch())
        .arg("--workspace-root")
        .arg(&context.workspace)
        .arg("--release-dir")
        .arg(context.artifact_dir())
        .arg("--output")
        .arg(&output)
        .arg("--channel")
        .arg(context.channel.as_str())
        .arg("--version")
        .arg(&context.release_version);

    util::run(&mut command, "dotnet run (Windows MSI builder)")?;

    if !output.is_file() {
        bail!(
            "the MSI builder reported success but {} does not exist",
            output.display()
        );
    }
    eprintln!("MSI: {}", output.display());
    Ok(())
}

/// The executable base name the MSI builder uses, matching `ChannelConfig` in
/// `tooling/windows_msi_builder`.
fn windows_base_name(channel: Channel) -> &'static str {
    match channel {
        Channel::Stable => "ZZZ",
        Channel::Preview => "ZZZPreview",
        Channel::Nightly => "ZZZNightly",
        Channel::Dev => "ZZZDev",
    }
}

// ---------------------------------------------------------------------------
// Inno Setup
// ---------------------------------------------------------------------------

struct InnoIdentity {
    app_id: &'static str,
    icon: &'static str,
    name: &'static str,
    display_name: &'static str,
    mutex: &'static str,
    exe_name: &'static str,
    registry_value: &'static str,
    user_id: &'static str,
    shell_name_short: &'static str,
    appx_full_name: &'static str,
}

fn inno_identity(channel: Channel) -> InnoIdentity {
    match channel {
        Channel::Stable => InnoIdentity {
            app_id: "{{2DB0DA96-CA55-49BB-AF4F-64AF36A86712}",
            icon: "app-icon",
            name: "ZZZ",
            display_name: "ZZZ",
            mutex: "ZZZ-Stable-Instance-Mutex",
            exe_name: "ZZZ",
            registry_value: "ZZZ",
            user_id: "ZZZIndustries.ZZZ",
            shell_name_short: "Z&ZZ",
            appx_full_name: "ZZZIndustries.ZZZ_1.0.0.0_neutral__japxn1gcva8rg",
        },
        Channel::Preview => InnoIdentity {
            app_id: "{{F70E4811-D0E2-4D88-AC99-D63752799F95}",
            icon: "app-icon-preview",
            name: "ZZZ Preview",
            display_name: "ZZZ Preview",
            mutex: "ZZZ-Preview-Instance-Mutex",
            exe_name: "ZZZ",
            registry_value: "ZZZPreview",
            user_id: "ZZZIndustries.ZZZ.Preview",
            shell_name_short: "Z&ZZ Preview",
            appx_full_name: "ZZZIndustries.ZZZ.Preview_1.0.0.0_neutral__japxn1gcva8rg",
        },
        Channel::Nightly => InnoIdentity {
            app_id: "{{1BDB21D3-14E7-433C-843C-9C97382B2FE0}",
            icon: "app-icon-nightly",
            name: "ZZZ Nightly",
            display_name: "ZZZ Nightly",
            mutex: "ZZZ-Nightly-Instance-Mutex",
            exe_name: "ZZZ",
            registry_value: "ZZZNightly",
            user_id: "ZZZIndustries.ZZZ.Nightly",
            shell_name_short: "Z&ZZ Editor Nightly",
            appx_full_name: "ZZZIndustries.ZZZ.Nightly_1.0.0.0_neutral__japxn1gcva8rg",
        },
        Channel::Dev => InnoIdentity {
            app_id: "{{8357632E-24A4-4F32-BA97-E575B4D1FE5D}",
            icon: "app-icon-dev",
            name: "ZZZ Dev",
            display_name: "ZZZ Dev",
            mutex: "ZZZ-Dev-Instance-Mutex",
            exe_name: "ZZZ",
            registry_value: "ZZZDev",
            user_id: "ZZZIndustries.ZZZ.Dev",
            shell_name_short: "Z&ZZ Dev",
            appx_full_name: "ZZZIndustries.ZZZ.Dev_1.0.0.0_neutral__japxn1gcva8rg",
        },
    }
}

fn build_inno(context: &Context) -> Result<()> {
    if Platform::host() != Platform::Windows {
        bail!("the Inno installer can only be built on Windows");
    }

    let identity = inno_identity(context.channel);
    let inno_dir = context.workspace.join("inno").join(context.arch());

    stage_inno(context, &inno_dir)?;
    build_appx(context, &inno_dir)?;
    sign_inno_files(context, &inno_dir)?;
    collect_inno_files(context, &inno_dir)?;
    run_iscc(context, &identity, &inno_dir)?;

    let installer = context
        .workspace
        .join("target")
        .join(format!("ZZZ-{}", context.arch()))
        .with_extension("exe");
    if context.install {
        install(&installer)?;
    }
    eprintln!("installer: {}", installer.display());
    Ok(())
}

/// Mirrors `PrepareForBundle` and the copy half of `BuildZZZAndItsFriends`: the
/// Inno script reads everything from one flat directory.
fn stage_inno(context: &Context, inno_dir: &Path) -> Result<()> {
    util::remove(inno_dir)?;
    util::create_dir(inno_dir)?;

    let resources = context.workspace.join("crates/zzz/resources/windows");
    copy_tree(&resources, inno_dir)?;
    for directory in ["make_appx", "appx", "bin", "tools"] {
        util::create_dir(&inno_dir.join(directory))?;
    }

    let artifact_dir = context.artifact_dir();
    util::copy(&artifact_dir.join("zzz.exe"), &inno_dir.join("ZZZ.exe"))?;
    util::copy(&artifact_dir.join("cli.exe"), &inno_dir.join("cli.exe"))?;
    util::copy(
        &artifact_dir.join("explorer_command_injector.dll"),
        &inno_dir.join("zzz_explorer_command_injector.dll"),
    )?;
    Ok(())
}

fn build_appx(context: &Context, inno_dir: &Path) -> Result<()> {
    let manifest_name = match context.channel {
        Channel::Stable => "AppxManifest.xml",
        Channel::Preview => "AppxManifest-Preview.xml",
        Channel::Nightly | Channel::Dev => "AppxManifest-Nightly.xml",
    };
    let manifest = context
        .workspace
        .join("crates/explorer_command_injector")
        .join(manifest_name);
    if !manifest.is_file() {
        bail!("the AppX manifest is missing: {}", manifest.display());
    }
    util::copy(&manifest, &inno_dir.join("make_appx/AppxManifest.xml"))?;

    let makeappx = find_makeappx()?;
    util::run(
        Command::new(makeappx)
            .arg("pack")
            .arg("/d")
            .arg(inno_dir.join("make_appx"))
            .arg("/p")
            .arg(inno_dir.join("zzz_explorer_command_injector.appx"))
            .arg("/nv"),
        "makeAppx",
    )
}

fn find_makeappx() -> Result<PathBuf> {
    if let Some(path) = util::which("makeAppx.exe").or_else(|| util::which("makeAppx")) {
        return Ok(path);
    }
    if let Some(path) = windows_kits_tool("makeappx.exe") {
        return Ok(path);
    }
    bail!(
        "makeAppx.exe was not found; install the Windows SDK, or add its \
         `bin\\<version>\\<arch>` directory to PATH"
    )
}

fn sign_inno_files(context: &Context, inno_dir: &Path) -> Result<()> {
    if !context.sign || !ci() {
        return Ok(());
    }

    let files = [
        inno_dir.join("ZZZ.exe"),
        inno_dir.join("cli.exe"),
        inno_dir.join("zzz_explorer_command_injector.dll"),
        inno_dir.join("zzz_explorer_command_injector.appx"),
    ]
    .into_iter()
    .filter(|path| path.is_file())
    .map(|path| path.display().to_string())
    .collect::<Vec<_>>()
    .join(",");

    util::run(
        Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(inno_dir.join("sign.ps1"))
            .arg(files),
        "sign.ps1",
    )
}

/// Moves the flat staging entries into the directory layout `zzz.iss` expects
/// and adds the ConPTY and AMD GPU Services runtime files.
fn collect_inno_files(context: &Context, inno_dir: &Path) -> Result<()> {
    move_file(
        &inno_dir.join("zzz_explorer_command_injector.appx"),
        &inno_dir.join("appx/zzz_explorer_command_injector.appx"),
    )?;
    move_file(
        &inno_dir.join("zzz_explorer_command_injector.dll"),
        &inno_dir.join("appx/zzz_explorer_command_injector.dll"),
    )?;
    move_file(&inno_dir.join("cli.exe"), &inno_dir.join("bin/zzz.exe"))?;
    move_file(&inno_dir.join("zzz.sh"), &inno_dir.join("bin/zzz"))?;
    move_file(&inno_dir.join("zzz.cmd"), &inno_dir.join("bin/zzz.cmd"))?;

    let conpty = conpty_runtime(context)?;
    util::copy(&conpty.conpty, &inno_dir.join("conpty.dll"))?;

    if context.arch() == "aarch64" {
        util::copy(
            &conpty.open_console_arm64,
            &inno_dir.join("arm64/OpenConsole.exe"),
        )?;
        return Ok(());
    }

    util::create_dir(&inno_dir.join("x64"))?;
    util::create_dir(&inno_dir.join("arm64"))?;

    // The x64 build also ships the ARM64 console host so a user profile that
    // roams between machines keeps working under emulation.
    util::copy(
        &conpty.open_console_x64,
        &inno_dir.join("x64/OpenConsole.exe"),
    )?;
    let arm64_fallback = context
        .workspace
        .join("crates/zzz/resources/windows/bin/x64/OpenConsole.exe");
    if arm64_fallback.is_file() {
        util::copy(&arm64_fallback, &inno_dir.join("arm64/OpenConsole.exe"))?;
    } else if conpty.open_console_arm64.is_file() {
        util::copy(
            &conpty.open_console_arm64,
            &inno_dir.join("arm64/OpenConsole.exe"),
        )?;
    }

    let ags = ags_runtime(context)?;
    util::copy(&ags, &inno_dir.join("amd_ags_x64.dll"))?;
    Ok(())
}

fn run_iscc(context: &Context, identity: &InnoIdentity, inno_dir: &Path) -> Result<()> {
    let iscc = find_iscc()?;
    let mut definitions = BTreeMap::new();
    definitions.insert("AppId", identity.app_id.to_owned());
    definitions.insert("AppIconName", identity.icon.to_owned());
    definitions.insert(
        "OutputDir",
        context.workspace.join("target").display().to_string(),
    );
    definitions.insert("AppSetupName", format!("ZZZ-{}", context.arch()));
    definitions.insert("AppName", identity.name.to_owned());
    definitions.insert("AppDisplayName", identity.display_name.to_owned());
    definitions.insert("RegValueName", identity.registry_value.to_owned());
    definitions.insert("AppMutex", identity.mutex.to_owned());
    definitions.insert("AppExeName", identity.exe_name.to_owned());
    definitions.insert("ResourcesDir", inno_dir.display().to_string());
    definitions.insert("ShellNameShort", identity.shell_name_short.to_owned());
    definitions.insert("AppUserId", identity.user_id.to_owned());
    definitions.insert("Version", context.release_version.clone());
    definitions.insert("SourceDir", context.workspace.display().to_string());
    definitions.insert("AppxFullName", identity.appx_full_name.to_owned());

    let mut command = Command::new(iscc);
    command.arg(inno_dir.join("zzz.iss"));
    for (key, value) in &definitions {
        command.arg(format!("/d{key}={value}"));
    }
    if context.sign && ci() {
        let sign_tool = format!(
            "powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File {} $f",
            inno_dir.join("sign.ps1").display()
        );
        command.arg(format!("/sDefaultsign={sign_tool}"));
    }

    util::run(&mut command, "Inno Setup")
}

fn find_iscc() -> Result<PathBuf> {
    if let Some(path) = util::which("ISCC.exe").or_else(|| util::which("iscc")) {
        return Ok(path);
    }
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        let Some(base) = std::env::var_os(variable) else {
            continue;
        };
        let candidate = PathBuf::from(base).join(INNO_SETUP);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    bail!("ISCC.exe was not found; install Inno Setup 6 or add its directory to PATH")
}

fn install(installer: &Path) -> Result<()> {
    if !installer.is_file() {
        bail!("{} does not exist", installer.display());
    }
    // The installer is interactive, so it must outlive this process; dropping
    // the handle detaches it rather than killing it.
    let _child = Command::new(installer)
        .spawn()
        .with_context(|| format!("failed to start {}", installer.display()))?;
    eprintln!("started {}", installer.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// Third-party runtime files
// ---------------------------------------------------------------------------

struct ConptyRuntime {
    conpty: PathBuf,
    open_console_x64: PathBuf,
    open_console_arm64: PathBuf,
}

/// Prefers the files `crates/zzz/build.rs` already downloaded into the artifact
/// directory, because those are the ones the editor was compiled against. The
/// fallback release download only runs when that best-effort step was skipped.
fn conpty_runtime(context: &Context) -> Result<ConptyRuntime> {
    let artifact_dir = context.artifact_dir();
    let conpty = artifact_dir.join("conpty.dll");
    let console = artifact_dir.join("OpenConsole.exe");
    let resource_arm64 = context
        .workspace
        .join("crates/zzz/resources/windows/bin/x64/OpenConsole.exe");

    if conpty.is_file() && console.is_file() {
        return Ok(ConptyRuntime {
            conpty,
            open_console_x64: console.clone(),
            open_console_arm64: if context.arch() == "aarch64" {
                console
            } else {
                resource_arm64
            },
        });
    }

    eprintln!("conpty.dll is missing from the artifact directory; downloading it");
    let asset = github_release::latest(CONPTY_REPOSITORY, CONPTY_PATTERN)?;
    eprintln!("bundling {} ({})", asset.name, asset.tag);
    let downloads = context.target_dir.join("downloads");
    util::create_dir(&downloads)?;
    let archive = downloads.join(&asset.name);
    github_release::download(&asset, &archive, context.offline)?;

    let extracted = downloads.join("conpty");
    util::remove(&extracted)?;
    extract_zip(&archive, &extracted)?;

    let (arch_directory, console_directory) = if context.arch() == "aarch64" {
        ("win-arm64", "arm64")
    } else {
        ("win-x64", "x64")
    };
    Ok(ConptyRuntime {
        conpty: extracted
            .join("runtimes")
            .join(arch_directory)
            .join("native/conpty.dll"),
        open_console_x64: extracted
            .join("build/native/runtimes")
            .join(console_directory)
            .join("OpenConsole.exe"),
        open_console_arm64: extracted.join("build/native/runtimes/arm64/OpenConsole.exe"),
    })
}

fn ags_runtime(context: &Context) -> Result<PathBuf> {
    let downloads = context.target_dir.join("downloads");
    util::create_dir(&downloads)?;
    let archive = downloads.join("AGS_SDK_v6.3.0.zip");
    util::download(AGS_URL, &archive, AGS_SHA256, context.offline)?;

    let extracted = downloads.join("ags");
    let library = extracted
        .join(AGS_DIRECTORY)
        .join("ags_lib/lib/amd_ags_x64.dll");
    if !library.is_file() {
        util::remove(&extracted)?;
        extract_zip(&archive, &extracted)?;
    }
    if !library.is_file() {
        bail!(
            "{} does not contain ags_lib/lib/amd_ags_x64.dll",
            archive.display()
        );
    }
    Ok(library)
}

fn extract_zip(archive: &Path, destination: &Path) -> Result<()> {
    util::create_dir(destination)?;
    let file =
        fs::File::open(archive).with_context(|| format!("failed to open {}", archive.display()))?;
    let mut contents = zip::ZipArchive::new(file)
        .with_context(|| format!("failed to read {}", archive.display()))?;
    contents
        .extract(destination)
        .with_context(|| format!("failed to extract {}", archive.display()))
}

// ---------------------------------------------------------------------------
// Debug symbols
// ---------------------------------------------------------------------------

fn zip_debug_symbols(context: &Context) -> Result<()> {
    let artifact_dir = context.artifact_dir();
    let candidates = [
        "zzz.pdb",
        "cli.pdb",
        "explorer_command_injector.pdb",
        "remote_server.pdb",
    ];
    let files = candidates
        .iter()
        .map(|name| artifact_dir.join(name))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    if files.is_empty() {
        return Ok(());
    }

    let destination = artifact_dir.join(format!(
        "zzz-{}-{}.dbg.zip",
        context.release_version,
        context.channel.as_str()
    ));
    util::remove(&destination)?;
    let file = fs::File::create(&destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for path in files {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        archive
            .start_file(name, options)
            .with_context(|| format!("failed to add {} to the debug archive", path.display()))?;
        let mut source =
            fs::File::open(&path).with_context(|| format!("failed to open {}", path.display()))?;
        std::io::copy(&mut source, &mut archive)
            .with_context(|| format!("failed to write {} to the debug archive", path.display()))?;
    }
    archive
        .finish()
        .context("failed to finish the debug archive")?;
    eprintln!("debug symbols: {}", destination.display());
    Ok(())
}

fn upload_symbols(context: &Context) -> Result<()> {
    if !std::env::var("ZZZ_ENABLE_SENTRY_UPLOAD").is_ok_and(|value| value == "1") {
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

    let artifact_dir = context.artifact_dir();
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
            .arg(&artifact_dir)
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

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn grammar_gate(context: &Context) -> Result<()> {
    if std::env::var("ZZZ_SKIP_GRAMMAR_CHECK").is_ok_and(|value| value == "1" || value == "true") {
        return Ok(());
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

fn ci() -> bool {
    std::env::var_os("CI").is_some()
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    util::create_dir(destination)?;
    let entries =
        fs::read_dir(source).with_context(|| format!("failed to read {}", source.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("failed to read {}", source.display()))?;
        let path = entry.path();
        let target = destination.join(entry.file_name());
        let metadata = entry
            .metadata()
            .with_context(|| format!("failed to inspect {}", path.display()))?;
        if metadata.is_dir() {
            copy_tree(&path, &target)?;
        } else {
            util::copy(&path, &target)?;
        }
    }
    Ok(())
}

fn move_file(source: &Path, destination: &Path) -> Result<()> {
    if !source.is_file() {
        bail!("expected {} to exist", source.display());
    }
    if let Some(parent) = destination.parent() {
        util::create_dir(parent)?;
    }
    util::remove(destination)?;
    fs::rename(source, destination).with_context(|| {
        format!(
            "failed to move {} to {}",
            source.display(),
            destination.display()
        )
    })
}

/// Searches the Windows SDK for a host-architecture build tool. Native Windows
/// runners do not always put the SDK bin directory on `PATH`.
fn windows_kits_tool(tool: &str) -> Option<PathBuf> {
    let host = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "x86",
        _ => return None,
    };

    let mut versions = Vec::new();
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        let Some(base) = std::env::var_os(variable) else {
            continue;
        };
        let bin = PathBuf::from(base).join("Windows Kits/10/bin");
        let Ok(entries) = fs::read_dir(&bin) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                versions.push(path);
            }
        }
    }
    versions.sort();

    for version in versions.into_iter().rev() {
        let candidate = version.join(host).join(tool);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stable_installer_keeps_its_double_brace() {
        // Inno expands `{{` to a literal `{`; dropping one changes the AppId
        // and makes an upgrade install side by side.
        assert_eq!(
            inno_identity(Channel::Stable).app_id,
            "{{2DB0DA96-CA55-49BB-AF4F-64AF36A86712}"
        );
    }

    #[test]
    fn every_channel_has_a_distinct_installer_identity() {
        let identities = [
            inno_identity(Channel::Stable),
            inno_identity(Channel::Preview),
            inno_identity(Channel::Nightly),
            inno_identity(Channel::Dev),
        ];
        for (index, identity) in identities.iter().enumerate() {
            for other in &identities[index + 1..] {
                assert_ne!(identity.app_id, other.app_id);
                assert_ne!(identity.user_id, other.user_id);
            }
            assert_eq!(identity.exe_name, "ZZZ");
        }
    }

    #[test]
    fn only_ffmpeg_runtime_dlls_are_staged() {
        assert!(is_ffmpeg_runtime("avcodec-62.dll"));
        assert!(is_ffmpeg_runtime("swscale-9.dll"));
        assert!(!is_ffmpeg_runtime("avcodec.dll"));
    }
}
