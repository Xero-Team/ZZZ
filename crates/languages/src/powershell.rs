use anyhow::{Context as _, Result, bail};
use async_trait::async_trait;
use collections::HashMap;
use futures::StreamExt;
use gpui::AsyncApp;
use http_client::github::{AssetKind, GitHubLspBinaryVersion, latest_github_release};
use http_client::github_download::{GithubBinaryMetadata, download_server_binary};
use language::{LspAdapter, LspAdapterDelegate, LspInstaller, Toolchain};
use lsp::{LanguageServerBinary, LanguageServerName, Uri};
use project::lsp_store::language_server_settings;
use serde_json::Value;
use smol::fs;
use std::{
    ffi::OsString,
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
};
use util::fs::remove_matching;
use util::{ResultExt, maybe};

const SERVER_NAME: LanguageServerName = LanguageServerName::new_static("powershell-es");
const RELEASE_ASSET_NAME: &str = "PowerShellEditorServices.zip";
const BUNDLE_DIRECTORY_PREFIX: &str = "powershell-es";
const START_SCRIPT_RELATIVE_PATH: &str = "PowerShellEditorServices/Start-EditorServices.ps1";
const SESSION_DETAILS_FILE_NAME: &str = "powershell-es.session.json";
const LOG_DIRECTORY_NAME: &str = "logs";

pub struct PowerShellLspAdapter;

impl PowerShellLspAdapter {
    async fn bundle_is_valid(start_script_path: &Path) -> Result<()> {
        fs::metadata(start_script_path).await.with_context(|| {
            format!(
                "missing Start-EditorServices script in extracted bundle at {start_script_path:?}"
            )
        })?;
        Ok(())
    }

    async fn powershell_binary(
        delegate: &dyn LspAdapterDelegate,
    ) -> Result<(PathBuf, HashMap<String, String>)> {
        let powershell_path = delegate.which("pwsh".as_ref()).await.context(
            "PowerShell 7+ (`pwsh`) must be installed to run PowerShell Editor Services",
        )?;
        let environment = delegate.shell_env().await;
        delegate
            .try_exec(LanguageServerBinary {
                path: powershell_path.clone(),
                arguments: vec!["-Version".into()],
                env: Some(environment.clone()),
            })
            .await
            .with_context(|| format!("failed to run `pwsh -Version` at {powershell_path:?}"))?;
        Ok((powershell_path, environment))
    }

    fn powershell_argument_string(value: &Path) -> String {
        value.to_string_lossy().replace('\'', "''")
    }

    fn server_binary(
        powershell_path: PathBuf,
        environment: HashMap<String, String>,
        bundle_directory: &Path,
    ) -> LanguageServerBinary {
        let start_script_path = bundle_directory.join(START_SCRIPT_RELATIVE_PATH);
        let session_details_path = bundle_directory.join(SESSION_DETAILS_FILE_NAME);
        let log_directory_path = bundle_directory.join(LOG_DIRECTORY_NAME);
        let bundled_modules_path = Self::powershell_argument_string(bundle_directory);
        let start_script_path = Self::powershell_argument_string(&start_script_path);
        let session_details_path = Self::powershell_argument_string(&session_details_path);
        let log_directory_path = Self::powershell_argument_string(&log_directory_path);
        let command = format!(
            "& '{start_script_path}' -BundledModulesPath '{bundled_modules_path}' -Stdio -SessionDetailsPath '{session_details_path}' -LogPath '{log_directory_path}' -FeatureFlags @() -AdditionalModules @() -HostName 'zzz' -HostProfileId '0' -HostVersion '1.0.0' -LogLevel 'Trace'"
        );

        LanguageServerBinary {
            path: powershell_path,
            env: Some(environment),
            arguments: vec![
                "-NoLogo".into(),
                "-NoProfile".into(),
                "-Command".into(),
                OsString::from(command),
            ],
        }
    }

    async fn cached_bundle_directory(container_dir: PathBuf) -> Option<PathBuf> {
        maybe!(async {
            let mut entries = fs::read_dir(&container_dir).await?;
            while let Some(entry) = entries.next().await {
                let path = entry?.path();
                let start_script_path = path.join(START_SCRIPT_RELATIVE_PATH);
                if fs::metadata(&start_script_path).await.is_ok() {
                    return Ok(path);
                }
            }
            bail!("missing extracted PowerShell Editor Services bundle in {container_dir:?}")
        })
        .await
        .log_err()
    }
}

