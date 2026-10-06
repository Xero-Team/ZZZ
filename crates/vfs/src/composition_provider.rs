use crate::{
    Atomicity, CopyOptions, CreateDirOptions, CreateDisposition, DirPage, DirPageRequest,
    EntryKind, EntryMetadata, EventBatch, FileAccess, MountId, NativePath, OpenOptions,
    OperationContext, ProviderCapabilities, ProviderDescriptor, ProviderFileKey, ProviderId,
    ProviderPath, RemoveOptions, RemoveOutcome, RenameOptions, StatOptions, SupportLevel,
    SymbolicLinkMode, VfsError, VfsErrorCode, VfsEvent, VfsEventKind, VfsFile, VfsOperation,
    VfsProvider, VfsResult, VfsVersion, WatchRequest, WriteAtOptions,
};
use async_lock::Mutex as AsyncMutex;
use async_trait::async_trait;
use futures::{StreamExt as _, stream::BoxStream};
use lru::LruCache;
use parking_lot::Mutex;
use std::{
    num::{NonZeroU64, NonZeroUsize},
    sync::Arc,
};

const DEFAULT_RANGE_CACHE_BYTES: usize = 128 * 1024 * 1024;
const DEFAULT_RANGE_CACHE_ENTRIES: usize = 4_096;
const DEFAULT_MAXIMUM_CACHEABLE_RANGE: u64 = 1024 * 1024;

#[derive(Clone)]
pub struct SubtreeProvider {
    descriptor: Arc<ProviderDescriptor>,
    inner: Arc<dyn VfsProvider>,
    prefix: ProviderPath,
    prefix_file_key: Option<ProviderFileKey>,
}

impl SubtreeProvider {
    pub async fn new(
        id: impl Into<Arc<str>>,
        inner: Arc<dyn VfsProvider>,
        prefix: ProviderPath,
    ) -> VfsResult<Self> {
        let id = id.into();
        let descriptor = Arc::new(ProviderDescriptor {
            id: ProviderId::new(id.clone()),
            display_name: id,
            path_encoding: inner.descriptor().path_encoding,
            case_sensitivity: inner.descriptor().case_sensitivity,
        });
        let mut provider = Self {
            descriptor,
            inner,
            prefix,
            prefix_file_key: None,
        };
        if provider.prefix.encoding() != provider.descriptor.path_encoding {
            return Err(provider
                .error(VfsErrorCode::InvalidPath, VfsOperation::Stat)
                .with_path(provider.prefix.clone())
                .with_detail("subtree prefix encoding does not match provider encoding"));
        }
        provider
            .ensure_inner_path_has_no_symlink(
                &provider.prefix,
                true,
                false,
                VfsOperation::Stat,
                OperationContext::default(),
            )
            .await?;
        let metadata = provider
            .inner
            .stat(
                &provider.prefix,
                StatOptions {
                    symbolic_link_mode: SymbolicLinkMode::DoNotFollow,
                    context: OperationContext::default(),
                },
            )
            .await
            .map_err(|error| provider.map_inner_error(error))?;
        if metadata.kind != EntryKind::Directory {
            return Err(provider
                .error(VfsErrorCode::NotDirectory, VfsOperation::Stat)
                .with_path(ProviderPath::root(provider.descriptor.path_encoding)));
        }
        if !provider.inner.capabilities().stable_file_key.is_supported() {
            return Err(provider
                .error(VfsErrorCode::Unsupported, VfsOperation::Stat)
                .with_path(ProviderPath::root(provider.descriptor.path_encoding))
                .with_detail("subtree authority requires a stable root file key"));
        }
        provider.prefix_file_key = Some(metadata.provider_file_key.ok_or_else(|| {
            provider
                .error(VfsErrorCode::Unsupported, VfsOperation::Stat)
                .with_path(ProviderPath::root(provider.descriptor.path_encoding))
                .with_detail("subtree root did not provide its stable file key")
        })?);
        Ok(provider)
    }

    pub fn prefix(&self) -> &ProviderPath {
        &self.prefix
    }

    fn error(&self, code: VfsErrorCode, operation: VfsOperation) -> VfsError {
        VfsError::new(code, operation, self.descriptor.id.clone())
    }

    fn validate_path_encoding(
        &self,
        path: &ProviderPath,
        operation: VfsOperation,
    ) -> VfsResult<()> {
        if path.encoding() != self.descriptor.path_encoding {
            return Err(self
                .error(VfsErrorCode::InvalidPath, operation)
                .with_path(path.clone())
                .with_detail("path encoding does not match subtree encoding"));
        }
        Ok(())
    }

    fn inner_path(&self, path: &ProviderPath, operation: VfsOperation) -> VfsResult<ProviderPath> {
        self.validate_path_encoding(path, operation)?;
        self.prefix.join(path).map_err(|error| {
            self.error(VfsErrorCode::InvalidPath, operation)
                .with_path(path.clone())
                .with_detail(error.to_string())
        })
    }

    fn exposed_path(&self, path: &ProviderPath) -> Option<ProviderPath> {
        path.strip_prefix(&self.prefix).ok()
    }

    fn map_inner_error(&self, error: VfsError) -> VfsError {
        let path = error.path().and_then(|path| self.exposed_path(path));
        error.remap(self.descriptor.id.clone(), path)
    }

    fn expose_metadata(&self, mut metadata: EntryMetadata, is_root: bool) -> EntryMetadata {
        if is_root {
            metadata.name = None;
        }
        metadata.symbolic_link_target = metadata
            .symbolic_link_target
            .as_ref()
            .and_then(|target| self.exposed_path(target));
        if metadata.symbolic_link_target.is_none() {
            metadata.symbolic_link_target_kind = None;
        }
        metadata
    }

    async fn validate_prefix_identity(
        &self,
        operation: VfsOperation,
        context: OperationContext,
    ) -> VfsResult<()> {
        let Some(expected_file_key) = &self.prefix_file_key else {
            return Ok(());
        };
        let metadata = self
            .inner
            .stat(
                &self.prefix,
                StatOptions {
                    symbolic_link_mode: SymbolicLinkMode::DoNotFollow,
                    context,
                },
            )
            .await;
        let metadata = match metadata {
            Ok(metadata) => metadata,
            Err(error) if error.code() == VfsErrorCode::NotFound => {
                return Err(self
                    .error(VfsErrorCode::StaleVersion, operation)
                    .with_path(ProviderPath::root(self.descriptor.path_encoding))
                    .with_detail("subtree root no longer exists; remount the provider"));
            }
            Err(error) => return Err(self.map_inner_error(error)),
        };
        if metadata.kind != EntryKind::Directory
            || metadata.provider_file_key.as_ref() != Some(expected_file_key)
        {
            return Err(self
                .error(VfsErrorCode::StaleVersion, operation)
                .with_path(ProviderPath::root(self.descriptor.path_encoding))
                .with_detail("subtree root identity changed; remount the provider"));
        }
        Ok(())
    }

    async fn ensure_inner_path_has_no_symlink(
        &self,
        inner_path: &ProviderPath,
        include_final_component: bool,
        allow_missing_tail: bool,
        operation: VfsOperation,
        context: OperationContext,
    ) -> VfsResult<()> {
        self.validate_prefix_identity(operation, context.clone())
            .await?;
        let component_count = if include_final_component {
            inner_path.component_count()
        } else {
            inner_path.component_count().saturating_sub(1)
        };
        let mut current = ProviderPath::root(inner_path.encoding());
        for component in inner_path.components().take(component_count) {
            current = current.join_component(component.clone()).map_err(|error| {
                self.error(VfsErrorCode::InvalidPath, operation)
                    .with_detail(error.to_string())
            })?;
            let metadata = self
                .inner
                .stat(
                    &current,
                    StatOptions {
                        symbolic_link_mode: SymbolicLinkMode::DoNotFollow,
                        context: context.clone(),
                    },
                )
                .await;
            let metadata = match metadata {
                Ok(metadata) => metadata,
                Err(error) if allow_missing_tail && error.code() == VfsErrorCode::NotFound => {
                    return Ok(());
                }
                Err(error) => return Err(self.map_inner_error(error)),
            };
            if metadata.kind == EntryKind::SymbolicLink {
                return Err(self
                    .error(VfsErrorCode::PermissionDenied, operation)
                    .with_path(
                        self.exposed_path(&current)
                            .unwrap_or_else(|| ProviderPath::root(self.descriptor.path_encoding)),
                    )
                    .with_detail("subtree operations do not follow symbolic links"));
            }
        }
        Ok(())
    }

    async fn ensure_path_has_no_symlink(
        &self,
        path: &ProviderPath,
        include_final_component: bool,
        allow_missing_tail: bool,
        operation: VfsOperation,
        context: OperationContext,
    ) -> VfsResult<ProviderPath> {
        let inner_path = self.inner_path(path, operation)?;
        self.ensure_inner_path_has_no_symlink(
            &inner_path,
            include_final_component,
            allow_missing_tail,
            operation,
            context,
        )
        .await?;
        Ok(inner_path)
    }

    fn translate_event(&self, event: VfsEvent) -> Option<VfsEvent> {
        let new_path = self.exposed_path(&event.path);
        match event.kind {
            VfsEventKind::Renamed { old_path } => {
                let old_path = self.exposed_path(&old_path);
                match (new_path, old_path) {
                    (Some(path), Some(old_path)) => Some(VfsEvent {
                        path,
                        kind: VfsEventKind::Renamed { old_path },
                    }),
                    (Some(path), None) => Some(VfsEvent {
                        path,
                        kind: VfsEventKind::Created,
                    }),
                    (None, Some(path)) => Some(VfsEvent {
                        path,
                        kind: VfsEventKind::Removed,
                    }),
                    (None, None) => None,
                }
            }
            VfsEventKind::Overflow { rescan_root } => {
                let path = new_path
                    .or_else(|| self.exposed_path(&rescan_root))
                    .unwrap_or_else(|| ProviderPath::root(self.descriptor.path_encoding));
                Some(VfsEvent {
                    path: path.clone(),
                    kind: VfsEventKind::Overflow { rescan_root: path },
                })
            }
            kind => new_path.map(|path| VfsEvent { path, kind }),
        }
    }

