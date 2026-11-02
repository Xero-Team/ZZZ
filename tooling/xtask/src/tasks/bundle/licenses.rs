//! Generates `assets/licenses.md`, which is baked into every bundle.
//!
//! The document combines the vendored theme and icon licenses, the Rust
//! dependency inventory produced by `cargo-about`, and the licenses for the
//! native libraries that ship with the editor. Keeping it in one place means a
//! bundle cannot be assembled without it.

#![allow(
    clippy::disallowed_methods,
    reason = "license generation drives cargo and cargo-about"
)]

use std::path::Path;
use std::process::Command;

use anyhow::{Context as _, Result, bail};

use crate::tasks::bundle::util;

pub const CARGO_ABOUT_VERSION: &str = "0.9.3";
pub const DEFAULT_OUTPUT: &str = "assets/licenses.md";
const TEMPLATE: &str = "script/licenses/template.md.hbs";
const CONFIGURATION: &str = "script/licenses/zzz-licenses.toml";

/// Regenerates the license document. `output` is relative to the workspace
/// unless absolute.
pub fn generate(workspace: &Path, output: Option<&Path>, offline: bool) -> Result<()> {
    let output = match output {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => workspace.join(path),
        None => workspace.join(DEFAULT_OUTPUT),
    };

    let mut document = String::new();
    document.push_str("# ###### THEME LICENSES ######\n\n");
    document.push_str(&read(workspace, "assets/themes/LICENSES")?);
    document.push_str("\n\n# ###### ICON LICENSES ######\n\n");
    document.push_str(&read(workspace, "assets/icons/LICENSES")?);
    document.push_str("\n\n# ###### CODE LICENSES ######\n");

    ensure_cargo_about(workspace, offline)?;

    let mut command = Command::new("cargo");
    command
        .current_dir(workspace)
        .args(["about", "generate", "-c", CONFIGURATION, TEMPLATE]);
    // A missing license makes `cargo about` print a warning. Treat any stderr
    // output as a failure unless the caller deliberately allows it, because the
    // bundle ships this document as the dependency inventory.
    let allow_missing = std::env::var_os("ALLOW_MISSING_LICENSES").is_some();
    if !allow_missing {
        command.arg("--fail");
    }
    let result = util::output(&mut command, "cargo about generate")?;
    if !allow_missing && !result.stderr.is_empty() {
        bail!(
            "cargo about reported a problem:\n{}",
            String::from_utf8_lossy(&result.stderr).trim()
        );
    }
    document.push_str(&unescape(&String::from_utf8_lossy(&result.stdout)));

    document.push_str("\n\n# ###### NATIVE LIBRARY LICENSES ######\n\n");
    document.push_str("#### GNU Lesser General Public License version 3\n\n");
    document.push_str("##### Used by:\n\n");
    let (series, archive, tag) = crate::tasks::bundle::ffmpeg::dependency_info(workspace);
    document.push_str(&format!("* FFmpeg {series} ({archive})\n"));
    document.push_str(&format!(
        "* Prebuilt by [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds/releases/tag/{tag})\n\n"
    ));
    document.push_str(&read(workspace, "script/licenses/ffmpeg-LICENSE.txt")?);

    std::fs::write(&output, document)
        .with_context(|| format!("failed to write {}", output.display()))?;
    eprintln!("licenses: {}", output.display());
    Ok(())
}

fn read(workspace: &Path, relative: &str) -> Result<String> {
    let path = workspace.join(relative);
    std::fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))
}

fn ensure_cargo_about(workspace: &Path, offline: bool) -> Result<()> {
    let version = util::stdout(
        Command::new("cargo")
            .current_dir(workspace)
            .args(["about", "--version"]),
        "cargo about --version",
    )
    .unwrap_or_default();
    if version.contains(&format!("cargo-about {CARGO_ABOUT_VERSION}")) {
        return Ok(());
    }
    if offline {
        bail!(
            "cargo-about {CARGO_ABOUT_VERSION} is required while --offline is set; \
             install it with `cargo install cargo-about@{CARGO_ABOUT_VERSION} --features cli`"
        );
    }
    eprintln!("installing cargo-about@{CARGO_ABOUT_VERSION}");
    util::run(
        Command::new("cargo").current_dir(workspace).args([
            "install",
            &format!("cargo-about@{CARGO_ABOUT_VERSION}"),
            "--features",
            "cli",
        ]),
        "cargo install cargo-about",
    )
}

fn unescape(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#x3D;", "=")
        .replace("&#x60;", "`")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}
