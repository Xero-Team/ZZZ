use crate::{
    CopyOptions as LegacyCopyOptions, CreateOptions as LegacyCreateOptions, Fs, Metadata,
    PathEventKind, RemoveOptions as LegacyRemoveOptions, RenameOptions as LegacyRenameOptions,
    Watcher, copy_recursive,
};
use anyhow::Error as AnyhowError;
use async_lock::Mutex as AsyncMutex;
use async_trait::async_trait;
use futures::{StreamExt as _, stream, stream::BoxStream};
use std::{
    collections::BTreeMap,
    io,
    num::{NonZeroU32, NonZeroU64},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, UNIX_EPOCH},
};
use vfs::{
    Atomicity, CaseSensitivity, CollisionPolicy, CopyOptions, CreateDirOptions, CreateDisposition,
    CreateParents, DirCursor, DirEntry, DirPage, DirPageRequest, DirectoryCapabilities, EntryKind,
    EntryMetadata, EntryName, EntryPermissions, EventBatch, ExactComponent, FileAccess,
    LinkCapabilities, MutationCapabilities, NativePath, NativePathRoot, OpenOptions,
    OperationContext, PathEncoding, ProviderCapabilities, ProviderDescriptor, ProviderFileKey,
    ProviderId, ProviderLimits, ProviderPath, ReadCapabilities, RemoveKind, RemoveOptions,
    RemoveOutcome, RemovedEntryCount, RenameOptions, StatOptions, SupportLevel, SymbolicLinkMode,
    TrashCapabilities, VfsError, VfsErrorCode, VfsEvent, VfsEventKind, VfsFile, VfsOperation,
    VfsProvider, VfsResult, VfsVersion, WatchCapabilities, WatchDepth, WatchRequest,
    WriteAtOptions, WriteCapabilities, provider_path_from_legacy_utf8,
    provider_path_to_legacy_utf8,
};

const WATCH_LATENCY: Duration = Duration::from_millis(10);

#[derive(Clone, Copy)]
enum FsProviderPathMode {
    LegacyUtf8,
    Native,
}

#[derive(Clone)]
struct FsProviderCore {
    descriptor: Arc<ProviderDescriptor>,
    root: Arc<Path>,
    canonical_root: Option<Arc<Path>>,
    filesystem: Arc<dyn Fs>,
    path_mode: FsProviderPathMode,
    watch_sequence: Arc<AtomicU64>,
    mutation_lock: Arc<AsyncMutex<()>>,
}

#[derive(Clone)]
pub struct LegacyFsProvider {
    core: FsProviderCore,
}

#[derive(Clone)]
pub struct LocalProvider {
    core: FsProviderCore,
}

impl LegacyFsProvider {
    pub fn new(
        id: impl Into<Arc<str>>,
        root: impl Into<Arc<Path>>,
        filesystem: Arc<dyn Fs>,
        case_sensitivity: CaseSensitivity,
    ) -> Self {
        Self {
            core: FsProviderCore::new(
                id,
                root,
                None,
                filesystem,
                FsProviderPathMode::LegacyUtf8,
                PathEncoding::PortableUtf8,
                case_sensitivity,
            ),
        }
    }
}

impl LocalProvider {
    pub async fn new(
        id: impl Into<Arc<str>>,
        root: impl Into<Arc<Path>>,
        filesystem: Arc<dyn Fs>,
    ) -> VfsResult<Self> {
        let id = id.into();
        let root = root.into();
        let provider_id = ProviderId::new(id.clone());
        let canonical_root = filesystem.canonicalize(&root).await.map_err(|error| {
            map_anyhow_error(&provider_id, VfsOperation::NativePath, None, error)
        })?;
        let case_sensitivity = if filesystem.is_case_sensitive().await {
            CaseSensitivity::Sensitive
        } else {
            CaseSensitivity::Insensitive
        };
        Ok(Self {
            core: FsProviderCore::new(
                id,
                root,
                Some(canonical_root.into()),
                filesystem,
                FsProviderPathMode::Native,
                local_path_encoding(),
                case_sensitivity,
            ),
        })
    }
}

impl FsProviderCore {
    fn new(
        id: impl Into<Arc<str>>,
        root: impl Into<Arc<Path>>,
        canonical_root: Option<Arc<Path>>,
        filesystem: Arc<dyn Fs>,
        path_mode: FsProviderPathMode,
        path_encoding: PathEncoding,
        case_sensitivity: CaseSensitivity,
    ) -> Self {
        let id = id.into();
        Self {
            descriptor: Arc::new(ProviderDescriptor {
                id: ProviderId::new(id.clone()),
                display_name: id,
                path_encoding,
                case_sensitivity,
            }),
            root: root.into(),
            canonical_root,
            filesystem,
            path_mode,
            watch_sequence: Arc::new(AtomicU64::new(0)),
            mutation_lock: Arc::new(AsyncMutex::new(())),
        }
    }

    fn error(&self, code: VfsErrorCode, operation: VfsOperation) -> VfsError {
        VfsError::new(code, operation, self.descriptor.id.clone())
    }

    fn map_error(
        &self,
        operation: VfsOperation,
        path: Option<&ProviderPath>,
        error: AnyhowError,
    ) -> VfsError {
        map_anyhow_error(&self.descriptor.id, operation, path, error)
    }

    fn validate_path(&self, path: &ProviderPath, operation: VfsOperation) -> VfsResult<()> {
        if path.encoding() != self.descriptor.path_encoding {
            return Err(self
                .error(VfsErrorCode::InvalidPath, operation)
                .with_path(path.clone())
                .with_detail("path encoding does not match provider encoding"));
        }
        Ok(())
    }

