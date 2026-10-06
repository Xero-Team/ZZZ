use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    io::Cursor,
    num::{NonZeroU32, NonZeroU64},
    panic::AssertUnwindSafe,
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use async_zip::{
    base::read1::{
        ZipOptions,
        seek::{ZipArchiveInner, ZipArchiveReader},
    },
    error::ZipError,
    spec::{constructs::CDR, headers1::Compression as ZipCompression},
};
use binrw::BinWrite as _;
use futures::{AsyncReadExt as _, FutureExt as _, io::BufReader, stream::BoxStream};

use crate::{
    Atomicity, CaseSensitivity, CopyOptions, CreateDirOptions, CreateDisposition, DirCursor,
    DirEntry, DirPage, DirPageRequest, DirectoryCapabilities, EntryKind, EntryMetadata, EntryName,
    EntryPermissions, EventBatch, FileAccess, LinkCapabilities, MutationCapabilities, OpenOptions,
    OperationContext, PathEncoding, ProviderCapabilities, ProviderDescriptor, ProviderFileKey,
    ProviderId, ProviderLimits, ProviderPath, ReadCapabilities, RemoveOptions, RemoveOutcome,
    RenameOptions, StatOptions, SupportLevel, TrashCapabilities, VfsError, VfsErrorCode, VfsFile,
    VfsFileCursor, VfsOperation, VfsProvider, VfsResult, VfsVersion, WatchCapabilities,
    WatchRequest, WriteAtOptions, WriteCapabilities,
};

