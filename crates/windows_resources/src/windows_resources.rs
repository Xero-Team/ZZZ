#![allow(
    clippy::disallowed_methods,
    reason = "build helper used only from build scripts"
)]

use std::process::Command;

fn git_sha() -> Option<String> {
    if let Ok(sha) = std::env::var("ZZZ_COMMIT_SHA") {
        return Some(sha);
    }

    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn product_version() -> String {
    let commit_sha = git_sha();
    let pkg_version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let channel = std::env::var("RELEASE_CHANNEL").unwrap_or_else(|_| "dev".into());
    let build_id = std::env::var("GITHUB_RUN_NUMBER").ok();

    format_product_version(
        &pkg_version,
        &channel,
        build_id.as_deref(),
        commit_sha.as_deref(),
    )
}

const ICON_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../zzz/resources/windows");
const MANIFEST_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/resources/manifest.xml");

fn product_identity(channel: &str) -> (&'static str, &'static str) {
    match channel {
        "stable" => ("app-icon.ico", "ZZZ"),
        "preview" => ("app-icon-preview.ico", "ZZZ Preview"),
        "nightly" => ("app-icon-nightly.ico", "ZZZ Nightly"),
        _ => ("app-icon-dev.ico", "ZZZ Dev"),
    }
}

fn format_product_version(
    pkg_version: &str,
    channel: &str,
    build_id: Option<&str>,
    commit_sha: Option<&str>,
) -> String {
    let mut metadata = channel.to_owned();
    if let Some(build_id) = build_id {
        metadata.push('.');
        metadata.push_str(build_id);
    }
    if let Some(commit_sha) = commit_sha {
        metadata.push('.');
        metadata.push_str(commit_sha);
    }

    format!("{pkg_version}+{metadata}")
}

fn file_version(pkg_version: &str) -> String {
    let mut version_parts = pkg_version
        .split('.')
        .map(|part| part.parse::<u16>().unwrap_or(0))
        .chain(std::iter::repeat(0));
    format!(
        "{},{},{},{}",
        version_parts.next().unwrap_or(0),
        version_parts.next().unwrap_or(0),
        version_parts.next().unwrap_or(0),
        version_parts.next().unwrap_or(0),
    )
}

pub fn compile(manifest: bool) -> Result<(), Box<dyn std::error::Error>> {
    let channel = option_env!("RELEASE_CHANNEL").unwrap_or("dev");
    let (icon_filename, product_name) = product_identity(channel);
    let icon = std::path::PathBuf::from(ICON_DIR).join(icon_filename);
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);

    #[cfg(windows)]
    let icon_escaped = icon.to_string_lossy().replace('\\', "\\\\");
    #[cfg(not(windows))]
    let icon_escaped = {
        let staged = out_dir.join(icon_filename);
        std::fs::copy(&icon, &staged)?;
        icon_filename.to_string()
    };

    let manifest_line = if manifest {
        #[cfg(windows)]
        {
            let escaped = MANIFEST_PATH.replace('\\', "\\\\");
            format!("1 24 \"{escaped}\"")
        }
        #[cfg(not(windows))]
        {
            let staged = out_dir.join("manifest.xml");
            std::fs::copy(MANIFEST_PATH, &staged)?;
            "1 24 \"manifest.xml\"".to_string()
        }
    } else {
        String::new()
    };

    let pkg_version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let product_version = product_version();
    let file_version = file_version(&pkg_version);

    let rc_content = format!(
        r#"1 ICON "{icon_escaped}"
{manifest_line}

1 VERSIONINFO
FILEVERSION {file_version}
PRODUCTVERSION {file_version}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "FileDescription", "{product_name}\0"
            VALUE "FileVersion", "{pkg_version}\0"
            VALUE "ProductName", "{product_name}\0"
            VALUE "ProductVersion", "{product_version}\0"
            VALUE "CompanyName", "Xero Team\0"
            VALUE "LegalCopyright", "Copyright 2022 - 2025 Zed Industries, Inc.\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#
    );

    let rc_path = out_dir.join("zzz_resources.rc");
    std::fs::write(&rc_path, rc_content)?;

    if let Ok(toolkit_path) = std::env::var("ZZZ_RC_TOOLKIT_PATH") {
        let rc_exe = std::path::Path::new(&toolkit_path).join("rc.exe");
        unsafe {
            std::env::set_var("RC", rc_exe);
        }
    }

    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_optional()
        .unwrap();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{file_version, format_product_version, product_identity};

    #[test]
    fn product_identity_matches_release_channel() {
        assert_eq!(product_identity("stable"), ("app-icon.ico", "ZZZ"));
        assert_eq!(
            product_identity("preview"),
            ("app-icon-preview.ico", "ZZZ Preview")
        );
        assert_eq!(
            product_identity("nightly"),
            ("app-icon-nightly.ico", "ZZZ Nightly")
        );
        assert_eq!(product_identity("dev"), ("app-icon-dev.ico", "ZZZ Dev"));
        assert_eq!(
            product_identity("unexpected"),
            ("app-icon-dev.ico", "ZZZ Dev")
        );
    }

    #[test]
    fn format_product_version_appends_build_and_commit_metadata_when_present() {
        assert_eq!(
            format_product_version("0.201.0", "preview", Some("42"), Some("abc123")),
            "0.201.0+preview.42.abc123"
        );
        assert_eq!(
            format_product_version("0.201.0", "stable", None, Some("abc123")),
            "0.201.0+stable.abc123"
        );
        assert_eq!(
            format_product_version("0.201.0", "dev", None, None),
            "0.201.0+dev"
        );
    }

    #[test]
    fn file_version_zero_fills_and_sanitizes_invalid_segments() {
        assert_eq!(file_version("1.2.3"), "1,2,3,0");
        assert_eq!(file_version("10.bad.30.40.50"), "10,0,30,40");
        assert_eq!(file_version(""), "0,0,0,0");
    }
}