    fn translate_batch(&self, batch: EventBatch) -> Option<EventBatch> {
        let events = batch
            .events
            .into_iter()
            .filter_map(|event| self.translate_event(event))
            .collect::<Vec<_>>();
        if events.is_empty() {
            None
        } else {
            Some(EventBatch {
                first_sequence: batch.first_sequence,
                last_sequence: batch.last_sequence,
                events,
            })
        }
    }
}

#[async_trait]
impl VfsProvider for SubtreeProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn capabilities(&self) -> ProviderCapabilities {
        let mut capabilities = self.inner.capabilities();
        capabilities.links.symbolic_links = SupportLevel::Unsupported;
        capabilities.links.native_path = SupportLevel::Unsupported;
        capabilities
    }

    async fn stat(&self, path: &ProviderPath, options: StatOptions) -> VfsResult<EntryMetadata> {
        options
            .context
            .cancellation
            .check(VfsOperation::Stat, &self.descriptor.id)?;
        let follows_final = options.symbolic_link_mode == SymbolicLinkMode::Follow;
        let inner_path = self
            .ensure_path_has_no_symlink(
                path,
                follows_final,
                false,
                VfsOperation::Stat,
                options.context.clone(),
            )
            .await?;
        let metadata = self
            .inner
            .stat(&inner_path, options)
            .await
            .map_err(|error| self.map_inner_error(error))?;
        Ok(self.expose_metadata(metadata, path.is_root()))
    }

    async fn read_dir(&self, path: &ProviderPath, request: DirPageRequest) -> VfsResult<DirPage> {
        request
            .context
            .cancellation
            .check(VfsOperation::ReadDirectory, &self.descriptor.id)?;
        let inner_path = self
            .ensure_path_has_no_symlink(
                path,
                true,
                false,
                VfsOperation::ReadDirectory,
                request.context.clone(),
            )
            .await?;
        let page = self
            .inner
            .read_dir(&inner_path, request)
            .await
            .map_err(|error| self.map_inner_error(error))?;
        let entries = page
            .entries
            .into_iter()
            .map(|entry| {
                let path = self.exposed_path(&entry.path).ok_or_else(|| {
                    self.error(VfsErrorCode::PermissionDenied, VfsOperation::ReadDirectory)
                        .with_detail("inner provider returned an entry outside the subtree")
                })?;
                Ok(crate::DirEntry {
                    metadata: self.expose_metadata(entry.metadata, path.is_root()),
                    path,
                })
            })
            .collect::<VfsResult<Vec<_>>>()?;
        Ok(DirPage {
            entries,
            next_cursor: page.next_cursor,
        })
    }

    async fn open(&self, path: &ProviderPath, options: OpenOptions) -> VfsResult<Arc<dyn VfsFile>> {
        options
            .context
            .cancellation
            .check(VfsOperation::Open, &self.descriptor.id)?;
        let allow_missing_tail = matches!(
            options.create,
            CreateDisposition::CreateNew | CreateDisposition::OpenOrCreate
        );
        let inner_path = self
            .ensure_path_has_no_symlink(
                path,
                true,
                allow_missing_tail,
                VfsOperation::Open,
                options.context.clone(),
            )
            .await?;
        let inner = self
            .inner
            .open(&inner_path, options)
            .await
            .map_err(|error| self.map_inner_error(error))?;
        Ok(Arc::new(SubtreeFile {
            inner,
            provider: self.clone(),
            path: path.clone(),
        }))
    }

    async fn create_dir(&self, path: &ProviderPath, options: CreateDirOptions) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::CreateDirectory, &self.descriptor.id)?;
        let inner_path = self
            .ensure_path_has_no_symlink(
                path,
                false,
                true,
                VfsOperation::CreateDirectory,
                options.context.clone(),
            )
            .await?;
        self.inner
            .create_dir(&inner_path, options)
            .await
            .map_err(|error| self.map_inner_error(error))
    }

    async fn remove(
        &self,
        path: &ProviderPath,
        options: RemoveOptions,
    ) -> VfsResult<RemoveOutcome> {
        options
            .context
            .cancellation
            .check(VfsOperation::Remove, &self.descriptor.id)?;
        if path.is_root() {
            return Err(self
                .error(VfsErrorCode::InvalidPath, VfsOperation::Remove)
                .with_path(path.clone())
                .with_detail("subtree root cannot be removed"));
        }
        let inner_path = self
            .ensure_path_has_no_symlink(
                path,
                false,
                false,
                VfsOperation::Remove,
                options.context.clone(),
            )
            .await?;
        self.inner
            .remove(&inner_path, options)
            .await
            .map_err(|error| self.map_inner_error(error))
    }

    async fn rename(
        &self,
        source: &ProviderPath,
        target: &ProviderPath,
        options: RenameOptions,
    ) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::Rename, &self.descriptor.id)?;
        if source.is_root() || target.is_root() {
            return Err(self
                .error(VfsErrorCode::InvalidPath, VfsOperation::Rename)
                .with_path(target.clone())
                .with_detail("subtree root cannot participate in rename"));
        }
        let source = self
            .ensure_path_has_no_symlink(
                source,
                false,
                false,
                VfsOperation::Rename,
                options.context.clone(),
            )
            .await?;
        let target = self
            .ensure_path_has_no_symlink(
                target,
                false,
                true,
                VfsOperation::Rename,
                options.context.clone(),
            )
            .await?;
        self.inner
            .rename(&source, &target, options)
            .await
            .map_err(|error| self.map_inner_error(error))
    }

    async fn copy(
        &self,
        source: &ProviderPath,
        target: &ProviderPath,
        options: CopyOptions,
    ) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::Copy, &self.descriptor.id)?;
        if source.is_root() || target.is_root() {
            return Err(self
                .error(VfsErrorCode::InvalidPath, VfsOperation::Copy)
                .with_path(target.clone())
                .with_detail("subtree root cannot participate in copy"));
        }
        let source = self
            .ensure_path_has_no_symlink(
                source,
                true,
                false,
                VfsOperation::Copy,
                options.context.clone(),
            )
            .await?;
        let target = self
            .ensure_path_has_no_symlink(
                target,
                false,
                true,
                VfsOperation::Copy,
                options.context.clone(),
            )
            .await?;
        self.inner
            .copy(&source, &target, options)
            .await
            .map_err(|error| self.map_inner_error(error))
    }

    async fn watch(
        &self,
        request: WatchRequest,
    ) -> VfsResult<BoxStream<'static, VfsResult<EventBatch>>> {
        request
            .context
            .cancellation
            .check(VfsOperation::Watch, &self.descriptor.id)?;
        let inner_path = self
            .ensure_path_has_no_symlink(
                &request.path,
                true,
                false,
                VfsOperation::Watch,
                request.context.clone(),
            )
            .await?;
        let stream = self
            .inner
            .watch(WatchRequest {
                path: inner_path,
                depth: request.depth,
                resume_after_sequence: request.resume_after_sequence,
                context: request.context,
            })
            .await
            .map_err(|error| self.map_inner_error(error))?;
        let provider = self.clone();
        Ok(Box::pin(stream.filter_map(move |result| {
            let provider = provider.clone();
            async move {
                match result {
                    Ok(batch) => provider.translate_batch(batch).map(Ok),
                    Err(error) => Some(Err(provider.map_inner_error(error))),
                }
            }
        })))
    }

    async fn native_path(
        &self,
        path: &ProviderPath,
        context: OperationContext,
    ) -> VfsResult<NativePath> {
        context
            .cancellation
            .check(VfsOperation::NativePath, &self.descriptor.id)?;
        self.validate_path_encoding(path, VfsOperation::NativePath)?;
        Err(self
            .error(VfsErrorCode::Unsupported, VfsOperation::NativePath)
            .with_path(path.clone())
            .with_detail("subtree providers do not expose host-native paths"))
    }
}

struct SubtreeFile {
    inner: Arc<dyn VfsFile>,
    provider: SubtreeProvider,
    path: ProviderPath,
}

impl SubtreeFile {
    fn map_error(&self, error: VfsError) -> VfsError {
        error.remap(self.provider.descriptor.id.clone(), Some(self.path.clone()))
    }

    async fn validate_authority(
        &self,
        operation: VfsOperation,
        context: OperationContext,
    ) -> VfsResult<()> {
        self.provider
            .ensure_path_has_no_symlink(&self.path, true, false, operation, context)
            .await
            .map(|_| ())
    }
}

#[async_trait]
impl VfsFile for SubtreeFile {
    async fn len(&self, context: OperationContext) -> VfsResult<u64> {
        self.validate_authority(VfsOperation::Stat, context.clone())
            .await?;
        self.inner
            .len(context)
            .await
            .map_err(|error| self.map_error(error))
    }

    async fn read_at(
        &self,
        offset: u64,
        buffer: &mut [u8],
        context: OperationContext,
    ) -> VfsResult<usize> {
        self.validate_authority(VfsOperation::Read, context.clone())
            .await?;
        self.inner
            .read_at(offset, buffer, context)
            .await
            .map_err(|error| self.map_error(error))
    }