const ARCHIVE_FORMAT_VERSION: u8 = 1;
const MEMBER_READ_CHUNK_SIZE: usize = 64 * 1024;
const MAXIMUM_MEMBER_RANGE_SIZE: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct ArchiveLimits {
    pub maximum_entry_count: u64,
    pub maximum_name_bytes: usize,
    pub maximum_central_directory_bytes: u64,
    pub maximum_uncompressed_member_bytes: u64,
    pub maximum_total_uncompressed_bytes: u64,
    pub maximum_compression_ratio: u64,
    pub maximum_nested_depth: u8,
    pub operation_deadline: Duration,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            maximum_entry_count: 250_000,
            maximum_name_bytes: 16 * 1024,
            maximum_central_directory_bytes: 256 * 1024 * 1024,
            maximum_uncompressed_member_bytes: 4 * 1024 * 1024 * 1024,
            maximum_total_uncompressed_bytes: 16 * 1024 * 1024 * 1024,
            maximum_compression_ratio: 1_000,
            maximum_nested_depth: 4,
            operation_deadline: Duration::from_secs(30),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveRecordMetadata {
    pub entry_index: usize,
    pub path: ProviderPath,
    pub kind: EntryKind,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub crc32: u32,
}

#[derive(Clone)]
struct ArchiveRecord {
    metadata: ArchiveRecordMetadata,
    compression: ZipCompression,
    encrypted: bool,
    executable: bool,
}

#[derive(Clone, Default)]
struct ArchiveNode {
    has_directory: bool,
    has_file: bool,
    has_symbolic_link: bool,
    explicit_entry_indices: Vec<usize>,
    children: BTreeSet<ProviderPath>,
}

impl ArchiveNode {
    fn implicit_directory() -> Self {
        Self {
            has_directory: true,
            ..Self::default()
        }
    }

    fn add_record(&mut self, record: &ArchiveRecord) {
        match record.metadata.kind {
            EntryKind::Directory => self.has_directory = true,
            EntryKind::File => self.has_file = true,
            EntryKind::SymbolicLink => self.has_symbolic_link = true,
            EntryKind::Special => {}
        }
        self.explicit_entry_indices
            .push(record.metadata.entry_index);
    }

    fn is_collision(&self) -> bool {
        let kind_count = usize::from(self.has_directory)
            + usize::from(self.has_file)
            + usize::from(self.has_symbolic_link);
        kind_count > 1 || self.explicit_entry_indices.len() > 1
    }

    fn kind(&self) -> EntryKind {
        if self.is_collision() {
            EntryKind::Special
        } else if self.has_symbolic_link {
            EntryKind::SymbolicLink
        } else if self.has_file {
            EntryKind::File
        } else {
            EntryKind::Directory
        }
    }

    fn record_index(&self) -> Option<usize> {
        (!self.is_collision())
            .then(|| self.explicit_entry_indices.first().copied())
            .flatten()
    }
}

#[derive(Clone)]
pub struct ArchiveProvider {
    descriptor: ProviderDescriptor,
    source: Arc<dyn VfsFile>,
    source_version: VfsVersion,
    version_probe: Option<Arc<dyn ArchiveVersionProbe>>,
    archive_inner: Arc<ZipArchiveInner>,
    nodes: Arc<BTreeMap<ProviderPath, ArchiveNode>>,
    records: Arc<Vec<ArchiveRecord>>,
    limits: ArchiveLimits,
    nested_depth: u8,
    zip64: bool,
}

#[async_trait]
trait ArchiveVersionProbe: Send + Sync {
    async fn current_version(&self, context: OperationContext) -> VfsResult<VfsVersion>;
}

struct ProviderArchiveVersionProbe {
    provider: Arc<dyn VfsProvider>,
    path: ProviderPath,
}

#[async_trait]
impl ArchiveVersionProbe for ProviderArchiveVersionProbe {
    async fn current_version(&self, context: OperationContext) -> VfsResult<VfsVersion> {
        Ok(self
            .provider
            .stat(
                &self.path,
                StatOptions {
                    symbolic_link_mode: crate::SymbolicLinkMode::DoNotFollow,
                    context,
                },
            )
            .await?
            .content_version)
    }
}

impl ArchiveProvider {
    pub async fn mount(
        id: impl Into<Arc<str>>,
        source: Arc<dyn VfsFile>,
        source_version: VfsVersion,
        nested_depth: u8,
        limits: ArchiveLimits,
        context: OperationContext,
    ) -> VfsResult<Self> {
        Self::mount_with_probe(
            id,
            source,
            source_version,
            None,
            nested_depth,
            limits,
            context,
        )
        .await
    }

    pub async fn mount_from_provider(
        id: impl Into<Arc<str>>,
        source_provider: Arc<dyn VfsProvider>,
        source_path: ProviderPath,
        nested_depth: u8,
        limits: ArchiveLimits,
        context: OperationContext,
    ) -> VfsResult<Self> {
        let metadata = source_provider
            .stat(
                &source_path,
                StatOptions {
                    symbolic_link_mode: crate::SymbolicLinkMode::DoNotFollow,
                    context: context.clone(),
                },
            )
            .await?;
        if metadata.kind != EntryKind::File {
            return Err(VfsError::new(
                VfsErrorCode::InvalidArgument,
                VfsOperation::Open,
                source_provider.descriptor().id.clone(),
            )
            .with_path(source_path)
            .with_detail("archive source is not a regular file"));
        }
        let source = source_provider
            .open(
                &source_path,
                OpenOptions {
                    access: FileAccess::Read,
                    create: CreateDisposition::OpenExisting,
                    expected_version: Some(metadata.content_version.clone()),
                    context: context.clone(),
                },
            )
            .await?;
        let probe: Arc<dyn ArchiveVersionProbe> = Arc::new(ProviderArchiveVersionProbe {
            provider: source_provider,
            path: source_path,
        });
        let provider = Self::mount_with_probe(
            id,
            source,
            metadata.content_version,
            Some(probe),
            nested_depth,
            limits,
            context.clone(),
        )
        .await?;
        provider.ensure_source_version(context).await?;
        Ok(provider)
    }

    async fn mount_with_probe(
        id: impl Into<Arc<str>>,
        source: Arc<dyn VfsFile>,
        source_version: VfsVersion,
        version_probe: Option<Arc<dyn ArchiveVersionProbe>>,
        nested_depth: u8,
        limits: ArchiveLimits,
        context: OperationContext,
    ) -> VfsResult<Self> {
        let id = ProviderId::new(id);
        if nested_depth > limits.maximum_nested_depth {
            return Err(
                VfsError::new(VfsErrorCode::TooLarge, VfsOperation::Open, id)
                    .with_detail("archive nesting depth exceeds the configured limit"),
            );
        }

        let descriptor = ProviderDescriptor {
            display_name: Arc::from("Archive"),
            path_encoding: PathEncoding::UnixBytes,
            case_sensitivity: CaseSensitivity::Sensitive,
            id,
        };
        let cursor = BufReader::new(VfsFileCursor::new(source.clone(), context.clone()));
        let options = zip_options(&limits);
        let provider_id = descriptor.id.clone();
        let open = async move {
            let result = AssertUnwindSafe(ZipArchiveReader::open_with_options(cursor, options))
                .catch_unwind()
                .await;
            match result {
                Ok(result) => result
                    .map_err(|error| map_zip_error(&provider_id, VfsOperation::Open, None, error)),
                Err(_) => Err(VfsError::new(
                    VfsErrorCode::CorruptData,
                    VfsOperation::Open,
                    provider_id,
                )
                .with_detail("archive parser panicked while reading untrusted metadata")),
            }
        };
        let reader = run_with_deadline(
            &descriptor.id,
            VfsOperation::Open,
            &context,
            limits.operation_deadline,
            open,
        )
        .await?;
        let archive_inner = reader.inner().clone();
        let zip64 = archive_inner.ceocdr().is_zip64();
        let (nodes, records) =
            build_index(&descriptor.id, archive_inner.cdrs(), &limits, &context)?;

        Ok(Self {
            descriptor,
            source,
            source_version,
            version_probe,
            archive_inner,
            nodes: Arc::new(nodes),
            records: Arc::new(records),
            limits,
            nested_depth,
            zip64,
        })
    }

    async fn ensure_source_version(&self, context: OperationContext) -> VfsResult<()> {
        let Some(version_probe) = &self.version_probe else {
            return Ok(());
        };
        let current_version = version_probe.current_version(context).await?;
        if current_version != self.source_version {
            return Err(VfsError::new(
                VfsErrorCode::StaleVersion,
                VfsOperation::Open,
                self.descriptor.id.clone(),
            )
            .with_detail("archive source version changed; remount is required"));
        }
        Ok(())
    }

    pub fn is_zip64(&self) -> bool {
        self.zip64
    }

    pub fn nested_depth(&self) -> u8 {
        self.nested_depth
    }

    pub fn source_version(&self) -> &VfsVersion {
        &self.source_version
    }

    pub fn records_for_path(&self, path: &ProviderPath) -> Vec<ArchiveRecordMetadata> {
        self.nodes
            .get(path)
            .into_iter()
            .flat_map(|node| &node.explicit_entry_indices)
            .filter_map(|index| self.records.get(*index))
            .map(|record| record.metadata.clone())
            .collect()
    }

    fn validate_path(&self, path: &ProviderPath, operation: VfsOperation) -> VfsResult<()> {
        if path.encoding() != PathEncoding::UnixBytes {
            return Err(self
                .error(VfsErrorCode::InvalidPath, operation, Some(path))
                .with_detail("archive paths use exact raw ZIP member bytes"));
        }
        Ok(())
    }

    fn error(
        &self,
        code: VfsErrorCode,
        operation: VfsOperation,
        path: Option<&ProviderPath>,
    ) -> VfsError {
        let error = VfsError::new(code, operation, self.descriptor.id.clone());
        match path {
            Some(path) => error.with_path(path.clone()),
            None => error,
        }
    }

    fn node(&self, path: &ProviderPath, operation: VfsOperation) -> VfsResult<&ArchiveNode> {
        self.validate_path(path, operation)?;
        let node = self
            .nodes
            .get(path)
            .ok_or_else(|| self.error(VfsErrorCode::NotFound, operation, Some(path)))?;
        if node.is_collision() {
            return Err(self
                .error(VfsErrorCode::NameCollision, operation, Some(path))
                .with_detail("multiple archive records resolve to this exact path"));
        }
        Ok(node)
    }

    fn metadata_for_node(
        &self,
        path: &ProviderPath,
        node: &ArchiveNode,
        expose_collision: bool,
    ) -> VfsResult<EntryMetadata> {
        if node.is_collision() && !expose_collision {
            return Err(self
                .error(VfsErrorCode::NameCollision, VfsOperation::Stat, Some(path))
                .with_detail("multiple archive records resolve to this exact path"));
        }
        let record = node
            .record_index()
            .and_then(|index| self.records.get(index));
        let kind = node.kind();
        let size = record.map_or(0, |record| record.metadata.uncompressed_size);
        let executable = record.is_some_and(|record| record.executable);
        let version = self.version_for_node(path, node);
        Ok(EntryMetadata {
            name: path.file_name().cloned().map(EntryName::from_exact),
            kind,
            size,
            modified_at: None,
            created_at: None,
            permissions: EntryPermissions {
                writable: false,
                executable,
                private: false,
                hidden: path
                    .file_name()
                    .is_some_and(|name| name.as_bytes().starts_with(b".")),
            },
            provider_file_key: Some(ProviderFileKey::new(provider_path_key(path))),
            content_version: version.clone(),
            structure_version: version,
            symbolic_link_target: None,
            symbolic_link_target_kind: None,
            case_sensitivity: CaseSensitivity::Sensitive,
        })
    }

    fn version_for_node(&self, path: &ProviderPath, node: &ArchiveNode) -> VfsVersion {
        let mut bytes = Vec::new();
        bytes.push(ARCHIVE_FORMAT_VERSION);
        bytes.extend_from_slice(self.source_version.as_bytes());
        bytes.extend_from_slice(&provider_path_key(path));
        bytes.push(match node.kind() {
            EntryKind::File => 1,
            EntryKind::Directory => 2,
            EntryKind::SymbolicLink => 3,
            EntryKind::Special => 4,
        });
        for index in &node.explicit_entry_indices {
            bytes.extend_from_slice(&index.to_be_bytes());
        }
        VfsVersion::new(bytes)
    }

    async fn validate_member_open(
        &self,
        path: &ProviderPath,
        record: &ArchiveRecord,
        context: &OperationContext,
    ) -> VfsResult<()> {
        if record.encrypted {
            return Err(self
                .error(VfsErrorCode::Unsupported, VfsOperation::Open, Some(path))
                .with_detail("encrypted archive entries are not supported"));
        }
        if !matches!(
            record.compression,
            ZipCompression::Stored | ZipCompression::Deflate | ZipCompression::Deflate64
        ) {
            return Err(self
                .error(VfsErrorCode::Unsupported, VfsOperation::Open, Some(path))
                .with_detail(format!(
                    "archive compression method {:?} is not supported",
                    record.compression
                )));
        }

        let cursor = BufReader::new(VfsFileCursor::new(self.source.clone(), context.clone()));
        let mut reader = ZipArchiveReader::new_with_inner(cursor, self.archive_inner.clone());
        let provider_id = self.descriptor.id.clone();
        let member_path = path.clone();
        let index = record.metadata.entry_index;
        let open = async move {
            let result = AssertUnwindSafe(reader.file(index)).catch_unwind().await;
            match result {
                Ok(result) => result.map(|_| ()).map_err(|error| {
                    map_zip_error(&provider_id, VfsOperation::Open, Some(&member_path), error)
                }),
                Err(_) => Err(VfsError::new(
                    VfsErrorCode::CorruptData,
                    VfsOperation::Open,
                    provider_id,
                )
                .with_path(member_path)
                .with_detail("archive parser panicked while opening an untrusted member")),
            }
        };
        run_with_deadline(
            &self.descriptor.id,
            VfsOperation::Open,
            context,
            self.limits.operation_deadline,
            open,
        )
        .await
    }
}

#[async_trait]
impl VfsProvider for ArchiveProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            read: ReadCapabilities {
                whole_file: SupportLevel::Emulated,
                stream: SupportLevel::Emulated,
                positioned: SupportLevel::Emulated,
                atomic_snapshot: SupportLevel::Native,
            },
            write: WriteCapabilities {
                positioned: SupportLevel::Unsupported,
                append: SupportLevel::Unsupported,
                truncate: SupportLevel::Unsupported,
                set_len: SupportLevel::Unsupported,
                flush: SupportLevel::Unsupported,
                sync: SupportLevel::Unsupported,
                conditional: SupportLevel::Unsupported,
                maximum_atomicity: Atomicity::BestEffort,
            },
            directories: DirectoryCapabilities {
                paged: SupportLevel::Native,
                recursive_list: SupportLevel::Emulated,
                stat_many: SupportLevel::Emulated,
            },
            mutations: MutationCapabilities {
                create_directory: SupportLevel::Unsupported,
                remove: SupportLevel::Unsupported,
                rename: SupportLevel::Unsupported,
                copy: SupportLevel::Unsupported,
                cross_provider_copy: SupportLevel::Unsupported,
                idempotency: SupportLevel::Unsupported,
                maximum_rename_atomicity: Atomicity::BestEffort,
                maximum_delete_atomicity: Atomicity::BestEffort,
            },
            watch: WatchCapabilities {
                watch: SupportLevel::Unsupported,
                recursive: SupportLevel::Unsupported,
                resumable_journal: SupportLevel::Unsupported,
            },
            links: LinkCapabilities {
                symbolic_links: SupportLevel::Unsupported,
                hard_links: SupportLevel::Unsupported,
                permissions: SupportLevel::Unsupported,
                extended_attributes: SupportLevel::Unsupported,
                native_path: SupportLevel::Unsupported,
            },
            trash: TrashCapabilities {
                trash: SupportLevel::Unsupported,
                restore: SupportLevel::Unsupported,
            },
            stable_file_key: SupportLevel::Native,
            case_sensitivity: CaseSensitivity::Sensitive,
            limits: ProviderLimits {
                maximum_page_size: NonZeroU32::new(4_096).unwrap_or(NonZeroU32::MIN),
                maximum_stat_batch: NonZeroU32::new(4_096).unwrap_or(NonZeroU32::MIN),
                maximum_range_size: NonZeroU64::new(8 * 1024 * 1024).unwrap_or(NonZeroU64::MIN),
                maximum_request_size: NonZeroU64::new(16 * 1024 * 1024).unwrap_or(NonZeroU64::MIN),
                maximum_open_handles: NonZeroU32::new(1_024).unwrap_or(NonZeroU32::MIN),
            },
        }
    }

    async fn stat(&self, path: &ProviderPath, options: StatOptions) -> VfsResult<EntryMetadata> {
        options
            .context
            .cancellation
            .check(VfsOperation::Stat, &self.descriptor.id)?;
        self.ensure_source_version(options.context.clone()).await?;
        let node = self.node(path, VfsOperation::Stat)?;
        self.metadata_for_node(path, node, false)
    }

    async fn read_dir(&self, path: &ProviderPath, request: DirPageRequest) -> VfsResult<DirPage> {
        request
            .context
            .cancellation
            .check(VfsOperation::ReadDirectory, &self.descriptor.id)?;
        self.ensure_source_version(request.context.clone()).await?;
        let node = self.node(path, VfsOperation::ReadDirectory)?;
        if node.kind() != EntryKind::Directory {
            return Err(self.error(
                VfsErrorCode::NotDirectory,
                VfsOperation::ReadDirectory,
                Some(path),
            ));
        }
        let start = decode_cursor(request.cursor.as_ref()).map_err(|detail| {
            self.error(
                VfsErrorCode::InvalidArgument,
                VfsOperation::ReadDirectory,
                Some(path),
            )
            .with_detail(detail)
        })?;
        let limit = usize::try_from(request.limit.get())
            .unwrap_or(usize::MAX)
            .min(4_096);
        let mut entries = Vec::with_capacity(limit.min(node.children.len().saturating_sub(start)));
        for child_path in node.children.iter().skip(start).take(limit) {
            request
                .context
                .cancellation
                .check(VfsOperation::ReadDirectory, &self.descriptor.id)?;
            let child = self.nodes.get(child_path).ok_or_else(|| {
                self.error(
                    VfsErrorCode::Internal,
                    VfsOperation::ReadDirectory,
                    Some(child_path),
                )
                .with_detail("archive directory index references a missing child")
            })?;
            entries.push(DirEntry {
                path: child_path.clone(),
                metadata: self.metadata_for_node(child_path, child, true)?,
            });
        }
        let next_index = start.saturating_add(entries.len());
        let next_cursor = (next_index < node.children.len()).then(|| encode_cursor(next_index));
        Ok(DirPage {
            entries,
            next_cursor,
        })
    }

    async fn open(&self, path: &ProviderPath, options: OpenOptions) -> VfsResult<Arc<dyn VfsFile>> {
        options
            .context
            .cancellation
            .check(VfsOperation::Open, &self.descriptor.id)?;
        self.ensure_source_version(options.context.clone()).await?;
        if options.access != FileAccess::Read || options.create != CreateDisposition::OpenExisting {
            return Err(self.error(VfsErrorCode::ReadOnly, VfsOperation::Open, Some(path)));
        }
        let node = self.node(path, VfsOperation::Open)?;
        match node.kind() {
            EntryKind::Directory => {
                return Err(self.error(VfsErrorCode::IsDirectory, VfsOperation::Open, Some(path)));
            }
            EntryKind::SymbolicLink | EntryKind::Special => {
                return Err(self
                    .error(VfsErrorCode::Unsupported, VfsOperation::Open, Some(path))
                    .with_detail("archive symbolic links and collisions cannot be opened"));
            }
            EntryKind::File => {}
        }
        let record_index = node.record_index().ok_or_else(|| {
            self.error(VfsErrorCode::Internal, VfsOperation::Open, Some(path))
                .with_detail("archive file has no backing central-directory record")
        })?;
        let record = self.records.get(record_index).ok_or_else(|| {
            self.error(VfsErrorCode::Internal, VfsOperation::Open, Some(path))
                .with_detail("archive file record index is out of bounds")
        })?;
        let current_version = self.version_for_node(path, node);
        if options
            .expected_version
            .as_ref()
            .is_some_and(|expected| expected != &current_version)
        {
            return Err(self.error(VfsErrorCode::StaleVersion, VfsOperation::Open, Some(path)));
        }
        self.validate_member_open(path, record, &options.context)
            .await?;
        self.ensure_source_version(options.context.clone()).await?;
        Ok(Arc::new(ArchiveMemberFile {
            provider_id: self.descriptor.id.clone(),
            path: path.clone(),
            source: self.source.clone(),
            archive_inner: self.archive_inner.clone(),
            record: record.clone(),
            version: current_version,
            source_version: self.source_version.clone(),
            version_probe: self.version_probe.clone(),
            deadline: self.limits.operation_deadline,
        }))
    }

    async fn create_dir(&self, path: &ProviderPath, options: CreateDirOptions) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::CreateDirectory, &self.descriptor.id)?;
        Err(self.error(
            VfsErrorCode::ReadOnly,
            VfsOperation::CreateDirectory,
            Some(path),
        ))
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
        Err(self.error(VfsErrorCode::ReadOnly, VfsOperation::Remove, Some(path)))
    }

    async fn rename(
        &self,
        source: &ProviderPath,
        _target: &ProviderPath,
        options: RenameOptions,
    ) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::Rename, &self.descriptor.id)?;
        Err(self.error(VfsErrorCode::ReadOnly, VfsOperation::Rename, Some(source)))
    }

    async fn copy(
        &self,
        source: &ProviderPath,
        _target: &ProviderPath,
        options: CopyOptions,
    ) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::Copy, &self.descriptor.id)?;
        Err(self.error(VfsErrorCode::ReadOnly, VfsOperation::Copy, Some(source)))
    }

    async fn watch(
        &self,
        request: WatchRequest,
    ) -> VfsResult<BoxStream<'static, VfsResult<EventBatch>>> {
        request
            .context
            .cancellation
            .check(VfsOperation::Watch, &self.descriptor.id)?;
        Err(self.error(
            VfsErrorCode::Unsupported,
            VfsOperation::Watch,
            Some(&request.path),
        ))
    }
}