    fn absolute_path(&self, path: &ProviderPath, operation: VfsOperation) -> VfsResult<PathBuf> {
        self.validate_path(path, operation)?;
        let relative = match self.path_mode {
            FsProviderPathMode::LegacyUtf8 => {
                PathBuf::from(provider_path_to_legacy_utf8(path).map_err(|error| {
                    self.error(VfsErrorCode::InvalidPath, operation)
                        .with_path(path.clone())
                        .with_detail(error.to_string())
                })?)
            }
            FsProviderPathMode::Native => NativePath::new(NativePathRoot::Relative, path.clone())
                .and_then(|path| path.to_local_path_buf())
                .map_err(|error| {
                    self.error(VfsErrorCode::InvalidPath, operation)
                        .with_path(path.clone())
                        .with_detail(error.to_string())
                })?,
        };
        Ok(self.root.join(relative))
    }

    fn provider_path(
        &self,
        absolute_path: &Path,
        operation: VfsOperation,
    ) -> VfsResult<ProviderPath> {
        let relative = absolute_path.strip_prefix(&self.root).map_err(|error| {
            self.error(VfsErrorCode::InvalidPath, operation)
                .with_detail(error.to_string())
        })?;
        match self.path_mode {
            FsProviderPathMode::LegacyUtf8 => {
                let relative = relative.to_str().ok_or_else(|| {
                    self.error(VfsErrorCode::InvalidPath, operation)
                        .with_detail("legacy filesystem returned a non-UTF-8 path")
                })?;
                provider_path_from_legacy_utf8(relative, self.descriptor.path_encoding).map_err(
                    |error| {
                        self.error(VfsErrorCode::InvalidPath, operation)
                            .with_detail(error.to_string())
                    },
                )
            }
            FsProviderPathMode::Native => NativePath::from_local_path(relative)
                .map(|path| path.provider_path().clone())
                .map_err(|error| {
                    self.error(VfsErrorCode::InvalidPath, operation)
                        .with_detail(error.to_string())
                }),
        }
    }

    fn visible_provider_path(
        &self,
        absolute_path: &Path,
        operation: VfsOperation,
    ) -> VfsResult<Option<ProviderPath>> {
        if !absolute_path.starts_with(self.root.as_ref()) {
            return Ok(None);
        }
        let result = self.provider_path(absolute_path, operation);
        if result
            .as_ref()
            .is_err_and(|error| error.code() == VfsErrorCode::InvalidPath)
        {
            return Ok(None);
        }
        result.map(Some)
    }

    async fn ensure_contained(
        &self,
        absolute_path: &Path,
        follow_final_component: bool,
        operation: VfsOperation,
    ) -> VfsResult<()> {
        let Some(canonical_root) = &self.canonical_root else {
            return Ok(());
        };
        let path_to_check = if follow_final_component {
            absolute_path
        } else {
            absolute_path.parent().unwrap_or(absolute_path)
        };
        let canonical = self
            .filesystem
            .canonicalize(path_to_check)
            .await
            .map_err(|error| self.map_error(operation, None, error))?;
        if !canonical.starts_with(canonical_root.as_ref()) {
            return Err(self
                .error(VfsErrorCode::PermissionDenied, operation)
                .with_detail("path resolves outside the provider root"));
        }
        Ok(())
    }

    async fn metadata(
        &self,
        path: &ProviderPath,
        options: StatOptions,
    ) -> VfsResult<EntryMetadata> {
        options
            .context
            .cancellation
            .check(VfsOperation::Stat, &self.descriptor.id)?;
        let absolute_path = self.absolute_path(path, VfsOperation::Stat)?;
        self.ensure_contained(
            &absolute_path,
            options.symbolic_link_mode == SymbolicLinkMode::Follow,
            VfsOperation::Stat,
        )
        .await?;
        let mut metadata = self
            .filesystem
            .metadata(&absolute_path)
            .await
            .map_err(|error| self.map_error(VfsOperation::Stat, Some(path), error))?
            .ok_or_else(|| {
                self.error(VfsErrorCode::NotFound, VfsOperation::Stat)
                    .with_path(path.clone())
            })?;
        let mut symbolic_link_target = None;
        if metadata.is_symlink {
            let link_target = self
                .filesystem
                .read_link(&absolute_path)
                .await
                .map_err(|error| self.map_error(VfsOperation::Stat, Some(path), error))?;
            let target_absolute = if link_target.is_absolute() {
                link_target
            } else {
                absolute_path
                    .parent()
                    .unwrap_or(self.root.as_ref())
                    .join(link_target)
            };
            symbolic_link_target =
                self.visible_provider_path(&target_absolute, VfsOperation::Stat)?;
            if options.symbolic_link_mode == SymbolicLinkMode::Follow {
                let canonical = self
                    .filesystem
                    .canonicalize(&absolute_path)
                    .await
                    .map_err(|error| self.map_error(VfsOperation::Stat, Some(path), error))?;
                metadata = self
                    .filesystem
                    .metadata(&canonical)
                    .await
                    .map_err(|error| self.map_error(VfsOperation::Stat, Some(path), error))?
                    .ok_or_else(|| {
                        self.error(VfsErrorCode::NotFound, VfsOperation::Stat)
                            .with_path(path.clone())
                    })?;
            }
        }
        Ok(metadata_to_entry(
            path,
            &metadata,
            symbolic_link_target,
            self.descriptor.case_sensitivity,
        ))
    }

    async fn check_expected_version(
        &self,
        path: &ProviderPath,
        expected_version: Option<&VfsVersion>,
        operation: VfsOperation,
    ) -> VfsResult<()> {
        let Some(expected_version) = expected_version else {
            return Ok(());
        };
        let metadata = self.metadata(path, StatOptions::default()).await?;
        let current_version = if metadata.kind == EntryKind::Directory {
            &metadata.structure_version
        } else {
            &metadata.content_version
        };
        if current_version != expected_version {
            return Err(self
                .error(VfsErrorCode::StaleVersion, operation)
                .with_path(path.clone()));
        }
        Ok(())
    }