    async fn write_at(
        &self,
        offset: u64,
        buffer: &[u8],
        options: WriteAtOptions,
    ) -> VfsResult<usize> {
        self.validate_authority(VfsOperation::Write, options.context.clone())
            .await?;
        self.inner
            .write_at(offset, buffer, options)
            .await
            .map_err(|error| self.map_error(error))
    }

    async fn set_len(&self, len: u64, options: WriteAtOptions) -> VfsResult<()> {
        self.validate_authority(VfsOperation::SetLength, options.context.clone())
            .await?;
        self.inner
            .set_len(len, options)
            .await
            .map_err(|error| self.map_error(error))
    }

    async fn flush(&self, context: OperationContext) -> VfsResult<()> {
        self.validate_authority(VfsOperation::Flush, context.clone())
            .await?;
        self.inner
            .flush(context)
            .await
            .map_err(|error| self.map_error(error))
    }

    async fn sync(&self, context: OperationContext) -> VfsResult<()> {
        self.validate_authority(VfsOperation::Sync, context.clone())
            .await?;
        self.inner
            .sync(context)
            .await
            .map_err(|error| self.map_error(error))
    }
}

#[derive(Clone)]
pub struct ReadOnlyProvider {
    descriptor: Arc<ProviderDescriptor>,
    inner: Arc<dyn VfsProvider>,
}

impl ReadOnlyProvider {
    pub fn new(id: impl Into<Arc<str>>, inner: Arc<dyn VfsProvider>) -> Self {
        let id = id.into();
        Self {
            descriptor: Arc::new(ProviderDescriptor {
                id: ProviderId::new(id.clone()),
                display_name: id,
                path_encoding: inner.descriptor().path_encoding,
                case_sensitivity: inner.descriptor().case_sensitivity,
            }),
            inner,
        }
    }

    fn error(&self, code: VfsErrorCode, operation: VfsOperation) -> VfsError {
        VfsError::new(code, operation, self.descriptor.id.clone())
    }

    fn map_inner_error(&self, error: VfsError) -> VfsError {
        let path = error.path().cloned();
        error.remap(self.descriptor.id.clone(), path)
    }

    fn read_only_error(
        &self,
        path: &ProviderPath,
        operation: VfsOperation,
        context: &OperationContext,
    ) -> VfsError {
        if let Err(error) = context.cancellation.check(operation, &self.descriptor.id) {
            return error.with_path(path.clone());
        }
        self.error(VfsErrorCode::ReadOnly, operation)
            .with_path(path.clone())
    }

    fn read_only_metadata(mut metadata: EntryMetadata) -> EntryMetadata {
        metadata.permissions.writable = false;
        metadata
    }
}

#[async_trait]
impl VfsProvider for ReadOnlyProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn capabilities(&self) -> ProviderCapabilities {
        let mut capabilities = self.inner.capabilities();
        capabilities.write.positioned = SupportLevel::Unsupported;
        capabilities.write.append = SupportLevel::Unsupported;
        capabilities.write.truncate = SupportLevel::Unsupported;
        capabilities.write.set_len = SupportLevel::Unsupported;
        capabilities.write.flush = SupportLevel::Unsupported;
        capabilities.write.sync = SupportLevel::Unsupported;
        capabilities.write.conditional = SupportLevel::Unsupported;
        capabilities.write.maximum_atomicity = Atomicity::BestEffort;
        capabilities.mutations.create_directory = SupportLevel::Unsupported;
        capabilities.mutations.remove = SupportLevel::Unsupported;
        capabilities.mutations.rename = SupportLevel::Unsupported;
        capabilities.mutations.copy = SupportLevel::Unsupported;
        capabilities.mutations.cross_provider_copy = SupportLevel::Unsupported;
        capabilities.mutations.idempotency = SupportLevel::Unsupported;
        capabilities.mutations.maximum_rename_atomicity = Atomicity::BestEffort;
        capabilities.mutations.maximum_delete_atomicity = Atomicity::BestEffort;
        capabilities.links.permissions = SupportLevel::Unsupported;
        capabilities.links.native_path = SupportLevel::Unsupported;
        capabilities.trash.trash = SupportLevel::Unsupported;
        capabilities.trash.restore = SupportLevel::Unsupported;
        capabilities
    }

    async fn stat(&self, path: &ProviderPath, options: StatOptions) -> VfsResult<EntryMetadata> {
        options
            .context
            .cancellation
            .check(VfsOperation::Stat, &self.descriptor.id)?;
        self.inner
            .stat(path, options)
            .await
            .map(Self::read_only_metadata)
            .map_err(|error| self.map_inner_error(error))
    }

    async fn read_dir(&self, path: &ProviderPath, request: DirPageRequest) -> VfsResult<DirPage> {
        request
            .context
            .cancellation
            .check(VfsOperation::ReadDirectory, &self.descriptor.id)?;
        let mut page = self
            .inner
            .read_dir(path, request)
            .await
            .map_err(|error| self.map_inner_error(error))?;
        for entry in &mut page.entries {
            entry.metadata.permissions.writable = false;
        }
        Ok(page)
    }

    async fn open(&self, path: &ProviderPath, options: OpenOptions) -> VfsResult<Arc<dyn VfsFile>> {
        if options.access != FileAccess::Read || options.create != CreateDisposition::OpenExisting {
            return Err(self.read_only_error(path, VfsOperation::Open, &options.context));
        }
        options
            .context
            .cancellation
            .check(VfsOperation::Open, &self.descriptor.id)?;
        let inner = self
            .inner
            .open(path, options)
            .await
            .map_err(|error| self.map_inner_error(error))?;
        Ok(Arc::new(ReadOnlyFile {
            inner,
            provider_id: self.descriptor.id.clone(),
            path: path.clone(),
        }))
    }

    async fn create_dir(&self, path: &ProviderPath, options: CreateDirOptions) -> VfsResult<()> {
        Err(self.read_only_error(path, VfsOperation::CreateDirectory, &options.context))
    }

    async fn remove(
        &self,
        path: &ProviderPath,
        options: RemoveOptions,
    ) -> VfsResult<RemoveOutcome> {
        Err(self.read_only_error(path, VfsOperation::Remove, &options.context))
    }

    async fn rename(
        &self,
        source: &ProviderPath,
        _target: &ProviderPath,
        options: RenameOptions,
    ) -> VfsResult<()> {
        Err(self.read_only_error(source, VfsOperation::Rename, &options.context))
    }

    async fn copy(
        &self,
        source: &ProviderPath,
        _target: &ProviderPath,
        options: CopyOptions,
    ) -> VfsResult<()> {
        Err(self.read_only_error(source, VfsOperation::Copy, &options.context))
    }

    async fn watch(
        &self,
        request: WatchRequest,
    ) -> VfsResult<BoxStream<'static, VfsResult<EventBatch>>> {
        request
            .context
            .cancellation
            .check(VfsOperation::Watch, &self.descriptor.id)?;
        let provider = self.clone();
        let stream = self
            .inner
            .watch(request)
            .await
            .map_err(|error| self.map_inner_error(error))?;
        Ok(Box::pin(stream.map(move |result| {
            result.map_err(|error| provider.map_inner_error(error))
        })))
    }

    async fn native_path(
        &self,
        path: &ProviderPath,
        context: OperationContext,
    ) -> VfsResult<NativePath> {
        context
            .cancellation
            .check(VfsOperation::NativePath, &self.descriptor.id)?;
        Err(self
            .error(VfsErrorCode::Unsupported, VfsOperation::NativePath)
            .with_path(path.clone())
            .with_detail("read-only providers do not expose mutable host-native paths"))
    }
}

struct ReadOnlyFile {
    inner: Arc<dyn VfsFile>,
    provider_id: ProviderId,
    path: ProviderPath,
}

impl ReadOnlyFile {
    fn map_error(&self, error: VfsError) -> VfsError {
        error.remap(self.provider_id.clone(), Some(self.path.clone()))
    }

    fn read_only_error(&self, operation: VfsOperation, context: &OperationContext) -> VfsError {
        if let Err(error) = context.cancellation.check(operation, &self.provider_id) {
            return error.with_path(self.path.clone());
        }
        VfsError::new(VfsErrorCode::ReadOnly, operation, self.provider_id.clone())
            .with_path(self.path.clone())
    }
}

#[async_trait]
impl VfsFile for ReadOnlyFile {
    async fn len(&self, context: OperationContext) -> VfsResult<u64> {
        self.inner
            .len(context)
            .await
            .map_err(|error| self.map_error(error))
    }

    async fn read_at(
        &self,
        offset: u64,
        buffer: &mut [u8],
        context: OperationContext,
    ) -> VfsResult<usize> {
        self.inner
            .read_at(offset, buffer, context)
            .await
            .map_err(|error| self.map_error(error))
    }

    async fn write_at(
        &self,
        _offset: u64,
        _buffer: &[u8],
        options: WriteAtOptions,
    ) -> VfsResult<usize> {
        Err(self.read_only_error(VfsOperation::Write, &options.context))
    }

    async fn set_len(&self, _len: u64, options: WriteAtOptions) -> VfsResult<()> {
        Err(self.read_only_error(VfsOperation::SetLength, &options.context))
    }

    async fn flush(&self, context: OperationContext) -> VfsResult<()> {
        Err(self.read_only_error(VfsOperation::Flush, &context))
    }

