//! `cargo xtask doctor`: one place to answer "why can't this machine build a
//! release".
//!
//! Each check reports what was found, whether the default release path needs it,
//! and the command that fixes it. Missing optional tools never fail the command;
//! `--strict` turns missing required tools into an error so a host can be
//! validated before a long build.

#![allow(
    clippy::disallowed_methods,
    reason = "doctor probes external tools; xtask has no UI thread"
)]

use std::path::Path;
use std::process::Command;

use anyhow::{Result, bail};
use clap::Args;
use serde::Serialize;

use crate::tasks::build::container::{TOOLCHAIN_DOCKERFILE, image_tag, with_msvc};
use crate::tasks::build::model::Platform;

#[derive(Args)]
pub struct DoctorArgs {
    /// Fail when a tool the default release path needs is missing.
    #[arg(long)]
    strict: bool,
    /// Print the report as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Serialize)]
struct Report {
    checks: Vec<Check>,
    missing_required: Vec<String>,
}

#[derive(Serialize)]
struct Check {
    name: String,
    category: &'static str,
    required: bool,
    status: Status,
    detail: String,
    remedy: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Status {
    Ok,
    Missing,
    Warning,
}

pub fn run_doctor(args: DoctorArgs) -> Result<()> {
    let workspace = std::env::current_dir()?;
    let mut checks = Vec::new();

    check_tool(
        &mut checks,
        "git",
        "toolchain",
        true,
        &["git", "--version"],
        "install git",
    );

    check_tool(
        &mut checks,
        "rustc",
        "toolchain",
        true,
        &["rustc", "--version"],
        "install the toolchain from rust-toolchain.toml with rustup",
    );
    check_toolchain_channel(&mut checks, &workspace);
    check_tool(
        &mut checks,
        "cargo",
        "toolchain",
        true,
        &["cargo", "--version"],
        "install rustup",
    );

    check_container(&mut checks, &workspace);
    check_tree_sitter(&mut checks, &workspace);
    // Resolved the same way the PGO steps resolve it: the copy shipped with
    // the active toolchain wins over whatever is on PATH, because a merger from
    // a different LLVM major version rejects the raw profiles.
    match crate::tasks::build::pgo::find_llvm_profdata() {
        Ok(path) => checks.push(Check {
            name: "llvm-profdata".to_owned(),
            category: "pgo",
            required: true,
            status: Status::Ok,
            detail: path.display().to_string(),
            remedy: None,
        }),
        Err(error) => checks.push(Check {
            name: "llvm-profdata".to_owned(),
            category: "pgo",
            required: true,
            status: Status::Missing,
            detail: error.to_string(),
            remedy: Some("rustup component add llvm-tools-preview".to_owned()),
        }),
    }
    // `xvfb-run` has no `--version` and exits non-zero for `--help`, so
    // presence on `PATH` is the only honest check. The default release path
    // trains inside the toolchain image, which already ships it, so only a
    // `--container never` training run needs the host copy.
    check_binary(
        &mut checks,
        "xvfb-run",
        "pgo",
        false,
        "xvfb-run",
        "apt-get install xvfb (only needed when PGO runs without the container)",
    );
    check_tool(
        &mut checks,
        "xdotool",
        "pgo",
        false,
        &["xdotool", "--version"],
        "apt-get install xdotool",
    );

    check_tool(
        &mut checks,
        "zig",
        "cross",
        false,
        &["zig", "version"],
        "install zig and put it on PATH",
    );
    check_tool(
        &mut checks,
        "cargo-zigbuild",
        "cross",
        false,
        &["cargo-zigbuild", "--version"],
        "cargo install cargo-zigbuild",
    );
    check_msvc_wine(&mut checks);
    check_dmg_toolchain(&mut checks);
    check_cargo_bundle(&mut checks);
    check_macos_sdk(&mut checks);
    check_freebsd_toolchain(&mut checks);

    check_tool(
        &mut checks,
        "cargo-deb",
        "packaging",
        false,
        &["cargo-deb", "--version"],
        "cargo install cargo-deb --version 3.8.0 --locked",
    );
    check_tool(
        &mut checks,
        "cargo-generate-rpm",
        "packaging",
        false,
        &["cargo-generate-rpm", "--version"],
        "cargo install cargo-generate-rpm --version 0.21.0 --locked",
    );
    check_tool(
        &mut checks,
        "flatpak-builder",
        "packaging",
        false,
        &["flatpak-builder", "--version"],
        "apt-get install flatpak-builder",
    );
    check_tool(
        &mut checks,
        "snapcraft",
        "packaging",
        false,
        &["snapcraft", "--version"],
        "snap install snapcraft --classic",
    );

    let missing_required = checks
        .iter()
        .filter(|check| check.required && check.status == Status::Missing)
        .map(|check| check.name.clone())
        .collect::<Vec<_>>();

    if args.json {
        let report = Report {
            checks,
            missing_required: missing_required.clone(),
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|error| anyhow::anyhow!(error))?
        );
    } else {
        print_report(&checks);
        println!();
        if missing_required.is_empty() {
            println!("all required tools are present");
        } else {
            println!("missing required tools: {}", missing_required.join(", "));
        }
    }

