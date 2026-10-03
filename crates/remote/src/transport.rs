use std::io::Write;
use std::path::Path;

use crate::{
    RemoteArch, RemoteConnectionStatus, RemoteOs, RemotePlatform,
    json_log::LogRecord,
    protocol::{MESSAGE_LEN_SIZE, message_len_from_buffer, read_message_with_len, write_message},
};
use anyhow::{Context as _, Result};
use futures::{
    AsyncReadExt as _, FutureExt as _, StreamExt as _,
    channel::mpsc::{Sender, UnboundedReceiver, UnboundedSender},
};
use gpui::{AppContext as _, AsyncApp, Task};
use release_channel::{AppCommitSha, ReleaseChannel};
use rpc::proto::Envelope;
use semver::Version;
use util::command::Child;

pub mod docker;
#[cfg(any(test, feature = "test-support"))]
pub mod mock;
pub mod ssh;
pub mod wsl;

pub(crate) struct EmbeddedRemoteServerFile {
    _file: tempfile::NamedTempFile,
}

impl EmbeddedRemoteServerFile {
    pub(crate) fn path(&self) -> &Path {
        self._file.path()
    }
}

pub(crate) fn materialize_embedded_remote_server(
    platform: RemotePlatform,
) -> Result<Option<EmbeddedRemoteServerFile>> {
    let Some((bytes, extension)) =
        remote_server_embed::compressed_archive(platform.os.as_str(), platform.arch.as_str())
    else {
        return Ok(None);
    };

    let mut file = tempfile::Builder::new()
        .prefix("zzz-remote-server-")
        .suffix(&format!(".{extension}"))
        .tempfile()
        .context("creating temp file for embedded remote server")?;
    file.write_all(bytes)
        .context("writing embedded remote server archive")?;
    file.flush()
        .context("flushing embedded remote server archive")?;
    Ok(Some(EmbeddedRemoteServerFile { _file: file }))
}

pub(crate) fn embedded_remote_server_extension(platform: RemotePlatform) -> Option<&'static str> {
    remote_server_embed::compressed_archive(platform.os.as_str(), platform.arch.as_str())
        .map(|(_, extension)| extension)
}

/// Returns the cache key used in the remote server binary filename.
///
/// Development server binaries are keyed by the client commit so a server
/// built from an older checkout cannot be reused for a newer client. Stable
/// binaries continue to use the semantic application version.
pub(crate) fn remote_server_binary_version(
    release_channel: ReleaseChannel,
    version: &Version,
    commit: Option<&AppCommitSha>,
) -> String {
    match release_channel {
        ReleaseChannel::Dev => development_commit(commit).unwrap_or_else(|| "build".to_owned()),
        ReleaseChannel::Stable => version.to_string(),
    }
}

fn development_commit(commit: Option<&AppCommitSha>) -> Option<String> {
    let commit = commit?.full();
    let commit = commit.trim();
    (!commit.is_empty()).then(|| commit.to_owned())
}

fn expected_remote_server_reported_version(
    release_channel: ReleaseChannel,
    version: &Version,
    commit: Option<&AppCommitSha>,
) -> Option<String> {
    match release_channel {
        ReleaseChannel::Stable => {
            let reported_version = version.to_string();
            let reported_version = reported_version
                .split_once('+')
                .map_or(reported_version.as_str(), |(version, _)| version);
            Some(reported_version.to_owned())
        }
        ReleaseChannel::Dev => development_commit(commit),
    }
}

/// Checks that the output of a remote server's `version` command belongs to
/// the client that is about to connect to it.
///
/// Shell startup files may write to stdout, so only the last non-empty line is
/// considered. Dev servers print either `<sha>` or `<build-id>+<sha>`;
/// stable servers print their semantic version.
pub(crate) fn remote_server_version_matches(
    release_channel: ReleaseChannel,
    version: &Version,
    commit: Option<&AppCommitSha>,
    output: &str,
) -> bool {
    let Some(actual) = output
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
    else {
        return false;
    };

    match release_channel {
        ReleaseChannel::Stable => {
            expected_remote_server_reported_version(release_channel, version, commit)
                .is_some_and(|expected| actual == expected)
        }
        ReleaseChannel::Dev => {
            let Some(expected) =
                expected_remote_server_reported_version(release_channel, version, commit)
            else {
                // Test and custom builds without a git revision cannot perform
                // an identity check. Preserve the previous behavior for them.
                return true;
            };
            actual == expected
                || actual
                    .rsplit_once('+')
                    .is_some_and(|(_, actual_commit)| actual_commit == expected)
        }
    }
}