#[derive(Clone)]
struct ArchiveMemberFile {
    provider_id: ProviderId,
    path: ProviderPath,
    source: Arc<dyn VfsFile>,
    archive_inner: Arc<ZipArchiveInner>,
    record: ArchiveRecord,
    version: VfsVersion,
    source_version: VfsVersion,
    version_probe: Option<Arc<dyn ArchiveVersionProbe>>,
    deadline: Duration,
}

#[async_trait]
impl VfsFile for ArchiveMemberFile {
    async fn len(&self, context: OperationContext) -> VfsResult<u64> {
        context
            .cancellation
            .check(VfsOperation::Read, &self.provider_id)?;
        self.ensure_source_version(context).await?;
        Ok(self.record.metadata.uncompressed_size)
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
        self.ensure_source_version(context.clone()).await?;
        if buffer.is_empty() || offset >= self.record.metadata.uncompressed_size {
            return Ok(0);
        }
        if buffer.len() > MAXIMUM_MEMBER_RANGE_SIZE {
            return Err(VfsError::new(
                VfsErrorCode::TooLarge,
                VfsOperation::Read,
                self.provider_id.clone(),
            )
            .with_path(self.path.clone())
            .with_detail("archive positioned read exceeds the provider range limit"));
        }
        let maximum_read = usize::try_from(
            (self.record.metadata.uncompressed_size - offset)
                .min(u64::try_from(buffer.len()).unwrap_or(u64::MAX)),
        )
        .unwrap_or(buffer.len());
        let provider_id = self.provider_id.clone();
        let member_path = self.path.clone();
        let source = self.source.clone();
        let inner = self.archive_inner.clone();
        let index = self.record.metadata.entry_index;
        let expected_length = self.record.metadata.uncompressed_size;
        let operation_context = context.clone();
        let read = async move {
            let cursor = BufReader::new(VfsFileCursor::new(source, operation_context.clone()));
            let mut archive = ZipArchiveReader::new_with_inner(cursor, inner);
            let member = AssertUnwindSafe(archive.file(index)).catch_unwind().await;
            let mut member = match member {
                Ok(result) => result.map_err(|error| {
                    map_zip_error(&provider_id, VfsOperation::Read, Some(&member_path), error)
                })?,
                Err(_) => {
                    return Err(VfsError::new(
                        VfsErrorCode::CorruptData,
                        VfsOperation::Read,
                        provider_id,
                    )
                    .with_path(member_path)
                    .with_detail("archive parser panicked while reading an untrusted member"));
                }
            };
            let mut remaining = offset;
            let mut discard = vec![0; MEMBER_READ_CHUNK_SIZE];
            while remaining > 0 {
                operation_context
                    .cancellation
                    .check(VfsOperation::Read, &provider_id)?;
                let discard_length = u64::try_from(discard.len()).unwrap_or(u64::MAX);
                let chunk = usize::try_from(remaining.min(discard_length)).unwrap_or(discard.len());
                let read = member
                    .read(&mut discard[..chunk])
                    .await
                    .map_err(|error| map_member_io_error(&provider_id, &member_path, error))?;
                if read == 0 {
                    return Ok(0);
                }
                let read = u64::try_from(read).map_err(|_| {
                    VfsError::new(
                        VfsErrorCode::Internal,
                        VfsOperation::Read,
                        provider_id.clone(),
                    )
                    .with_path(member_path.clone())
                    .with_detail("archive read byte count overflowed")
                })?;
                remaining = remaining.saturating_sub(read);
            }

            let mut total = 0;
            while total < maximum_read {
                operation_context
                    .cancellation
                    .check(VfsOperation::Read, &provider_id)?;
                let read = member
                    .read(&mut buffer[total..maximum_read])
                    .await
                    .map_err(|error| map_member_io_error(&provider_id, &member_path, error))?;
                if read == 0 {
                    break;
                }
                total += read;
            }
            let total_u64 = u64::try_from(total).map_err(|_| {
                VfsError::new(
                    VfsErrorCode::Internal,
                    VfsOperation::Read,
                    provider_id.clone(),
                )
                .with_path(member_path.clone())
                .with_detail("archive read byte count overflowed")
            })?;
            if offset.saturating_add(total_u64) >= expected_length {
                let mut end_probe = [0; 1];
                let read = member
                    .read(&mut end_probe)
                    .await
                    .map_err(|error| map_member_io_error(&provider_id, &member_path, error))?;
                if read != 0 {
                    return Err(VfsError::new(
                        VfsErrorCode::CorruptData,
                        VfsOperation::Read,
                        provider_id,
                    )
                    .with_path(member_path)
                    .with_detail("archive member exceeded its declared uncompressed size"));
                }
            }
            Ok(total)
        };
        let bytes_read = run_with_deadline(
            &self.provider_id,
            VfsOperation::Read,
            &context,
            self.deadline,
            read,
        )
        .await?;
        self.ensure_source_version(context).await?;
        Ok(bytes_read)
    }