    if args.strict && !missing_required.is_empty() {
        bail!("missing required tools: {}", missing_required.join(", "));
    }
    Ok(())
}

fn print_report(checks: &[Check]) {
    let width = checks
        .iter()
        .map(|check| check.name.len())
        .max()
        .unwrap_or(0);
    for check in checks {
        let marker = match check.status {
            Status::Ok => "ok",
            Status::Missing if check.required => "MISSING",
            Status::Missing => "missing",
            Status::Warning => "warn",
        };
        println!(
            "{:<width$}  {:<6}  {:<10}  {}",
            check.name,
            marker,
            check.category,
            check.detail,
            width = width
        );
        if let Some(remedy) = &check.remedy {
            if check.status != Status::Ok {
                println!("{:<width$}  fix: {remedy}", "", width = width);
            }
        }
    }
}

fn check_tool(
    checks: &mut Vec<Check>,
    name: &str,
    category: &'static str,
    required: bool,
    command: &[&str],
    remedy: &str,
) {
    let Some((program, arguments)) = command.split_first() else {
        return;
    };
    match probe(program, arguments) {
        Some(version) => checks.push(Check {
            name: name.to_owned(),
            category,
            required,
            status: Status::Ok,
            detail: version,
            remedy: None,
        }),
        None => checks.push(Check {
            name: name.to_owned(),
            category,
            required,
            status: Status::Missing,
            detail: format!("`{program}` not found"),
            remedy: Some(remedy.to_owned()),
        }),
    }
}

fn check_binary(
    checks: &mut Vec<Check>,
    name: &str,
    category: &'static str,
    required: bool,
    binary: &str,
    remedy: &str,
) {
    match on_path(binary) {
        Some(path) => checks.push(Check {
            name: name.to_owned(),
            category,
            required,
            status: Status::Ok,
            detail: path.display().to_string(),
            remedy: None,
        }),
        None => checks.push(Check {
            name: name.to_owned(),
            category,
            required,
            status: Status::Missing,
            detail: format!("`{binary}` not found on PATH"),
            remedy: Some(remedy.to_owned()),
        }),
    }
}

fn on_path(binary: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(binary))
        .find(|candidate| candidate.is_file())
}

