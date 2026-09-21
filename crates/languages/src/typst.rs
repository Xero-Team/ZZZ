//! LSP support for Typst, via [tinymist](https://myriad-dreamin.github.io/tinymist/).
//!
//! Ported from Gram's built-in Typst support, which in turn is based on the
//! `zed-extensions/typst` extension.
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use futures::StreamExt;
use gpui::AsyncApp;
use http_client::github::{AssetKind, GitHubLspBinaryVersion, latest_github_release};
use http_client::github_download::{GithubBinaryMetadata, download_server_binary};
use language::{LanguageServerName, LspAdapter, LspAdapterDelegate, LspInstaller, Toolchain};
use lsp::LanguageServerBinary;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use util::fs::{make_file_executable, remove_matching};

pub struct TypstLspAdapter;

impl TypstLspAdapter {
    const SERVER_NAME: LanguageServerName = LanguageServerName::new_static("tinymist");

    #[cfg(target_os = "macos")]
    const OS_NAME: &'static str = "apple-darwin";
    #[cfg(target_os = "linux")]
    const OS_NAME: &'static str = "unknown-linux-musl";
    #[cfg(target_os = "windows")]
    const OS_NAME: &'static str = "pc-windows-msvc";

    #[cfg(target_os = "windows")]
    const GITHUB_ASSET_KIND: AssetKind = AssetKind::Zip;
    #[cfg(not(target_os = "windows"))]
    const GITHUB_ASSET_KIND: AssetKind = AssetKind::TarGz;

    fn build_asset_base_name() -> Result<String> {
        let arch = match std::env::consts::ARCH {
            "aarch64" => "aarch64",
            "x86_64" => "x86_64",
            "x86" => "x86",
            other => return Err(anyhow!("unsupported architecture: {other}")),
        };

        Ok(format!("tinymist-{arch}-{}", Self::OS_NAME))
    }
}

fn with_exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

impl LspInstaller for TypstLspAdapter {
    type BinaryVersion = GitHubLspBinaryVersion;

    async fn check_if_user_installed(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        _: Option<Toolchain>,
        _cx: &AsyncApp,
    ) -> Option<LanguageServerBinary> {
        let path = delegate.which(with_exe("tinymist").as_ref()).await?;
        Some(LanguageServerBinary {
            path,
            arguments: vec!["lsp".into()],
            env: None,
        })
    }

    async fn fetch_latest_server_version(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        pre_release: bool,
        _cx: &mut AsyncApp,
    ) -> Result<GitHubLspBinaryVersion> {
        let release = latest_github_release(
            "Myriad-Dreamin/tinymist",
            true,
            pre_release,
            delegate.http_client(),
        )
        .await?;

        let asset_name = format!(
            "{}.{}",
            Self::build_asset_base_name()?,
            match Self::GITHUB_ASSET_KIND {
                AssetKind::TarGz => "tar.gz",
                AssetKind::Zip => "zip",
                _ => unreachable!(),
            }
        );

        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| anyhow!("no matching asset found for {asset_name}"))?;

        Ok(GitHubLspBinaryVersion {
            name: release.tag_name.clone(),
            url: asset.browser_download_url.clone(),
            digest: None,
        })
    }

    fn fetch_server_binary(
        &self,
        version: GitHubLspBinaryVersion,
        container_dir: PathBuf,
        delegate: &Arc<dyn LspAdapterDelegate>,
    ) -> impl Send + Future<Output = Result<LanguageServerBinary>> + use<> {
        let delegate = delegate.clone();

        async move {
            let GitHubLspBinaryVersion {
                name: version_name,
                url,
                digest: expected_digest,
            } = version;

            let asset_basename = Self::build_asset_base_name()?;
            let destination_path = container_dir.join(format!("tinymist-{version_name}"));
            let server_path = destination_path
                .join(&asset_basename)
                .join(with_exe("tinymist"));

            let binary = LanguageServerBinary {
                path: server_path.clone(),
                env: None,
                arguments: vec!["lsp".into()],
            };

            let metadata_path = destination_path.with_extension("metadata");
            let metadata = GithubBinaryMetadata::read_from_file(&metadata_path)
                .await
                .ok();
            if let Some(metadata) = metadata {
                let validity_check = async || {
                    delegate
                        .try_exec(LanguageServerBinary {
                            path: server_path.clone(),
                            arguments: vec!["--version".into()],
                            env: None,
                        })
                        .await
                        .inspect_err(|err| {
                            log::warn!(
                                "Unable to run {server_path:?} asset, redownloading: {err:#}",
                            )
                        })
                };
                if let (Some(actual_digest), Some(expected_digest)) =
                    (&metadata.digest, &expected_digest)
                {
                    if actual_digest == expected_digest {
                        if validity_check().await.is_ok() {
                            return Ok(binary);
                        }
                    } else {
                        log::info!(
                            "SHA-256 mismatch for {destination_path:?} asset, downloading new asset. Expected: {expected_digest}, Got: {actual_digest}"
                        );
                    }
                } else if validity_check().await.is_ok() {
                    return Ok(binary);
                }
            }

            download_server_binary(
                &*delegate.http_client(),
                &url,
                expected_digest.as_deref(),
                &destination_path,
                Self::GITHUB_ASSET_KIND,
            )
            .await?;
            make_file_executable(&server_path).await?;
            remove_matching(&container_dir, |path| path != destination_path).await;
            GithubBinaryMetadata::write_to_file(
                &GithubBinaryMetadata {
                    metadata_version: 1,
                    digest: expected_digest,
                },
                &metadata_path,
            )
            .await?;

            Ok(binary)
        }
    }

    async fn cached_server_binary(
        &self,
        container_dir: PathBuf,
        _: &dyn LspAdapterDelegate,
    ) -> Option<LanguageServerBinary> {
        let asset_basename = Self::build_asset_base_name().ok()?;
        let mut entries = smol::fs::read_dir(&container_dir).await.ok()?;
        while let Some(entry) = entries.next().await {
            let path = entry.ok()?.path();
            if path.extension().is_some_and(|ext| ext == "metadata") {
                continue;
            }

            let server_path = path.join(&asset_basename).join(with_exe("tinymist"));
            if smol::fs::metadata(&server_path).await.is_ok() {
                return Some(LanguageServerBinary {
                    path: server_path,
                    arguments: vec!["lsp".into()],
                    env: None,
                });
            }
        }

        None
    }
}

#[async_trait(?Send)]
impl LspAdapter for TypstLspAdapter {
    fn name(&self) -> LanguageServerName {
        Self::SERVER_NAME
    }
}