    async fn write_at(
        &self,
        _offset: u64,
        _buffer: &[u8],
        options: WriteAtOptions,
    ) -> VfsResult<usize> {
        options
            .context
            .cancellation
            .check(VfsOperation::Write, &self.provider_id)?;
        Err(self.read_only_error(VfsOperation::Write))
    }

    async fn set_len(&self, _len: u64, options: WriteAtOptions) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::SetLength, &self.provider_id)?;
        Err(self.read_only_error(VfsOperation::SetLength))
    }

    async fn flush(&self, context: OperationContext) -> VfsResult<()> {
        context
            .cancellation
            .check(VfsOperation::Flush, &self.provider_id)?;
        Err(self.read_only_error(VfsOperation::Flush))
    }

    async fn sync(&self, context: OperationContext) -> VfsResult<()> {
        context
            .cancellation
            .check(VfsOperation::Sync, &self.provider_id)?;
        Err(self.read_only_error(VfsOperation::Sync))
    }
}

impl ArchiveMemberFile {
    async fn ensure_source_version(&self, context: OperationContext) -> VfsResult<()> {
        let Some(version_probe) = &self.version_probe else {
            return Ok(());
        };
        if version_probe.current_version(context).await? != self.source_version {
            return Err(VfsError::new(
                VfsErrorCode::StaleVersion,
                VfsOperation::Read,
                self.provider_id.clone(),
            )
            .with_path(self.path.clone())
            .with_detail("archive source version changed; remount is required"));
        }
        Ok(())
    }

    fn read_only_error(&self, operation: VfsOperation) -> VfsError {
        VfsError::new(VfsErrorCode::ReadOnly, operation, self.provider_id.clone())
            .with_path(self.path.clone())
            .with_detail(format!(
                "archive member version {:?} is read-only",
                self.version
            ))
    }
}