/// The bundle scripts refuse a Tree-sitter CLI other than the one pinned by the
/// vendored grammar manifest, because generated parsers are not stable across
/// releases. Checking the same pin here turns that failure into a report.
fn check_tree_sitter(checks: &mut Vec<Check>, workspace: &Path) {
    let manifest = workspace.join("crates/grammars/vendor/manifest.toml");
    let pinned = std::fs::read_to_string(&manifest)
        .ok()
        .and_then(|source| toml::from_str::<toml::Value>(&source).ok())
        .and_then(|manifest| {
            manifest
                .get("tree-sitter-cli-version")
                .and_then(toml::Value::as_str)
                .map(str::to_owned)
        });

    let Some(pinned) = pinned else {
        checks.push(Check {
            name: "tree-sitter".to_owned(),
            category: "toolchain",
            required: true,
            status: Status::Warning,
            detail: format!("could not read the pin from {}", manifest.display()),
            remedy: None,
        });
        return;
    };

    let remedy = format!("cargo install --locked --version {pinned} tree-sitter-cli");
    let actual = probe("tree-sitter", &["--version"]);
    let installed = actual
        .as_deref()
        .and_then(|line| line.split_whitespace().nth(1))
        .map(str::to_owned);

    checks.push(Check {
        name: "tree-sitter".to_owned(),
        category: "toolchain",
        required: true,
        status: if installed.as_deref() == Some(pinned.as_str()) {
            Status::Ok
        } else if actual.is_some() {
            Status::Warning
        } else {
            Status::Missing
        },
        detail: match &actual {
            Some(line) => format!("want {pinned}, have {line}"),
            None => format!("`tree-sitter` not found; want {pinned}"),
        },
        remedy: (installed.as_deref() != Some(pinned.as_str())).then_some(remedy),
    });
}

fn check_toolchain_channel(checks: &mut Vec<Check>, workspace: &Path) {
    let path = workspace.join("rust-toolchain.toml");
    let Ok(source) = std::fs::read_to_string(&path) else {
        checks.push(Check {
            name: "rust-toolchain".to_owned(),
            category: "toolchain",
            required: true,
            status: Status::Missing,
            detail: format!("{} is missing", path.display()),
            remedy: Some("restore rust-toolchain.toml".to_owned()),
        });
        return;
    };
    let Ok(manifest) = toml::from_str::<toml::Value>(&source) else {
        checks.push(Check {
            name: "rust-toolchain".to_owned(),
            category: "toolchain",
            required: true,
            status: Status::Warning,
            detail: format!("{} could not be parsed", path.display()),
            remedy: None,
        });
        return;
    };
    let channel = manifest
        .get("toolchain")
        .and_then(|toolchain| toolchain.get("channel"))
        .and_then(toml::Value::as_str)
        .unwrap_or("unknown");
    let actual = probe("rustc", &["--version"]).unwrap_or_else(|| "rustc missing".to_owned());
    let matches = actual.contains(channel);
    checks.push(Check {
        name: "rust-toolchain".to_owned(),
        category: "toolchain",
        required: true,
        status: if matches { Status::Ok } else { Status::Warning },
        detail: format!("want {channel}, have {actual}"),
        remedy: (!matches).then(|| "rustup toolchain install".to_owned()),
    });
}

fn check_container(checks: &mut Vec<Check>, workspace: &Path) {
    let engine = ["podman", "docker"]
        .into_iter()
        .find(|binary| probe(binary, &["--version"]).is_some());

    let Some(engine) = engine else {
        checks.push(Check {
            name: "container-engine".to_owned(),
            category: "container",
            required: true,
            status: Status::Missing,
            detail: "neither podman nor docker is installed".to_owned(),
            remedy: Some("install podman (preferred) or docker".to_owned()),
        });
        return;
    };

    checks.push(Check {
        name: "container-engine".to_owned(),
        category: "container",
        required: true,
        status: Status::Ok,
        detail: probe(engine, &["--version"]).unwrap_or_else(|| engine.to_owned()),
        remedy: None,
    });

    let Ok(tag) = image_tag(workspace, false) else {
        checks.push(Check {
            name: "toolchain-image".to_owned(),
            category: "container",
            required: true,
            status: Status::Warning,
            detail: "could not hash the toolchain inputs".to_owned(),
            remedy: None,
        });
        return;
    };
    let present = Command::new(engine)
        .args(["image", "exists", &tag])
        .status()
        .is_ok_and(|status| status.success());
    checks.push(Check {
        name: "toolchain-image".to_owned(),
        category: "container",
        required: true,
        status: if present { Status::Ok } else { Status::Warning },
        detail: if present {
            tag.clone()
        } else {
            format!("{tag} is not built yet")
        },
        remedy: (!present)
            .then(|| format!("{engine} build --file {TOOLCHAIN_DOCKERFILE} --tag {tag} .")),
    });

    checks.push(Check {
        name: "windows-cross".to_owned(),
        category: "container",
        required: false,
        status: Status::Ok,
        detail: if with_msvc() {
            "toolchain image includes the msvc-wine layer".to_owned()
        } else {
            "the image gains the msvc-wine layer when a plan builds a Windows target".to_owned()
        },
        remedy: None,
    });
}