    async fn sync(&self, context: OperationContext) -> VfsResult<()> {
        Err(self.read_only_error(VfsOperation::Sync, &context))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheProviderLimits {
    pub maximum_bytes: usize,
    pub maximum_entries: NonZeroUsize,
    pub maximum_cacheable_range: NonZeroU64,
}

impl Default for CacheProviderLimits {
    fn default() -> Self {
        Self {
            maximum_bytes: DEFAULT_RANGE_CACHE_BYTES,
            maximum_entries: NonZeroUsize::new(DEFAULT_RANGE_CACHE_ENTRIES)
                .unwrap_or(NonZeroUsize::MIN),
            maximum_cacheable_range: NonZeroU64::new(DEFAULT_MAXIMUM_CACHEABLE_RANGE)
                .unwrap_or(NonZeroU64::MIN),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheStatistics {
    pub entries: usize,
    pub bytes: usize,
    pub hits: u64,
    pub misses: u64,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RangeCacheKey {
    mount_id: MountId,
    path: ProviderPath,
    offset: u64,
    length: u64,
    source_version: VfsVersion,
}

struct RangeCacheState {
    entries: LruCache<RangeCacheKey, Arc<[u8]>>,
    bytes: usize,
    hits: u64,
    misses: u64,
}

#[derive(Clone)]
pub struct CacheProvider {
    descriptor: Arc<ProviderDescriptor>,
    mount_id: MountId,
    inner: Arc<dyn VfsProvider>,
    limits: CacheProviderLimits,
    cache: Arc<Mutex<RangeCacheState>>,
}

impl CacheProvider {
    pub fn new(id: impl Into<Arc<str>>, mount_id: MountId, inner: Arc<dyn VfsProvider>) -> Self {
        Self::with_limits(id, mount_id, inner, CacheProviderLimits::default())
    }

    pub fn with_limits(
        id: impl Into<Arc<str>>,
        mount_id: MountId,
        inner: Arc<dyn VfsProvider>,
        limits: CacheProviderLimits,
    ) -> Self {
        let id = id.into();
        Self {
            descriptor: Arc::new(ProviderDescriptor {
                id: ProviderId::new(id.clone()),
                display_name: id,
                path_encoding: inner.descriptor().path_encoding,
                case_sensitivity: inner.descriptor().case_sensitivity,
            }),
            mount_id,
            inner,
            limits,
            cache: Arc::new(Mutex::new(RangeCacheState {
                entries: LruCache::unbounded(),
                bytes: 0,
                hits: 0,
                misses: 0,
            })),
        }
    }

    pub fn statistics(&self) -> CacheStatistics {
        let cache = self.cache.lock();
        CacheStatistics {
            entries: cache.entries.len(),
            bytes: cache.bytes,
            hits: cache.hits,
            misses: cache.misses,
        }
    }

    fn error(&self, code: VfsErrorCode, operation: VfsOperation) -> VfsError {
        VfsError::new(code, operation, self.descriptor.id.clone())
    }

    fn map_inner_error(&self, error: VfsError) -> VfsError {
        let path = error.path().cloned();
        error.remap(self.descriptor.id.clone(), path)
    }

    fn lookup_range(&self, key: &RangeCacheKey) -> Option<Arc<[u8]>> {
        let mut cache = self.cache.lock();
        if let Some(bytes) = cache.entries.get(key).cloned() {
            cache.hits = cache.hits.saturating_add(1);
            Some(bytes)
        } else {
            cache.misses = cache.misses.saturating_add(1);
            None
        }
    }

    fn insert_range(&self, key: RangeCacheKey, bytes: Arc<[u8]>) {
        if self.limits.maximum_bytes == 0
            || bytes.len() > self.limits.maximum_bytes
            || key.length > self.limits.maximum_cacheable_range.get()
        {
            return;
        }
        let mut cache = self.cache.lock();
        if let Some(replaced) = cache.entries.put(key, bytes.clone()) {
            cache.bytes = cache.bytes.saturating_sub(replaced.len());
        }
        cache.bytes = cache.bytes.saturating_add(bytes.len());
        while cache.bytes > self.limits.maximum_bytes
            || cache.entries.len() > self.limits.maximum_entries.get()
        {
            let Some((_key, evicted)) = cache.entries.pop_lru() else {
                break;
            };
            cache.bytes = cache.bytes.saturating_sub(evicted.len());
        }
    }

    fn invalidate_all(&self) {
        let mut cache = self.cache.lock();
        cache.entries.clear();
        cache.bytes = 0;
    }
}

#[async_trait]
impl VfsProvider for CacheProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.inner.capabilities()
    }

    async fn stat(&self, path: &ProviderPath, options: StatOptions) -> VfsResult<EntryMetadata> {
        options
            .context
            .cancellation
            .check(VfsOperation::Stat, &self.descriptor.id)?;
        self.inner
            .stat(path, options)
            .await
            .map_err(|error| self.map_inner_error(error))
    }

    async fn read_dir(&self, path: &ProviderPath, request: DirPageRequest) -> VfsResult<DirPage> {
        request
            .context
            .cancellation
            .check(VfsOperation::ReadDirectory, &self.descriptor.id)?;
        self.inner
            .read_dir(path, request)
            .await
            .map_err(|error| self.map_inner_error(error))
    }

    async fn open(&self, path: &ProviderPath, options: OpenOptions) -> VfsResult<Arc<dyn VfsFile>> {
        options
            .context
            .cancellation
            .check(VfsOperation::Open, &self.descriptor.id)?;
        let open_result = self.inner.open(path, options.clone()).await;
        if options.create != CreateDisposition::OpenExisting {
            self.invalidate_all();
        }
        let inner = open_result.map_err(|error| self.map_inner_error(error))?;
        let metadata = self
            .inner
            .stat(
                path,
                StatOptions {
                    symbolic_link_mode: SymbolicLinkMode::Follow,
                    context: options.context,
                },
            )
            .await
            .map_err(|error| self.map_inner_error(error))?;
        Ok(Arc::new(CacheFile {
            inner,
            provider: self.clone(),
            path: path.clone(),
            source_version: AsyncMutex::new(CacheSourceVersion::Current(metadata.content_version)),
        }))
    }

    async fn create_dir(&self, path: &ProviderPath, options: CreateDirOptions) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::CreateDirectory, &self.descriptor.id)?;
        let result = self.inner.create_dir(path, options).await;
        self.invalidate_all();
        result.map_err(|error| self.map_inner_error(error))
    }

    async fn remove(
        &self,
        path: &ProviderPath,
        options: RemoveOptions,
    ) -> VfsResult<RemoveOutcome> {
        options
            .context
            .cancellation
            .check(VfsOperation::Remove, &self.descriptor.id)?;
        let result = self.inner.remove(path, options).await;
        self.invalidate_all();
        result.map_err(|error| self.map_inner_error(error))
    }

    async fn rename(
        &self,
        source: &ProviderPath,
        target: &ProviderPath,
        options: RenameOptions,
    ) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::Rename, &self.descriptor.id)?;
        let result = self.inner.rename(source, target, options).await;
        self.invalidate_all();
        result.map_err(|error| self.map_inner_error(error))
    }

    async fn copy(
        &self,
        source: &ProviderPath,
        target: &ProviderPath,
        options: CopyOptions,
    ) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::Copy, &self.descriptor.id)?;
        let result = self.inner.copy(source, target, options).await;
        self.invalidate_all();
        result.map_err(|error| self.map_inner_error(error))
    }

    async fn watch(
        &self,
        request: WatchRequest,
    ) -> VfsResult<BoxStream<'static, VfsResult<EventBatch>>> {
        request
            .context
            .cancellation
            .check(VfsOperation::Watch, &self.descriptor.id)?;
        let provider = self.clone();
        let stream = self
            .inner
            .watch(request)
            .await
            .map_err(|error| self.map_inner_error(error))?;
        Ok(Box::pin(stream.map(move |result| {
            provider.invalidate_all();
            result.map_err(|error| provider.map_inner_error(error))
        })))
    }

    async fn native_path(
        &self,
        path: &ProviderPath,
        context: OperationContext,
    ) -> VfsResult<NativePath> {
        context
            .cancellation
            .check(VfsOperation::NativePath, &self.descriptor.id)?;
        self.inner
            .native_path(path, context)
            .await
            .map_err(|error| self.map_inner_error(error))
    }
}

enum CacheSourceVersion {
    Current(VfsVersion),
    RefreshFailed(VfsErrorCode),
}

struct CacheFile {
    inner: Arc<dyn VfsFile>,
    provider: CacheProvider,
    path: ProviderPath,
    source_version: AsyncMutex<CacheSourceVersion>,
}

impl CacheFile {
    fn map_error(&self, error: VfsError) -> VfsError {
        self.provider.map_inner_error(error)
    }

    async fn refresh_source_version(
        &self,
        source_version: &mut CacheSourceVersion,
        context: OperationContext,
    ) {
        match self
            .provider
            .inner
            .stat(
                &self.path,
                StatOptions {
                    symbolic_link_mode: SymbolicLinkMode::Follow,
                    context,
                },
            )
            .await
        {
            Ok(metadata) => {
                *source_version = CacheSourceVersion::Current(metadata.content_version);
            }
            Err(error) => {
                *source_version = CacheSourceVersion::RefreshFailed(error.code());
            }
        }
    }

    async fn validate_source_version(
        &self,
        source_version: &CacheSourceVersion,
        context: OperationContext,
        operation: VfsOperation,
    ) -> VfsResult<VfsVersion> {
        let expected_version = match source_version {
            CacheSourceVersion::Current(version) => version,
            CacheSourceVersion::RefreshFailed(code) => {
                return Err(self
                    .provider
                    .error(VfsErrorCode::StaleVersion, operation)
                    .with_path(self.path.clone())
                    .with_detail(format!(
                        "source version refresh failed after mutation: {code:?}"
                    )));
            }
        };
        let metadata = self
            .provider
            .inner
            .stat(
                &self.path,
                StatOptions {
                    symbolic_link_mode: SymbolicLinkMode::Follow,
                    context,
                },
            )
            .await
            .map_err(|error| self.map_error(error))?;
        if metadata.content_version != *expected_version {
            return Err(self
                .provider
                .error(VfsErrorCode::StaleVersion, operation)
                .with_path(self.path.clone())
                .with_detail("cache handle source version changed; reopen the file"));
        }
        Ok(expected_version.clone())
    }
}

