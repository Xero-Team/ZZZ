#![allow(clippy::disallowed_methods, reason = "build scripts are exempt")]

fn main() {
    println!("cargo::rustc-check-cfg=cfg(gpui_cross_runtime_shaders)");
    // Build scripts run on the host, so the target platform must come from
    // Cargo's environment rather than `cfg!`.
    if matches!(
        std::env::var("CARGO_CFG_TARGET_OS").as_deref(),
        Ok("macos" | "ios")
    ) {
        apple_build::run();
    }
}

mod apple_build {
    use std::{
        env,
        ffi::OsString,
        path::{Path, PathBuf},
        process::{self, Command},
    };

    use cbindgen::Config;

    pub fn run() {
        let header_path = generate_shader_bindings();
        let can_compile_metal = metal_compiler_available();
        if can_compile_metal {
            compile_metal_shaders(&header_path);
        } else {
            println!("cargo:rustc-cfg=gpui_cross_runtime_shaders");
        }
        // The runtime-shader feature and the cross-compilation fallback both
        // read this file, so stage it even when a metallib was also produced.
        emit_stitched_shaders(&header_path);
    }

    fn generate_shader_bindings() -> PathBuf {
        let output_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("scene.h");

        let gpui_dir = find_gpui_crate_dir();

        let mut config = Config {
            include_guard: Some("SCENE_H".into()),
            language: cbindgen::Language::C,
            no_includes: true,
            ..Default::default()
        };
        config.export.include.extend([
            "Bounds".into(),
            "Corners".into(),
            "Edges".into(),
            "Size".into(),
            "Pixels".into(),
            "PointF".into(),
            "Hsla".into(),
            "ContentMask".into(),
            "Uniforms".into(),
            "AtlasTile".into(),
            "PathRasterizationInputIndex".into(),
            "PathVertex_ScaledPixels".into(),
            "PathRasterizationVertex".into(),
            "ShadowInputIndex".into(),
            "Shadow".into(),
            "QuadInputIndex".into(),
            "Underline".into(),
            "UnderlineInputIndex".into(),
            "Quad".into(),
            "BorderStyle".into(),
            "SpriteInputIndex".into(),
            "MonochromeSprite".into(),
            "PolychromeSprite".into(),
            "PathSprite".into(),
            "SurfaceInputIndex".into(),
            "SurfaceBounds".into(),
            "TransformationMatrix".into(),
        ]);
        config.no_includes = true;
        config.enumeration.prefix_with_name = true;

        let mut builder = cbindgen::Builder::new();

        let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

        // cbindgen's with_src does not follow window.rs's `mod` declarations into
        // window/. List those files explicitly so ContentMask resolves to a
        // concrete struct instead of an unresolved `ContentMask<ScaledPixels>`.
        let gpui_src_paths = [
            gpui_dir.join("src/atlas.rs"),
            gpui_dir.join("src/scene.rs"),
            gpui_dir.join("src/geometry.rs"),
            gpui_dir.join("src/color.rs"),
            gpui_dir.join("src/window.rs"),
            gpui_dir.join("src/window/content_mask.rs"),
            gpui_dir.join("src/platform.rs"),
        ];

        let local_src_paths = [crate_dir.join("src/metal_renderer.rs")];

        for src_path in gpui_src_paths.iter().chain(local_src_paths.iter()) {
            println!("cargo:rerun-if-changed={}", src_path.display());
            builder = builder.with_src(src_path);
        }

        builder
            .with_config(config)
            .generate()
            .expect("Unable to generate bindings")
            .write_to_file(&output_path);

        output_path
    }

    fn find_gpui_crate_dir() -> PathBuf {
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../gpui")
    }