/// msvc-wine is a directory of wrappers around a Microsoft toolchain. The
/// directory alone is not enough — an image built without `WITH_MSVC=1` sets
/// `MSVC_ROOT` to a path that exists but is empty — so the check looks for the
/// compiler the Windows cross script actually needs.
fn check_msvc_wine(checks: &mut Vec<Check>) {
    let root = std::env::var("MSVC_ROOT").unwrap_or_else(|_| "/opt/msvc".to_owned());
    let host = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };
    let compiler = Path::new(&root).join("bin").join(host).join("cl");

    checks.push(if compiler.is_file() {
        Check {
            name: "msvc-wine".to_owned(),
            category: "cross",
            required: false,
            status: Status::Ok,
            detail: compiler.display().to_string(),
            remedy: None,
        }
    } else {
        Check {
            name: "msvc-wine".to_owned(),
            category: "cross",
            required: false,
            status: Status::Missing,
            detail: format!("no compiler at {}", compiler.display()),
            remedy: Some(
                "run msvc-wine's vsdownload.py and install.sh, or set MSVC_ROOT".to_owned(),
            ),
        }
    });
}

/// The macOS cross script needs three binaries to turn a `.app` into a DMG:
/// `mkfs.hfsplus` creates the filesystem, then `hfsplus` and `dmg` populate and
/// wrap it. They come from two different packages, so report them together.
fn check_dmg_toolchain(checks: &mut Vec<Check>) {
    let requirements = [
        ("mkfs.hfsplus", "install hfsprogs (or hfsplus-tools)"),
        ("hfsplus", "build libdmg-hfsplus and install it"),
        ("dmg", "build libdmg-hfsplus and install it"),
    ];
    let missing = requirements
        .iter()
        .filter(|(binary, _)| on_path(binary).is_none())
        .map(|(binary, remedy)| (*binary, *remedy))
        .collect::<Vec<_>>();

    checks.push(if missing.is_empty() {
        Check {
            name: "dmg-tools".to_owned(),
            category: "cross",
            required: false,
            status: Status::Ok,
            detail: "mkfs.hfsplus, hfsplus, dmg".to_owned(),
            remedy: None,
        }
    } else {
        let names = missing
            .iter()
            .map(|(binary, _)| *binary)
            .collect::<Vec<_>>()
            .join(", ");
        Check {
            name: "dmg-tools".to_owned(),
            category: "cross",
            required: false,
            status: Status::Missing,
            detail: format!("{names} not found on PATH"),
            remedy: Some(missing[0].1.to_owned()),
        }
    });
}

/// The macOS bundler refuses anything but Zed's cargo-bundle fork, because the
/// upstream release cannot assemble a deployable bundle.
fn check_cargo_bundle(checks: &mut Vec<Check>) {
    let version = probe("cargo", &["bundle", "--help"]).unwrap_or_default();
    let fork = version.contains("zed");

    checks.push(Check {
        name: "cargo-bundle".to_owned(),
        category: "cross",
        required: false,
        status: if fork {
            Status::Ok
        } else if version.is_empty() {
            Status::Missing
        } else {
            Status::Warning
        },
        detail: if version.is_empty() {
            "`cargo bundle` not found".to_owned()
        } else {
            version
        },
        remedy: (!fork).then(|| {
            "cargo install --locked --git https://github.com/zed-industries/cargo-bundle \
             --rev 2be2669 cargo-bundle"
                .to_owned()
        }),
    });
}

fn check_macos_sdk(checks: &mut Vec<Check>) {
    let candidates = macos_sdk_candidates();
    let found = candidates.iter().find(|path| is_macos_sdk(path));
    checks.push(Check {
        name: "macos-sdk".to_owned(),
        category: "cross",
        required: false,
        status: if found.is_some() {
            Status::Ok
        } else {
            Status::Warning
        },
        detail: found.map_or_else(
            || "no SDK cached; it is downloaded on first cross build".to_owned(),
            |path| path.display().to_string(),
        ),
        remedy: None,
    });
}