    async fn read_directory(
        &self,
        path: &ProviderPath,
        request: DirPageRequest,
    ) -> VfsResult<DirPage> {
        request
            .context
            .cancellation
            .check(VfsOperation::ReadDirectory, &self.descriptor.id)?;
        if request.limit > provider_limits().maximum_page_size {
            return Err(self
                .error(VfsErrorCode::TooLarge, VfsOperation::ReadDirectory)
                .with_path(path.clone()));
        }
        let absolute_path = self.absolute_path(path, VfsOperation::ReadDirectory)?;
        self.ensure_contained(&absolute_path, true, VfsOperation::ReadDirectory)
            .await?;
        let cursor = match request.cursor {
            Some(cursor) => Some(
                ExactComponent::new(self.descriptor.path_encoding, cursor.as_bytes().to_vec())
                    .map_err(|error| {
                        self.error(VfsErrorCode::InvalidArgument, VfsOperation::ReadDirectory)
                            .with_path(path.clone())
                            .with_detail(error.to_string())
                    })?,
            ),
            None => None,
        };
        let mut read_dir = self
            .filesystem
            .read_dir(&absolute_path)
            .await
            .map_err(|error| self.map_error(VfsOperation::ReadDirectory, Some(path), error))?;
        let page_limit = request.limit.get() as usize;
        let mut entries = BTreeMap::new();
        let mut has_more = false;
        while let Some(child) = read_dir.next().await {
            request
                .context
                .cancellation
                .check(VfsOperation::ReadDirectory, &self.descriptor.id)?;
            let child = child
                .map_err(|error| self.map_error(VfsOperation::ReadDirectory, Some(path), error))?;
            let provider_path = self.provider_path(&child, VfsOperation::ReadDirectory)?;
            let Some(file_name) = provider_path.file_name().cloned() else {
                continue;
            };
            if cursor.as_ref().is_some_and(|cursor| &file_name <= cursor) {
                continue;
            }
            let metadata = self
                .metadata(&provider_path, StatOptions::default())
                .await?;
            entries.insert(
                file_name,
                DirEntry {
                    path: provider_path,
                    metadata,
                },
            );
            if entries.len() > page_limit {
                if let Some(last_key) = entries.last_key_value().map(|(key, _)| key.clone()) {
                    entries.remove(&last_key);
                    has_more = true;
                }
            }
        }
        let next_cursor = if has_more {
            entries
                .last_key_value()
                .map(|(name, _)| DirCursor::new(name.as_bytes().to_vec()))
        } else {
            None
        };
        Ok(DirPage {
            entries: entries.into_values().collect(),
            next_cursor,
        })
    }

    async fn create_directory(
        &self,
        path: &ProviderPath,
        options: CreateDirOptions,
    ) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::CreateDirectory, &self.descriptor.id)?;
        let paths = if options.parents == CreateParents::Yes {
            let mut paths = Vec::new();
            let mut current = ProviderPath::root(path.encoding());
            for component in path.components().cloned() {
                current = current.join_component(component).map_err(|error| {
                    self.error(VfsErrorCode::InvalidPath, VfsOperation::CreateDirectory)
                        .with_detail(error.to_string())
                })?;
                paths.push(current.clone());
            }
            paths
        } else {
            vec![path.clone()]
        };
        for path in paths {
            let absolute_path = self.absolute_path(&path, VfsOperation::CreateDirectory)?;
            if self.filesystem.is_dir(&absolute_path).await {
                continue;
            }
            self.ensure_contained(&absolute_path, false, VfsOperation::CreateDirectory)
                .await?;
            self.filesystem
                .create_dir(&absolute_path)
                .await
                .map_err(|error| {
                    self.map_error(VfsOperation::CreateDirectory, Some(&path), error)
                })?;
        }
        Ok(())
    }

    async fn remove_path(
        &self,
        path: &ProviderPath,
        options: RemoveOptions,
    ) -> VfsResult<RemoveOutcome> {
        options
            .context
            .cancellation
            .check(VfsOperation::Remove, &self.descriptor.id)?;
        self.check_expected_version(
            path,
            options.expected_version.as_ref(),
            VfsOperation::Remove,
        )
        .await?;
        let absolute_path = self.absolute_path(path, VfsOperation::Remove)?;
        self.ensure_contained(&absolute_path, false, VfsOperation::Remove)
            .await?;
        match options.kind {
            RemoveKind::File => {
                self.filesystem
                    .remove_file(&absolute_path, LegacyRemoveOptions::default())
                    .await
            }
            RemoveKind::EmptyDirectory => {
                self.filesystem
                    .remove_dir(&absolute_path, LegacyRemoveOptions::default())
                    .await
            }
            RemoveKind::Recursive => {
                let metadata = self.metadata(path, StatOptions::default()).await?;
                if metadata.kind == EntryKind::Directory {
                    self.filesystem
                        .remove_dir(
                            &absolute_path,
                            LegacyRemoveOptions {
                                recursive: true,
                                ignore_if_not_exists: false,
                            },
                        )
                        .await
                } else {
                    self.filesystem
                        .remove_file(&absolute_path, LegacyRemoveOptions::default())
                        .await
                }
            }
        }
        .map_err(|error| self.map_error(VfsOperation::Remove, Some(path), error))?;
        Ok(RemoveOutcome {
            removed_entries: if options.kind == RemoveKind::Recursive {
                RemovedEntryCount::Unknown
            } else {
                RemovedEntryCount::Exact(1)
            },
        })
    }

    async fn rename_path(
        &self,
        source: &ProviderPath,
        target: &ProviderPath,
        options: RenameOptions,
    ) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::Rename, &self.descriptor.id)?;
        if options.required_atomicity > Atomicity::AtomicWithinDirectory {
            return Err(self.error(VfsErrorCode::Unsupported, VfsOperation::Rename));
        }
        self.check_expected_version(
            source,
            options.expected_version.as_ref(),
            VfsOperation::Rename,
        )
        .await?;
        let source_absolute = self.absolute_path(source, VfsOperation::Rename)?;
        let target_absolute = self.absolute_path(target, VfsOperation::Rename)?;
        self.ensure_contained(&source_absolute, false, VfsOperation::Rename)
            .await?;
        self.ensure_contained(&target_absolute, false, VfsOperation::Rename)
            .await?;
        self.filesystem
            .rename(
                &source_absolute,
                &target_absolute,
                LegacyRenameOptions {
                    overwrite: options.collision == CollisionPolicy::Replace,
                    ignore_if_exists: false,
                    create_parents: false,
                },
            )
            .await
            .map_err(|error| self.map_error(VfsOperation::Rename, Some(source), error))
    }

    async fn copy_path(
        &self,
        source: &ProviderPath,
        target: &ProviderPath,
        options: CopyOptions,
    ) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::Copy, &self.descriptor.id)?;
        self.check_expected_version(
            source,
            options.expected_version.as_ref(),
            VfsOperation::Copy,
        )
        .await?;
        let source_absolute = self.absolute_path(source, VfsOperation::Copy)?;
        let target_absolute = self.absolute_path(target, VfsOperation::Copy)?;
        self.ensure_contained(&source_absolute, true, VfsOperation::Copy)
            .await?;
        self.ensure_contained(&target_absolute, false, VfsOperation::Copy)
            .await?;
        copy_recursive(
            self.filesystem.as_ref(),
            &source_absolute,
            &target_absolute,
            LegacyCopyOptions {
                overwrite: options.collision == CollisionPolicy::Replace,
                ignore_if_exists: false,
            },
        )
        .await
        .map_err(|error| self.map_error(VfsOperation::Copy, Some(source), error))
    }

    async fn watch_path(
        &self,
        request: WatchRequest,
    ) -> VfsResult<BoxStream<'static, VfsResult<EventBatch>>> {
        request
            .context
            .cancellation
            .check(VfsOperation::Watch, &self.descriptor.id)?;
        let absolute_path = self.absolute_path(&request.path, VfsOperation::Watch)?;
        self.ensure_contained(&absolute_path, true, VfsOperation::Watch)
            .await?;
        let (events, watcher) = self.filesystem.watch(&absolute_path, WATCH_LATENCY).await;
        let mut initial = Vec::new();
        if request.resume_after_sequence.is_some() {
            let sequence = self.next_watch_sequence()?;
            initial.push(Ok(EventBatch {
                first_sequence: sequence,
                last_sequence: sequence,
                events: vec![VfsEvent {
                    path: request.path.clone(),
                    kind: VfsEventKind::Overflow {
                        rescan_root: request.path.clone(),
                    },
                }],
            }));
        }
        let core = self.clone();
        let request_path = request.path;
        let mapped = events.map(move |events| {
            let watcher_guard: &Arc<dyn Watcher> = &watcher;
            let _watcher_lifetime = watcher_guard;
            let sequence = core.next_watch_sequence()?;
            let mut mapped_events = Vec::with_capacity(events.len());
            for event in events {
                let path = core.provider_path(&event.path, VfsOperation::Watch)?;
                if request.depth == WatchDepth::DirectChildren
                    && path != request_path
                    && path.parent().as_ref() != Some(&request_path)
                {
                    continue;
                }
                let kind = match event.kind {
                    Some(PathEventKind::Removed) => VfsEventKind::Removed,
                    Some(PathEventKind::Created) => VfsEventKind::Created,
                    Some(PathEventKind::Changed) | None => VfsEventKind::Modified,
                    Some(PathEventKind::Rescan) => VfsEventKind::Overflow {
                        rescan_root: request_path.clone(),
                    },
                };
                mapped_events.push(VfsEvent { path, kind });
            }
            Ok(EventBatch {
                first_sequence: sequence,
                last_sequence: sequence,
                events: mapped_events,
            })
        });
        Ok(Box::pin(stream::iter(initial).chain(mapped)))
    }

    fn next_watch_sequence(&self) -> VfsResult<u64> {
        self.watch_sequence
            .try_update(Ordering::AcqRel, Ordering::Acquire, |sequence| {
                sequence.checked_add(1)
            })
            .map(|previous| previous + 1)
            .map_err(|_| {
                self.error(VfsErrorCode::Internal, VfsOperation::Watch)
                    .with_detail("watch sequence exhausted")
            })
    }
}

