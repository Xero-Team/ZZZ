use anyhow::Result;
use async_trait::async_trait;
use gpui::AsyncApp;
use language::{LspAdapter, LspAdapterDelegate, LspInstaller, Toolchain};
use lsp::{LanguageServerBinary, LanguageServerName};
use node_runtime::{NodeRuntime, VersionStrategy};
use semver::Version;
use std::{
    ffi::OsString,
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
};
use util::{ResultExt, maybe};

const SERVER_PATH: &str = "node_modules/dockerfile-language-server-nodejs/bin/docker-langserver";

fn server_binary_arguments(server_path: &Path) -> Vec<OsString> {
    vec![server_path.into(), "--stdio".into()]
}

pub struct DockerfileLspAdapter {
    node: NodeRuntime,
}

impl DockerfileLspAdapter {
    const SERVER_NAME: LanguageServerName =
        LanguageServerName::new_static("dockerfile-language-server");
    const PACKAGE_NAME: &str = "dockerfile-language-server-nodejs";

    pub fn new(node: NodeRuntime) -> Self {
        Self { node }
    }
}

impl LspInstaller for DockerfileLspAdapter {
    type BinaryVersion = Version;

    async fn fetch_latest_server_version(
        &self,
        _: &dyn LspAdapterDelegate,
        _: bool,
        _: &mut AsyncApp,
    ) -> Result<Self::BinaryVersion> {
        self.node
            .npm_package_latest_version(Self::PACKAGE_NAME)
            .await
    }

    async fn check_if_user_installed(
        &self,
        delegate: &dyn LspAdapterDelegate,
        _: Option<Toolchain>,
        _: &AsyncApp,
    ) -> Option<LanguageServerBinary> {
        let path = delegate.which("docker-langserver".as_ref()).await?;
        let env = delegate.shell_env().await;

        Some(LanguageServerBinary {
            path,
            env: Some(env),
            arguments: vec!["--stdio".into()],
        })
    }

    fn fetch_server_binary(
        &self,
        _latest_version: Self::BinaryVersion,
        container_dir: PathBuf,
        _: &Arc<dyn LspAdapterDelegate>,
    ) -> impl Send + Future<Output = Result<LanguageServerBinary>> + use<> {
        let node = self.node.clone();

        async move {
            let server_path = container_dir.join(SERVER_PATH);

            node.npm_install_latest_packages(&container_dir, &[Self::PACKAGE_NAME])
                .await?;

            Ok(LanguageServerBinary {
                path: node.binary_path().await?,
                env: None,
                arguments: server_binary_arguments(&server_path),
            })
        }
    }

    fn check_if_version_installed(
        &self,
        version: &Self::BinaryVersion,
        container_dir: &PathBuf,
        _: &Arc<dyn LspAdapterDelegate>,
    ) -> impl Send + Future<Output = Option<LanguageServerBinary>> + use<> {
        let node = self.node.clone();
        let version = version.clone();
        let container_dir = container_dir.clone();

        async move {
            let server_path = container_dir.join(SERVER_PATH);

            let should_install_language_server = node
                .should_install_npm_package(
                    Self::PACKAGE_NAME,
                    &server_path,
                    &container_dir,
                    VersionStrategy::Latest(&version),
                )
                .await;

            if should_install_language_server {
                None
            } else {
                Some(LanguageServerBinary {
                    path: node.binary_path().await.ok()?,
                    env: None,
                    arguments: server_binary_arguments(&server_path),
                })
            }
        }
    }

    async fn cached_server_binary(
        &self,
        container_dir: PathBuf,
        _: &dyn LspAdapterDelegate,
    ) -> Option<LanguageServerBinary> {
        get_cached_server_binary(container_dir, &self.node).await
    }
}

#[async_trait(?Send)]
impl LspAdapter for DockerfileLspAdapter {
    fn name(&self) -> LanguageServerName {
        Self::SERVER_NAME
    }
}

async fn get_cached_server_binary(
    container_dir: PathBuf,
    node: &NodeRuntime,
) -> Option<LanguageServerBinary> {
    maybe!(async {
        let server_path = container_dir.join(SERVER_PATH);
        anyhow::ensure!(
            server_path.exists(),
            "missing executable in directory {server_path:?}"
        );
        Ok(LanguageServerBinary {
            path: node.binary_path().await?,
            env: None,
            arguments: server_binary_arguments(&server_path),
        })
    })
    .await
    .log_err()
}
