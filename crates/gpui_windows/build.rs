#![allow(clippy::disallowed_methods, reason = "build scripts are exempt")]

fn main() {
    if targeting_windows() && !target_debug_assertions() {
        compile_shaders();
    }
}

fn targeting_windows() -> bool {
    std::env::var("CARGO_CFG_TARGET_OS").ok().as_deref() == Some("windows")
}

fn target_debug_assertions() -> bool {
    std::env::var("CARGO_CFG_DEBUG_ASSERTIONS").is_ok()
}

mod shader_compilation {
    use std::{
        fs,
        io::Write,
        path::{Path, PathBuf},
        process::{self, Command},
    };

    pub fn compile_shaders() {
        let shader_path =
            PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("src/shaders.hlsl");
        let out_dir = std::env::var("OUT_DIR").unwrap();

        println!("cargo:rerun-if-changed={}", shader_path.display());
        println!("cargo:rerun-if-env-changed=GPUI_FXC_PATH");
        println!("cargo:rerun-if-env-changed=MSVC_ROOT");

        let fxc_path = find_fxc_compiler();

        let modules = [
            "quad",
            "shadow",
            "path_rasterization",
            "path_sprite",
            "underline",
            "monochrome_sprite",
            "subpixel_sprite",
            "polychrome_sprite",
        ];

        let rust_binding_path = format!("{}/shaders_bytes.rs", out_dir);
        if Path::new(&rust_binding_path).exists() {
            fs::remove_file(&rust_binding_path)
                .expect("Failed to remove existing Rust binding file");
        }
        for module in modules {
            compile_shader_for_module(
                module,
                &out_dir,
                &fxc_path,
                shader_path.to_str().unwrap(),
                &rust_binding_path,
            );
        }

        {
            let shader_path = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
                .join("src/color_text_raster.hlsl");
            compile_shader_for_module(
                "emoji_rasterization",
                &out_dir,
                &fxc_path,
                shader_path.to_str().unwrap(),
                &rust_binding_path,
            );
        }
    }

    #[cfg(windows)]
    pub fn find_latest_windows_sdk_binary(
        binary: &str,
    ) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
        let key = windows_registry::LOCAL_MACHINE
            .open("SOFTWARE\\WOW6432Node\\Microsoft\\Microsoft SDKs\\Windows\\v10.0")?;

        let install_folder: String = key.get_string("InstallationFolder")?;
        let install_folder_bin = Path::new(&install_folder).join("bin");

        let mut versions: Vec<_> = std::fs::read_dir(&install_folder_bin)?
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();

        versions.sort_by_key(|s| {
            s.split('.')
                .filter_map(|p| p.parse().ok())
                .collect::<Vec<u32>>()
        });

        let arch = match std::env::consts::ARCH {
            "x86_64" => "x64",
            "aarch64" => "arm64",
            _ => Err(format!(
                "Unsupported architecture: {}",
                std::env::consts::ARCH
            ))?,
        };

        if let Some(highest_version) = versions.last() {
            return Ok(Some(
                install_folder_bin
                    .join(highest_version)
                    .join(arch)
                    .join(binary),
            ));
        }