#[async_trait]
impl VfsProvider for LegacyFsProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.core.descriptor
    }

    fn capabilities(&self) -> ProviderCapabilities {
        filesystem_capabilities(false, self.core.descriptor.case_sensitivity)
    }

    async fn stat(&self, path: &ProviderPath, options: StatOptions) -> VfsResult<EntryMetadata> {
        self.core.metadata(path, options).await
    }

    async fn read_dir(&self, path: &ProviderPath, request: DirPageRequest) -> VfsResult<DirPage> {
        self.core.read_directory(path, request).await
    }

    async fn open(&self, path: &ProviderPath, options: OpenOptions) -> VfsResult<Arc<dyn VfsFile>> {
        options
            .context
            .cancellation
            .check(VfsOperation::Open, &self.core.descriptor.id)?;
        let absolute_path = self.core.absolute_path(path, VfsOperation::Open)?;
        let metadata = self
            .core
            .filesystem
            .metadata(&absolute_path)
            .await
            .map_err(|error| self.core.map_error(VfsOperation::Open, Some(path), error))?;
        if metadata.is_some_and(|metadata| metadata.is_dir) {
            return Err(self
                .core
                .error(VfsErrorCode::IsDirectory, VfsOperation::Open)
                .with_path(path.clone()));
        }
        match (metadata, options.create) {
            (None, CreateDisposition::OpenExisting | CreateDisposition::TruncateExisting) => {
                return Err(self
                    .core
                    .error(VfsErrorCode::NotFound, VfsOperation::Open)
                    .with_path(path.clone()));
            }
            (Some(_), CreateDisposition::CreateNew) => {
                return Err(self
                    .core
                    .error(VfsErrorCode::AlreadyExists, VfsOperation::Open)
                    .with_path(path.clone()));
            }
            (None, CreateDisposition::CreateNew | CreateDisposition::OpenOrCreate) => {
                if !options.access.can_write() {
                    return Err(self
                        .core
                        .error(VfsErrorCode::InvalidArgument, VfsOperation::Open)
                        .with_path(path.clone()));
                }
                self.core
                    .filesystem
                    .create_file(&absolute_path, LegacyCreateOptions::default())
                    .await
                    .map_err(|error| self.core.map_error(VfsOperation::Open, Some(path), error))?;
            }
            (Some(_), CreateDisposition::TruncateExisting) => {
                self.core
                    .check_expected_version(
                        path,
                        options.expected_version.as_ref(),
                        VfsOperation::Open,
                    )
                    .await?;
                self.core
                    .filesystem
                    .write(&absolute_path, &[])
                    .await
                    .map_err(|error| self.core.map_error(VfsOperation::Open, Some(path), error))?;
            }
            (Some(_), CreateDisposition::OpenExisting | CreateDisposition::OpenOrCreate) => {
                self.core
                    .check_expected_version(
                        path,
                        options.expected_version.as_ref(),
                        VfsOperation::Open,
                    )
                    .await?;
            }
        }
        Ok(Arc::new(LegacyVfsFile {
            provider: self.clone(),
            path: path.clone(),
            absolute_path,
            access: options.access,
        }))
    }

    async fn create_dir(&self, path: &ProviderPath, options: CreateDirOptions) -> VfsResult<()> {
        self.core.create_directory(path, options).await
    }

    async fn remove(
        &self,
        path: &ProviderPath,
        options: RemoveOptions,
    ) -> VfsResult<RemoveOutcome> {
        self.core.remove_path(path, options).await
    }

    async fn rename(
        &self,
        source: &ProviderPath,
        target: &ProviderPath,
        options: RenameOptions,
    ) -> VfsResult<()> {
        self.core.rename_path(source, target, options).await
    }

    async fn copy(
        &self,
        source: &ProviderPath,
        target: &ProviderPath,
        options: CopyOptions,
    ) -> VfsResult<()> {
        self.core.copy_path(source, target, options).await
    }

    async fn watch(
        &self,
        request: WatchRequest,
    ) -> VfsResult<BoxStream<'static, VfsResult<EventBatch>>> {
        self.core.watch_path(request).await
    }
}