impl LspInstaller for PowerShellLspAdapter {
    type BinaryVersion = GitHubLspBinaryVersion;

    async fn check_if_user_installed(
        &self,
        _: &Arc<dyn LspAdapterDelegate>,
        _: Option<Toolchain>,
        _: &AsyncApp,
    ) -> Option<LanguageServerBinary> {
        None
    }

    async fn fetch_latest_server_version(
        &self,
        delegate: &Arc<dyn LspAdapterDelegate>,
        _: bool,
        _: &mut AsyncApp,
    ) -> Result<GitHubLspBinaryVersion> {
        let release = latest_github_release(
            "PowerShell/PowerShellEditorServices",
            true,
            false,
            delegate.http_client(),
        )
        .await?;
        let asset = release
            .assets
            .into_iter()
            .find(|asset| asset.name == RELEASE_ASSET_NAME)
            .with_context(|| format!("no asset found matching `{RELEASE_ASSET_NAME}`"))?;
        Ok(GitHubLspBinaryVersion {
            name: release.tag_name,
            url: asset.browser_download_url,
            digest: asset.digest,
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
                name,
                url,
                digest: expected_digest,
            } = version;
            let destination_path = container_dir.join(format!("{BUNDLE_DIRECTORY_PREFIX}-{name}"));
            let start_script_path = destination_path.join(START_SCRIPT_RELATIVE_PATH);
            let metadata_path = destination_path.with_extension("metadata");

            let metadata = GithubBinaryMetadata::read_from_file(&metadata_path)
                .await
                .ok();
            if let Some(metadata) = metadata {
                if let (Some(actual_digest), Some(expected_digest)) =
                    (&metadata.digest, &expected_digest)
                {
                    if actual_digest == expected_digest
                        && Self::bundle_is_valid(&start_script_path).await.is_ok()
                    {
                        let (powershell_path, environment) =
                            Self::powershell_binary(delegate.as_ref()).await?;
                        return Ok(Self::server_binary(
                            powershell_path,
                            environment,
                            &destination_path,
                        ));
                    }
                } else if Self::bundle_is_valid(&start_script_path).await.is_ok() {
                    let (powershell_path, environment) =
                        Self::powershell_binary(delegate.as_ref()).await?;
                    return Ok(Self::server_binary(
                        powershell_path,
                        environment,
                        &destination_path,
                    ));
                }
            }

            download_server_binary(
                &*delegate.http_client(),
                &url,
                expected_digest.as_deref(),
                &destination_path,
                AssetKind::Zip,
            )
            .await?;
            remove_matching(&container_dir, |path| path != destination_path).await;
            GithubBinaryMetadata::write_to_file(
                &GithubBinaryMetadata {
                    metadata_version: 1,
                    digest: expected_digest,
                },
                &metadata_path,
            )
            .await?;

            let (powershell_path, environment) = Self::powershell_binary(delegate.as_ref()).await?;
            Ok(Self::server_binary(
                powershell_path,
                environment,
                &destination_path,
            ))
        }
    }

    async fn cached_server_binary(
        &self,
        container_dir: PathBuf,
        delegate: &dyn LspAdapterDelegate,
    ) -> Option<LanguageServerBinary> {
        let bundle_directory = Self::cached_bundle_directory(container_dir).await?;
        let (powershell_path, environment) = Self::powershell_binary(delegate).await.ok()?;
        Some(Self::server_binary(
            powershell_path,
            environment,
            &bundle_directory,
        ))
    }
}

#[async_trait(?Send)]
impl LspAdapter for PowerShellLspAdapter {
    fn name(&self) -> LanguageServerName {
        SERVER_NAME
    }

    async fn initialization_options(
        self: Arc<Self>,
        delegate: &Arc<dyn LspAdapterDelegate>,
        cx: &mut AsyncApp,
    ) -> Result<Option<Value>> {
        Ok(cx.update(|cx| {
            language_server_settings(delegate.as_ref(), &SERVER_NAME, cx)
                .and_then(|settings| settings.initialization_options.clone())
        }))
    }

    async fn workspace_configuration(
        self: Arc<Self>,
        delegate: &Arc<dyn LspAdapterDelegate>,
        _: Option<Toolchain>,
        _: Option<Uri>,
        cx: &mut AsyncApp,
    ) -> Result<Value> {
        Ok(cx
            .update(|cx| {
                language_server_settings(delegate.as_ref(), &SERVER_NAME, cx)
                    .and_then(|settings| settings.settings.clone())
            })
            .unwrap_or_default())
    }
}
