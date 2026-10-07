use std::sync::Arc;

use fs::{Fs, GitService, ProcessService};
use gpui::{App, AsyncApp, Entity};
use lsp::Uri;
use vfs::{
    MountId, NativePath, OperationContext, ProviderCapabilities, ProviderId, ProviderPath,
    ResourceId, StatOptions, VfsErrorCode, VfsPath, VfsProvider, VfsSnapshot,
};

use crate::worktree_store::WorktreeStore;

#[derive(Clone)]
pub struct NativeExecutionContext {
    worktree_store: Entity<WorktreeStore>,
    file_system: Arc<dyn Fs>,
    git_service: Arc<dyn GitService>,
    process_service: Arc<dyn ProcessService>,
}

impl NativeExecutionContext {
    pub fn new(worktree_store: Entity<WorktreeStore>, file_system: Arc<dyn Fs>) -> Self {
        let git_service: Arc<dyn GitService> = file_system.clone();
        let process_service: Arc<dyn ProcessService> = file_system.clone();
        Self {
            worktree_store,
            file_system,
            git_service,
            process_service,
        }
    }

    pub fn file_system(&self) -> &Arc<dyn Fs> {
        &self.file_system
    }

    pub fn git_service(&self) -> &Arc<dyn GitService> {
        &self.git_service
    }

    pub fn process_service(&self) -> &Arc<dyn ProcessService> {
        &self.process_service
    }

    pub fn lsp_path_mapper(&self) -> LspPathMapper {
        LspPathMapper {
            worktree_store: self.worktree_store.clone(),
        }
    }
}

#[derive(Clone)]
pub struct LspPathMapper {
    worktree_store: Entity<WorktreeStore>,
}

pub struct MappedLspResource {
    native_path: NativePath,
    resource_id: Option<ResourceId>,
    vfs_path: VfsPath,
    provider: Arc<dyn VfsProvider>,
    capabilities: ProviderCapabilities,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LspResourceOperation {
    Edit,
    CreateFile,
    CreateDirectory,
    Rename,
    Delete,
}

impl MappedLspResource {
    pub fn native_path(&self) -> &NativePath {
        &self.native_path
    }

    pub fn resource_id(&self) -> Option<ResourceId> {
        self.resource_id
    }

    pub fn vfs_path(&self) -> &VfsPath {
        &self.vfs_path
    }

    pub fn capabilities(&self) -> &ProviderCapabilities {
        &self.capabilities
    }