#[async_trait]
impl VfsProvider for LocalProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.core.descriptor
    }

    fn capabilities(&self) -> ProviderCapabilities {
        filesystem_capabilities(true, self.core.descriptor.case_sensitivity)
    }

    async fn stat(&self, path: &ProviderPath, options: StatOptions) -> VfsResult<EntryMetadata> {
        self.core.metadata(path, options).await
    }

    async fn read_dir(&self, path: &ProviderPath, request: DirPageRequest) -> VfsResult<DirPage> {
        self.core.read_directory(path, request).await
    }

    async fn open(&self, path: &ProviderPath, options: OpenOptions) -> VfsResult<Arc<dyn VfsFile>> {
        options
            .context
            .cancellation
            .check(VfsOperation::Open, &self.core.descriptor.id)?;
        self.core
            .check_expected_version(path, options.expected_version.as_ref(), VfsOperation::Open)
            .await?;
        let absolute_path = self.core.absolute_path(path, VfsOperation::Open)?;
        let existing_metadata = self
            .core
            .filesystem
            .metadata(&absolute_path)
            .await
            .map_err(|error| self.core.map_error(VfsOperation::Open, Some(path), error))?;
        if existing_metadata.is_some_and(|metadata| metadata.is_dir) {
            return Err(self
                .core
                .error(VfsErrorCode::IsDirectory, VfsOperation::Open)
                .with_path(path.clone()));
        }
        let follows_existing = matches!(
            options.create,
            CreateDisposition::OpenExisting
                | CreateDisposition::OpenOrCreate
                | CreateDisposition::TruncateExisting
        ) && existing_metadata.is_some();
        self.core
            .ensure_contained(&absolute_path, follows_existing, VfsOperation::Open)
            .await?;
        let access = options.access;
        let create = options.create;
        let open_path = absolute_path.clone();
        let file = smol::unblock(move || {
            let mut native_options = std::fs::OpenOptions::new();
            native_options
                .read(access.can_read())
                .write(access.can_write());
            match create {
                CreateDisposition::OpenExisting => {}
                CreateDisposition::CreateNew => {
                    native_options.create_new(true);
                }
                CreateDisposition::OpenOrCreate => {
                    native_options.create(true);
                }
                CreateDisposition::TruncateExisting => {
                    native_options.truncate(true);
                }
            }
            native_options.open(open_path)
        })
        .await
        .map_err(|error| {
            self.core
                .map_error(VfsOperation::Open, Some(path), AnyhowError::from(error))
        })?;
        Ok(Arc::new(LocalVfsFile {
            provider: self.clone(),
            path: path.clone(),
            file: Arc::new(file),
            access,
        }))
    }

    async fn create_dir(&self, path: &ProviderPath, options: CreateDirOptions) -> VfsResult<()> {
        self.core.create_directory(path, options).await
    }

    async fn remove(
        &self,
        path: &ProviderPath,
        options: RemoveOptions,
    ) -> VfsResult<RemoveOutcome> {
        self.core.remove_path(path, options).await
    }

    async fn rename(
        &self,
        source: &ProviderPath,
        target: &ProviderPath,
        options: RenameOptions,
    ) -> VfsResult<()> {
        self.core.rename_path(source, target, options).await
    }

    async fn copy(
        &self,
        source: &ProviderPath,
        target: &ProviderPath,
        options: CopyOptions,
    ) -> VfsResult<()> {
        self.core.copy_path(source, target, options).await
    }

    async fn watch(
        &self,
        request: WatchRequest,
    ) -> VfsResult<BoxStream<'static, VfsResult<EventBatch>>> {
        self.core.watch_path(request).await
    }

    async fn native_path(
        &self,
        path: &ProviderPath,
        context: OperationContext,
    ) -> VfsResult<NativePath> {
        context
            .cancellation
            .check(VfsOperation::NativePath, &self.core.descriptor.id)?;
        let absolute_path = self.core.absolute_path(path, VfsOperation::NativePath)?;
        self.core
            .ensure_contained(&absolute_path, false, VfsOperation::NativePath)
            .await?;
        NativePath::from_local_path(&absolute_path).map_err(|error| {
            self.core
                .error(VfsErrorCode::InvalidPath, VfsOperation::NativePath)
                .with_path(path.clone())
                .with_detail(error.to_string())
        })
    }
}