pub(crate) fn ensure_remote_server_version_matches(
    release_channel: ReleaseChannel,
    version: &Version,
    commit: Option<&AppCommitSha>,
    output: &str,
) -> Result<()> {
    let expected = expected_remote_server_reported_version(release_channel, version, commit)
        .unwrap_or_else(|| "any non-empty development version".to_owned());
    anyhow::ensure!(
        remote_server_version_matches(release_channel, version, commit, output),
        "remote server reported version {:?}, expected {:?}",
        output.trim(),
        expected,
    );
    Ok(())
}

/// Parses the output of `uname -sm` to determine the remote platform.
/// Takes the last line to skip possible shell initialization output.
fn parse_platform(output: &str) -> Result<RemotePlatform> {
    let output = output.trim();
    let uname = output.rsplit_once('\n').map_or(output, |(_, last)| last);
    let Some((os, arch)) = uname.split_once(" ") else {
        anyhow::bail!("unknown uname: {uname:?}")
    };

    let os = match os {
        "Darwin" => RemoteOs::MacOs,
        "Linux" => RemoteOs::Linux,
        _ => anyhow::bail!("unsupported remote OS {os:?}"),
    };

    // exclude armv5,6,7 as they are 32-bit.
    let arch = if arch.starts_with("armv8")
        || arch.starts_with("armv9")
        || arch.starts_with("arm64")
        || arch.starts_with("aarch64")
    {
        RemoteArch::Aarch64
    } else if arch.starts_with("x86") {
        RemoteArch::X86_64
    } else {
        anyhow::bail!("unsupported remote architecture {arch:?}")
    };

    Ok(RemotePlatform { os, arch })
}

/// Parses the output of `echo $SHELL` to determine the remote shell.
/// Takes the last line to skip possible shell initialization output.
fn parse_shell(output: &str, fallback_shell: &str) -> String {
    let output = output.trim();
    let shell = output.rsplit_once('\n').map_or(output, |(_, last)| last);
    if shell.is_empty() {
        log::error!("$SHELL is not set, falling back to {fallback_shell}");
        fallback_shell.to_owned()
    } else {
        shell.to_owned()
    }
}