fn zip_options(limits: &ArchiveLimits) -> ZipOptions {
    ZipOptions {
        max_uncompressed_size_per_file: limits.maximum_uncompressed_member_bytes,
        max_compressed_size_per_file: limits.maximum_uncompressed_member_bytes,
        max_cd_num_files: limits.maximum_entry_count,
        max_cd_num_files_load: limits.maximum_entry_count,
        max_cd_size_in_bytes: limits.maximum_central_directory_bytes,
        ..ZipOptions::untrusted()
    }
}

fn build_index(
    provider_id: &ProviderId,
    central_directory_records: &[CDR],
    limits: &ArchiveLimits,
    context: &OperationContext,
) -> VfsResult<(BTreeMap<ProviderPath, ArchiveNode>, Vec<ArchiveRecord>)> {
    let started_at = Instant::now();
    let root = ProviderPath::root(PathEncoding::UnixBytes);
    let mut nodes = BTreeMap::from([(root, ArchiveNode::implicit_directory())]);
    let mut records = Vec::with_capacity(central_directory_records.len());
    let mut total_uncompressed_bytes = 0_u64;

    for (entry_index, central_directory_record) in central_directory_records.iter().enumerate() {
        context
            .cancellation
            .check(VfsOperation::Open, provider_id)?;
        if started_at.elapsed() > limits.operation_deadline {
            return Err(VfsError::new(
                VfsErrorCode::Timeout,
                VfsOperation::Open,
                provider_id.clone(),
            )
            .with_detail("archive indexing exceeded the configured deadline"));
        }
        let raw_name = central_directory_record.insecure_file_name.as_bytes();
        let (path, directory_by_name) = archive_member_path(provider_id, raw_name, limits)?;
        let uncompressed_size = central_directory_record
            .uncompressed_size()
            .map_err(|error| map_zip_error(provider_id, VfsOperation::Open, Some(&path), error))?;
        let compressed_size = central_directory_record
            .compressed_size()
            .map_err(|error| map_zip_error(provider_id, VfsOperation::Open, Some(&path), error))?;
        if uncompressed_size > limits.maximum_uncompressed_member_bytes {
            return Err(VfsError::new(
                VfsErrorCode::TooLarge,
                VfsOperation::Open,
                provider_id.clone(),
            )
            .with_path(path)
            .with_detail("archive member exceeds the configured uncompressed-size limit"));
        }
        if uncompressed_size > 0
            && (compressed_size == 0
                || uncompressed_size
                    > compressed_size.saturating_mul(limits.maximum_compression_ratio))
        {
            return Err(VfsError::new(
                VfsErrorCode::TooLarge,
                VfsOperation::Open,
                provider_id.clone(),
            )
            .with_path(path)
            .with_detail("archive member exceeds the configured compression-ratio limit"));
        }
        total_uncompressed_bytes = total_uncompressed_bytes
            .checked_add(uncompressed_size)
            .ok_or_else(|| {
                VfsError::new(
                    VfsErrorCode::TooLarge,
                    VfsOperation::Open,
                    provider_id.clone(),
                )
                .with_detail("archive total uncompressed size overflowed")
            })?;
        if total_uncompressed_bytes > limits.maximum_total_uncompressed_bytes {
            return Err(VfsError::new(
                VfsErrorCode::TooLarge,
                VfsOperation::Open,
                provider_id.clone(),
            )
            .with_detail("archive exceeds the configured total uncompressed-size limit"));
        }

        let unix_mode =
            u16::try_from(central_directory_record.cdrh.exter_attr >> 16).unwrap_or(u16::MAX);
        let file_type = unix_mode & 0o170000;
        let kind = if directory_by_name || file_type == 0o040000 {
            EntryKind::Directory
        } else if file_type == 0o120000 {
            EntryKind::SymbolicLink
        } else {
            EntryKind::File
        };
        let encrypted = general_purpose_flag_bits(central_directory_record).map_err(|detail| {
            VfsError::new(
                VfsErrorCode::CorruptData,
                VfsOperation::Open,
                provider_id.clone(),
            )
            .with_path(path.clone())
            .with_detail(detail)
        })? & 1
            != 0;
        let record = ArchiveRecord {
            metadata: ArchiveRecordMetadata {
                entry_index,
                path: path.clone(),
                kind,
                compressed_size,
                uncompressed_size,
                crc32: central_directory_record.cdrh.crc,
            },
            compression: central_directory_record.cdrh.compression,
            encrypted,
            executable: unix_mode & 0o111 != 0,
        };
        records.push(record.clone());
        insert_archive_record(provider_id, &mut nodes, &record)?;
    }

    Ok((nodes, records))
}

fn insert_archive_record(
    provider_id: &ProviderId,
    nodes: &mut BTreeMap<ProviderPath, ArchiveNode>,
    record: &ArchiveRecord,
) -> VfsResult<()> {
    let root = ProviderPath::root(PathEncoding::UnixBytes);
    let components = record
        .metadata
        .path
        .components()
        .cloned()
        .collect::<Vec<_>>();
    let mut parent = root;
    for (component_index, component) in components.iter().enumerate() {
        let path = parent.join_component(component.clone()).map_err(|error| {
            VfsError::new(
                VfsErrorCode::InvalidPath,
                VfsOperation::Open,
                provider_id.clone(),
            )
            .with_detail(error.to_string())
        })?;
        let is_member = component_index + 1 == components.len();
        nodes
            .entry(path.clone())
            .or_insert_with(ArchiveNode::implicit_directory);
        let parent_node = nodes.get_mut(&parent).ok_or_else(|| {
            VfsError::new(
                VfsErrorCode::Internal,
                VfsOperation::Open,
                provider_id.clone(),
            )
            .with_detail("archive index lost a parent directory")
        })?;
        parent_node.has_directory = true;
        parent_node.children.insert(path.clone());
        if is_member {
            let node = nodes.get_mut(&path).ok_or_else(|| {
                VfsError::new(
                    VfsErrorCode::Internal,
                    VfsOperation::Open,
                    provider_id.clone(),
                )
                .with_detail("archive index lost a member node")
            })?;
            if record.metadata.kind != EntryKind::Directory
                && node.explicit_entry_indices.is_empty()
            {
                node.has_directory = false;
            }
            node.add_record(record);
        }
        parent = path;
    }
    Ok(())
}

fn archive_member_path(
    provider_id: &ProviderId,
    raw_name: &[u8],
    limits: &ArchiveLimits,
) -> VfsResult<(ProviderPath, bool)> {
    if raw_name.is_empty() {
        return Err(VfsError::new(
            VfsErrorCode::InvalidPath,
            VfsOperation::Open,
            provider_id.clone(),
        )
        .with_detail("archive member name is empty"));
    }
    if raw_name[0] == b'/' || raw_name.contains(&b'\\') || raw_name.contains(&0) {
        return Err(VfsError::new(
            VfsErrorCode::InvalidPath,
            VfsOperation::Open,
            provider_id.clone(),
        )
        .with_detail("archive member name is absolute, contains backslashes, or contains NUL"));
    }
    let directory = raw_name.ends_with(b"/");
    let path_bytes = raw_name.strip_suffix(b"/").unwrap_or(raw_name);
    if path_bytes.is_empty() {
        return Err(VfsError::new(
            VfsErrorCode::InvalidPath,
            VfsOperation::Open,
            provider_id.clone(),
        )
        .with_detail("archive member path resolves to the archive root"));
    }
    let components = path_bytes.split(|byte| *byte == b'/').collect::<Vec<_>>();
    if components.iter().any(|component| component.is_empty()) {
        return Err(VfsError::new(
            VfsErrorCode::InvalidPath,
            VfsOperation::Open,
            provider_id.clone(),
        )
        .with_detail("archive member path contains an empty component"));
    }
    if components
        .iter()
        .any(|component| component.len() > limits.maximum_name_bytes)
    {
        return Err(VfsError::new(
            VfsErrorCode::TooLarge,
            VfsOperation::Open,
            provider_id.clone(),
        )
        .with_detail("archive member component exceeds the configured byte limit"));
    }
    let path = ProviderPath::from_byte_components(PathEncoding::UnixBytes, components).map_err(
        |error| {
            VfsError::new(
                VfsErrorCode::InvalidPath,
                VfsOperation::Open,
                provider_id.clone(),
            )
            .with_detail(error.to_string())
        },
    )?;
    Ok((path, directory))
}