struct LegacyVfsFile {
    provider: LegacyFsProvider,
    path: ProviderPath,
    absolute_path: PathBuf,
    access: FileAccess,
}

#[async_trait]
impl VfsFile for LegacyVfsFile {
    async fn len(&self, context: OperationContext) -> VfsResult<u64> {
        context
            .cancellation
            .check(VfsOperation::Stat, &self.provider.core.descriptor.id)?;
        Ok(self
            .provider
            .core
            .metadata(&self.path, StatOptions::default())
            .await?
            .size)
    }

    async fn read_at(
        &self,
        offset: u64,
        buffer: &mut [u8],
        context: OperationContext,
    ) -> VfsResult<usize> {
        context
            .cancellation
            .check(VfsOperation::Read, &self.provider.core.descriptor.id)?;
        ensure_access(
            self.access.can_read(),
            &self.provider.core,
            &self.path,
            VfsOperation::Read,
        )?;
        let contents = self
            .provider
            .core
            .filesystem
            .load_bytes(&self.absolute_path)
            .await
            .map_err(|error| {
                self.provider
                    .core
                    .map_error(VfsOperation::Read, Some(&self.path), error)
            })?;
        copy_read_range(&contents, offset, buffer)
    }

    async fn write_at(
        &self,
        offset: u64,
        buffer: &[u8],
        options: WriteAtOptions,
    ) -> VfsResult<usize> {
        options
            .context
            .cancellation
            .check(VfsOperation::Write, &self.provider.core.descriptor.id)?;
        ensure_access(
            self.access.can_write(),
            &self.provider.core,
            &self.path,
            VfsOperation::Write,
        )?;
        let _mutation_guard = self.provider.core.mutation_lock.lock().await;
        self.provider
            .core
            .check_expected_version(
                &self.path,
                options.expected_version.as_ref(),
                VfsOperation::Write,
            )
            .await?;
        let mut contents = self
            .provider
            .core
            .filesystem
            .load_bytes(&self.absolute_path)
            .await
            .map_err(|error| {
                self.provider
                    .core
                    .map_error(VfsOperation::Read, Some(&self.path), error)
            })?;
        write_range(
            &mut contents,
            offset,
            buffer,
            &self.provider.core,
            &self.path,
        )?;
        self.provider
            .core
            .filesystem
            .write(&self.absolute_path, &contents)
            .await
            .map_err(|error| {
                self.provider
                    .core
                    .map_error(VfsOperation::Write, Some(&self.path), error)
            })?;
        Ok(buffer.len())
    }

    async fn set_len(&self, len: u64, options: WriteAtOptions) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::SetLength, &self.provider.core.descriptor.id)?;
        ensure_access(
            self.access.can_write(),
            &self.provider.core,
            &self.path,
            VfsOperation::SetLength,
        )?;
        let _mutation_guard = self.provider.core.mutation_lock.lock().await;
        self.provider
            .core
            .check_expected_version(
                &self.path,
                options.expected_version.as_ref(),
                VfsOperation::SetLength,
            )
            .await?;
        let mut contents = self
            .provider
            .core
            .filesystem
            .load_bytes(&self.absolute_path)
            .await
            .map_err(|error| {
                self.provider
                    .core
                    .map_error(VfsOperation::Read, Some(&self.path), error)
            })?;
        let len = usize::try_from(len).map_err(|error| {
            self.provider
                .core
                .error(VfsErrorCode::TooLarge, VfsOperation::SetLength)
                .with_path(self.path.clone())
                .with_source(error)
        })?;
        contents.resize(len, 0);
        self.provider
            .core
            .filesystem
            .write(&self.absolute_path, &contents)
            .await
            .map_err(|error| {
                self.provider
                    .core
                    .map_error(VfsOperation::Write, Some(&self.path), error)
            })
    }

    async fn flush(&self, context: OperationContext) -> VfsResult<()> {
        context
            .cancellation
            .check(VfsOperation::Flush, &self.provider.core.descriptor.id)
    }

    async fn sync(&self, context: OperationContext) -> VfsResult<()> {
        context
            .cancellation
            .check(VfsOperation::Sync, &self.provider.core.descriptor.id)
    }
}

struct LocalVfsFile {
    provider: LocalProvider,
    path: ProviderPath,
    file: Arc<std::fs::File>,
    access: FileAccess,
}

#[async_trait]
impl VfsFile for LocalVfsFile {
    async fn len(&self, context: OperationContext) -> VfsResult<u64> {
        context
            .cancellation
            .check(VfsOperation::Stat, &self.provider.core.descriptor.id)?;
        let file = self.file.clone();
        smol::unblock(move || file.metadata().map(|metadata| metadata.len()))
            .await
            .map_err(|error| {
                self.provider.core.map_error(
                    VfsOperation::Stat,
                    Some(&self.path),
                    AnyhowError::from(error),
                )
            })
    }

    async fn read_at(
        &self,
        offset: u64,
        buffer: &mut [u8],
        context: OperationContext,
    ) -> VfsResult<usize> {
        context
            .cancellation
            .check(VfsOperation::Read, &self.provider.core.descriptor.id)?;
        ensure_access(
            self.access.can_read(),
            &self.provider.core,
            &self.path,
            VfsOperation::Read,
        )?;
        let file = self.file.clone();
        let buffer_len = buffer.len();
        let (read, bytes) = smol::unblock(move || {
            let mut bytes = vec![0; buffer_len];
            let read = native_read_at(&file, offset, &mut bytes)?;
            Ok::<_, io::Error>((read, bytes))
        })
        .await
        .map_err(|error| {
            self.provider.core.map_error(
                VfsOperation::Read,
                Some(&self.path),
                AnyhowError::from(error),
            )
        })?;
        if let (Some(target), Some(source)) = (buffer.get_mut(..read), bytes.get(..read)) {
            target.copy_from_slice(source);
        }
        Ok(read)
    }