fn handle_rpc_messages_over_child_process_stdio(
    mut remote_proxy_process: Child,
    incoming_sender: UnboundedSender<Envelope>,
    mut outgoing_receiver: UnboundedReceiver<Envelope>,
    mut connection_activity_sender: Sender<()>,
    cx: &AsyncApp,
) -> Task<Result<i32>> {
    let mut child_stderr = remote_proxy_process
        .stderr
        .take()
        .expect("remote proxy stderr should be piped");
    let mut child_stdout = remote_proxy_process
        .stdout
        .take()
        .expect("remote proxy stdout should be piped");
    let mut child_stdin = remote_proxy_process
        .stdin
        .take()
        .expect("remote proxy stdin should be piped");

    let mut stdin_buffer = Vec::new();
    let mut stdout_buffer = Vec::new();
    let mut stderr_buffer = Vec::new();
    let mut stderr_offset = 0;

    let stdin_task = cx.background_spawn(async move {
        while let Some(outgoing) = outgoing_receiver.next().await {
            write_message(&mut child_stdin, &mut stdin_buffer, outgoing).await?;
        }
        anyhow::Ok(())
    });

    let stdout_task = cx.background_spawn({
        let mut connection_activity_sender = connection_activity_sender.clone();
        async move {
            loop {
                stdout_buffer.resize(MESSAGE_LEN_SIZE, 0);
                let bytes_read = child_stdout.read(&mut stdout_buffer).await?;

                if bytes_read == 0 {
                    return anyhow::Ok(());
                }

                if bytes_read < MESSAGE_LEN_SIZE {
                    child_stdout
                        .read_exact(&mut stdout_buffer[bytes_read..])
                        .await?;
                }

                let message_len = message_len_from_buffer(&stdout_buffer)?;
                let envelope =
                    read_message_with_len(&mut child_stdout, &mut stdout_buffer, message_len)
                        .await?;
                notify_connection_activity(&mut connection_activity_sender);
                if incoming_sender.unbounded_send(envelope).is_err() {
                    return anyhow::Ok(());
                }
            }
        }
    });

    let stderr_task: Task<anyhow::Result<()>> = cx.background_spawn(async move {
        loop {
            stderr_buffer.resize(stderr_offset + 1024, 0);

            let bytes_read = child_stderr
                .read(&mut stderr_buffer[stderr_offset..])
                .await?;
            if bytes_read == 0 {
                return anyhow::Ok(());
            }

            stderr_offset += bytes_read;
            let mut start_index = 0;
            while let Some(relative_newline_index) = stderr_buffer[start_index..stderr_offset]
                .iter()
                .position(|byte| byte == &b'\n')
            {
                let line_index = start_index + relative_newline_index;
                let content = &stderr_buffer[start_index..line_index];
                start_index = line_index + 1;
                if let Ok(record) = serde_json::from_slice::<LogRecord>(content) {
                    record.log(log::logger())
                } else {
                    std::io::stderr()
                        .write_fmt(format_args!(
                            "(remote) {}\n",
                            String::from_utf8_lossy(content)
                        ))
                        .context("writing remote process stderr")?;
                }
            }
            stderr_buffer.drain(0..start_index);
            stderr_offset -= start_index;

            notify_connection_activity(&mut connection_activity_sender);
        }
    });

    cx.background_spawn(async move {
        let result = futures::select! {
            result = stdin_task.fuse() => {
                result.context("stdin")
            }
            result = stdout_task.fuse() => {
                result.context("stdout")
            }
            result = stderr_task.fuse() => {
                result.context("stderr")
            }
        };
        let exit_status = remote_proxy_process.status().await?;
        let status = exit_status.code().unwrap_or_else(|| {
            #[cfg(unix)]
            let status = std::os::unix::process::ExitStatusExt::signal(&exit_status).unwrap_or(1);
            #[cfg(not(unix))]
            let status = 1;
            status
        });
        match result {
            Ok(_) => Ok(status),
            Err(error) => Err(error),
        }
    })
}

fn notify_connection_activity(sender: &mut Sender<()>) {
    if let Err(error) = sender.try_send(())
        && error.is_disconnected()
    {
        log::debug!("remote connection activity receiver already closed");
    }
}