fn general_purpose_flag_bits(record: &CDR) -> Result<u16, String> {
    let mut cursor = Cursor::new(Vec::with_capacity(2));
    record
        .cdrh
        .gpf
        .clone()
        .write_le(&mut cursor)
        .map_err(|error| error.to_string())?;
    let bytes = cursor.into_inner();
    let bytes: [u8; 2] = bytes
        .try_into()
        .map_err(|_| String::from("archive general-purpose flag had an invalid size"))?;
    Ok(u16::from_le_bytes(bytes))
}

fn provider_path_key(path: &ProviderPath) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.push(ARCHIVE_FORMAT_VERSION);
    for component in path.components() {
        let component_length = u64::try_from(component.as_bytes().len()).unwrap_or(u64::MAX);
        bytes.extend_from_slice(&component_length.to_be_bytes());
        bytes.extend_from_slice(component.as_bytes());
    }
    bytes
}

fn encode_cursor(index: usize) -> DirCursor {
    DirCursor::new(index.to_be_bytes().to_vec())
}

fn decode_cursor(cursor: Option<&DirCursor>) -> Result<usize, &'static str> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    let bytes: [u8; size_of::<usize>()] = cursor
        .as_bytes()
        .try_into()
        .map_err(|_| "archive directory cursor has an invalid length")?;
    Ok(usize::from_be_bytes(bytes))
}

async fn run_with_deadline<T>(
    provider_id: &ProviderId,
    operation: VfsOperation,
    context: &OperationContext,
    deadline: Duration,
    future: impl Future<Output = VfsResult<T>>,
) -> VfsResult<T> {
    context.cancellation.check(operation, provider_id)?;
    let cancellation = context.cancellation.clone();
    let operation_future = future.fuse();
    let cancellation_future = async move { cancellation.cancelled().await }.fuse();
    let timeout_future = async_io::Timer::after(deadline).fuse();
    futures::pin_mut!(operation_future, cancellation_future, timeout_future);
    futures::select_biased! {
        result = operation_future => result,
        _ = cancellation_future => Err(VfsError::new(VfsErrorCode::Cancelled, operation, provider_id.clone())),
        _ = timeout_future => Err(VfsError::new(VfsErrorCode::Timeout, operation, provider_id.clone())
            .with_detail("archive operation exceeded the configured deadline")),
    }
}

fn map_zip_error(
    provider_id: &ProviderId,
    operation: VfsOperation,
    path: Option<&ProviderPath>,
    error: ZipError,
) -> VfsError {
    let code = match &error {
        ZipError::FeatureNotSupported(_) | ZipError::CompressionNotSupported(_) => {
            VfsErrorCode::Unsupported
        }
        ZipError::NumFilesAboveMax(_)
        | ZipError::CDSizeAboveMax(_)
        | ZipError::UncompressedSizeAboveMax(_)
        | ZipError::CompressedSizeAboveMax(_)
        | ZipError::ExtraFieldSizeAboveMax(_)
        | ZipError::ExtraFieldNumAboveMax(_) => VfsErrorCode::TooLarge,
        ZipError::UpstreamReadError(io_error) => io_error
            .get_ref()
            .and_then(|source| source.downcast_ref::<VfsError>())
            .map_or(VfsErrorCode::CorruptData, VfsError::code),
        _ => VfsErrorCode::CorruptData,
    };
    let mut mapped = VfsError::new(code, operation, provider_id.clone())
        .with_detail(error.to_string())
        .with_source(error);
    if let Some(path) = path {
        mapped = mapped.with_path(path.clone());
    }
    mapped
}

