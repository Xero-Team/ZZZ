#![allow(clippy::disallowed_methods, reason = "build scripts are exempt")]

use std::{env, path::PathBuf, process::Command};

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() != "macos" {
        return;
    }

    println!("cargo:rerun-if-changed=src/bindings.h");
    println!("cargo:rerun-if-env-changed=SDKROOT");
    clear_zig_bindgen_args();

    let sdk_path = macos_sdk_path();
    let mut builder = bindgen::Builder::default()
        .header("src/bindings.h")
        .clang_arg("-xobjective-c")
        .clang_arg("-fblocks")
        .clang_arg("-isysroot")
        .clang_arg(&sdk_path)
        .clang_arg("-iframework")
        .clang_arg(format!("{sdk_path}/System/Library/Frameworks"));
    if let Ok(target) = env::var("TARGET") {
        builder = builder.clang_arg(format!("--target={target}"));
    }

    let bindings = builder
        .allowlist_type("CMItemIndex")
        .allowlist_type("CMSampleTimingInfo")
        .allowlist_type("CMVideoCodecType")
        .allowlist_type("VTEncodeInfoFlags")
        .allowlist_function("CMTimeMake")
        .allowlist_var("kCVPixelFormatType_.*")
        .allowlist_var("kCVReturn.*")
        .allowlist_var("VTEncodeInfoFlags_.*")
        .allowlist_var("kCMVideoCodecType_.*")
        .allowlist_var("kCMTime.*")
        .allowlist_var("kCMSampleAttachmentKey_.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .layout_tests(false)
        .generate()
        .expect("unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("couldn't write dispatch bindings");
}

fn clear_zig_bindgen_args() {
    unsafe {
        env::remove_var("BINDGEN_EXTRA_CLANG_ARGS");
        if let Ok(target) = env::var("TARGET") {
            env::remove_var(format!("BINDGEN_EXTRA_CLANG_ARGS_{target}"));
            env::remove_var(format!(
                "BINDGEN_EXTRA_CLANG_ARGS_{}",
                target.replace('-', "_")
            ));
        }
    }
}

fn macos_sdk_path() -> String {
    if let Ok(path) = env::var("SDKROOT") {
        let path = path.trim().to_string();
        if !path.is_empty() {
            return path;
        }
    }

    let output = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .expect("xcrun not found; set SDKROOT for macOS cross-compilation");
    assert!(
        output.status.success(),
        "xcrun --show-sdk-path failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("xcrun sdk path was not utf-8")
        .trim_end()
        .to_string()
}
