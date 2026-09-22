#![allow(clippy::disallowed_methods, reason = "build scripts are exempt")]

use std::env;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

struct RemoteTarget {
    triple: &'static str,
    os: &'static str,
    arch: &'static str,
    windows: bool,
    musl: bool,
}

const TARGETS: &[RemoteTarget] = &[
    RemoteTarget {
        triple: "x86_64-unknown-linux-musl",
        os: "linux",
        arch: "x86_64",
        windows: false,
        musl: true,
    },
    RemoteTarget {
        triple: "aarch64-unknown-linux-musl",
        os: "linux",
        arch: "aarch64",
        windows: false,
        musl: true,
    },
    RemoteTarget {
        triple: "x86_64-apple-darwin",
        os: "macos",
        arch: "x86_64",
        windows: false,
        musl: false,
    },
    RemoteTarget {
        triple: "aarch64-apple-darwin",
        os: "macos",
        arch: "aarch64",
        windows: false,
        musl: false,
    },
    RemoteTarget {
        triple: "x86_64-pc-windows-msvc",
        os: "windows",
        arch: "x86_64",
        windows: true,
        musl: false,
    },
    RemoteTarget {
        triple: "aarch64-pc-windows-msvc",
        os: "windows",
        arch: "aarch64",
        windows: true,
        musl: false,
    },
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=ZZZ_SKIP_EMBED_REMOTE_SERVER");
    println!("cargo:rerun-if-env-changed=ZZZ_EMBED_REMOTE_SERVERS");
    println!("cargo:rerun-if-env-changed=ZZZ_EMBED_REMOTE_SERVER_DIR");
    println!("cargo:rerun-if-env-changed=ZZZ_BUILDING_REMOTE_SERVER");
    println!("cargo:rerun-if-env-changed=SDKROOT");
    println!("cargo:rerun-if-env-changed=PROFILE");
    println!("cargo:rerun-if-env-changed=CLIPPY_ARGS");
    println!("cargo:rerun-if-env-changed=RUSTC_WORKSPACE_WRAPPER");
    println!("cargo:rerun-if-env-changed=RUSTC_WRAPPER");
    println!("cargo:rerun-if-changed=../remote_server/Cargo.toml");
    println!("cargo:rerun-if-changed=../remote_server/src");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let generated = out_dir.join("embedded_remote_servers.rs");

    if skip_embed() {
        write_empty_match(&generated);
        return;
    }

    let mut archives = Vec::new();
    if let Ok(dir) = env::var("ZZZ_EMBED_REMOTE_SERVER_DIR") {
        collect_prebuilt(&PathBuf::from(dir), &out_dir, &mut archives);
    } else {
        build_and_collect(&out_dir, &mut archives);
    }

    write_match(&generated, &archives);
}

fn skip_embed() -> bool {
    if env_truthy("ZZZ_BUILDING_REMOTE_SERVER") || env_truthy("ZZZ_SKIP_EMBED_REMOTE_SERVER") {
        return true;
    }
    if running_under_clippy() {
        return true;
    }
    if env_truthy("ZZZ_EMBED_REMOTE_SERVERS") {
        return false;
    }
    env::var("PROFILE").unwrap_or_default() == "debug"
}

fn running_under_clippy() -> bool {
    env::var_os("CLIPPY_ARGS").is_some()
        || wrapper_is_clippy("RUSTC_WORKSPACE_WRAPPER")
        || wrapper_is_clippy("RUSTC_WRAPPER")
}

fn wrapper_is_clippy(name: &str) -> bool {
    env::var(name).is_ok_and(|wrapper| wrapper.contains("clippy"))
}