fn map_member_io_error(
    provider_id: &ProviderId,
    path: &ProviderPath,
    error: std::io::Error,
) -> VfsError {
    let code = error
        .get_ref()
        .and_then(|source| source.downcast_ref::<VfsError>())
        .map_or(VfsErrorCode::CorruptData, VfsError::code);
    VfsError::new(code, VfsOperation::Read, provider_id.clone())
        .with_path(path.clone())
        .with_detail(error.to_string())
        .with_source(error)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use async_zip::{
        Compression, StringEncoding, ZipEntryBuilder, ZipString, base::write::ZipFileWriter,
    };

    #[derive(Clone)]
    struct CountingBytesFile {
        bytes: Arc<[u8]>,
        bytes_read: Arc<AtomicU64>,
    }

    #[async_trait]
    impl VfsFile for CountingBytesFile {
        async fn len(&self, context: OperationContext) -> VfsResult<u64> {
            context
                .cancellation
                .check(VfsOperation::Read, &ProviderId::new("archive-source"))?;
            Ok(self.bytes.len() as u64)
        }

        async fn read_at(
            &self,
            offset: u64,
            buffer: &mut [u8],
            context: OperationContext,
        ) -> VfsResult<usize> {
            context
                .cancellation
                .check(VfsOperation::Read, &ProviderId::new("archive-source"))?;
            let offset = usize::try_from(offset).unwrap_or(usize::MAX);
            if offset >= self.bytes.len() {
                return Ok(0);
            }
            let bytes_read = buffer.len().min(self.bytes.len() - offset);
            buffer[..bytes_read].copy_from_slice(&self.bytes[offset..offset + bytes_read]);
            self.bytes_read.fetch_add(
                u64::try_from(bytes_read).unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
            Ok(bytes_read)
        }

        async fn write_at(
            &self,
            _offset: u64,
            _buffer: &[u8],
            _options: WriteAtOptions,
        ) -> VfsResult<usize> {
            Err(source_read_only_error(VfsOperation::Write))
        }

        async fn set_len(&self, _len: u64, _options: WriteAtOptions) -> VfsResult<()> {
            Err(source_read_only_error(VfsOperation::SetLength))
        }

        async fn flush(&self, _context: OperationContext) -> VfsResult<()> {
            Err(source_read_only_error(VfsOperation::Flush))
        }

        async fn sync(&self, _context: OperationContext) -> VfsResult<()> {
            Err(source_read_only_error(VfsOperation::Sync))
        }
    }

    struct PendingFile;

    #[async_trait]
    impl VfsFile for PendingFile {
        async fn len(&self, _context: OperationContext) -> VfsResult<u64> {
            futures::future::pending().await
        }

        async fn read_at(
            &self,
            _offset: u64,
            _buffer: &mut [u8],
            _context: OperationContext,
        ) -> VfsResult<usize> {
            futures::future::pending().await
        }

        async fn write_at(
            &self,
            _offset: u64,
            _buffer: &[u8],
            _options: WriteAtOptions,
        ) -> VfsResult<usize> {
            futures::future::pending().await
        }

        async fn set_len(&self, _len: u64, _options: WriteAtOptions) -> VfsResult<()> {
            futures::future::pending().await
        }

        async fn flush(&self, _context: OperationContext) -> VfsResult<()> {
            futures::future::pending().await
        }

        async fn sync(&self, _context: OperationContext) -> VfsResult<()> {
            futures::future::pending().await
        }
    }

    fn source_read_only_error(operation: VfsOperation) -> VfsError {
        VfsError::new(
            VfsErrorCode::ReadOnly,
            operation,
            ProviderId::new("archive-source"),
        )
    }

    fn expect_vfs_error<T>(result: VfsResult<T>, message: &str) -> VfsError {
        match result {
            Ok(_) => panic!("{message}"),
            Err(error) => error,
        }
    }

    async fn zip_bytes(
        entries: Vec<(ZipString, Compression, Vec<u8>, Option<u16>)>,
        force_zip64: bool,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        let writer = ZipFileWriter::new(&mut bytes);
        let mut writer = if force_zip64 {
            writer.force_zip64()
        } else {
            writer
        };
        for (name, compression, contents, permissions) in entries {
            let mut builder = ZipEntryBuilder::new(name, compression);
            if let Some(permissions) = permissions {
                builder = builder.unix_permissions(permissions);
            }
            writer
                .write_entry_whole(builder, &contents)
                .await
                .expect("ZIP fixture entry should write");
        }
        writer.close().await.expect("ZIP fixture should close");
        bytes
    }

    fn counting_file(bytes: Vec<u8>) -> (Arc<dyn VfsFile>, Arc<AtomicU64>) {
        let bytes_read = Arc::new(AtomicU64::new(0));
        (
            Arc::new(CountingBytesFile {
                bytes: bytes.into(),
                bytes_read: bytes_read.clone(),
            }),
            bytes_read,
        )
    }

    fn path(components: &[&[u8]]) -> ProviderPath {
        ProviderPath::from_byte_components(PathEncoding::UnixBytes, components.iter().copied())
            .expect("archive fixture path should be valid")
    }

    async fn mount_bytes(bytes: Vec<u8>) -> ArchiveProvider {
        let (source, _) = counting_file(bytes);
        ArchiveProvider::mount(
            "archive-test",
            source,
            VfsVersion::new(b"outer-v1".to_vec()),
            1,
            ArchiveLimits::default(),
            OperationContext::default(),
        )
        .await
        .expect("archive fixture should mount")
    }

    #[test]
    fn archive_provider_lists_without_extracting_and_reads_exact_members() {
        futures::executor::block_on(async {
            let bytes = zip_bytes(
                vec![
                    ("dir/".into(), Compression::Stored, Vec::new(), Some(0o755)),
                    (
                        "dir/hello.txt".into(),
                        Compression::Deflate,
                        b"hello archive".to_vec(),
                        Some(0o644),
                    ),
                    (
                        ZipString::new(b"raw-\xff.bin".to_vec(), StringEncoding::Raw),
                        Compression::Stored,
                        b"raw bytes".to_vec(),
                        None,
                    ),
                ],
                false,
            )
            .await;
            let (source, bytes_read) = counting_file(bytes);
            let provider = ArchiveProvider::mount(
                "archive-test",
                source,
                VfsVersion::new(b"outer-v1".to_vec()),
                1,
                ArchiveLimits::default(),
                OperationContext::default(),
            )
            .await
            .expect("archive should mount");
            let bytes_after_mount = bytes_read.load(Ordering::Relaxed);

            let root = ProviderPath::root(PathEncoding::UnixBytes);
            let mut listed = BTreeSet::new();
            let mut cursor = None;
            loop {
                let page = provider
                    .read_dir(
                        &root,
                        DirPageRequest {
                            cursor,
                            limit: NonZeroU32::MIN,
                            context: OperationContext::default(),
                        },
                    )
                    .await
                    .expect("archive root should list");
                listed.extend(page.entries.into_iter().map(|entry| entry.path));
                let Some(next_cursor) = page.next_cursor else {
                    break;
                };
                cursor = Some(next_cursor);
            }
            assert_eq!(
                listed,
                BTreeSet::from([path(&[b"dir"]), path(&[b"raw-\xff.bin"])])
            );
            assert_eq!(
                bytes_read.load(Ordering::Relaxed),
                bytes_after_mount,
                "directory listing must not read or decompress member contents"
            );

            let member_path = path(&[b"dir", b"hello.txt"]);
            let file = provider
                .open(&member_path, OpenOptions::default())
                .await
                .expect("archive member should open");
            let mut contents = vec![0; 13];
            let read = file
                .read_at(0, &mut contents, OperationContext::default())
                .await
                .expect("archive member should read");
            assert_eq!(read, contents.len());
            assert_eq!(contents, b"hello archive");
            let mut range = [0; 7];
            file.read_at(6, &mut range, OperationContext::default())
                .await
                .expect("archive positioned range should read");
            assert_eq!(&range, b"archive");
            assert!(bytes_read.load(Ordering::Relaxed) > bytes_after_mount);

            let raw_path = path(&[b"raw-\xff.bin"]);
            assert_eq!(
                provider
                    .stat(&raw_path, StatOptions::default())
                    .await
                    .expect("raw member should stat")
                    .name
                    .expect("raw member should have a name")
                    .exact()
                    .as_bytes(),
                b"raw-\xff.bin"
            );
            assert_eq!(
                provider
                    .native_path(&raw_path, OperationContext::default())
                    .await
                    .expect_err("archive member must not have a native path")
                    .code(),
                VfsErrorCode::Unsupported
            );
            assert_eq!(
                provider
                    .remove(&raw_path, RemoveOptions::default())
                    .await
                    .expect_err("archive member must be read-only")
                    .code(),
                VfsErrorCode::ReadOnly
            );
        });
    }

    #[test]
    fn archive_provider_supports_zip64_and_extension_compatible_payloads() {
        futures::executor::block_on(async {
            for extension in ["zip", "jar", "apk", "aab", "whl", "vsix"] {
                let provider = mount_bytes(
                    zip_bytes(
                        vec![(
                            format!("payload.{extension}").into(),
                            Compression::Stored,
                            extension.as_bytes().to_vec(),
                            None,
                        )],
                        true,
                    )
                    .await,
                )
                .await;
                assert!(provider.is_zip64());
                let member = path(&[format!("payload.{extension}").as_bytes()]);
                let file = provider
                    .open(&member, OpenOptions::default())
                    .await
                    .expect("Zip64 member should open");
                let mut contents = vec![0; extension.len()];
                file.read_at(0, &mut contents, OperationContext::default())
                    .await
                    .expect("Zip64 member should read");
                assert_eq!(contents, extension.as_bytes());
            }
        });
    }

    #[test]
    fn archive_provider_reports_duplicate_exact_names_as_collisions() {
        futures::executor::block_on(async {
            let provider = mount_bytes(
                zip_bytes(
                    vec![
                        (
                            "duplicate.txt".into(),
                            Compression::Stored,
                            b"first".to_vec(),
                            None,
                        ),
                        (
                            "duplicate.txt".into(),
                            Compression::Stored,
                            b"second".to_vec(),
                            None,
                        ),
                    ],
                    false,
                )
                .await,
            )
            .await;
            let duplicate = path(&[b"duplicate.txt"]);
            assert_eq!(provider.records_for_path(&duplicate).len(), 2);
            assert_eq!(
                provider
                    .stat(&duplicate, StatOptions::default())
                    .await
                    .expect_err("duplicate lookup should reject")
                    .code(),
                VfsErrorCode::NameCollision
            );
        });
    }

    #[test]
    fn archive_provider_rejects_traversal_backslashes_and_limits() {
        futures::executor::block_on(async {
            for name in ["../escape", "/absolute", "dir\\escape", "dir//empty"] {
                let bytes = zip_bytes(
                    vec![(name.into(), Compression::Stored, b"bad".to_vec(), None)],
                    false,
                )
                .await;
                let (source, _) = counting_file(bytes);
                let error = expect_vfs_error(
                    ArchiveProvider::mount(
                        "invalid-archive",
                        source,
                        VfsVersion::new(vec![1]),
                        1,
                        ArchiveLimits::default(),
                        OperationContext::default(),
                    )
                    .await,
                    "invalid archive name should reject",
                );
                assert_eq!(error.code(), VfsErrorCode::InvalidPath);
            }

            let bytes = zip_bytes(
                vec![
                    ("one".into(), Compression::Stored, b"1234".to_vec(), None),
                    ("two".into(), Compression::Stored, b"5678".to_vec(), None),
                ],
                false,
            )
            .await;
            for limits in [
                ArchiveLimits {
                    maximum_entry_count: 1,
                    ..ArchiveLimits::default()
                },
                ArchiveLimits {
                    maximum_total_uncompressed_bytes: 7,
                    ..ArchiveLimits::default()
                },
                ArchiveLimits {
                    maximum_uncompressed_member_bytes: 3,
                    ..ArchiveLimits::default()
                },
                ArchiveLimits {
                    maximum_name_bytes: 2,
                    ..ArchiveLimits::default()
                },
            ] {
                let (source, _) = counting_file(bytes.clone());
                let error = expect_vfs_error(
                    ArchiveProvider::mount(
                        "limited-archive",
                        source,
                        VfsVersion::new(vec![1]),
                        1,
                        limits,
                        OperationContext::default(),
                    )
                    .await,
                    "configured archive limit should reject",
                );
                assert_eq!(error.code(), VfsErrorCode::TooLarge);
            }

            let (source, _) = counting_file(bytes);
            let error = expect_vfs_error(
                ArchiveProvider::mount(
                    "nested-archive",
                    source,
                    VfsVersion::new(vec![1]),
                    5,
                    ArchiveLimits::default(),
                    OperationContext::default(),
                )
                .await,
                "nested archive limit should reject",
            );
            assert_eq!(error.code(), VfsErrorCode::TooLarge);
        });
    }

    #[test]
    fn archive_provider_rejects_bombs_encryption_unsupported_compression_and_crc_mismatch() {
        futures::executor::block_on(async {
            let bomb = zip_bytes(
                vec![(
                    "bomb".into(),
                    Compression::Deflate,
                    vec![0; 256 * 1024],
                    None,
                )],
                false,
            )
            .await;
            let (source, _) = counting_file(bomb);
            let error = expect_vfs_error(
                ArchiveProvider::mount(
                    "bomb",
                    source,
                    VfsVersion::new(vec![1]),
                    1,
                    ArchiveLimits {
                        maximum_compression_ratio: 2,
                        ..ArchiveLimits::default()
                    },
                    OperationContext::default(),
                )
                .await,
                "compression bomb should reject",
            );
            assert_eq!(error.code(), VfsErrorCode::TooLarge);

            let encrypted = patch_general_purpose_flag(
                zip_bytes(
                    vec![(
                        "secret".into(),
                        Compression::Stored,
                        b"secret".to_vec(),
                        None,
                    )],
                    false,
                )
                .await,
                1,
            );
            let provider = mount_bytes(encrypted).await;
            let error = expect_vfs_error(
                provider
                    .open(&path(&[b"secret"]), OpenOptions::default())
                    .await,
                "encrypted member should reject",
            );
            assert_eq!(error.code(), VfsErrorCode::Unsupported);

            let unsupported = patch_compression_method(
                zip_bytes(
                    vec![(
                        "unsupported".into(),
                        Compression::Stored,
                        b"data".to_vec(),
                        None,
                    )],
                    false,
                )
                .await,
                98,
            );
            let provider = mount_bytes(unsupported).await;
            let error = expect_vfs_error(
                provider
                    .open(&path(&[b"unsupported"]), OpenOptions::default())
                    .await,
                "unsupported compression should reject",
            );
            assert_eq!(error.code(), VfsErrorCode::Unsupported);

            let symbolic_link = mount_bytes(
                zip_bytes(
                    vec![(
                        "link".into(),
                        Compression::Stored,
                        b"target".to_vec(),
                        Some(0o120777),
                    )],
                    false,
                )
                .await,
            )
            .await;
            let link_path = path(&[b"link"]);
            assert_eq!(
                symbolic_link
                    .stat(&link_path, StatOptions::default())
                    .await
                    .expect("symbolic link should stat")
                    .kind,
                EntryKind::SymbolicLink
            );
            assert_eq!(
                expect_vfs_error(
                    symbolic_link.open(&link_path, OpenOptions::default()).await,
                    "archive symbolic link should not open",
                )
                .code(),
                VfsErrorCode::Unsupported
            );

            let mut corrupted = zip_bytes(
                vec![(
                    "crc".into(),
                    Compression::Stored,
                    b"unique-crc-payload".to_vec(),
                    None,
                )],
                false,
            )
            .await;
            let payload_offset = corrupted
                .windows(b"unique-crc-payload".len())
                .position(|window| window == b"unique-crc-payload")
                .expect("stored payload should be present");
            corrupted[payload_offset] ^= 0xff;
            let provider = mount_bytes(corrupted).await;
            let file = provider
                .open(&path(&[b"crc"]), OpenOptions::default())
                .await
                .expect("corrupt member metadata should still open");
            let mut contents = vec![0; b"unique-crc-payload".len()];
            let error = file
                .read_at(0, &mut contents, OperationContext::default())
                .await
                .expect_err("CRC mismatch should reject the read");
            assert_eq!(error.code(), VfsErrorCode::CorruptData);

            let mut truncated = zip_bytes(
                vec![(
                    "truncated".into(),
                    Compression::Stored,
                    b"data".to_vec(),
                    None,
                )],
                false,
            )
            .await;
            truncated.truncate(truncated.len().saturating_sub(8));
            let (source, _) = counting_file(truncated);
            assert_eq!(
                expect_vfs_error(
                    ArchiveProvider::mount(
                        "truncated",
                        source,
                        VfsVersion::new(vec![1]),
                        1,
                        ArchiveLimits::default(),
                        OperationContext::default(),
                    )
                    .await,
                    "truncated central directory should reject",
                )
                .code(),
                VfsErrorCode::CorruptData
            );
        });
    }

    #[test]
    fn archive_provider_mount_honors_cancellation_and_deadline() {
        futures::executor::block_on(async {
            let cancellation = crate::CancellationToken::default();
            cancellation.cancel();
            let error = expect_vfs_error(
                ArchiveProvider::mount(
                    "cancelled",
                    Arc::new(PendingFile),
                    VfsVersion::new(vec![1]),
                    1,
                    ArchiveLimits::default(),
                    OperationContext {
                        operation_id: crate::OperationId::new(1),
                        cancellation,
                    },
                )
                .await,
                "cancelled mount should reject",
            );
            assert_eq!(error.code(), VfsErrorCode::Cancelled);

            let error = expect_vfs_error(
                ArchiveProvider::mount(
                    "timed-out",
                    Arc::new(PendingFile),
                    VfsVersion::new(vec![1]),
                    1,
                    ArchiveLimits {
                        operation_deadline: Duration::from_millis(5),
                        ..ArchiveLimits::default()
                    },
                    OperationContext::default(),
                )
                .await,
                "timed out mount should reject",
            );
            assert_eq!(error.code(), VfsErrorCode::Timeout);
        });
    }

    fn patch_general_purpose_flag(mut bytes: Vec<u8>, flag: u16) -> Vec<u8> {
        patch_u16_after_signatures(&mut bytes, b"PK\x03\x04", 6, flag);
        patch_u16_after_signatures(&mut bytes, b"PK\x01\x02", 8, flag);
        bytes
    }

    fn patch_compression_method(mut bytes: Vec<u8>, method: u16) -> Vec<u8> {
        patch_u16_after_signatures(&mut bytes, b"PK\x03\x04", 8, method);
        patch_u16_after_signatures(&mut bytes, b"PK\x01\x02", 10, method);
        bytes
    }

    fn patch_u16_after_signatures(bytes: &mut [u8], signature: &[u8], offset: usize, value: u16) {
        let mut search_start = 0;
        while let Some(relative_position) = bytes[search_start..]
            .windows(signature.len())
            .position(|window| window == signature)
        {
            let position = search_start + relative_position + offset;
            bytes[position..position + 2].copy_from_slice(&value.to_le_bytes());
            search_start = position + 2;
        }
    }
}