fn macos_sdk_candidates() -> Vec<std::path::PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(sdkroot) = std::env::var("SDKROOT") {
        candidates.push(sdkroot.into());
    }
    let cache = std::env::var("ZZZ_MACOS_SDK_ROOT")
        .map(std::path::PathBuf::from)
        .or_else(|_| {
            std::env::var("XDG_CACHE_HOME")
                .map(|root| std::path::PathBuf::from(root).join("zzz-macos-sdk"))
                .or_else(|_| {
                    std::env::var("HOME")
                        .map(|home| std::path::PathBuf::from(home).join(".cache/zzz-macos-sdk"))
                })
        });
    if let Ok(cache) = cache {
        candidates.push(cache.join("MacOSX14.5.sdk"));
    }
    candidates.push(
        std::env::temp_dir()
            .join("zzz-macos-sdk")
            .join("MacOSX14.5.sdk"),
    );
    candidates
}

fn is_macos_sdk(path: &Path) -> bool {
    path.join("System/Library/Frameworks/CoreServices.framework/Headers/CoreServices.h")
        .is_file()
}

/// FreeBSD cross builds bring their own sysroot, linker, and rust-std. Native
/// FreeBSD hosts need only the installed standard library, so the cross
/// requirements are reported as optional here: a Linux host that has not set
/// them up can still build every other target.
fn check_freebsd_toolchain(checks: &mut Vec<Check>) {
    if Platform::host() == Platform::FreeBSD {
        checks.push(Check {
            name: "freebsd-rust-std".to_owned(),
            category: "cross",
            required: false,
            status: Status::Ok,
            detail: "native FreeBSD build; no cross toolchain needed".to_owned(),
            remedy: None,
        });
        return;
    }

    let sysroot = std::env::var_os(crate::tasks::build::freebsd::SYSROOT_VARIABLE)
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_dir());
    checks.push(match sysroot {
        Some(path) => Check {
            name: "freebsd-sysroot".to_owned(),
            category: "cross",
            required: false,
            status: Status::Ok,
            detail: path.display().to_string(),
            remedy: None,
        },
        None => Check {
            name: "freebsd-sysroot".to_owned(),
            category: "cross",
            required: false,
            status: Status::Missing,
            detail: format!(
                "{} is not set",
                crate::tasks::build::freebsd::SYSROOT_VARIABLE
            ),
            remedy: Some(
                "set ZZZ_FREEBSD_SYSROOT to a FreeBSD sysroot to cross-build FreeBSD targets"
                    .to_owned(),
            ),
        },
    });

    for target in [
        crate::tasks::build::model::Target::FreeBSDX86_64,
        crate::tasks::build::model::Target::FreeBSDAarch64,
    ] {
        let linker = std::env::var_os(format!(
            "CARGO_TARGET_{}_LINKER",
            target.triple().to_ascii_uppercase().replace('-', "_")
        ))
        .or_else(|| {
            std::env::var_os(format!(
                "ZZZ_FREEBSD_LINKER_{}",
                target.triple().to_ascii_uppercase().replace('-', "_")
            ))
        });
        checks.push(match linker {
            Some(value) => Check {
                name: format!("freebsd-linker-{}", target.arch().as_str()),
                category: "cross",
                required: false,
                status: Status::Ok,
                detail: value.to_string_lossy().into_owned(),
                remedy: None,
            },
            None => Check {
                name: format!("freebsd-linker-{}", target.arch().as_str()),
                category: "cross",
                required: false,
                status: Status::Missing,
                detail: format!("no linker configured for {}", target.triple()),
                remedy: Some(format!(
                    "set CARGO_TARGET_{}_LINKER (or ZZZ_FREEBSD_LINKER_{0}) to a clang/lld \
                     driver with --target={}",
                    target.triple().to_ascii_uppercase().replace('-', "_"),
                    target.triple()
                )),
            },
        });
    }
}

fn probe(program: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new(program).args(arguments).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    if text.trim().is_empty() {
        text = String::from_utf8_lossy(&output.stderr).into_owned();
    }
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}