    /// Resolves the SDK and minimum-version flag for the Metal toolchain.
    ///
    /// Build scripts run on the host, so the target platform must come from
    /// Cargo's environment rather than `cfg!`. The target environment, not the
    /// target name, identifies simulators: `x86_64-apple-ios` has no `-sim`
    /// suffix.
    fn metal_toolchain() -> Option<(OsString, &'static str)> {
        let target_os = env::var("CARGO_CFG_TARGET_OS").ok()?;
        let target_env = env::var("CARGO_CFG_TARGET_ENV").ok()?;
        let (sdk, minimum_version_argument) = match (target_os.as_str(), target_env.as_str()) {
            ("macos", _) => ("macosx", "-mmacosx-version-min=12.0"),
            ("ios", "sim") => ("iphonesimulator", "-mios-simulator-version-min=15.0"),
            ("ios", "") => ("iphoneos", "-mios-version-min=15.0"),
            _ => return None,
        };

        let sdk = if target_os == "macos" {
            env::var_os("SDKROOT").unwrap_or_else(|| OsString::from(sdk))
        } else {
            OsString::from(sdk)
        };
        Some((sdk, minimum_version_argument))
    }

    /// The metal toolchain only exists with the Apple developer tools that ship
    /// `xcrun`; without it the renderer falls back to compiling the stitched
    /// shader source at runtime.
    fn metal_compiler_available() -> bool {
        let Some((sdk, _)) = metal_toolchain() else {
            return false;
        };
        Command::new("xcrun")
            .arg("-sdk")
            .arg(&sdk)
            .args(["-f", "metal"])
            .output()
            .is_ok_and(|output| output.status.success())
    }

    fn emit_stitched_shaders(header_path: &Path) {
        let shader_source_path = "./src/shaders.metal";
        let header_contents = std::fs::read_to_string(header_path).unwrap();
        let shader_contents = std::fs::read_to_string(shader_source_path).unwrap();
        let stitched_contents = format!("{header_contents}\n{shader_contents}");
        let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("stitched_shaders.metal");
        std::fs::write(&out_path, stitched_contents).unwrap();
        println!("cargo:rerun-if-changed={shader_source_path}");
    }

    fn compile_metal_shaders(header_path: &Path) {
        println!("cargo:rerun-if-env-changed=SDKROOT");
        println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");

        let Some((sdk, minimum_version_argument)) = metal_toolchain() else {
            println!("cargo::error=unsupported Metal shader target");
            process::exit(1);
        };

        let shader_path = "./src/shaders.metal";
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let air_output_path = out_dir.join("shaders.air");
        let metallib_output_path = out_dir.join("shaders.metallib");
        println!("cargo:rerun-if-changed={}", shader_path);

        // The metal compiler records the resolved absolute path of its input
        // unconditionally. Compile a copy staged in OUT_DIR so the recorded
        // location is the build's canonical output directory, never the checkout.
        let staged_shader_path = out_dir.join("shaders.metal");
        std::fs::copy(shader_path, &staged_shader_path).unwrap();

        // When targeting macOS 12+, both `metal` and `metallib` also record their
        // working directory in the metallib. Run them from OUT_DIR so that path
        // isn't the checkout either.
        let output = Command::new("xcrun")
            .current_dir(&out_dir)
            .arg("-sdk")
            .arg(&sdk)
            .args([
                "metal",
                "-gline-tables-only",
                minimum_version_argument,
                "-MO",
                "-c",
            ])
            .arg(&staged_shader_path)
            .args(["-include", header_path.to_str().unwrap(), "-o"])
            .arg(&air_output_path)
            .output()
            .unwrap();

        if !output.status.success() {
            println!(
                "cargo::error=metal shader compilation failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            process::exit(1);
        }

        let output = Command::new("xcrun")
            .current_dir(&out_dir)
            .arg("-sdk")
            .arg(&sdk)
            .arg("metallib")
            .arg(&air_output_path)
            .arg("-o")
            .arg(&metallib_output_path)
            .output()
            .unwrap();

        if !output.status.success() {
            println!(
                "cargo::error=metallib compilation failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            process::exit(1);
        }

        // Nothing reads the .air intermediate after this point.
        std::fs::remove_file(&air_output_path).unwrap();
    }
}