    async fn write_at(
        &self,
        offset: u64,
        buffer: &[u8],
        options: WriteAtOptions,
    ) -> VfsResult<usize> {
        options
            .context
            .cancellation
            .check(VfsOperation::Write, &self.provider.core.descriptor.id)?;
        ensure_access(
            self.access.can_write(),
            &self.provider.core,
            &self.path,
            VfsOperation::Write,
        )?;
        let _mutation_guard = if options.expected_version.is_some() {
            Some(self.provider.core.mutation_lock.lock().await)
        } else {
            None
        };
        if options.expected_version.is_some() {
            self.provider
                .core
                .check_expected_version(
                    &self.path,
                    options.expected_version.as_ref(),
                    VfsOperation::Write,
                )
                .await?;
        }
        let file = self.file.clone();
        let bytes = buffer.to_vec();
        smol::unblock(move || native_write_at(&file, offset, &bytes))
            .await
            .map_err(|error| {
                self.provider.core.map_error(
                    VfsOperation::Write,
                    Some(&self.path),
                    AnyhowError::from(error),
                )
            })
    }

    async fn set_len(&self, len: u64, options: WriteAtOptions) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::SetLength, &self.provider.core.descriptor.id)?;
        ensure_access(
            self.access.can_write(),
            &self.provider.core,
            &self.path,
            VfsOperation::SetLength,
        )?;
        let _mutation_guard = if options.expected_version.is_some() {
            Some(self.provider.core.mutation_lock.lock().await)
        } else {
            None
        };
        if options.expected_version.is_some() {
            self.provider
                .core
                .check_expected_version(
                    &self.path,
                    options.expected_version.as_ref(),
                    VfsOperation::SetLength,
                )
                .await?;
        }
        let file = self.file.clone();
        smol::unblock(move || file.set_len(len))
            .await
            .map_err(|error| {
                self.provider.core.map_error(
                    VfsOperation::SetLength,
                    Some(&self.path),
                    AnyhowError::from(error),
                )
            })
    }

    async fn flush(&self, context: OperationContext) -> VfsResult<()> {
        context
            .cancellation
            .check(VfsOperation::Flush, &self.provider.core.descriptor.id)?;
        let file = self.file.clone();
        smol::unblock(move || file.sync_data())
            .await
            .map_err(|error| {
                self.provider.core.map_error(
                    VfsOperation::Flush,
                    Some(&self.path),
                    AnyhowError::from(error),
                )
            })
    }

    async fn sync(&self, context: OperationContext) -> VfsResult<()> {
        context
            .cancellation
            .check(VfsOperation::Sync, &self.provider.core.descriptor.id)?;
        let file = self.file.clone();
        smol::unblock(move || file.sync_all())
            .await
            .map_err(|error| {
                self.provider.core.map_error(
                    VfsOperation::Sync,
                    Some(&self.path),
                    AnyhowError::from(error),
                )
            })
    }
}

fn metadata_to_entry(
    path: &ProviderPath,
    metadata: &Metadata,
    symbolic_link_target: Option<ProviderPath>,
    case_sensitivity: CaseSensitivity,
) -> EntryMetadata {
    let kind = if metadata.is_symlink {
        EntryKind::SymbolicLink
    } else if metadata.is_dir {
        EntryKind::Directory
    } else if metadata.is_fifo {
        EntryKind::Special
    } else {
        EntryKind::File
    };
    let version = metadata_version(metadata);
    EntryMetadata {
        name: path.file_name().cloned().map(EntryName::from_exact),
        kind,
        size: metadata.len,
        modified_at: Some(metadata.mtime.timestamp_for_user()),
        created_at: None,
        permissions: EntryPermissions {
            writable: metadata.is_writable,
            executable: metadata.is_executable,
            private: false,
            hidden: false,
        },
        provider_file_key: Some(ProviderFileKey::new(metadata.inode.to_be_bytes().to_vec())),
        content_version: version.clone(),
        structure_version: version,
        symbolic_link_target,
        case_sensitivity,
    }
}

fn metadata_version(metadata: &Metadata) -> VfsVersion {
    let mut bytes = Vec::with_capacity(46);
    bytes.extend_from_slice(&metadata.inode.to_be_bytes());
    bytes.extend_from_slice(&metadata.len.to_be_bytes());
    match metadata
        .mtime
        .timestamp_for_user()
        .duration_since(UNIX_EPOCH)
    {
        Ok(duration) => {
            bytes.push(1);
            bytes.extend_from_slice(&duration.as_secs().to_be_bytes());
            bytes.extend_from_slice(&duration.subsec_nanos().to_be_bytes());
        }
        Err(error) => {
            let duration = error.duration();
            bytes.push(0);
            bytes.extend_from_slice(&duration.as_secs().to_be_bytes());
            bytes.extend_from_slice(&duration.subsec_nanos().to_be_bytes());
        }
    }
    match metadata.change_token {
        Some(change_token) => {
            bytes.push(1);
            bytes.extend_from_slice(&change_token);
        }
        None => bytes.push(0),
    }
    VfsVersion::new(bytes)
}