    pub(crate) fn provider(&self) -> &Arc<dyn VfsProvider> {
        &self.provider
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NativeExecutionError {
    #[error("LSP URI does not use the file scheme: {uri}")]
    NonFileUri { uri: String },
    #[error("LSP URI cannot be converted to an execution-host path: {uri}")]
    InvalidFileUri { uri: String },
    #[error("native execution context path style does not match this host")]
    ExecutionHostMismatch,
    #[error("native path is outside every mounted worktree: {path}")]
    UnmappedNativePath { path: String },
    #[error("VFS mount {mount_id:?} is not available on this execution host")]
    UnmappedVfsMount { mount_id: MountId },
    #[error("provider {provider} does not expose a native path on this execution host")]
    NativePathUnsupported { provider: ProviderId },
    #[error("provider native mapping disagrees with the LSP URI for {path:?}")]
    NativePathMismatch { path: VfsPath },
    #[error("provider {provider} does not support {operation:?} for this resource")]
    UnsupportedOperation {
        provider: ProviderId,
        operation: LspResourceOperation,
    },
    #[error("provider {provider} exposes {operation:?} as read-only for {path:?}")]
    ReadOnlyResource {
        provider: ProviderId,
        operation: LspResourceOperation,
        path: VfsPath,
    },
    #[error("LSP rename crosses VFS mounts {source_mount:?} and {target_mount:?}")]
    CrossMountRename {
        source_mount: MountId,
        target_mount: MountId,
    },
    #[error("LSP text edit path cannot be represented by the text-buffer path model: {path:?}")]
    TextBufferPathUnavailable { path: VfsPath },
    #[error("provider returned a native path that this host cannot represent")]
    InvalidNativePath(#[source] vfs::PathError),
    #[error("native path cannot be represented as a file URI: {path}")]
    InvalidNativeUri { path: String },
    #[error(transparent)]
    Provider(#[from] vfs::VfsError),
}

impl LspPathMapper {
    pub fn uri_to_resource(
        &self,
        uri: &Uri,
        cx: &App,
    ) -> Result<MappedLspResource, NativeExecutionError> {
        if uri.scheme() != "file" {
            return Err(NativeExecutionError::NonFileUri {
                uri: uri.to_string(),
            });
        }

        let worktree_store = self.worktree_store.read(cx);
        if worktree_store.path_style() != util::paths::PathStyle::local() {
            return Err(NativeExecutionError::ExecutionHostMismatch);
        }
        let native_path = native_path_from_file_uri(uri, worktree_store.path_style())?;

        let mut best_match: Option<(usize, ProviderPath, VfsSnapshot)> = None;
        for worktree in worktree_store.worktrees() {
            let worktree = worktree.read(cx);
            let native_root = worktree.native_abs_path();
            let Ok(native_relative_path) = native_path.strip_prefix(&native_root) else {
                continue;
            };
            let Some(snapshot) = worktree.vfs_snapshot() else {
                continue;
            };
            let relative_path = if native_relative_path.encoding()
                == snapshot.provider().descriptor().path_encoding
            {
                native_relative_path
            } else {
                let components = native_relative_path
                    .components()
                    .map(|component| component.as_bytes());
                let Ok(relative_path) = ProviderPath::from_byte_components(
                    snapshot.provider().descriptor().path_encoding,
                    components,
                ) else {
                    continue;
                };
                relative_path
            };
            let root_depth = native_root.provider_path().component_count();
            if best_match
                .as_ref()
                .is_some_and(|(best_depth, ..)| *best_depth >= root_depth)
            {
                continue;
            }
            best_match = Some((root_depth, relative_path, snapshot));
        }

        let Some((_, relative_path, snapshot)) = best_match else {
            return Err(NativeExecutionError::UnmappedNativePath {
                path: display_native_path(&native_path),
            });
        };
        let provider = snapshot.provider().clone();
        let capabilities = provider.capabilities();
        if !capabilities.links.native_path.is_supported() {
            return Err(NativeExecutionError::NativePathUnsupported {
                provider: provider.descriptor().id.clone(),
            });
        }
        let vfs_path = VfsPath::new(snapshot.registry().mount_id(), relative_path.clone());
        let resource_id = snapshot.registry().id_for_path(&relative_path);

        Ok(MappedLspResource {
            native_path,
            resource_id,
            vfs_path,
            provider,
            capabilities,
        })
    }

    pub async fn resource_to_uri(
        &self,
        path: &VfsPath,
        cx: &mut AsyncApp,
    ) -> Result<Uri, NativeExecutionError> {
        let provider = self.worktree_store.read_with(cx, |worktree_store, cx| {
            worktree_store.worktrees().find_map(|worktree| {
                let worktree = worktree.read(cx);
                let snapshot = worktree.vfs_snapshot()?;
                (snapshot.registry().mount_id() == path.mount_id())
                    .then(|| snapshot.provider().clone())
            })
        });
        let Some(provider) = provider else {
            return Err(NativeExecutionError::UnmappedVfsMount {
                mount_id: path.mount_id(),
            });
        };
        if !provider.capabilities().links.native_path.is_supported() {
            return Err(NativeExecutionError::NativePathUnsupported {
                provider: provider.descriptor().id.clone(),
            });
        }
        let native_path = provider
            .native_path(path.provider_path(), OperationContext::default())
            .await?;
        let local_path = native_path
            .to_local_path_buf()
            .map_err(NativeExecutionError::InvalidNativePath)?;
        Uri::from_file_path(&local_path).map_err(|()| NativeExecutionError::InvalidNativeUri {
            path: local_path.to_string_lossy().into_owned(),
        })
    }

    pub async fn validate_resource_operation(
        &self,
        resource: &MappedLspResource,
        operation: LspResourceOperation,
    ) -> Result<(), NativeExecutionError> {
        let provider_native_path = resource
            .provider()
            .native_path(
                resource.vfs_path().provider_path(),
                OperationContext::default(),
            )
            .await?;
        if provider_native_path != *resource.native_path() {
            return Err(NativeExecutionError::NativePathMismatch {
                path: resource.vfs_path().clone(),
            });
        }

        let capabilities = resource.capabilities();
        let supported = match operation {
            LspResourceOperation::Edit | LspResourceOperation::CreateFile => {
                capabilities.write.positioned.is_supported()
                    || capabilities.write.truncate.is_supported()
            }
            LspResourceOperation::CreateDirectory => {
                capabilities.mutations.create_directory.is_supported()
            }
            LspResourceOperation::Rename => capabilities.mutations.rename.is_supported(),
            LspResourceOperation::Delete => capabilities.mutations.remove.is_supported(),
        };
        if !supported {
            return Err(NativeExecutionError::UnsupportedOperation {
                provider: resource.provider().descriptor().id.clone(),
                operation,
            });
        }

        if matches!(
            operation,
            LspResourceOperation::CreateFile
                | LspResourceOperation::CreateDirectory
                | LspResourceOperation::Rename
                | LspResourceOperation::Delete
        ) {
            self.validate_writable_parent(resource, operation).await?;
        }

        if matches!(
            operation,
            LspResourceOperation::Edit | LspResourceOperation::Rename
        ) {
            match resource
                .provider()
                .stat(resource.vfs_path().provider_path(), StatOptions::default())
                .await
            {
                Ok(metadata) if !metadata.permissions.writable => {
                    return Err(NativeExecutionError::ReadOnlyResource {
                        provider: resource.provider().descriptor().id.clone(),
                        operation,
                        path: resource.vfs_path().clone(),
                    });
                }
                Ok(_) => {}
                Err(error) if error.code() == VfsErrorCode::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }

        Ok(())
    }

    async fn validate_writable_parent(
        &self,
        resource: &MappedLspResource,
        operation: LspResourceOperation,
    ) -> Result<(), NativeExecutionError> {
        let mut parent_path = resource
            .vfs_path()
            .provider_path()
            .parent()
            .unwrap_or_else(|| resource.vfs_path().provider_path().clone());
        loop {
            match resource
                .provider()
                .stat(&parent_path, StatOptions::default())
                .await
            {
                Ok(metadata) if !metadata.permissions.writable => {
                    return Err(NativeExecutionError::ReadOnlyResource {
                        provider: resource.provider().descriptor().id.clone(),
                        operation,
                        path: resource.vfs_path().clone(),
                    });
                }
                Ok(_) => return Ok(()),
                Err(error) if error.code() == VfsErrorCode::NotFound => {
                    let Some(next_parent) = parent_path.parent() else {
                        return Err(error.into());
                    };
                    parent_path = next_parent;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }

    pub async fn validate_rename(
        &self,
        source: &MappedLspResource,
        target: &MappedLspResource,
    ) -> Result<(), NativeExecutionError> {
        if source.vfs_path().mount_id() != target.vfs_path().mount_id() {
            return Err(NativeExecutionError::CrossMountRename {
                source_mount: source.vfs_path().mount_id(),
                target_mount: target.vfs_path().mount_id(),
            });
        }
        self.validate_resource_operation(source, LspResourceOperation::Rename)
            .await?;
        self.validate_resource_operation(target, LspResourceOperation::Rename)
            .await
    }
}

fn display_native_path(path: &NativePath) -> String {
    path.to_local_path_buf().map_or_else(
        |_| path.provider_path().display(),
        |path| path.to_string_lossy().into_owned(),
    )
}

fn native_path_from_file_uri(
    uri: &Uri,
    path_style: util::paths::PathStyle,
) -> Result<NativePath, NativeExecutionError> {
    let decoded_path = percent_encoding::percent_decode(uri.path().as_bytes()).collect::<Vec<_>>();
    let native_path = match path_style {
        util::paths::PathStyle::Posix => {
            if !matches!(uri.host_str(), None | Some("localhost")) {
                return Err(NativeExecutionError::InvalidFileUri {
                    uri: uri.to_string(),
                });
            }
            NativePath::from_unix_bytes(&decoded_path)
        }
        util::paths::PathStyle::Windows => {
            let mut windows_path = Vec::new();
            if let Some(host) = uri.host_str().filter(|host| *host != "localhost") {
                windows_path.extend_from_slice(br"\\");
                windows_path.extend_from_slice(host.as_bytes());
                windows_path.push(b'\\');
                windows_path.extend(
                    decoded_path
                        .iter()
                        .copied()
                        .skip_while(|byte| *byte == b'/'),
                );
            } else {
                let decoded_path =
                    if decoded_path.first() == Some(&b'/') && decoded_path.get(2) == Some(&b':') {
                        &decoded_path[1..]
                    } else {
                        decoded_path.as_slice()
                    };
                windows_path.extend_from_slice(decoded_path);
            }
            for byte in &mut windows_path {
                if *byte == b'/' {
                    *byte = b'\\';
                }
            }
            NativePath::from_windows_wtf8(&windows_path)
        }
    };
    native_path.map_err(|error| NativeExecutionError::InvalidFileUri {
        uri: format!("{uri} ({error})"),
    })
}
