#![allow(clippy::disallowed_methods, reason = "build scripts are exempt")]
use std::process::Command;

const ZZZ_MANIFEST: &str = include_str!("../zzz/Cargo.toml");

fn main() {
    let zzz_cargo_toml: cargo_toml::Manifest =
        toml::from_str(ZZZ_MANIFEST).expect("failed to parse zzz Cargo.toml");
    println!(
        "cargo:rustc-env=ZZZ_PKG_VERSION={}",
        zzz_cargo_toml.package.unwrap().version.unwrap()
    );
    println!(
        "cargo:rustc-env=TARGET={}",
        std::env::var("TARGET").unwrap()
    );

    // Deterministic builds inject the commit because their source trees may omit .git.
    println!("cargo:rerun-if-changed=../../.git/logs/HEAD");
    println!("cargo:rerun-if-env-changed=ZZZ_COMMIT_SHA");
    if let Some(commit_sha) = commit_sha() {
        println!("cargo:rustc-env=ZZZ_COMMIT_SHA={commit_sha}");
    }
    if let Some(build_identifier) = option_env!("GITHUB_RUN_NUMBER") {
        println!("cargo:rustc-env=ZZZ_BUILD_ID={build_identifier}");
    }
}

fn commit_sha() -> Option<String> {
    if let Ok(commit_sha) = std::env::var("ZZZ_COMMIT_SHA") {
        let commit_sha = commit_sha.trim();
        if !commit_sha.is_empty() {
            return Some(commit_sha.to_owned());
        }
    }

    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let commit_sha = String::from_utf8_lossy(&output.stdout);
    let commit_sha = commit_sha.trim();
    (!commit_sha.is_empty()).then(|| commit_sha.to_owned())
}