#[async_trait]
impl VfsFile for CacheFile {
    async fn len(&self, context: OperationContext) -> VfsResult<u64> {
        context
            .cancellation
            .check(VfsOperation::Stat, &self.provider.descriptor.id)?;
        let source_version_guard = self.source_version.lock().await;
        self.validate_source_version(&source_version_guard, context.clone(), VfsOperation::Stat)
            .await?;
        let validation_context = context.clone();
        let length = self
            .inner
            .len(context)
            .await
            .map_err(|error| self.map_error(error))?;
        self.validate_source_version(
            &source_version_guard,
            validation_context,
            VfsOperation::Stat,
        )
        .await?;
        Ok(length)
    }

    async fn read_at(
        &self,
        offset: u64,
        buffer: &mut [u8],
        context: OperationContext,
    ) -> VfsResult<usize> {
        context
            .cancellation
            .check(VfsOperation::Read, &self.provider.descriptor.id)?;
        if buffer.is_empty() {
            return Ok(0);
        }
        let source_version_guard = self.source_version.lock().await;
        let source_version = self
            .validate_source_version(&source_version_guard, context.clone(), VfsOperation::Read)
            .await?;
        let length = u64::try_from(buffer.len()).map_err(|error| {
            self.provider
                .error(VfsErrorCode::TooLarge, VfsOperation::Read)
                .with_path(self.path.clone())
                .with_source(error)
        })?;
        let key = RangeCacheKey {
            mount_id: self.provider.mount_id,
            path: self.path.clone(),
            offset,
            length,
            source_version,
        };
        if length <= self.provider.limits.maximum_cacheable_range.get() {
            if let Some(cached) = self.provider.lookup_range(&key) {
                let read_length = cached.len().min(buffer.len());
                if let (Some(target), Some(source)) =
                    (buffer.get_mut(..read_length), cached.get(..read_length))
                {
                    target.copy_from_slice(source);
                }
                self.validate_source_version(&source_version_guard, context, VfsOperation::Read)
                    .await?;
                return Ok(read_length);
            }
        }
        let validation_context = context.clone();
        let read_length = self
            .inner
            .read_at(offset, buffer, context)
            .await
            .map_err(|error| self.map_error(error))?;
        self.validate_source_version(
            &source_version_guard,
            validation_context,
            VfsOperation::Read,
        )
        .await?;
        if length <= self.provider.limits.maximum_cacheable_range.get() {
            let Some(bytes) = buffer.get(..read_length) else {
                return Err(self
                    .provider
                    .error(VfsErrorCode::Internal, VfsOperation::Read)
                    .with_path(self.path.clone())
                    .with_detail("inner provider returned a read length larger than the buffer"));
            };
            self.provider
                .insert_range(key, Arc::<[u8]>::from(bytes.to_vec()));
        }
        Ok(read_length)
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
            .check(VfsOperation::Write, &self.provider.descriptor.id)?;
        let mut source_version = self.source_version.lock().await;
        let result = self.inner.write_at(offset, buffer, options.clone()).await;
        self.provider.invalidate_all();
        let written = result.map_err(|error| self.map_error(error))?;
        self.refresh_source_version(&mut source_version, options.context)
            .await;
        Ok(written)
    }