fn filesystem_capabilities(
    native_positioned_io: bool,
    case_sensitivity: CaseSensitivity,
) -> ProviderCapabilities {
    ProviderCapabilities {
        read: ReadCapabilities {
            whole_file: SupportLevel::Native,
            stream: SupportLevel::Emulated,
            positioned: if native_positioned_io {
                SupportLevel::Native
            } else {
                SupportLevel::Emulated
            },
            atomic_snapshot: SupportLevel::Unsupported,
        },
        write: WriteCapabilities {
            positioned: if native_positioned_io {
                SupportLevel::Native
            } else {
                SupportLevel::Emulated
            },
            append: SupportLevel::Emulated,
            truncate: SupportLevel::Native,
            set_len: if native_positioned_io {
                SupportLevel::Native
            } else {
                SupportLevel::Emulated
            },
            flush: if native_positioned_io {
                SupportLevel::Native
            } else {
                SupportLevel::Emulated
            },
            sync: if native_positioned_io {
                SupportLevel::Native
            } else {
                SupportLevel::Emulated
            },
            conditional: SupportLevel::Emulated,
            maximum_atomicity: Atomicity::BestEffort,
        },
        directories: DirectoryCapabilities {
            paged: SupportLevel::Emulated,
            recursive_list: SupportLevel::Emulated,
            stat_many: SupportLevel::Emulated,
        },
        mutations: MutationCapabilities {
            create_directory: SupportLevel::Native,
            remove: SupportLevel::Native,
            rename: SupportLevel::Native,
            copy: SupportLevel::Emulated,
            cross_provider_copy: SupportLevel::Unsupported,
            idempotency: SupportLevel::Unsupported,
            maximum_rename_atomicity: Atomicity::AtomicWithinDirectory,
            maximum_delete_atomicity: Atomicity::BestEffort,
        },
        watch: WatchCapabilities {
            watch: SupportLevel::Native,
            recursive: SupportLevel::Native,
            resumable_journal: SupportLevel::Unsupported,
        },
        links: LinkCapabilities {
            symbolic_links: SupportLevel::Native,
            hard_links: SupportLevel::Unsupported,
            permissions: SupportLevel::Native,
            extended_attributes: SupportLevel::Unsupported,
            native_path: if native_positioned_io {
                SupportLevel::Native
            } else {
                SupportLevel::Unsupported
            },
        },
        trash: TrashCapabilities {
            trash: SupportLevel::Unsupported,
            restore: SupportLevel::Unsupported,
        },
        stable_file_key: SupportLevel::Native,
        case_sensitivity,
        limits: provider_limits(),
    }
}

fn provider_limits() -> ProviderLimits {
    ProviderLimits {
        maximum_page_size: NonZeroU32::new(4_096).unwrap_or(NonZeroU32::MIN),
        maximum_stat_batch: NonZeroU32::new(4_096).unwrap_or(NonZeroU32::MIN),
        maximum_range_size: NonZeroU64::new(1024 * 1024).unwrap_or(NonZeroU64::MIN),
        maximum_request_size: NonZeroU64::new(8 * 1024 * 1024).unwrap_or(NonZeroU64::MIN),
        maximum_open_handles: NonZeroU32::new(65_536).unwrap_or(NonZeroU32::MIN),
    }
}

fn ensure_access(
    allowed: bool,
    core: &FsProviderCore,
    path: &ProviderPath,
    operation: VfsOperation,
) -> VfsResult<()> {
    if allowed {
        Ok(())
    } else {
        Err(core
            .error(VfsErrorCode::PermissionDenied, operation)
            .with_path(path.clone()))
    }
}

fn copy_read_range(contents: &[u8], offset: u64, buffer: &mut [u8]) -> VfsResult<usize> {
    let Ok(offset) = usize::try_from(offset) else {
        return Ok(0);
    };
    let Some(contents) = contents.get(offset..) else {
        return Ok(0);
    };
    let read = contents.len().min(buffer.len());
    if let (Some(target), Some(source)) = (buffer.get_mut(..read), contents.get(..read)) {
        target.copy_from_slice(source);
    }
    Ok(read)
}

fn write_range(
    contents: &mut Vec<u8>,
    offset: u64,
    buffer: &[u8],
    core: &FsProviderCore,
    path: &ProviderPath,
) -> VfsResult<()> {
    let offset = usize::try_from(offset).map_err(|error| {
        core.error(VfsErrorCode::TooLarge, VfsOperation::Write)
            .with_path(path.clone())
            .with_source(error)
    })?;
    let end = offset.checked_add(buffer.len()).ok_or_else(|| {
        core.error(VfsErrorCode::TooLarge, VfsOperation::Write)
            .with_path(path.clone())
    })?;
    if contents.len() < end {
        contents.resize(end, 0);
    }
    if let Some(target) = contents.get_mut(offset..end) {
        target.copy_from_slice(buffer);
    }
    Ok(())
}

fn map_anyhow_error(
    provider: &ProviderId,
    operation: VfsOperation,
    path: Option<&ProviderPath>,
    error: AnyhowError,
) -> VfsError {
    let code =
        error
            .downcast_ref::<io::Error>()
            .map_or(VfsErrorCode::Internal, |error| match error.kind() {
                io::ErrorKind::NotFound => VfsErrorCode::NotFound,
                io::ErrorKind::AlreadyExists => VfsErrorCode::AlreadyExists,
                io::ErrorKind::PermissionDenied => VfsErrorCode::PermissionDenied,
                io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => {
                    VfsErrorCode::InvalidArgument
                }
                io::ErrorKind::TimedOut => VfsErrorCode::Timeout,
                _ => VfsErrorCode::Internal,
            });
    let detail = error.to_string();
    let mut vfs_error = VfsError::new(code, operation, provider.clone())
        .with_detail(detail)
        .with_source(AnyhowSource(error));
    if let Some(path) = path {
        vfs_error = vfs_error.with_path(path.clone());
    }
    vfs_error
}

#[derive(Debug)]
struct AnyhowSource(AnyhowError);

impl std::fmt::Display for AnyhowSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:#}", self.0)
    }
}

impl std::error::Error for AnyhowSource {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

#[cfg(unix)]
fn native_read_at(file: &std::fs::File, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
    use std::os::unix::fs::FileExt as _;
    file.read_at(buffer, offset)
}

#[cfg(windows)]
fn native_read_at(file: &std::fs::File, offset: u64, buffer: &mut [u8]) -> io::Result<usize> {
    use std::os::windows::fs::FileExt as _;
    file.seek_read(buffer, offset)
}

#[cfg(unix)]
fn native_write_at(file: &std::fs::File, offset: u64, buffer: &[u8]) -> io::Result<usize> {
    use std::os::unix::fs::FileExt as _;
    file.write_at(buffer, offset)
}

#[cfg(windows)]
fn native_write_at(file: &std::fs::File, offset: u64, buffer: &[u8]) -> io::Result<usize> {
    use std::os::windows::fs::FileExt as _;
    file.seek_write(buffer, offset)
}

#[cfg(unix)]
fn local_path_encoding() -> PathEncoding {
    PathEncoding::UnixBytes
}

#[cfg(windows)]
fn local_path_encoding() -> PathEncoding {
    PathEncoding::WindowsWtf8
}