fn env_truthy(name: &str) -> bool {
    matches!(
        env::var(name).ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

fn collect_prebuilt(dir: &Path, out_dir: &Path, archives: &mut Vec<Archive>) {
    println!("cargo:rerun-if-changed={}", dir.display());
    for target in TARGETS {
        let ext = if target.windows { "zip" } else { "gz" };
        let name = format!("zzz-remote-server-{}-{}.{}", target.os, target.arch, ext);
        let src = dir.join(&name);
        if src.is_file() {
            match copy_archive(&src, out_dir, target.os, target.arch, ext) {
                Ok(archive) => archives.push(archive),
                Err(error) => println!("cargo:warning=skipping {}: {error}", src.display()),
            }
        }
    }
}

fn build_and_collect(out_dir: &Path, archives: &mut Vec<Archive>) {
    let workspace = workspace_dir();
    let host = env::var("HOST").unwrap_or_default();
    for target in TARGETS {
        match build_target(&workspace, target, &host) {
            Ok(binary) => match compress_binary(&binary, out_dir, target) {
                Ok(archive) => archives.push(archive),
                Err(error) => println!(
                    "cargo:warning=failed to compress remote_server for {}: {error}",
                    target.triple
                ),
            },
            Err(error) => {
                if target.os == "linux" && target.arch == "x86_64" {
                    match build_linux_gnu(&workspace, "x86_64-unknown-linux-gnu") {
                        Ok(binary) => match compress_binary(&binary, out_dir, target) {
                            Ok(archive) => {
                                println!(
                                    "cargo:warning=musl remote_server failed ({error}); embedded glibc linux-x86_64 instead"
                                );
                                archives.push(archive);
                            }
                            Err(compress_error) => {
                                println!(
                                    "cargo:warning=failed to compress gnu remote_server: {compress_error}"
                                );
                            }
                        },
                        Err(gnu_error) => {
                            println!(
                                "cargo:warning=failed to embed host remote_server ({}: {error}; gnu: {gnu_error})",
                                target.triple
                            );
                        }
                    }
                } else {
                    println!(
                        "cargo:warning=skipping remote_server for {}: {error}",
                        target.triple
                    );
                }
            }
        }
    }
}

fn build_target(workspace: &Path, target: &RemoteTarget, host: &str) -> Result<PathBuf, String> {
    rustup_target_add(target.triple);

    let target_dir = workspace.join("target").join("remote_server");
    let mut rustflags = env::var("ZZZ_REMOTE_SERVER_RUSTFLAGS").unwrap_or_default();
    if target.musl && !rustflags.contains("target-feature=+crt-static") {
        rustflags.push_str(" -C target-feature=+crt-static");
    }
    let cross = target.triple != host && !target.triple.starts_with(&host_arch(host));
    let subcommand = if cross && which("cargo-zigbuild") && which("zig") {
        "zigbuild"
    } else {
        "build"
    };
    let mut command = cargo_command(workspace, subcommand, target, &target_dir, rustflags.trim());
    if subcommand == "zigbuild" {
        apply_tmpfs_zig_cache(&mut command);
        invalidate_corrupt_aws_lc_sys(&target_dir, target.triple);
    }
    if target.musl
        && let Some(cc) = musl_cc(target.triple)
    {
        command.env(format!("CC_{}", target.triple.replace('-', "_")), cc);
    }
    if target.os == "macos" {
        command.env("SDKROOT", ensure_macos_sdk(workspace)?);
    }

    let status = command
        .status()
        .map_err(|error| format!("failed to spawn cargo: {error}"))?;
    if !status.success() {
        return Err(format!("cargo exited with {status}"));
    }

    let binary_name = if target.windows {
        "remote_server.exe"
    } else {
        "remote_server"
    };
    let binary = target_dir
        .join(target.triple)
        .join("release")
        .join(binary_name);
    if !binary.is_file() {
        return Err(format!("missing {}", binary.display()));
    }
    Ok(binary)
}

fn compress_binary(
    binary: &Path,
    out_dir: &Path,
    target: &RemoteTarget,
) -> Result<Archive, String> {
    let ext = if target.windows { "zip" } else { "gz" };
    let dest = out_dir.join(format!("{}-{}.{ext}", target.os, target.arch));
    if target.windows {
        zip_file(binary, &dest)?;
    } else {
        gzip_file(binary, &dest)?;
    }
    Ok(Archive {
        os: target.os,
        arch: target.arch,
        ext,
        path: dest,
    })
}

fn gzip_file(src: &Path, dest: &Path) -> Result<(), String> {
    let input = fs::File::open(src).map_err(|error| error.to_string())?;
    let output = fs::File::create(dest).map_err(|error| error.to_string())?;
    let mut encoder = flate2::write::GzEncoder::new(output, flate2::Compression::best());
    let mut reader = std::io::BufReader::new(input);
    std::io::copy(&mut reader, &mut encoder).map_err(|error| error.to_string())?;
    encoder.finish().map_err(|error| error.to_string())?;
    Ok(())
}

fn zip_file(src: &Path, dest: &Path) -> Result<(), String> {
    let file_name = src
        .file_name()
        .ok_or_else(|| format!("missing file name for {}", src.display()))?
        .to_string_lossy();
    let output = fs::File::create(dest).map_err(|error| error.to_string())?;
    let mut zip = zip::ZipWriter::new(output);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(file_name.as_ref(), options)
        .map_err(|error| error.to_string())?;
    let mut input = fs::File::open(src).map_err(|error| error.to_string())?;
    std::io::copy(&mut input, &mut zip).map_err(|error| error.to_string())?;
    zip.finish().map_err(|error| error.to_string())?;
    Ok(())
}

fn copy_archive(
    src: &Path,
    out_dir: &Path,
    os: &'static str,
    arch: &'static str,
    ext: &'static str,
) -> Result<Archive, String> {
    let dest = out_dir.join(format!("{os}-{arch}.{ext}"));
    fs::copy(src, &dest).map_err(|error| error.to_string())?;
    Ok(Archive {
        os,
        arch,
        ext,
        path: dest,
    })
}

fn ensure_macos_sdk(workspace: &Path) -> Result<PathBuf, String> {
    let script = workspace.join("script/ensure-macos-sdk");
    let output = Command::new(&script)
        .output()
        .map_err(|error| format!("failed to spawn {}: {error}", script.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} failed: {}",
            script.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let path = String::from_utf8(output.stdout)
        .map_err(|error| error.to_string())?
        .trim()
        .to_string();
    if path.is_empty() {
        return Err("ensure-macos-sdk printed no path".into());
    }
    Ok(PathBuf::from(path))
}

fn rustup_target_add(triple: &str) {
    if !which("rustup") {
        return;
    }
    let _ = Command::new("rustup")
        .args(["target", "add", triple])
        .status();
}

fn musl_cc(triple: &str) -> Option<PathBuf> {
    let arch = triple.split('-').next().unwrap_or("");
    which_path(&format!("{arch}-linux-musl-gcc")).or_else(|| {
        let host = env::var("HOST").unwrap_or_default();
        if host.starts_with(arch) {
            which_path("musl-gcc")
        } else {
            None
        }
    })
}

fn host_arch(host: &str) -> String {
    host.split('-').next().unwrap_or(host).to_string()
}

fn zig_cache_root() -> PathBuf {
    #[cfg(unix)]
    {
        let tmp = PathBuf::from("/tmp");
        if tmp.is_dir() {
            return tmp.join("zzz-zig-cache");
        }
    }
    env::temp_dir().join("zzz-zig-cache")
}

fn apply_tmpfs_zig_cache(command: &mut Command) {
    let cache = env::var_os("ZIG_GLOBAL_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(zig_cache_root);
    let local = env::var_os("ZIG_LOCAL_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| cache.join("local"));
    if let Err(error) = fs::create_dir_all(&local) {
        println!(
            "cargo:warning=failed to create zig cache {}: {error}",
            local.display()
        );
    }
    command
        .env("ZIG_GLOBAL_CACHE_DIR", &cache)
        .env("ZIG_LOCAL_CACHE_DIR", &local);
}

fn object_missing_named_symbols(path: &Path) -> bool {
    let Ok(output) = Command::new("nm").arg(path).output() else {
        return false;
    };
    if !output.status.success() {
        return true;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .all(|line| !line.contains(" T ") && !line.contains(" t "))
}

fn invalidate_corrupt_aws_lc_sys(target_dir: &Path, triple: &str) {
    let profile_dir = target_dir.join(triple).join("release");
    let build_dir = profile_dir.join("build");
    let Ok(entries) = fs::read_dir(&build_dir) else {
        return;
    };
    let mut corrupt = false;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("aws-lc-sys-") {
            continue;
        }
        let out = entry.path().join("out");
        let Ok(objects) = fs::read_dir(out) else {
            continue;
        };
        if objects.flatten().any(|object| {
            object
                .file_name()
                .to_string_lossy()
                .ends_with("bignum_sqr.o")
                && object_missing_named_symbols(&object.path())
        }) {
            corrupt = true;
            if let Err(error) = fs::remove_dir_all(entry.path()) {
                println!(
                    "cargo:warning=failed to remove corrupt aws-lc-sys build {}: {error}",
                    entry.path().display()
                );
            }
        }
    }
    if !corrupt {
        return;
    }
    println!("cargo:warning=removed corrupt aws-lc-sys objects for {triple}");
    for dir_name in [".fingerprint", "deps"] {
        let Ok(entries) = fs::read_dir(profile_dir.join(dir_name)) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("aws-lc-sys-") || name.starts_with("libaws_lc") {
                let path = entry.path();
                let result = if path.is_dir() {
                    fs::remove_dir_all(&path)
                } else {
                    fs::remove_file(&path)
                };
                if let Err(error) = result {
                    println!("cargo:warning=failed to remove {}: {error}", path.display());
                }
            }
        }
    }
}

fn cargo_command(
    workspace: &Path,
    subcommand: &str,
    target: &RemoteTarget,
    target_dir: &Path,
    rustflags: &str,
) -> Command {
    let mut command = Command::new("cargo");
    command
        .current_dir(workspace)
        .args([
            subcommand,
            "--release",
            "--package",
            "remote_server",
            "--target",
            target.triple,
            "--target-dir",
        ])
        .arg(target_dir)
        .env("ZZZ_BUILDING_REMOTE_SERVER", "1")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env("RUSTFLAGS", rustflags);
    command
}

fn build_linux_gnu(workspace: &Path, triple: &str) -> Result<PathBuf, String> {
    rustup_target_add(triple);
    let target_dir = workspace.join("target").join("remote_server");
    let status = Command::new("cargo")
        .current_dir(workspace)
        .args([
            "build",
            "--release",
            "--package",
            "remote_server",
            "--target",
            triple,
            "--target-dir",
        ])
        .arg(&target_dir)
        .env("ZZZ_BUILDING_REMOTE_SERVER", "1")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .status()
        .map_err(|error| format!("failed to spawn cargo: {error}"))?;
    if !status.success() {
        return Err(format!("cargo exited with {status}"));
    }
    let binary = target_dir
        .join(triple)
        .join("release")
        .join("remote_server");
    if !binary.is_file() {
        return Err(format!("missing {}", binary.display()));
    }
    Ok(binary)
}

fn which(name: &str) -> bool {
    which_path(name).is_some()
}

fn which_path(name: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths).find_map(|dir| {
            let candidate = dir.join(name);
            candidate.is_file().then_some(candidate)
        })
    })
}

fn workspace_dir() -> PathBuf {
    PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace dir")
}

struct Archive {
    os: &'static str,
    arch: &'static str,
    ext: &'static str,
    path: PathBuf,
}

fn write_empty_match(path: &Path) {
    write_match(path, &[]);
}

fn write_match(path: &Path, archives: &[Archive]) {
    let mut file = fs::File::create(path).expect("create embedded_remote_servers.rs");
    if archives.is_empty() {
        writeln!(
            file,
            "{{\n    let _ = (os, arch);\n    None::<(&'static [u8], &'static str)>\n}}"
        )
        .expect("write");
        return;
    }
    writeln!(file, "match (os, arch) {{").expect("write");
    for archive in archives {
        let include_path = archive.path.display().to_string().replace('\\', "/");
        writeln!(
            file,
            "    ({:?}, {:?}) => Some((include_bytes!({include_path:?}), {:?})),",
            archive.os, archive.arch, archive.ext
        )
        .expect("write");
    }
    writeln!(file, "    _ => None,\n}}").expect("write");
}
