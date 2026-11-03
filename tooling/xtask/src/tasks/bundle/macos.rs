//! macOS packaging: the `.app` bundle and the DMG.
//!
//! One implementation covers both a native macOS host and the Linux cross path,
//! because the only difference is how the binaries are produced: native runs
//! `cargo build`, Linux runs `cargo zigbuild` against a downloaded SDK. The
//! bundle assembly, the embedded git, the DMG, and the plist checks are shared.
//!
//! Signing and notarization stay on macOS: the tools (`security`, `codesign`,
//! `notarytool`, `stapler`) do not exist anywhere else. The cross path therefore
//! produces an unsigned `.app` and `.dmg`.

#![allow(
    clippy::disallowed_methods,
    reason = "packaging drives cargo-bundle, codesign, hdiutil, and hfsplus"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};

use crate::tasks::build::model::{Channel, Format, Platform, Profile};
use crate::tasks::bundle::{
    Context, app_identity, github_release, licenses, macos_sdk, remote_server, util,
};

/// The team in the provisioning profile. The App Store build must use it.
const IDENTITY: &str = "Xero Team";
const GIT_REPOSITORY: &str = "desktop/dugite-native";
const ENTITLEMENTS: &str = "crates/zzz/resources/zzz.entitlements";

pub fn bundle(context: &Context) -> Result<()> {
    if context.target.platform() != Platform::Mac {
        bail!("the macOS bundler only handles apple-darwin targets");
    }
    let build_app = context.formats.contains(&Format::App);
    let build_dmg = context.formats.contains(&Format::Dmg);
    if !build_app && !build_dmg {
        bail!("the macOS bundler produces app and dmg; nothing else is shippable");
    }
    if build_dmg && context.profile == Profile::Dev {
        bail!("a debug build has no DMG; request `--formats app` instead");
    }

    let native = Platform::host() == Platform::Mac;

    if !context.skip_build {
        grammar_gate(context)?;
        licenses::generate(&context.workspace, None, context.offline)?;
        util::rustup_add_target(context.target.triple())?;
        build_binaries(context, native)?;
    }

    let app = assemble_app(context)?;

    upload_symbols(context)?;

    if native {
        sign_and_notarize(context, &app)?;
        install_or_open(context, &app)?;
    }

    if build_dmg {
        create_dmg(context, &app, native)?;
    }

    eprintln!("macOS bundle: {}", app.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// Building the binaries
// ---------------------------------------------------------------------------

fn build_binaries(context: &Context, native: bool) -> Result<()> {
    // Point the embed build script at the cached matrix instead of compiling a
    // second copy of every remote server inside this bundle.
    let remote_server_dir = remote_server::ensure(context)?;

    let mut command = if native {
        Command::new("cargo")
    } else {
        require_cross_tools()?;
        Command::new("cargo")
    };
    command.current_dir(&context.workspace);

    if native {
        command.arg("build");
    } else {
        command.arg("zigbuild");
    }
    command.args(context.profile_flag());
    command
        .args(["--target", context.target.triple()])
        .args(["--package", "zzz", "--package", "cli"])
        .env("ZZZ_BUNDLE", "true")
        .env("ZZZ_RELEASE_CHANNEL", context.channel.as_str())
        .env("RELEASE_VERSION", &context.release_version)
        .env("CARGO_BUILD_JOBS", context.jobs.to_string())
        // macOS has no libstdc++ headers, so C++ dependencies have to use
        // libc++.
        .env("CXXFLAGS", "-stdlib=libc++");
    if let Some(directory) = remote_server_dir {
        command.env("ZZZ_EMBED_REMOTE_SERVER_DIR", directory);
    }

    if !native {
        let sdk = macos_sdk::ensure(&context.workspace, None, context.offline)?;
        command.env("SDKROOT", sdk);
        util::configure_zig(&mut command, &context.workspace, context.target.triple());
    }
    if context.offline {
        command.env("CARGO_NET_OFFLINE", "true");
    }

    util::run(
        &mut command,
        &format!("cargo build for {}", context.target.triple()),
    )
}

fn require_cross_tools() -> Result<()> {
    if Platform::host() != Platform::Linux {
        bail!("cross-compiling a macOS bundle is only supported from Linux");
    }
    util::require_tool_with_args(
        "zig",
        &["version"],
        "Install zig from https://ziglang.org/learn/getting-started or with zigup.",
    )?;
    util::require_tool(
        "cargo-zigbuild",
        "Install it with `cargo install cargo-zigbuild`.",
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// The `.app` directory
// ---------------------------------------------------------------------------

fn assemble_app(context: &Context) -> Result<PathBuf> {
    let produced = cargo_bundle(context)?;
    verify_info_plist(&produced)?;

    let identity = app_identity(context.channel);
    let destination = context
        .artifact_dir()
        .join(format!("{}.app", identity.name));
    if produced != destination {
        util::remove(&destination)?;
        copy_tree(&produced, &destination)?;
    }

    // `DocumentTypes.plist` references `CFBundleTypeIconFile = Document`, so the
    // app icon stands in for the document icon.
    let document_icon = context.workspace.join("crates/zzz/resources/Document.icns");
    if document_icon.is_file() {
        util::copy(
            &document_icon,
            &destination.join("Contents/Resources/Document.icns"),
        )?;
    } else {
        eprintln!(
            "warning: {} is missing; macOS document icons will not appear",
            document_icon.display()
        );
    }

    let artifact_dir = context.artifact_dir();
    util::copy(
        &artifact_dir.join("zzz"),
        &destination.join("Contents/MacOS/zzz"),
    )?;
    util::copy(
        &artifact_dir.join("cli"),
        &destination.join("Contents/MacOS/cli"),
    )?;

    download_git(context, &destination)?;

    // The embedded provision profile lets the signed app keep universal links.
    let profile = context
        .workspace
        .join("crates/zzz/contents")
        .join(context.channel.as_str())
        .join("embedded.provisionprofile");
    if profile.is_file() {
        util::copy(
            &profile,
            &destination.join("Contents/embedded.provisionprofile"),
        )?;
    }

    Ok(destination)
}

/// Runs `cargo bundle` with the channel's metadata table promoted to the default
/// one. The manifest edit is restored even when cargo-bundle fails.
fn cargo_bundle(context: &Context) -> Result<PathBuf> {
    let manifest = context.workspace.join("crates/zzz/Cargo.toml");
    let original = fs::read_to_string(&manifest)
        .with_context(|| format!("failed to read {}", manifest.display()))?;
    // `dev` is already the default table; only the named release channels carry
    // a `bundle-<channel>` table that has to be promoted for cargo-bundle.
    let rewritten = if context.channel == Channel::Dev {
        original.clone()
    } else {
        let from = format!("package.metadata.bundle-{}", context.channel.as_str());
        let rewritten = original.replace(&from, "package.metadata.bundle");
        if rewritten == original {
            bail!("{} does not declare [{from}]", manifest.display());
        }
        rewritten
    };
    let mut guard = None;
    if rewritten != original {
        fs::write(&manifest, rewritten)
            .with_context(|| format!("failed to update {}", manifest.display()))?;
        guard = Some(ManifestGuard {
            path: manifest,
            contents: original,
        });
    }

    let mut command = Command::new("cargo");
    command
        .current_dir(&context.workspace)
        .arg("bundle")
        .args(context.profile_flag())
        .args(["--target", context.target.triple()])
        .args(["--select-workspace-root", "--bin", "zzz"])
        .env("CARGO_BUNDLE_SKIP_BUILD", "true")
        .env("CARGO_TARGET_DIR", &context.target_dir)
        .env("ZZZ_RELEASE_CHANNEL", context.channel.as_str())
        .env("ZZZ_BUNDLE", "true");
    if context.offline {
        command.env("CARGO_NET_OFFLINE", "true");
    }

    let output = util::output(&mut command, "cargo bundle")?;
    drop(guard);

    let path = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| {
            Path::new(line)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
        })
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("cargo bundle did not report the `.app` it created"))?;
    if !path.is_dir() {
        bail!(
            "cargo bundle reported {}, which does not exist",
            path.display()
        );
    }
    Ok(path)
}

struct ManifestGuard {
    path: PathBuf,
    contents: String,
}

impl Drop for ManifestGuard {
    fn drop(&mut self) {
        if let Err(error) = fs::write(&self.path, &self.contents) {
            eprintln!(
                "warning: failed to restore {}: {error}",
                self.path.display()
            );
        }
    }
}

/// `cargo-bundle` silently falls back to a stub plist (`zzz.zzz`) when the
/// `[package.metadata.bundle.bin.zzz]` table is missing, and macOS then closes
/// the menu bar as soon as it opens.
fn verify_info_plist(app: &Path) -> Result<()> {
    let path = app.join("Contents/Info.plist");
    let text = fs::read_to_string(&path)
        .with_context(|| format!("cargo-bundle did not produce {}", path.display()))?;

    if !text.contains("CFBundleIdentifier") || text.contains("<string>zzz.zzz</string>") {
        bail!(
            "cargo-bundle produced a stub Info.plist; ensure \
             [package.metadata.bundle.bin.zzz] exists for this channel"
        );
    }
    if !text.contains("CFBundleIconFile") {
        bail!("the bundled Info.plist is missing CFBundleIconFile");
    }
    if !text.contains("LSMinimumSystemVersion") {
        bail!("the bundled Info.plist is missing LSMinimumSystemVersion");
    }
    Ok(())
}

fn download_git(context: &Context, app: &Path) -> Result<()> {
    let suffix = match context.target.arch().as_str() {
        "aarch64" => "macOS-arm64",
        other => {
            debug_assert_eq!(other, "x86_64");
            "macOS-x64"
        }
    };
    let pattern = format!("dugite-native-*-{suffix}.tar.gz");
    let asset = github_release::latest(GIT_REPOSITORY, &pattern)?;
    eprintln!("bundling {} ({})", asset.name, asset.tag);

    let archive = context.target_dir.join("downloads").join(&asset.name);
    github_release::download(&asset, &archive, context.offline)?;
    let extracted = tempfile::Builder::new()
        .prefix("zzz-dugite-")
        .tempdir()
        .context("failed to create a temporary directory for git")?;
    util::extract(&archive, extracted.path())?;
    let git = extracted.path().join("bin/git");
    if !git.is_file() {
        bail!("{} does not contain bin/git", asset.name);
    }
    util::copy(&git, &app.join("Contents/MacOS/git"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Sentry symbol upload
// ---------------------------------------------------------------------------

/// Uploads the unstripped host binaries so stack traces can be symbolized.
///
/// Opt-in only, and `dsymutil` is Apple-only, so a cross build and a local
/// install skip it instead of failing a release.
fn upload_symbols(context: &Context) -> Result<()> {
    if context.install {
        eprintln!("local install; skipping sentry upload.");
        return Ok(());
    }
    if !std::env::var("ZZZ_ENABLE_SENTRY_UPLOAD").is_ok_and(|value| value == "1") {
        eprintln!("ZZZ_ENABLE_SENTRY_UPLOAD is not set to 1; skipped sentry upload.");
        return Ok(());
    }
    if std::env::var_os("SENTRY_AUTH_TOKEN").is_none() {
        eprintln!("SENTRY_AUTH_TOKEN is missing; skipped sentry upload.");
        return Ok(());
    }
    if !util::command_exists("dsymutil") || !util::command_exists("sentry-cli") {
        eprintln!("dsymutil or sentry-cli is not available; skipped sentry upload.");
        return Ok(());
    }

    let artifact_dir = context.artifact_dir();
    let mut binaries = vec![artifact_dir.join("zzz")];
    // The matrix builds the remote server outside this target directory, so
    // its debug info is only available when that build left a binary behind.
    let remote_server = context
        .workspace
        .join(crate::tasks::build::remote_server::CACHE_DIRECTORY)
        .join("build")
        .join(context.target.triple())
        .join("release/remote_server");
    if remote_server.is_file() {
        binaries.push(remote_server);
    }

    let mut debug_files = Vec::new();
    for binary in binaries {
        util::run(
            Command::new("dsymutil").arg("--flat").arg(&binary),
            &format!("dsymutil {}", binary.display()),
        )?;
        debug_files.push(binary.with_extension("dwarf"));
    }

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
            .args(&debug_files)
            .status();
        if status.is_ok_and(|status| status.success()) {
            eprintln!("sentry upload succeeded");
            return Ok(());
        }
        // An upload failure must never fail a release.
        if attempt == 3 {
            eprintln!("warning: every sentry upload attempt failed");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Signing, notarization, installation
// ---------------------------------------------------------------------------

fn signing_credentials() -> bool {
    [
        "MACOS_CERTIFICATE",
        "MACOS_CERTIFICATE_PASSWORD",
        "APPLE_NOTARIZATION_KEY",
        "APPLE_NOTARIZATION_KEY_ID",
        "APPLE_NOTARIZATION_ISSUER_ID",
    ]
    .into_iter()
    .all(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

fn sign_and_notarize(context: &Context, app: &Path) -> Result<()> {
    if !context.sign {
        eprintln!("--no-sign was requested; leaving the bundle unsigned");
        return Ok(());
    }
    let entitlements = context.workspace.join(ENTITLEMENTS);

    if signing_credentials() {
        let keychain = Keychain::create()?;
        sign_binaries(app, &entitlements)?;
        codesign(
            &["--force", "--timestamp", "--options", "runtime"],
            &[],
            app,
        )?;
        // Forgetting the keychain is handled by `Keychain::drop`.
        let _ = &keychain;
    } else {
        eprintln!(
            "one or more signing variables are missing; signing without every \
             entitlement (universal links will not work)"
        );
        let stripped = app.join("Contents/Resources/zzz.entitlements");
        let source = fs::read_to_string(&entitlements)
            .with_context(|| format!("failed to read {}", entitlements.display()))?;
        let filtered = strip_associated_domains(&source);
        if let Some(parent) = stripped.parent() {
            util::create_dir(parent)?;
        }
        fs::write(&stripped, filtered)
            .with_context(|| format!("failed to write {}", stripped.display()))?;
        let identity = std::env::var("MACOS_SIGNING_KEY").unwrap_or_else(|_| "-".to_owned());
        util::run(
            Command::new("codesign")
                .arg("--force")
                .arg("--deep")
                .arg("--entitlements")
                .arg(&stripped)
                .arg("--sign")
                .arg(&identity)
                .arg(app)
                .arg("-v"),
            "codesign",
        )?;
    }
    Ok(())
}

fn sign_binaries(app: &Path, entitlements: &Path) -> Result<()> {
    let macos = app.join("Contents/MacOS");
    for name in ["cli", "git"] {
        codesign(
            &["--deep", "--force", "--timestamp", "--options", "runtime"],
            &[],
            &macos.join(name),
        )?;
    }
    codesign(
        &["--deep", "--force", "--timestamp", "--options", "runtime"],
        &["--entitlements", &entitlements.to_string_lossy()],
        &macos.join("zzz"),
    )
}

fn codesign(flags: &[&str], extra: &[&str], target: &Path) -> Result<()> {
    let mut command = Command::new("codesign");
    command.args(flags).args(extra);
    command.arg("--sign").arg(IDENTITY).arg(target).arg("-v");
    util::run(&mut command, "codesign")
}

/// Removes the `com.apple.developer.associated-domains` entitlement and its
/// value when the app is signed without the provisioning profile.
fn strip_associated_domains(source: &str) -> String {
    let mut lines = source.lines();
    let mut output = String::new();
    while let Some(line) = lines.next() {
        if line.contains("com.apple.developer.associated-domains") {
            // Drop this line and the following `<array>...</array>` or
            // `<string>...</string>` value, whichever the file uses.
            for value in lines.by_ref() {
                if value.contains("</array>") || value.contains("</string>") {
                    break;
                }
            }
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }
    output
}

/// A throwaway keychain that is unlocked for `codesign` and removed on drop.
struct Keychain;

impl Keychain {
    fn create() -> Result<Self> {
        let password = std::env::var("MACOS_CERTIFICATE_PASSWORD").unwrap_or_default();
        let certificate = std::env::var("MACOS_CERTIFICATE").unwrap_or_default();
        let name = "zzz.keychain";
        // A stale keychain from an interrupted run must not block this one.
        let _ = Command::new("security")
            .args(["delete-keychain", name])
            .status();
        util::run(
            Command::new("security").args(["create-keychain", "-p", &password, name]),
            "security create-keychain",
        )?;
        util::run(
            Command::new("security").args(["default-keychain", "-s", name]),
            "security default-keychain",
        )?;
        util::run(
            Command::new("security").args(["unlock-keychain", "-p", &password, name]),
            "security unlock-keychain",
        )?;
        util::run(
            Command::new("security").args(["set-keychain-settings", name]),
            "security set-keychain-settings",
        )?;

        let decoded = base64_decode(&certificate)?;
        let p12 = std::env::temp_dir().join("zzz-certificate.p12");
        fs::write(&p12, decoded).with_context(|| format!("failed to write {}", p12.display()))?;
        util::run(
            Command::new("security").args(["import"]).arg(&p12).args([
                "-k",
                name,
                "-P",
                &password,
                "-T",
                "/usr/bin/codesign",
            ]),
            "security import",
        )?;
        util::remove(&p12)?;
        util::run(
            Command::new("security").args([
                "set-key-partition-list",
                "-S",
                "apple-tool:,apple:,codesign:",
                "-s",
                "-k",
                &password,
                name,
            ]),
            "security set-key-partition-list",
        )?;
        Ok(Self)
    }
}

impl Drop for Keychain {
    fn drop(&mut self) {
        eprintln!("cleaning up the signing keychain");
        let _ = Command::new("security")
            .args(["default-keychain", "-s", "login.keychain"])
            .status();
        let _ = Command::new("security")
            .args(["delete-keychain", "zzz.keychain"])
            .status();
    }
}

fn base64_decode(value: &str) -> Result<Vec<u8>> {
    use std::io::Write as _;

    // `base64 --decode` exists on both macOS and Linux, and avoids pulling a
    // crate into xtask for a single workflow.
    let mut child = Command::new("base64")
        .arg("--decode")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .context("failed to start base64 --decode")?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(value.as_bytes())
            .context("failed to feed base64 --decode")?;
    }
    let output = child
        .wait_with_output()
        .context("failed to read base64 --decode output")?;
    if !output.status.success() {
        bail!("base64 --decode failed with {}", output.status);
    }
    Ok(output.stdout)
}

fn install_or_open(context: &Context, app: &Path) -> Result<()> {
    let name = app
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| anyhow::anyhow!("{} has no file name", app.display()))?;

    if context.install {
        let destination = PathBuf::from("/Applications").join(&name);
        util::remove(&destination)?;
        fs::rename(app, &destination).with_context(|| {
            format!(
                "failed to move {} to {}",
                app.display(),
                destination.display()
            )
        })?;
        eprintln!("installed {}", destination.display());
        if context.open {
            util::run(
                Command::new("open").arg(&destination),
                "open the installed app",
            )?;
        }
    } else if context.open {
        util::run(Command::new("open").arg(app), "open the app")?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// The DMG
// ---------------------------------------------------------------------------

fn create_dmg(context: &Context, app: &Path, native: bool) -> Result<()> {
    let artifact_dir = context.artifact_dir();
    let dmg = artifact_dir.join(format!("ZZZ-{}.dmg", context.arch()));
    let source = artifact_dir.join("dmg");
    util::remove(&source)?;
    util::create_dir(&source)?;
    let name = app
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("{} has no file name", app.display()))?;
    copy_tree(app, &source.join(name))?;

    util::remove(&dmg)?;
    if native {
        create_native_dmg(context, &source, &dmg)?;
    } else {
        create_cross_dmg(&source, &dmg)?;
    }
    eprintln!("DMG: {}", dmg.display());
    Ok(())
}

fn create_native_dmg(context: &Context, source: &Path, dmg: &Path) -> Result<()> {
    let applications = source.join("Applications");
    if !applications.exists() {
        std::os::unix::fs::symlink("/Applications", &applications)
            .with_context(|| format!("failed to create {}", applications.display()))?;
    }

    util::run(
        Command::new("hdiutil")
            .args(["create", "-volname", "ZZZ", "-srcfolder"])
            .arg(source)
            .args(["-ov", "-format", "UDZO"])
            .arg(dmg),
        "hdiutil create",
    )?;

    // The symlink makes Finder index the whole filesystem when the project
    // itself lives under /Applications, so it must not survive in the source.
    util::remove(&applications)?;

    util::require_tool(
        "dmg-license",
        "Install the pinned packaging tools in the development environment.",
    )?;
    util::run(
        Command::new("dmg-license")
            .arg(context.workspace.join("script/terms/terms.json"))
            .arg(dmg),
        "dmg-license",
    )?;

    if context.sign && signing_credentials() {
        util::run(
            Command::new("codesign")
                .arg("--deep")
                .arg("--force")
                .arg("--timestamp")
                .arg("--options")
                .arg("runtime")
                .arg("--sign")
                .arg(IDENTITY)
                .arg(dmg)
                .arg("-v"),
            "codesign the DMG",
        )?;

        let key = std::env::var("APPLE_NOTARIZATION_KEY").unwrap_or_default();
        let key_id = std::env::var("APPLE_NOTARIZATION_KEY_ID").unwrap_or_default();
        let issuer = std::env::var("APPLE_NOTARIZATION_ISSUER_ID").unwrap_or_default();
        let key_file =
            tempfile::NamedTempFile::new().context("failed to create a notarization key file")?;
        fs::write(key_file.path(), key)
            .with_context(|| format!("failed to write {}", key_file.path().display()))?;
        let xcode = util::stdout(Command::new("xcode-select").arg("-p"), "xcode-select -p")?;
        let bin = PathBuf::from(xcode).join("usr/bin");

        util::run(
            Command::new(bin.join("notarytool"))
                .args(["submit", "--wait", "--key"])
                .arg(key_file.path())
                .args(["--key-id", &key_id, "--issuer", &issuer])
                .arg(dmg),
            "notarytool submit",
        )?;
        util::run(
            Command::new(bin.join("stapler")).arg("staple").arg(dmg),
            "stapler staple",
        )?;
    } else if !context.sign {
        eprintln!("--no-sign was requested; the DMG is unsigned");
    }

    if context.open {
        util::run(
            Command::new("open").arg(dmg.parent().unwrap_or(Path::new("."))),
            "open the DMG directory",
        )?;
    }
    Ok(())
}

fn create_cross_dmg(source: &Path, dmg: &Path) -> Result<()> {
    for tool in ["mkfs.hfsplus", "hfsplus", "dmg"] {
        util::require_tool(
            tool,
            "Install hfsprogs and build libdmg-hfsplus (https://github.com/planetbeing/libdmg-hfsplus).",
        )?;
    }

    let megabytes = directory_size_mb(source)? + directory_size_mb(source)? / 5 + 16;
    let image = tempfile::Builder::new()
        .suffix(".hfs")
        .tempfile()
        .context("failed to create an HFS image")?;
    let image_path = image.path().to_path_buf();

    util::run(
        Command::new("dd")
            .arg("if=/dev/zero")
            .arg(format!("of={}", image_path.display()))
            .arg("bs=1M")
            .arg(format!("count={megabytes}"))
            .arg("status=none"),
        "dd",
    )?;

    util::run(
        Command::new("mkfs.hfsplus").arg(&image_path),
        "mkfs.hfsplus",
    )?;
    util::run(
        Command::new("hfsplus")
            .arg(&image_path)
            .args(["addall"])
            .arg(source)
            .arg("/"),
        "hfsplus addall",
    )?;

    // libdmg-hfsplus stores every file as 0644, which makes the app binary and
    // the bundled git unlaunchable.
    for executable in executable_files(source)? {
        let relative = executable
            .strip_prefix(source)
            .unwrap_or(&executable)
            .to_string_lossy()
            .into_owned();
        util::run(
            Command::new("hfsplus")
                .arg(&image_path)
                .args(["chmod", "755"])
                .arg(format!("/{relative}")),
            "hfsplus chmod",
        )?;
    }

    util::run(
        Command::new("dmg").arg("build").arg(&image_path).arg(dmg),
        "dmg build",
    )?;
    Ok(())
}

fn directory_size_mb(directory: &Path) -> Result<u64> {
    let output = util::stdout(Command::new("du").args(["-sm"]).arg(directory), "du -sm")?;
    let value = output
        .split_whitespace()
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(1);
    Ok(value.max(1))
}

fn executable_files(directory: &Path) -> Result<Vec<PathBuf>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;

        let mut files = Vec::new();
        for entry in fs::read_dir(directory)
            .with_context(|| format!("failed to read {}", directory.display()))?
        {
            let path = entry?.path();
            let metadata = fs::metadata(&path)?;
            if metadata.is_dir() {
                files.extend(executable_files(&path)?);
            } else if metadata.permissions().mode() & 0o111 != 0 {
                files.push(path);
            }
        }
        Ok(files)
    }
    #[cfg(not(unix))]
    {
        let _ = directory;
        Ok(Vec::new())
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        util::create_dir(parent)?;
    }
    util::run(
        Command::new("cp").arg("-R").arg(source).arg(destination),
        "cp -R",
    )
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn associated_domains_are_removed_with_their_value() {
        let source = "<dict>\n<key>com.apple.developer.associated-domains</key>\n<array>\n<string>applinks:zzz.dev</string>\n</array>\n<key>other</key>\n<true/>\n</dict>\n";
        let stripped = strip_associated_domains(source);
        assert!(!stripped.contains("associated-domains"));
        assert!(stripped.contains("<key>other</key>"));
    }
}
