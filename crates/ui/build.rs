#![allow(clippy::disallowed_methods, reason = "build scripts are exempt")]

fn main() {
    println!("cargo::rustc-check-cfg=cfg(macos_sdk_26_or_later)");

    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "macos" {
        return;
    }

    println!("cargo:rerun-if-env-changed=SDKROOT");
    if let Some(major) = macos_sdk_major_version()
        && major >= 26
    {
        println!("cargo:rustc-cfg=macos_sdk_26_or_later");
    }
}

fn macos_sdk_major_version() -> Option<u32> {
    if let Ok(sdkroot) = std::env::var("SDKROOT") {
        let sdkroot = sdkroot.trim();
        if !sdkroot.is_empty() {
            return sdk_major_from_path(std::path::Path::new(sdkroot));
        }
    }

    let output = std::process::Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-version"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()?
        .trim()
        .split('.')
        .next()
        .and_then(|v| v.parse().ok())
}

fn sdk_major_from_path(sdkroot: &std::path::Path) -> Option<u32> {
    let name = sdkroot.file_name()?.to_str()?;
    let version = name
        .strip_prefix("MacOSX")?
        .strip_suffix(".sdk")
        .unwrap_or(name);
    version.split('.').next()?.parse().ok()
}