        Ok(None)
    }

    fn find_fxc_compiler() -> String {
        if let Ok(path) = std::env::var("GPUI_FXC_PATH")
            && Path::new(&path).exists()
        {
            return path;
        }

        #[cfg(windows)]
        {
            if let Ok(output) = std::process::Command::new("where.exe")
                .arg("fxc.exe")
                .output()
                && output.status.success()
            {
                let path = String::from_utf8_lossy(&output.stdout);
                return path.trim().to_owned();
            }

            if let Ok(Some(path)) = find_latest_windows_sdk_binary("fxc.exe") {
                return path.to_string_lossy().into_owned();
            }
        }

        #[cfg(not(windows))]
        {
            if let Some(path) = find_on_path("fxc.exe").or_else(|| find_on_path("fxc")) {
                return path;
            }
            if let Some(path) = find_fxc_in_msvc_sysroot() {
                return path;
            }
        }

        panic!("Failed to find fxc.exe");
    }

    #[cfg(not(windows))]
    fn find_on_path(name: &str) -> Option<String> {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path).find_map(|dir| {
            let candidate = dir.join(name);
            candidate
                .is_file()
                .then(|| candidate.to_string_lossy().into_owned())
        })
    }

    #[cfg(not(windows))]
    fn find_fxc_in_msvc_sysroot() -> Option<String> {
        let mut roots = Vec::new();
        if let Ok(root) = std::env::var("MSVC_ROOT") {
            roots.push(PathBuf::from(root));
        }
        roots.push(PathBuf::from("/opt/msvc"));

        let kit_bins = ["Windows Kits/10/bin", "kits/10/bin"];
        let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
        let arches: &[&str] = if target_arch == "aarch64" {
            &["arm64", "x64"]
        } else {
            &["x64", "arm64"]
        };

        for root in roots {
            for kit_bin in kit_bins {
                let bin_root = root.join(kit_bin);
                let Ok(entries) = fs::read_dir(&bin_root) else {
                    continue;
                };
                let mut versions: Vec<_> = entries
                    .flatten()
                    .filter(|entry| entry.path().is_dir())
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .collect();
                versions.sort_by_key(|s| {
                    s.split('.')
                        .filter_map(|p| p.parse::<u32>().ok())
                        .collect::<Vec<u32>>()
                });
                for version in versions.iter().rev() {
                    for arch in arches {
                        let candidate = bin_root.join(version).join(arch).join("fxc.exe");
                        if candidate.is_file() {
                            return Some(candidate.to_string_lossy().into_owned());
                        }
                    }
                }
            }
        }
        None
    }

    fn compile_shader_for_module(
        module: &str,
        out_dir: &str,
        fxc_path: &str,
        shader_path: &str,
        rust_binding_path: &str,
    ) {
        let output_file = format!("{}/{}_vs.h", out_dir, module);
        let const_name = format!("{}_VERTEX_BYTES", module.to_uppercase());
        compile_shader_impl(
            fxc_path,
            &format!("{module}_vertex"),
            &output_file,
            &const_name,
            shader_path,
            "vs_4_1",
        );
        generate_rust_binding(&const_name, &output_file, rust_binding_path);

        let output_file = format!("{}/{}_ps.h", out_dir, module);
        let const_name = format!("{}_FRAGMENT_BYTES", module.to_uppercase());
        compile_shader_impl(
            fxc_path,
            &format!("{module}_fragment"),
            &output_file,
            &const_name,
            shader_path,
            "ps_4_1",
        );
        generate_rust_binding(&const_name, &output_file, rust_binding_path);
    }

    fn fxc_command(fxc_path: &str) -> Command {
        #[cfg(windows)]
        {
            Command::new(fxc_path)
        }
        #[cfg(not(windows))]
        {
            let path = Path::new(fxc_path);
            if path.extension().is_none_or(|ext| ext != "exe") {
                return Command::new(fxc_path);
            }
            if let Some(wrapper) = wine_msvc_wrapper() {
                let mut command = Command::new(wrapper);
                command.arg(fxc_path);
                command
            } else {
                let mut command = Command::new("wine");
                command.arg(fxc_path);
                command
            }
        }
    }

    #[cfg(not(windows))]
    fn wine_msvc_wrapper() -> Option<PathBuf> {
        find_on_path("cl").and_then(|cl| {
            let wrapper = Path::new(&cl).parent()?.join("wine-msvc.sh");
            wrapper.is_file().then_some(wrapper)
        })
    }

    fn compile_shader_impl(
        fxc_path: &str,
        entry_point: &str,
        output_path: &str,
        var_name: &str,
        shader_path: &str,
        target: &str,
    ) {
        let output = fxc_command(fxc_path)
            .args([
                "/T",
                target,
                "/E",
                entry_point,
                "/Fh",
                output_path,
                "/Vn",
                var_name,
                "/O3",
                shader_path,
            ])
            .output();

        match output {
            Ok(result) => {
                if result.status.success() {
                    return;
                }
                println!(
                    "cargo::error=Shader compilation failed for {}:\n{}",
                    entry_point,
                    String::from_utf8_lossy(&result.stderr)
                );
                process::exit(1);
            }
            Err(e) => {
                println!("cargo::error=Failed to run fxc for {}: {}", entry_point, e);
                process::exit(1);
            }
        }
    }

    fn generate_rust_binding(const_name: &str, head_file: &str, output_path: &str) {
        let header_content = fs::read_to_string(head_file).expect("Failed to read header file");
        let const_definition = {
            let global_var_start = header_content.find("const BYTE").unwrap();
            let global_var = &header_content[global_var_start..];
            let equal = global_var.find('=').unwrap();
            global_var[equal + 1..].trim()
        };
        let rust_binding = format!(
            "const {}: &[u8] = &{}\n",
            const_name,
            const_definition.replace('{', "[").replace('}', "]")
        );
        let mut options = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(output_path)
            .expect("Failed to open Rust binding file");
        options
            .write_all(rust_binding.as_bytes())
            .expect("Failed to write Rust binding file");
    }
}

use shader_compilation::compile_shaders;