#[cfg(any(debug_assertions, feature = "build-remote-server-binary"))]
async fn build_remote_server_from_source(
    platform: &crate::RemotePlatform,
    delegate: &dyn crate::RemoteClientDelegate,
    binary_exists_on_server: bool,
    cx: &mut AsyncApp,
) -> Result<Option<std::path::PathBuf>> {
    use std::env::VarError;
    use std::path::Path;
    use util::command::{Command, Stdio, new_command};

    if let Ok(path) = std::env::var("ZZZ_COPY_REMOTE_SERVER") {
        let path = std::path::PathBuf::from(path);
        if path.exists() {
            return Ok(Some(path));
        }
        log::warn!(
            "ZZZ_COPY_REMOTE_SERVER path does not exist, falling back to ZZZ_BUILD_REMOTE_SERVER: {}",
            path.display()
        );
    }

    // By default, we make building remote server from source opt-out and we do not force artifact compression
    // for quicker builds.
    let build_remote_server =
        std::env::var("ZZZ_BUILD_REMOTE_SERVER").unwrap_or("nocompress".into());

    if &*build_remote_server == "never" {
        return Ok(None);
    } else if let "false" | "no" | "off" | "0" = &*build_remote_server {
        if binary_exists_on_server {
            return Ok(None);
        }
        log::warn!("ZZZ_BUILD_REMOTE_SERVER is disabled, but no server binary exists on the server")
    }

    async fn run_cmd(command: &mut Command) -> Result<()> {
        let output = command
            .kill_on_drop(true)
            .stdout(Stdio::inherit())
            .output()
            .await?;
        anyhow::ensure!(
            output.status.success(),
            "Failed to run command: {command:?}: output: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }

    fn apply_tmpfs_zig_cache(command: &mut Command) {
        let cache = std::env::var_os("ZIG_GLOBAL_CACHE_DIR").map_or_else(
            || {
                #[cfg(unix)]
                {
                    let tmp = std::path::PathBuf::from("/tmp");
                    if tmp.is_dir() {
                        return tmp.join("zzz-zig-cache");
                    }
                }
                std::env::temp_dir().join("zzz-zig-cache")
            },
            std::path::PathBuf::from,
        );
        let local = std::env::var_os("ZIG_LOCAL_CACHE_DIR")
            .map_or_else(|| cache.join("local"), std::path::PathBuf::from);
        if let Err(error) = std::fs::create_dir_all(&local) {
            log::warn!("failed to create zig cache {}: {error}", local.display());
        }
        command
            .env("ZIG_GLOBAL_CACHE_DIR", &cache)
            .env("ZIG_LOCAL_CACHE_DIR", &local);
    }

    async fn ensure_rustup_target(
        triple: &str,
        delegate: &dyn crate::RemoteClientDelegate,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        let rustup = which("rustup", cx)
            .await?
            .context("rustup not found on $PATH, install rustup (see https://rustup.rs/)")?;
        delegate.set_status(Some(RemoteConnectionStatus::AddingRustupTarget), cx);
        log::info!("adding rustup target");
        run_cmd(
            new_command(rustup)
                .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
                .args(["target", "add"])
                .arg(&triple),
        )
        .await?;
        Ok(())
    }

    enum RemoteServerBuildMode {
        Native,
        Xwin,
        Zig,
    }

    impl RemoteServerBuildMode {
        fn build_command(&self) -> &[&'static str] {
            match self {
                RemoteServerBuildMode::Native => &["build"],
                RemoteServerBuildMode::Xwin => &["xwin", "build"],
                RemoteServerBuildMode::Zig => &["zigbuild"],
            }
        }
    }

    let use_musl = !build_remote_server.contains("nomusl");
    let triple = format!(
        "{}-{}",
        platform.arch,
        match platform.os {
            RemoteOs::Linux =>
                if use_musl {
                    "unknown-linux-musl"
                } else {
                    "unknown-linux-gnu"
                },
            RemoteOs::MacOs => "apple-darwin",
            RemoteOs::Windows => "pc-windows-msvc",
        }
    );
    let mut rust_flags = match std::env::var("RUSTFLAGS") {
        Ok(val) => val,
        Err(VarError::NotPresent) => String::new(),
        Err(e) => {
            log::error!("Failed to get env var `RUSTFLAGS` value: {e}");
            String::new()
        }
    };
    if platform.os == RemoteOs::Linux && use_musl {
        rust_flags.push_str(" -C target-feature=+crt-static");

        if let Ok(path) = std::env::var("ZZZ_ZSTD_MUSL_LIB") {
            rust_flags.push_str(&format!(" -C link-arg=-L{path}"));
        }
    }
    let macos_sdkroot = if platform.os == RemoteOs::MacOs {
        delegate.set_status(Some(RemoteConnectionStatus::PreparingMacOsSdk), cx);
        let output = new_command(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../script/ensure-macos-sdk"
        ))
        .kill_on_drop(true)
        .output()
        .await?;
        anyhow::ensure!(
            output.status.success(),
            "ensure-macos-sdk failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Some(
            String::from_utf8(output.stdout)
                .context("ensure-macos-sdk output was not utf-8")?
                .trim()
                .to_string(),
        )
    } else {
        None
    };
    let remote_build_mode = if platform.arch.as_str() == std::env::consts::ARCH
        && platform.os.as_str() == std::env::consts::OS
    {
        RemoteServerBuildMode::Native
    } else if platform.os.as_str() == "windows" {
        RemoteServerBuildMode::Xwin
    } else {
        RemoteServerBuildMode::Zig
    };

    match remote_build_mode {
        RemoteServerBuildMode::Native => {
            delegate.set_status(
                Some(RemoteConnectionStatus::BuildingRemoteServerFromSource),
                cx,
            );
            log::info!("building remote server binary from source");
        }
        RemoteServerBuildMode::Zig => {
            if which("zig", cx).await?.is_none() {
                anyhow::bail!(if cfg!(not(windows)) {
                    "zig not found on $PATH, install zig (see https://ziglang.org/learn/getting-started or use zigup)"
                } else {
                    "zig not found on $PATH, install zig (use `winget install -e --id zig.zig` or see https://ziglang.org/learn/getting-started or use zigup)"
                });
            }

            ensure_rustup_target(&triple, delegate, cx).await?;

            if which("cargo-zigbuild", cx).await?.is_none() {
                delegate.set_status(Some(RemoteConnectionStatus::InstallingCargoZigbuild), cx);
                log::info!("installing cargo-zigbuild");
                run_cmd(new_command("cargo").args(["install", "--locked", "cargo-zigbuild"]))
                    .await?;
            }

            delegate.set_status(
                Some(RemoteConnectionStatus::BuildingRemoteBinaryWithZig {
                    target: triple.clone(),
                }),
                cx,
            );
            log::info!("building remote binary from source for {triple} with Zig");
            // rustc passes `-O1` to the linker on optimized builds; Zig's cc wrapper
            // reports it as a deprecated setting (rust-lang/rust#158192), so allow
            // the resulting false-positive linker message.
            rust_flags.push_str(" -A linker_messages");
        }
        RemoteServerBuildMode::Xwin => {
            if which("clang", cx).await?.is_none() {
                anyhow::bail!(
                    "clang not found on $PATH, install clang to cross-compile the Windows remote server (see https://clang.llvm.org/)"
                );
            }

            if which("cargo-xwin", cx).await?.is_none() {
                anyhow::bail!(
                    "cargo-xwin not found on $PATH. Install it with `cargo install --locked cargo-xwin`.\n\n\
                     Note that cargo-xwin downloads Microsoft's CRT and Windows SDK; by using it you \
                     accept Microsoft's license (see https://go.microsoft.com/fwlink/?LinkId=2086102)"
                );
            }

            ensure_rustup_target(&triple, delegate, cx).await?;

            delegate.set_status(Some(RemoteConnectionStatus::AddingLlvmTools), cx);
            log::info!("adding llvm-tools component");
            run_cmd(
                new_command("rustup")
                    .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
                    .args(["component", "add", "llvm-tools"]),
            )
            .await?;

            delegate.set_status(
                Some(RemoteConnectionStatus::BuildingRemoteBinaryWithXwin {
                    target: triple.clone(),
                }),
                cx,
            );
            log::info!("building remote binary from source for {triple} with xwin");
        }
    };

    let mut command = new_command("cargo");
    command
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args(remote_build_mode.build_command())
        .args([
            "--package",
            "remote_server",
            "--features",
            "debug-embed",
            "--target-dir",
            "target/remote_server",
            "--target",
            &triple,
        ])
        .env("RUSTFLAGS", &rust_flags)
        // Runtime builds must identify the current checkout rather than inherit
        // commit metadata from the client build environment.
        .env_remove("ZZZ_COMMIT_SHA");
    if matches!(remote_build_mode, RemoteServerBuildMode::Zig) {
        apply_tmpfs_zig_cache(&mut command);
    }
    if let Some(sdkroot) = &macos_sdkroot {
        command.env("SDKROOT", sdkroot);
    }
    run_cmd(&mut command).await?;
    let bin_path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .join("target")
        .join("remote_server")
        .join(&triple)
        .join("debug")
        .join("remote_server")
        .with_extension(if platform.os.is_windows() { "exe" } else { "" });

    let path = if build_remote_server.contains("nocompress") {
        bin_path
    } else {
        delegate.set_status(Some(RemoteConnectionStatus::CompressingBinary), cx);

        #[cfg(not(target_os = "windows"))]
        let archive_path = {
            run_cmd(new_command("gzip").arg("-f").arg(&bin_path)).await?;
            bin_path.with_extension("gz")
        };

        #[cfg(target_os = "windows")]
        let archive_path = {
            let zip_path = bin_path.with_extension("zip");
            if smol::fs::metadata(&zip_path).await.is_ok() {
                smol::fs::remove_file(&zip_path).await?;
            }
            let compress_command = format!(
                "Compress-Archive -Path '{}' -DestinationPath '{}' -Force",
                bin_path.display(),
                zip_path.display(),
            );
            run_cmd(new_command("powershell.exe").args([
                "-NoProfile",
                "-Command",
                &compress_command,
            ]))
            .await?;
            zip_path
        };

        std::env::current_dir()?.join(archive_path)
    };

    Ok(Some(path))
}

#[cfg(any(debug_assertions, feature = "build-remote-server-binary"))]
async fn which(
    binary_name: impl AsRef<str>,
    cx: &mut AsyncApp,
) -> Result<Option<std::path::PathBuf>> {
    let binary_name = binary_name.as_ref().to_owned();
    let binary_name_cloned = binary_name.clone();
    let res = cx
        .background_spawn(async move { which::which(binary_name_cloned) })
        .await;
    match res {
        Ok(path) => Ok(Some(path)),
        Err(which::Error::CannotFindBinaryPath) => Ok(None),
        Err(err) => Err(anyhow::anyhow!(
            "Failed to run 'which' to find the binary '{binary_name}': {err}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_platform() {
        let result = parse_platform("Linux x86_64\n").unwrap();
        assert_eq!(result.os, RemoteOs::Linux);
        assert_eq!(result.arch, RemoteArch::X86_64);

        let result = parse_platform("Darwin arm64\n").unwrap();
        assert_eq!(result.os, RemoteOs::MacOs);
        assert_eq!(result.arch, RemoteArch::Aarch64);

        let result = parse_platform("Linux x86_64").unwrap();
        assert_eq!(result.os, RemoteOs::Linux);
        assert_eq!(result.arch, RemoteArch::X86_64);

        let result = parse_platform("some shell init output\nLinux aarch64\n").unwrap();
        assert_eq!(result.os, RemoteOs::Linux);
        assert_eq!(result.arch, RemoteArch::Aarch64);

        let result = parse_platform("some shell init output\nLinux aarch64").unwrap();
        assert_eq!(result.os, RemoteOs::Linux);
        assert_eq!(result.arch, RemoteArch::Aarch64);

        assert_eq!(
            parse_platform("Linux armv8l\n").unwrap().arch,
            RemoteArch::Aarch64
        );
        assert_eq!(
            parse_platform("Linux aarch64\n").unwrap().arch,
            RemoteArch::Aarch64
        );
        assert_eq!(
            parse_platform("Linux x86_64\n").unwrap().arch,
            RemoteArch::X86_64
        );

        let result = parse_platform(
            r#"Linux x86_64 - What you're referring to as Linux, is in fact, GNU/Linux...\n"#,
        )
        .unwrap();
        assert_eq!(result.os, RemoteOs::Linux);
        assert_eq!(result.arch, RemoteArch::X86_64);

        assert!(parse_platform("Windows x86_64\n").is_err());
        assert!(parse_platform("Linux armv7l\n").is_err());
    }

    #[test]
    fn test_parse_shell() {
        assert_eq!(parse_shell("/bin/bash\n", "sh"), "/bin/bash");
        assert_eq!(parse_shell("/bin/zsh\n", "sh"), "/bin/zsh");

        assert_eq!(parse_shell("/bin/bash", "sh"), "/bin/bash");
        assert_eq!(
            parse_shell("some shell init output\n/bin/bash\n", "sh"),
            "/bin/bash"
        );
        assert_eq!(
            parse_shell("some shell init output\n/bin/bash", "sh"),
            "/bin/bash"
        );
        assert_eq!(parse_shell("", "sh"), "sh");
        assert_eq!(parse_shell("\n", "sh"), "sh");
    }

    #[test]
    fn test_remote_server_binary_version() {
        let version = Version::new(1, 23, 0);
        let commit = AppCommitSha::new("53604c81da82a0e99c3ee3d6077ca152446403c7".to_owned());
        let empty_commit = AppCommitSha::new(String::new());
        let padded_commit =
            AppCommitSha::new(" 53604c81da82a0e99c3ee3d6077ca152446403c7 ".to_owned());

        assert_eq!(
            remote_server_binary_version(ReleaseChannel::Dev, &version, Some(&commit)),
            commit.full()
        );
        assert_eq!(
            remote_server_binary_version(ReleaseChannel::Dev, &version, None),
            "build"
        );
        assert_eq!(
            remote_server_binary_version(ReleaseChannel::Dev, &version, Some(&empty_commit)),
            "build"
        );
        assert_eq!(
            remote_server_binary_version(ReleaseChannel::Dev, &version, Some(&padded_commit)),
            commit.full()
        );
        assert_eq!(
            remote_server_binary_version(ReleaseChannel::Stable, &version, Some(&commit)),
            "1.23.0"
        );
    }

    #[test]
    fn test_remote_server_version_matches() {
        let version = Version::new(1, 23, 0);
        let commit = AppCommitSha::new("53604c81da82a0e99c3ee3d6077ca152446403c7".to_owned());
        let empty_commit = AppCommitSha::new(String::new());
        let stable_version = Version::parse("1.23.0+stable").expect("valid test version");

        assert!(remote_server_version_matches(
            ReleaseChannel::Dev,
            &version,
            Some(&commit),
            "shell init output\n53604c81da82a0e99c3ee3d6077ca152446403c7\n"
        ));
        assert!(remote_server_version_matches(
            ReleaseChannel::Dev,
            &version,
            Some(&commit),
            "12345+53604c81da82a0e99c3ee3d6077ca152446403c7\n"
        ));
        assert!(!remote_server_version_matches(
            ReleaseChannel::Dev,
            &version,
            Some(&commit),
            "28d60b8f28a9448fea506fc18aeeaf4698d047e9\n"
        ));
        assert!(remote_server_version_matches(
            ReleaseChannel::Dev,
            &version,
            None,
            "dev\n"
        ));
        assert!(remote_server_version_matches(
            ReleaseChannel::Dev,
            &version,
            Some(&empty_commit),
            "dev\n"
        ));
        assert!(!remote_server_version_matches(
            ReleaseChannel::Dev,
            &version,
            None,
            "\n"
        ));
        assert!(remote_server_version_matches(
            ReleaseChannel::Stable,
            &stable_version,
            None,
            "shell init output\n1.23.0\n"
        ));
        assert!(!remote_server_version_matches(
            ReleaseChannel::Stable,
            &stable_version,
            None,
            "1.22.0\n"
        ));
    }

    #[test]
    fn test_ensure_remote_server_version_matches() {
        let version = Version::new(1, 23, 0);
        let commit = AppCommitSha::new("53604c81da82a0e99c3ee3d6077ca152446403c7".to_owned());

        ensure_remote_server_version_matches(
            ReleaseChannel::Dev,
            &version,
            Some(&commit),
            "53604c81da82a0e99c3ee3d6077ca152446403c7\n",
        )
        .expect("matching version should be accepted");

        let error = ensure_remote_server_version_matches(
            ReleaseChannel::Dev,
            &version,
            Some(&commit),
            "28d60b8f28a9448fea506fc18aeeaf4698d047e9\n",
        )
        .expect_err("mismatched version should be rejected");
        assert!(error.to_string().contains("28d60b8f"));
        assert!(error.to_string().contains("53604c81"));
    }
}