    async fn set_len(&self, len: u64, options: WriteAtOptions) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::SetLength, &self.provider.descriptor.id)?;
        let mut source_version = self.source_version.lock().await;
        let result = self.inner.set_len(len, options.clone()).await;
        self.provider.invalidate_all();
        result.map_err(|error| self.map_error(error))?;
        self.refresh_source_version(&mut source_version, options.context)
            .await;
        Ok(())
    }

    async fn flush(&self, context: OperationContext) -> VfsResult<()> {
        context
            .cancellation
            .check(VfsOperation::Flush, &self.provider.descriptor.id)?;
        self.inner
            .flush(context)
            .await
            .map_err(|error| self.map_error(error))
    }

    async fn sync(&self, context: OperationContext) -> VfsResult<()> {
        context
            .cancellation
            .check(VfsOperation::Sync, &self.provider.descriptor.id)?;
        self.inner
            .sync(context)
            .await
            .map_err(|error| self.map_error(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CaseSensitivity, CreateParents, EntryName, EntryPermissions, ExactComponent,
        MemoryProvider, PathEncoding, ProviderFileKey, test_support::run_provider_conformance,
    };
    use futures::executor::block_on;
    use std::{
        collections::{BTreeMap, BTreeSet},
        sync::atomic::{AtomicUsize, Ordering},
        time::SystemTime,
    };

    fn path(components: &[&[u8]]) -> ProviderPath {
        ProviderPath::from_byte_components(PathEncoding::PortableUtf8, components.iter().copied())
            .unwrap_or_else(|error| panic!("fixture path must be valid: {error}"))
    }

    async fn create_directory(provider: &dyn VfsProvider, path: &ProviderPath) {
        provider
            .create_dir(
                path,
                CreateDirOptions {
                    parents: CreateParents::Yes,
                    context: OperationContext::default(),
                },
            )
            .await
            .unwrap_or_else(|error| panic!("fixture directory creation failed: {error}"));
    }

    async fn create_file(provider: &dyn VfsProvider, path: &ProviderPath, contents: &[u8]) {
        let file = provider
            .open(
                path,
                OpenOptions {
                    access: FileAccess::ReadWrite,
                    create: CreateDisposition::CreateNew,
                    expected_version: None,
                    context: OperationContext::default(),
                },
            )
            .await
            .unwrap_or_else(|error| panic!("fixture file creation failed: {error}"));
        let written = file
            .write_at(0, contents, WriteAtOptions::default())
            .await
            .unwrap_or_else(|error| panic!("fixture file write failed: {error}"));
        assert_eq!(written, contents.len());
    }

    #[test]
    fn subtree_provider_passes_shared_conformance() {
        let result = block_on(async {
            let inner = Arc::new(MemoryProvider::new(
                "subtree-inner",
                PathEncoding::PortableUtf8,
            ));
            let prefix = path(&[b"authority"]);
            create_directory(inner.as_ref(), &prefix).await;
            let provider = SubtreeProvider::new("subtree", inner, prefix)
                .await
                .map_err(|error| error.to_string())?;
            run_provider_conformance(Arc::new(provider))
                .await
                .map_err(|error| error.to_string())
        });
        assert!(result.is_ok(), "subtree conformance failed: {result:?}");
    }

    #[test]
    fn subtree_provider_confines_exact_paths_and_hides_native_mapping() {
        let result = block_on(async {
            let inner = Arc::new(MemoryProvider::new(
                "subtree-containment-inner",
                PathEncoding::PortableUtf8,
            ));
            let prefix = path(&[b"authority"]);
            let outside = path(&[b"outside"]);
            create_directory(inner.as_ref(), &prefix).await;
            create_directory(inner.as_ref(), &outside).await;
            create_file(
                inner.as_ref(),
                &path(&[b"authority", b"inside.bin"]),
                b"inside",
            )
            .await;
            create_file(
                inner.as_ref(),
                &path(&[b"outside", b"secret.bin"]),
                b"secret",
            )
            .await;
            let provider =
                SubtreeProvider::new("subtree-containment", inner.clone(), prefix.clone()).await?;
            let root = ProviderPath::root(PathEncoding::PortableUtf8);
            let page = provider.read_dir(&root, DirPageRequest::default()).await?;
            let listed = page
                .entries
                .into_iter()
                .map(|entry| entry.path)
                .collect::<BTreeSet<_>>();
            assert_eq!(listed, BTreeSet::from([path(&[b"inside.bin"])]));
            let outside_lookup = provider
                .stat(&path(&[b"secret.bin"]), StatOptions::default())
                .await;
            assert_eq!(
                outside_lookup.as_ref().err().map(VfsError::code),
                Some(VfsErrorCode::NotFound)
            );
            let native_path = provider
                .native_path(&path(&[b"inside.bin"]), OperationContext::default())
                .await;
            assert_eq!(
                native_path.as_ref().err().map(VfsError::code),
                Some(VfsErrorCode::Unsupported)
            );
            assert_eq!(
                ExactComponent::new(PathEncoding::PortableUtf8, b"..".to_vec()).err(),
                Some(crate::PathError::ParentDirectory)
            );
            assert_eq!(
                provider
                    .remove(&root, RemoveOptions::default())
                    .await
                    .as_ref()
                    .err()
                    .map(VfsError::code),
                Some(VfsErrorCode::InvalidPath)
            );
            assert_eq!(
                provider
                    .rename(&root, &path(&[b"moved"]), RenameOptions::default())
                    .await
                    .as_ref()
                    .err()
                    .map(VfsError::code),
                Some(VfsErrorCode::InvalidPath)
            );
            assert_eq!(
                provider
                    .copy(&path(&[b"inside.bin"]), &root, CopyOptions::default())
                    .await
                    .as_ref()
                    .err()
                    .map(VfsError::code),
                Some(VfsErrorCode::InvalidPath)
            );
            let opened_before_root_replacement = provider
                .open(&path(&[b"inside.bin"]), OpenOptions::default())
                .await?;
            inner
                .rename(
                    &prefix,
                    &path(&[b"retired-authority"]),
                    RenameOptions::default(),
                )
                .await?;
            create_directory(inner.as_ref(), &prefix).await;
            assert_eq!(
                provider
                    .stat(&root, StatOptions::default())
                    .await
                    .as_ref()
                    .err()
                    .map(VfsError::code),
                Some(VfsErrorCode::StaleVersion)
            );
            let mut stale_buffer = [0; 6];
            assert_eq!(
                opened_before_root_replacement
                    .read_at(0, &mut stale_buffer, OperationContext::default())
                    .await
                    .as_ref()
                    .err()
                    .map(VfsError::code),
                Some(VfsErrorCode::StaleVersion)
            );
            Ok::<(), VfsError>(())
        });
        assert!(result.is_ok(), "subtree containment failed: {result:?}");
    }

    #[test]
    fn subtree_provider_rejects_symlink_escape() {
        let result = block_on(async {
            let inner = Arc::new(EscapingSymlinkProvider::new().await);
            let provider =
                SubtreeProvider::new("subtree-symlink", inner, path(&[b"authority"])).await?;
            let link = path(&[b"link"]);
            let metadata = provider.stat(&link, StatOptions::default()).await?;
            assert_eq!(metadata.kind, EntryKind::SymbolicLink);
            assert_eq!(metadata.symbolic_link_target, None);
            let followed = provider
                .stat(
                    &link,
                    StatOptions {
                        symbolic_link_mode: SymbolicLinkMode::Follow,
                        context: OperationContext::default(),
                    },
                )
                .await;
            assert_eq!(
                followed.as_ref().err().map(VfsError::code),
                Some(VfsErrorCode::PermissionDenied)
            );
            let opened = provider
                .open(
                    &link,
                    OpenOptions {
                        access: FileAccess::Read,
                        create: CreateDisposition::OpenExisting,
                        expected_version: None,
                        context: OperationContext::default(),
                    },
                )
                .await;
            assert_eq!(
                opened.as_ref().err().map(VfsError::code),
                Some(VfsErrorCode::PermissionDenied)
            );
            Ok::<(), VfsError>(())
        });
        assert!(result.is_ok(), "symlink containment failed: {result:?}");
    }

    #[test]
    fn read_only_provider_recalculates_capabilities_and_rejects_all_writes() {
        let result = block_on(async {
            let inner = Arc::new(MemoryProvider::new(
                "readonly-inner",
                PathEncoding::PortableUtf8,
            ));
            let file_path = path(&[b"readable.bin"]);
            create_file(inner.as_ref(), &file_path, b"read-only").await;
            let provider = ReadOnlyProvider::new("readonly", inner);
            let capabilities = provider.capabilities();
            assert_eq!(capabilities.write.positioned, SupportLevel::Unsupported);
            assert_eq!(capabilities.write.append, SupportLevel::Unsupported);
            assert_eq!(capabilities.write.truncate, SupportLevel::Unsupported);
            assert_eq!(capabilities.write.set_len, SupportLevel::Unsupported);
            assert_eq!(capabilities.write.flush, SupportLevel::Unsupported);
            assert_eq!(capabilities.write.sync, SupportLevel::Unsupported);
            assert_eq!(capabilities.write.conditional, SupportLevel::Unsupported);
            assert_eq!(
                capabilities.mutations.create_directory,
                SupportLevel::Unsupported
            );
            assert_eq!(capabilities.mutations.remove, SupportLevel::Unsupported);
            assert_eq!(capabilities.mutations.rename, SupportLevel::Unsupported);
            assert_eq!(capabilities.mutations.copy, SupportLevel::Unsupported);
            assert_eq!(capabilities.links.native_path, SupportLevel::Unsupported);
            let metadata = provider.stat(&file_path, StatOptions::default()).await?;
            assert!(!metadata.permissions.writable);
            let file = provider
                .open(
                    &file_path,
                    OpenOptions {
                        access: FileAccess::Read,
                        create: CreateDisposition::OpenExisting,
                        expected_version: None,
                        context: OperationContext::default(),
                    },
                )
                .await?;
            let mut contents = vec![0; 9];
            let read = file
                .read_at(0, &mut contents, OperationContext::default())
                .await?;
            assert_eq!(read, 9);
            assert_eq!(contents, b"read-only");
            assert_read_only(file.write_at(0, b"write", WriteAtOptions::default()).await);
            assert_read_only(file.set_len(0, WriteAtOptions::default()).await);
            assert_read_only(file.flush(OperationContext::default()).await);
            assert_read_only(file.sync(OperationContext::default()).await);
            assert_read_only(
                provider
                    .open(
                        &file_path,
                        OpenOptions {
                            access: FileAccess::Write,
                            create: CreateDisposition::OpenExisting,
                            expected_version: None,
                            context: OperationContext::default(),
                        },
                    )
                    .await,
            );
            assert_read_only(
                provider
                    .create_dir(&path(&[b"new"]), CreateDirOptions::default())
                    .await,
            );
            assert_read_only(provider.remove(&file_path, RemoveOptions::default()).await);
            assert_read_only(
                provider
                    .rename(
                        &file_path,
                        &path(&[b"renamed.bin"]),
                        RenameOptions::default(),
                    )
                    .await,
            );
            assert_read_only(
                provider
                    .copy(&file_path, &path(&[b"copied.bin"]), CopyOptions::default())
                    .await,
            );
            Ok::<(), VfsError>(())
        });
        assert!(result.is_ok(), "read-only conformance failed: {result:?}");
    }

    #[test]
    fn cache_provider_passes_shared_conformance() {
        let provider: Arc<dyn VfsProvider> = Arc::new(CacheProvider::new(
            "cache-conformance",
            MountId::new(81),
            Arc::new(MemoryProvider::new(
                "cache-conformance-inner",
                PathEncoding::PortableUtf8,
            )),
        ));
        let result = block_on(run_provider_conformance(provider));
        assert!(result.is_ok(), "cache conformance failed: {result:?}");
    }

    #[test]
    fn cache_provider_uses_versioned_bounded_range_keys_and_rejects_stale_handles() {
        let result = block_on(async {
            let inner = Arc::new(MemoryProvider::new(
                "cache-range-inner",
                PathEncoding::PortableUtf8,
            ));
            let file_path = path(&[b"range.bin"]);
            create_file(inner.as_ref(), &file_path, b"abcdefgh").await;
            let provider = CacheProvider::with_limits(
                "cache-range",
                MountId::new(91),
                inner.clone(),
                CacheProviderLimits {
                    maximum_bytes: 4,
                    maximum_entries: NonZeroUsize::MIN,
                    maximum_cacheable_range: NonZeroU64::new(4).unwrap_or(NonZeroU64::MIN),
                },
            );
            let cached_file = provider.open(&file_path, OpenOptions::default()).await?;
            let mut first = [0; 4];
            cached_file
                .read_at(0, &mut first, OperationContext::default())
                .await?;
            assert_eq!(&first, b"abcd");
            cached_file
                .read_at(0, &mut first, OperationContext::default())
                .await?;
            assert_eq!(
                provider.statistics(),
                CacheStatistics {
                    entries: 1,
                    bytes: 4,
                    hits: 1,
                    misses: 1,
                }
            );
            let mut second = [0; 4];
            cached_file
                .read_at(4, &mut second, OperationContext::default())
                .await?;
            assert_eq!(&second, b"efgh");
            assert_eq!(provider.statistics().entries, 1);
            cached_file
                .read_at(0, &mut first, OperationContext::default())
                .await?;
            assert_eq!(provider.statistics().misses, 3);

            let direct_file = inner
                .open(
                    &file_path,
                    OpenOptions {
                        access: FileAccess::ReadWrite,
                        create: CreateDisposition::OpenExisting,
                        expected_version: None,
                        context: OperationContext::default(),
                    },
                )
                .await?;
            direct_file
                .write_at(0, b"WXYZ", WriteAtOptions::default())
                .await?;
            let stale_read = cached_file
                .read_at(0, &mut first, OperationContext::default())
                .await;
            assert_eq!(
                stale_read.as_ref().err().map(VfsError::code),
                Some(VfsErrorCode::StaleVersion)
            );
            let reopened = provider.open(&file_path, OpenOptions::default()).await?;
            reopened
                .read_at(0, &mut first, OperationContext::default())
                .await?;
            assert_eq!(&first, b"WXYZ");
            Ok::<(), VfsError>(())
        });
        assert!(result.is_ok(), "cache range contract failed: {result:?}");
    }

    #[test]
    fn cache_provider_rejects_a_version_change_during_the_range_read() {
        let result = block_on(async {
            let provider = CacheProvider::new(
                "cache-read-race",
                MountId::new(96),
                Arc::new(ChangingVersionProvider::new()),
            );
            let file_path = path(&[b"racing.bin"]);
            let file = provider.open(&file_path, OpenOptions::default()).await?;
            let mut contents = [0; 4];
            let read = file
                .read_at(0, &mut contents, OperationContext::default())
                .await;
            assert_eq!(
                read.as_ref().err().map(VfsError::code),
                Some(VfsErrorCode::StaleVersion)
            );
            assert_eq!(provider.statistics().entries, 0);
            Ok::<(), VfsError>(())
        });
        assert!(
            result.is_ok(),
            "cache read race was not rejected: {result:?}"
        );
    }

    #[test]
    fn composed_wrappers_reduce_capabilities_monotonically() {
        let result = block_on(async {
            let memory = Arc::new(MemoryProvider::new(
                "composition-memory",
                PathEncoding::PortableUtf8,
            ));
            let prefix = path(&[b"authority"]);
            create_directory(memory.as_ref(), &prefix).await;
            create_file(
                memory.as_ref(),
                &path(&[b"authority", b"file.bin"]),
                b"composed",
            )
            .await;
            let subtree =
                Arc::new(SubtreeProvider::new("composition-subtree", memory, prefix).await?);
            let cache = Arc::new(CacheProvider::new(
                "composition-cache",
                MountId::new(101),
                subtree.clone(),
            ));
            let read_only = ReadOnlyProvider::new("composition-readonly", cache.clone());
            assert_capabilities_no_stronger(&subtree.capabilities(), &memory_capabilities());
            assert_capabilities_no_stronger(&cache.capabilities(), &subtree.capabilities());
            assert_capabilities_no_stronger(&read_only.capabilities(), &cache.capabilities());
            assert_eq!(
                read_only.capabilities().write.positioned,
                SupportLevel::Unsupported
            );
            assert_eq!(
                read_only.capabilities().mutations.rename,
                SupportLevel::Unsupported
            );
            assert_eq!(
                read_only.capabilities().links.native_path,
                SupportLevel::Unsupported
            );
            let file = read_only
                .open(&path(&[b"file.bin"]), OpenOptions::default())
                .await?;
            let mut contents = vec![0; 8];
            file.read_at(0, &mut contents, OperationContext::default())
                .await?;
            assert_eq!(contents, b"composed");
            Ok::<(), VfsError>(())
        });
        assert!(result.is_ok(), "composition model failed: {result:?}");
    }

    #[test]
    fn overlay_model_defines_precedence_merge_whiteout_and_disconnect_translation() {
        let lower = BTreeMap::from([
            ("shared.txt", OverlayEntry::File("lower")),
            ("lower-only.txt", OverlayEntry::File("lower-only")),
            ("removed.txt", OverlayEntry::File("removed")),
        ]);
        let upper = BTreeMap::from([
            ("shared.txt", OverlayEntry::File("upper")),
            ("upper-only.txt", OverlayEntry::File("upper-only")),
            ("removed.txt", OverlayEntry::Whiteout),
        ]);
        let model = OverlayModel { upper, lower };
        assert_eq!(model.visible("shared.txt"), Some("upper"));
        assert_eq!(model.visible("lower-only.txt"), Some("lower-only"));
        assert_eq!(model.visible("removed.txt"), None);
        assert_eq!(
            model.merged_names(),
            BTreeSet::from(["lower-only.txt", "shared.txt", "upper-only.txt"])
        );
        assert_eq!(
            OverlayModel::translate_source_disconnect(path(&[b"workspace"])),
            VfsEvent {
                path: path(&[b"workspace"]),
                kind: VfsEventKind::Overflow {
                    rescan_root: path(&[b"workspace"]),
                },
            }
        );
    }

    #[test]
    fn overlay_model_rejects_cross_layer_rename_without_durable_transaction() {
        let rename = OverlayRenameModel::lower_only("source.txt", "target.txt");
        assert_eq!(
            rename.visible_after_committed_steps(0),
            BTreeSet::from(["source.txt"])
        );
        assert_eq!(
            rename.visible_after_committed_steps(1),
            BTreeSet::from(["source.txt", "target.txt"]),
            "a crash after copy-up exposes both names"
        );
        assert_eq!(
            rename.visible_after_committed_steps(2),
            BTreeSet::from(["target.txt"])
        );
        assert_eq!(
            overlay_decision(OverlayBackendCapabilities {
                durable_whiteouts: true,
                atomic_multi_path_commit: false,
                resumable_watch: true,
            }),
            OverlayDecision::Rejected(
                "cross-layer rename cannot be crash-consistent without an atomic multi-path commit"
            )
        );
    }

    fn assert_read_only<T>(result: VfsResult<T>) {
        assert_eq!(
            result.as_ref().err().map(VfsError::code),
            Some(VfsErrorCode::ReadOnly)
        );
    }

    fn memory_capabilities() -> ProviderCapabilities {
        MemoryProvider::new(
            "composition-capability-reference",
            PathEncoding::PortableUtf8,
        )
        .capabilities()
    }

    fn assert_capabilities_no_stronger(outer: &ProviderCapabilities, inner: &ProviderCapabilities) {
        for (outer_level, inner_level) in [
            (outer.read.whole_file, inner.read.whole_file),
            (outer.read.stream, inner.read.stream),
            (outer.read.positioned, inner.read.positioned),
            (outer.read.atomic_snapshot, inner.read.atomic_snapshot),
            (outer.write.positioned, inner.write.positioned),
            (outer.write.append, inner.write.append),
            (outer.write.truncate, inner.write.truncate),
            (outer.write.set_len, inner.write.set_len),
            (outer.write.flush, inner.write.flush),
            (outer.write.sync, inner.write.sync),
            (outer.write.conditional, inner.write.conditional),
            (outer.directories.paged, inner.directories.paged),
            (
                outer.directories.recursive_list,
                inner.directories.recursive_list,
            ),
            (outer.directories.stat_many, inner.directories.stat_many),
            (
                outer.mutations.create_directory,
                inner.mutations.create_directory,
            ),
            (outer.mutations.remove, inner.mutations.remove),
            (outer.mutations.rename, inner.mutations.rename),
            (outer.mutations.copy, inner.mutations.copy),
            (
                outer.mutations.cross_provider_copy,
                inner.mutations.cross_provider_copy,
            ),
            (outer.mutations.idempotency, inner.mutations.idempotency),
            (outer.watch.watch, inner.watch.watch),
            (outer.watch.recursive, inner.watch.recursive),
            (outer.watch.resumable_journal, inner.watch.resumable_journal),
            (outer.links.symbolic_links, inner.links.symbolic_links),
            (outer.links.hard_links, inner.links.hard_links),
            (outer.links.permissions, inner.links.permissions),
            (
                outer.links.extended_attributes,
                inner.links.extended_attributes,
            ),
            (outer.links.native_path, inner.links.native_path),
            (outer.trash.trash, inner.trash.trash),
            (outer.trash.restore, inner.trash.restore),
            (outer.stable_file_key, inner.stable_file_key),
        ] {
            assert!(
                support_rank(outer_level) <= support_rank(inner_level),
                "wrapper capability {outer_level:?} strengthened inner {inner_level:?}"
            );
        }
        assert!(outer.write.maximum_atomicity <= inner.write.maximum_atomicity);
        assert!(
            outer.mutations.maximum_rename_atomicity <= inner.mutations.maximum_rename_atomicity
        );
        assert!(
            outer.mutations.maximum_delete_atomicity <= inner.mutations.maximum_delete_atomicity
        );
        assert_eq!(outer.case_sensitivity, inner.case_sensitivity);
        assert!(outer.limits.maximum_page_size <= inner.limits.maximum_page_size);
        assert!(outer.limits.maximum_stat_batch <= inner.limits.maximum_stat_batch);
        assert!(outer.limits.maximum_range_size <= inner.limits.maximum_range_size);
        assert!(outer.limits.maximum_request_size <= inner.limits.maximum_request_size);
        assert!(outer.limits.maximum_open_handles <= inner.limits.maximum_open_handles);
    }

    fn support_rank(level: SupportLevel) -> u8 {
        match level {
            SupportLevel::Unsupported => 0,
            SupportLevel::Emulated => 1,
            SupportLevel::Native => 2,
        }
    }

    #[derive(Clone)]
    struct EscapingSymlinkProvider {
        inner: MemoryProvider,
    }

    impl EscapingSymlinkProvider {
        async fn new() -> Self {
            let inner = MemoryProvider::new("escaping-symlink-inner", PathEncoding::PortableUtf8);
            create_directory(&inner, &path(&[b"authority"])).await;
            create_directory(&inner, &path(&[b"outside"])).await;
            Self { inner }
        }

        fn symlink_metadata(&self) -> EntryMetadata {
            EntryMetadata {
                name: Some(EntryName::from_exact(
                    ExactComponent::new(PathEncoding::PortableUtf8, b"link".to_vec())
                        .unwrap_or_else(|error| panic!("fixture component must be valid: {error}")),
                )),
                kind: EntryKind::SymbolicLink,
                size: 0,
                modified_at: Some(SystemTime::UNIX_EPOCH),
                created_at: None,
                permissions: EntryPermissions {
                    writable: true,
                    executable: false,
                    private: false,
                    hidden: false,
                },
                provider_file_key: Some(ProviderFileKey::new(b"link".to_vec())),
                content_version: VfsVersion::new(b"link-content".to_vec()),
                structure_version: VfsVersion::new(b"link-structure".to_vec()),
                symbolic_link_target: Some(path(&[b"outside"])),
                symbolic_link_target_kind: Some(EntryKind::Directory),
                case_sensitivity: CaseSensitivity::Sensitive,
            }
        }
    }

    #[async_trait]
    impl VfsProvider for EscapingSymlinkProvider {
        fn descriptor(&self) -> &ProviderDescriptor {
            self.inner.descriptor()
        }

        fn capabilities(&self) -> ProviderCapabilities {
            let mut capabilities = self.inner.capabilities();
            capabilities.links.symbolic_links = SupportLevel::Native;
            capabilities
        }

        async fn stat(
            &self,
            requested_path: &ProviderPath,
            options: StatOptions,
        ) -> VfsResult<EntryMetadata> {
            if requested_path == &path(&[b"authority", b"link"])
                && options.symbolic_link_mode == SymbolicLinkMode::DoNotFollow
            {
                options
                    .context
                    .cancellation
                    .check(VfsOperation::Stat, &self.descriptor().id)?;
                return Ok(self.symlink_metadata());
            }
            self.inner.stat(requested_path, options).await
        }

        async fn read_dir(
            &self,
            requested_path: &ProviderPath,
            request: DirPageRequest,
        ) -> VfsResult<DirPage> {
            self.inner.read_dir(requested_path, request).await
        }

        async fn open(
            &self,
            requested_path: &ProviderPath,
            options: OpenOptions,
        ) -> VfsResult<Arc<dyn VfsFile>> {
            self.inner.open(requested_path, options).await
        }

        async fn create_dir(
            &self,
            requested_path: &ProviderPath,
            options: CreateDirOptions,
        ) -> VfsResult<()> {
            self.inner.create_dir(requested_path, options).await
        }

        async fn remove(
            &self,
            requested_path: &ProviderPath,
            options: RemoveOptions,
        ) -> VfsResult<RemoveOutcome> {
            self.inner.remove(requested_path, options).await
        }

        async fn rename(
            &self,
            source: &ProviderPath,
            target: &ProviderPath,
            options: RenameOptions,
        ) -> VfsResult<()> {
            self.inner.rename(source, target, options).await
        }

        async fn copy(
            &self,
            source: &ProviderPath,
            target: &ProviderPath,
            options: CopyOptions,
        ) -> VfsResult<()> {
            self.inner.copy(source, target, options).await
        }

        async fn watch(
            &self,
            request: WatchRequest,
        ) -> VfsResult<BoxStream<'static, VfsResult<EventBatch>>> {
            self.inner.watch(request).await
        }
    }

    #[derive(Clone)]
    struct ChangingVersionProvider {
        descriptor_source: MemoryProvider,
        stat_count: Arc<AtomicUsize>,
    }

    impl ChangingVersionProvider {
        fn new() -> Self {
            Self {
                descriptor_source: MemoryProvider::new(
                    "changing-version-inner",
                    PathEncoding::PortableUtf8,
                ),
                stat_count: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn metadata(&self, requested_path: &ProviderPath) -> EntryMetadata {
            let stat_index = self.stat_count.fetch_add(1, Ordering::SeqCst);
            let version = if stat_index < 2 { 1_u8 } else { 2_u8 };
            EntryMetadata {
                name: requested_path
                    .file_name()
                    .cloned()
                    .map(EntryName::from_exact),
                kind: EntryKind::File,
                size: 4,
                modified_at: Some(SystemTime::UNIX_EPOCH),
                created_at: None,
                permissions: EntryPermissions {
                    writable: false,
                    executable: false,
                    private: false,
                    hidden: false,
                },
                provider_file_key: Some(ProviderFileKey::new(b"racing-file".to_vec())),
                content_version: VfsVersion::new(vec![version]),
                structure_version: VfsVersion::new(vec![version]),
                symbolic_link_target: None,
                symbolic_link_target_kind: None,
                case_sensitivity: CaseSensitivity::Sensitive,
            }
        }
    }

    #[async_trait]
    impl VfsProvider for ChangingVersionProvider {
        fn descriptor(&self) -> &ProviderDescriptor {
            self.descriptor_source.descriptor()
        }

        fn capabilities(&self) -> ProviderCapabilities {
            self.descriptor_source.capabilities()
        }

        async fn stat(
            &self,
            requested_path: &ProviderPath,
            options: StatOptions,
        ) -> VfsResult<EntryMetadata> {
            options
                .context
                .cancellation
                .check(VfsOperation::Stat, &self.descriptor().id)?;
            Ok(self.metadata(requested_path))
        }

        async fn read_dir(
            &self,
            requested_path: &ProviderPath,
            request: DirPageRequest,
        ) -> VfsResult<DirPage> {
            self.descriptor_source
                .read_dir(requested_path, request)
                .await
        }

        async fn open(
            &self,
            requested_path: &ProviderPath,
            options: OpenOptions,
        ) -> VfsResult<Arc<dyn VfsFile>> {
            options
                .context
                .cancellation
                .check(VfsOperation::Open, &self.descriptor().id)?;
            Ok(Arc::new(ChangingVersionFile {
                provider_id: self.descriptor().id.clone(),
                path: requested_path.clone(),
            }))
        }

        async fn create_dir(
            &self,
            requested_path: &ProviderPath,
            options: CreateDirOptions,
        ) -> VfsResult<()> {
            self.descriptor_source
                .create_dir(requested_path, options)
                .await
        }

        async fn remove(
            &self,
            requested_path: &ProviderPath,
            options: RemoveOptions,
        ) -> VfsResult<RemoveOutcome> {
            self.descriptor_source.remove(requested_path, options).await
        }

        async fn rename(
            &self,
            source: &ProviderPath,
            target: &ProviderPath,
            options: RenameOptions,
        ) -> VfsResult<()> {
            self.descriptor_source.rename(source, target, options).await
        }

        async fn copy(
            &self,
            source: &ProviderPath,
            target: &ProviderPath,
            options: CopyOptions,
        ) -> VfsResult<()> {
            self.descriptor_source.copy(source, target, options).await
        }

        async fn watch(
            &self,
            request: WatchRequest,
        ) -> VfsResult<BoxStream<'static, VfsResult<EventBatch>>> {
            self.descriptor_source.watch(request).await
        }
    }

    struct ChangingVersionFile {
        provider_id: ProviderId,
        path: ProviderPath,
    }

    #[async_trait]
    impl VfsFile for ChangingVersionFile {
        async fn len(&self, context: OperationContext) -> VfsResult<u64> {
            context
                .cancellation
                .check(VfsOperation::Stat, &self.provider_id)?;
            Ok(4)
        }

        async fn read_at(
            &self,
            offset: u64,
            buffer: &mut [u8],
            context: OperationContext,
        ) -> VfsResult<usize> {
            context
                .cancellation
                .check(VfsOperation::Read, &self.provider_id)?;
            let contents = b"old!";
            let Ok(offset) = usize::try_from(offset) else {
                return Ok(0);
            };
            let Some(contents) = contents.get(offset..) else {
                return Ok(0);
            };
            let read_length = contents.len().min(buffer.len());
            if let (Some(target), Some(source)) =
                (buffer.get_mut(..read_length), contents.get(..read_length))
            {
                target.copy_from_slice(source);
            }
            Ok(read_length)
        }

        async fn write_at(
            &self,
            _offset: u64,
            _buffer: &[u8],
            options: WriteAtOptions,
        ) -> VfsResult<usize> {
            Err(read_only_test_error(
                &self.provider_id,
                &self.path,
                VfsOperation::Write,
                &options.context,
            ))
        }

        async fn set_len(&self, _len: u64, options: WriteAtOptions) -> VfsResult<()> {
            Err(read_only_test_error(
                &self.provider_id,
                &self.path,
                VfsOperation::SetLength,
                &options.context,
            ))
        }

        async fn flush(&self, context: OperationContext) -> VfsResult<()> {
            Err(read_only_test_error(
                &self.provider_id,
                &self.path,
                VfsOperation::Flush,
                &context,
            ))
        }

        async fn sync(&self, context: OperationContext) -> VfsResult<()> {
            Err(read_only_test_error(
                &self.provider_id,
                &self.path,
                VfsOperation::Sync,
                &context,
            ))
        }
    }

    fn read_only_test_error(
        provider_id: &ProviderId,
        requested_path: &ProviderPath,
        operation: VfsOperation,
        context: &OperationContext,
    ) -> VfsError {
        if let Err(error) = context.cancellation.check(operation, provider_id) {
            return error.with_path(requested_path.clone());
        }
        VfsError::new(VfsErrorCode::ReadOnly, operation, provider_id.clone())
            .with_path(requested_path.clone())
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum OverlayEntry {
        File(&'static str),
        Whiteout,
    }

    struct OverlayModel {
        upper: BTreeMap<&'static str, OverlayEntry>,
        lower: BTreeMap<&'static str, OverlayEntry>,
    }

    impl OverlayModel {
        fn visible(&self, name: &str) -> Option<&'static str> {
            match self.upper.get(name) {
                Some(OverlayEntry::File(contents)) => Some(contents),
                Some(OverlayEntry::Whiteout) => None,
                None => match self.lower.get(name) {
                    Some(OverlayEntry::File(contents)) => Some(contents),
                    Some(OverlayEntry::Whiteout) | None => None,
                },
            }
        }

        fn merged_names(&self) -> BTreeSet<&'static str> {
            self.lower
                .keys()
                .chain(self.upper.keys())
                .copied()
                .filter(|name| self.visible(name).is_some())
                .collect()
        }

        fn translate_source_disconnect(root: ProviderPath) -> VfsEvent {
            VfsEvent {
                path: root.clone(),
                kind: VfsEventKind::Overflow { rescan_root: root },
            }
        }
    }

    struct OverlayRenameModel {
        source: &'static str,
        target: &'static str,
    }

    impl OverlayRenameModel {
        fn lower_only(source: &'static str, target: &'static str) -> Self {
            Self { source, target }
        }

        fn visible_after_committed_steps(&self, committed_steps: usize) -> BTreeSet<&'static str> {
            let mut visible = BTreeSet::from([self.source]);
            if committed_steps >= 1 {
                visible.insert(self.target);
            }
            if committed_steps >= 2 {
                visible.remove(self.source);
            }
            visible
        }
    }

    #[derive(Clone, Copy)]
    struct OverlayBackendCapabilities {
        durable_whiteouts: bool,
        atomic_multi_path_commit: bool,
        resumable_watch: bool,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum OverlayDecision {
        Accepted,
        Rejected(&'static str),
    }

    fn overlay_decision(capabilities: OverlayBackendCapabilities) -> OverlayDecision {
        if !capabilities.durable_whiteouts {
            return OverlayDecision::Rejected("whiteouts are not durable");
        }
        if !capabilities.atomic_multi_path_commit {
            return OverlayDecision::Rejected(
                "cross-layer rename cannot be crash-consistent without an atomic multi-path commit",
            );
        }
        if !capabilities.resumable_watch {
            return OverlayDecision::Rejected("source disconnect cannot resume watcher ordering");
        }
        OverlayDecision::Accepted
    }
}
