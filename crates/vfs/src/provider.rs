use crate::{EntryName, NativePath, ProviderPath};
use async_trait::async_trait;
use event_listener::Event;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    num::{NonZeroU32, NonZeroU64},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::SystemTime,
};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderId(Arc<str>);

impl ProviderId {
    pub fn new(id: impl Into<Arc<str>>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderDescriptor {
    pub id: ProviderId,
    pub display_name: Arc<str>,
    pub path_encoding: crate::PathEncoding,
    pub case_sensitivity: CaseSensitivity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    Unsupported,
    Emulated,
    Native,
}

impl SupportLevel {
    pub const fn is_supported(self) -> bool {
        !matches!(self, Self::Unsupported)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Atomicity {
    BestEffort,
    AtomicWithinFile,
    AtomicWithinDirectory,
    AtomicWithinMount,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseSensitivity {
    Sensitive,
    Insensitive,
    PerDirectory,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReadCapabilities {
    pub whole_file: SupportLevel,
    pub stream: SupportLevel,
    pub positioned: SupportLevel,
    pub atomic_snapshot: SupportLevel,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WriteCapabilities {
    pub positioned: SupportLevel,
    pub append: SupportLevel,
    pub truncate: SupportLevel,
    pub set_len: SupportLevel,
    pub flush: SupportLevel,
    pub sync: SupportLevel,
    pub conditional: SupportLevel,
    pub maximum_atomicity: Atomicity,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DirectoryCapabilities {
    pub paged: SupportLevel,
    pub recursive_list: SupportLevel,
    pub stat_many: SupportLevel,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MutationCapabilities {
    pub create_directory: SupportLevel,
    pub remove: SupportLevel,
    pub rename: SupportLevel,
    pub copy: SupportLevel,
    pub cross_provider_copy: SupportLevel,
    pub idempotency: SupportLevel,
    pub maximum_rename_atomicity: Atomicity,
    pub maximum_delete_atomicity: Atomicity,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WatchCapabilities {
    pub watch: SupportLevel,
    pub recursive: SupportLevel,
    pub resumable_journal: SupportLevel,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LinkCapabilities {
    /// Whether the provider exposes symbolic links as first-class entries.
    pub symbolic_links: SupportLevel,
    /// Whether the provider can report hard links distinctly.
    ///
    /// A provider that cannot distinguish a hard link from an ordinary file
    /// must report this as unsupported: callers may not assume that an entry's
    /// identity is confined to the provider root when the host filesystem can
    /// alias it through a hard link.
    pub hard_links: SupportLevel,
    pub permissions: SupportLevel,
    pub extended_attributes: SupportLevel,
    /// Whether [`VfsProvider::native_path`] can return a host path.
    pub native_path: SupportLevel,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrashCapabilities {
    pub trash: SupportLevel,
    pub restore: SupportLevel,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderLimits {
    pub maximum_page_size: NonZeroU32,
    pub maximum_stat_batch: NonZeroU32,
    pub maximum_range_size: NonZeroU64,
    pub maximum_request_size: NonZeroU64,
    pub maximum_open_handles: NonZeroU32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub read: ReadCapabilities,
    pub write: WriteCapabilities,
    pub directories: DirectoryCapabilities,
    pub mutations: MutationCapabilities,
    pub watch: WatchCapabilities,
    pub links: LinkCapabilities,
    pub trash: TrashCapabilities,
    pub stable_file_key: SupportLevel,
    pub case_sensitivity: CaseSensitivity,
    pub limits: ProviderLimits,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VfsErrorCode {
    NotFound,
    AlreadyExists,
    NotDirectory,
    IsDirectory,
    PermissionDenied,
    ReadOnly,
    Unsupported,
    InvalidPath,
    InvalidArgument,
    Conflict,
    NameCollision,
    StaleVersion,
    Disconnected,
    Cancelled,
    Timeout,
    Quota,
    TooLarge,
    CorruptData,
    WatchOverflow,
    Internal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VfsOperation {
    Stat,
    StatMany,
    ReadDirectory,
    Open,
    Read,
    Write,
    SetLength,
    Flush,
    Sync,
    CreateDirectory,
    Remove,
    Rename,
    Copy,
    Watch,
    ReadLink,
    Symlink,
    SetPermissions,
    Trash,
    Restore,
    NativePath,
}

pub struct VfsError {
    code: VfsErrorCode,
    operation: VfsOperation,
    provider: ProviderId,
    path: Option<ProviderPath>,
    detail: Option<Arc<str>>,
    source: Option<Arc<dyn Error + Send + Sync>>,
}

impl VfsError {
    pub fn new(code: VfsErrorCode, operation: VfsOperation, provider: ProviderId) -> Self {
        Self {
            code,
            operation,
            provider,
            path: None,
            detail: None,
            source: None,
        }
    }

    pub fn with_path(mut self, path: ProviderPath) -> Self {
        self.path = Some(path);
        self
    }

    pub fn with_detail(mut self, detail: impl Into<Arc<str>>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_source<Source>(mut self, source: Source) -> Self
    where
        Source: Error + Send + Sync + 'static,
    {
        self.source = Some(Arc::new(source));
        self
    }

    pub fn code(&self) -> VfsErrorCode {
        self.code
    }

    pub fn operation(&self) -> VfsOperation {
        self.operation
    }

    pub fn provider(&self) -> &ProviderId {
        &self.provider
    }

    pub fn path(&self) -> Option<&ProviderPath> {
        self.path.as_ref()
    }

    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    pub fn remap(mut self, provider: ProviderId, path: Option<ProviderPath>) -> Self {
        self.provider = provider;
        self.path = path;
        self
    }
}

impl fmt::Debug for VfsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VfsError")
            .field("code", &self.code)
            .field("operation", &self.operation)
            .field("provider", &self.provider)
            .field("path", &self.path)
            .field("detail", &self.detail)
            .field("source", &self.source.as_ref().map(ToString::to_string))
            .finish()
    }
}

impl fmt::Display for VfsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:?} failed for provider {}: {:?}",
            self.operation, self.provider, self.code
        )?;
        if let Some(path) = &self.path {
            write!(formatter, " at {}", path.display())?;
        }
        if let Some(detail) = &self.detail {
            write!(formatter, ": {detail}")?;
        }
        Ok(())
    }
}

impl Error for VfsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

pub type VfsResult<T> = Result<T, VfsError>;

#[derive(Default)]
struct CancellationState {
    cancelled: AtomicBool,
    event: Event,
}

#[derive(Clone, Default)]
pub struct CancellationToken(Arc<CancellationState>);

impl CancellationToken {
    pub fn cancel(&self) {
        if !self.0.cancelled.swap(true, Ordering::AcqRel) {
            self.0.event.notify(usize::MAX);
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }

    pub async fn cancelled(&self) {
        loop {
            let listener = self.0.event.listen();
            if self.is_cancelled() {
                return;
            }
            listener.await;
        }
    }

    pub fn check(&self, operation: VfsOperation, provider: &ProviderId) -> VfsResult<()> {
        if self.is_cancelled() {
            Err(VfsError::new(
                VfsErrorCode::Cancelled,
                operation,
                provider.clone(),
            ))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OperationId(u64);

impl OperationId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone)]
pub struct OperationContext {
    pub operation_id: OperationId,
    pub cancellation: CancellationToken,
}

impl Default for OperationContext {
    fn default() -> Self {
        static NEXT_OPERATION_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            operation_id: OperationId::new(NEXT_OPERATION_ID.fetch_add(1, Ordering::Relaxed)),
            cancellation: CancellationToken::default(),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VfsVersion(Arc<[u8]>);

impl VfsVersion {
    pub fn new(bytes: impl Into<Arc<[u8]>>) -> Self {
        Self(bytes.into())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderFileKey(Arc<[u8]>);

impl ProviderFileKey {
    pub fn new(bytes: impl Into<Arc<[u8]>>) -> Self {
        Self(bytes.into())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Directory,
    SymbolicLink,
    Special,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EntryPermissions {
    pub writable: bool,
    pub executable: bool,
    pub private: bool,
    pub hidden: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntryMetadata {
    pub name: Option<EntryName>,
    pub kind: EntryKind,
    pub size: u64,
    pub modified_at: Option<SystemTime>,
    pub created_at: Option<SystemTime>,
    pub permissions: EntryPermissions,
    pub provider_file_key: Option<ProviderFileKey>,
    pub content_version: VfsVersion,
    pub structure_version: VfsVersion,
    pub symbolic_link_target: Option<ProviderPath>,
    pub symbolic_link_target_kind: Option<EntryKind>,
    pub case_sensitivity: CaseSensitivity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolicLinkMode {
    DoNotFollow,
    Follow,
}

#[derive(Clone)]
pub struct StatOptions {
    pub symbolic_link_mode: SymbolicLinkMode,
    pub context: OperationContext,
}

impl Default for StatOptions {
    fn default() -> Self {
        Self {
            symbolic_link_mode: SymbolicLinkMode::DoNotFollow,
            context: OperationContext::default(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DirCursor(Arc<[u8]>);

impl DirCursor {
    pub fn new(bytes: impl Into<Arc<[u8]>>) -> Self {
        Self(bytes.into())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone)]
pub struct DirPageRequest {
    pub cursor: Option<DirCursor>,
    pub limit: NonZeroU32,
    pub context: OperationContext,
}

impl Default for DirPageRequest {
    fn default() -> Self {
        Self {
            cursor: None,
            limit: NonZeroU32::new(1_024).unwrap_or(NonZeroU32::MIN),
            context: OperationContext::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DirEntry {
    pub path: ProviderPath,
    pub metadata: EntryMetadata,
}

#[derive(Clone, Debug)]
pub struct DirPage {
    pub entries: Vec<DirEntry>,
    pub next_cursor: Option<DirCursor>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileAccess {
    Read,
    Write,
    ReadWrite,
}

impl FileAccess {
    pub const fn can_read(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }

    pub const fn can_write(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreateDisposition {
    OpenExisting,
    CreateNew,
    OpenOrCreate,
    TruncateExisting,
}

#[derive(Clone)]
pub struct OpenOptions {
    pub access: FileAccess,
    pub create: CreateDisposition,
    pub expected_version: Option<VfsVersion>,
    pub context: OperationContext,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            access: FileAccess::Read,
            create: CreateDisposition::OpenExisting,
            expected_version: None,
            context: OperationContext::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreateParents {
    No,
    Yes,
}

#[derive(Clone)]
pub struct CreateDirOptions {
    pub parents: CreateParents,
    pub context: OperationContext,
}

impl Default for CreateDirOptions {
    fn default() -> Self {
        Self {
            parents: CreateParents::No,
            context: OperationContext::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoveKind {
    File,
    EmptyDirectory,
    Recursive,
}

#[derive(Clone)]
pub struct RemoveOptions {
    pub kind: RemoveKind,
    pub expected_version: Option<VfsVersion>,
    pub context: OperationContext,
}

impl Default for RemoveOptions {
    fn default() -> Self {
        Self {
            kind: RemoveKind::File,
            expected_version: None,
            context: OperationContext::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollisionPolicy {
    Fail,
    Replace,
}

#[derive(Clone)]
pub struct RenameOptions {
    pub collision: CollisionPolicy,
    pub required_atomicity: Atomicity,
    pub expected_version: Option<VfsVersion>,
    pub context: OperationContext,
}

impl Default for RenameOptions {
    fn default() -> Self {
        Self {
            collision: CollisionPolicy::Fail,
            required_atomicity: Atomicity::BestEffort,
            expected_version: None,
            context: OperationContext::default(),
        }
    }
}

#[derive(Clone)]
pub struct CopyOptions {
    pub collision: CollisionPolicy,
    pub expected_version: Option<VfsVersion>,
    pub context: OperationContext,
}

impl Default for CopyOptions {
    fn default() -> Self {
        Self {
            collision: CollisionPolicy::Fail,
            expected_version: None,
            context: OperationContext::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RemoveOutcome {
    pub removed_entries: RemovedEntryCount,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemovedEntryCount {
    Exact(u64),
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WatchDepth {
    DirectChildren,
    Recursive,
}

#[derive(Clone)]
pub struct WatchRequest {
    pub path: ProviderPath,
    pub depth: WatchDepth,
    pub resume_after_sequence: Option<u64>,
    pub context: OperationContext,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VfsEventKind {
    Created,
    Modified,
    Removed,
    Renamed { old_path: ProviderPath },
    Overflow { rescan_root: ProviderPath },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VfsEvent {
    pub path: ProviderPath,
    pub kind: VfsEventKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventBatch {
    pub first_sequence: u64,
    pub last_sequence: u64,
    pub events: Vec<VfsEvent>,
}

#[derive(Clone, Default)]
pub struct WriteAtOptions {
    pub expected_version: Option<VfsVersion>,
    pub context: OperationContext,
}

#[derive(Clone, Default)]
pub struct SymlinkOptions {
    pub context: OperationContext,
}

/// A partial permission update. `None` leaves the corresponding flag unchanged.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PermissionsUpdate {
    pub writable: Option<bool>,
    pub executable: Option<bool>,
}

#[derive(Clone, Default)]
pub struct SetPermissionsOptions {
    pub permissions: PermissionsUpdate,
    pub context: OperationContext,
}

#[derive(Clone, Default)]
pub struct TrashOptions {
    pub context: OperationContext,
}

/// An entry that was moved to the provider's trash, retaining enough
/// information to restore it.
///
/// `restore_token` is opaque provider-specific data; callers must pass the
/// token back to the same provider unchanged.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrashedEntry {
    pub original_path: ProviderPath,
    pub restore_token: Arc<[u8]>,
}

#[async_trait]
pub trait VfsFile: Send + Sync {
    async fn len(&self, context: OperationContext) -> VfsResult<u64>;
    async fn read_at(
        &self,
        offset: u64,
        buffer: &mut [u8],
        context: OperationContext,
    ) -> VfsResult<usize>;
    async fn write_at(
        &self,
        offset: u64,
        buffer: &[u8],
        options: WriteAtOptions,
    ) -> VfsResult<usize>;
    async fn set_len(&self, len: u64, options: WriteAtOptions) -> VfsResult<()>;
    async fn flush(&self, context: OperationContext) -> VfsResult<()>;
    async fn sync(&self, context: OperationContext) -> VfsResult<()>;
}

#[async_trait]
pub trait VfsProvider: Send + Sync {
    fn descriptor(&self) -> &ProviderDescriptor;
    fn capabilities(&self) -> ProviderCapabilities;

    async fn stat(&self, path: &ProviderPath, options: StatOptions) -> VfsResult<EntryMetadata>;

    /// Stats several paths in one call.
    ///
    /// The default implementation issues one `stat` per path so that every
    /// provider honours the `directories.stat_many` capability; providers with a
    /// genuinely batched implementation may override it. Per-path failures are
    /// returned in place so one missing path does not fail the whole batch.
    async fn stat_many(
        &self,
        paths: &[ProviderPath],
        options: StatOptions,
    ) -> VfsResult<Vec<VfsResult<EntryMetadata>>> {
        let mut results = Vec::with_capacity(paths.len());
        for path in paths {
            results.push(self.stat(path, options.clone()).await);
        }
        Ok(results)
    }

    /// Reads the target of a symbolic link.
    ///
    /// The default implementation derives the target from `stat` in
    /// `DoNotFollow` mode, so providers that expose symlink targets through
    /// metadata do not need a bespoke implementation.
    async fn read_link(
        &self,
        path: &ProviderPath,
        context: OperationContext,
    ) -> VfsResult<ProviderPath> {
        let metadata = self
            .stat(
                path,
                StatOptions {
                    symbolic_link_mode: SymbolicLinkMode::DoNotFollow,
                    context,
                },
            )
            .await?;
        if metadata.kind != EntryKind::SymbolicLink {
            return Err(VfsError::new(
                VfsErrorCode::InvalidArgument,
                VfsOperation::ReadLink,
                self.descriptor().id.clone(),
            )
            .with_path(path.clone())
            .with_detail("path is not a symbolic link"));
        }
        metadata.symbolic_link_target.ok_or_else(|| {
            VfsError::new(
                VfsErrorCode::NotFound,
                VfsOperation::ReadLink,
                self.descriptor().id.clone(),
            )
            .with_path(path.clone())
            .with_detail("symbolic link target is not visible within the provider root")
        })
    }

    async fn read_dir(&self, path: &ProviderPath, request: DirPageRequest) -> VfsResult<DirPage>;

    async fn open(&self, path: &ProviderPath, options: OpenOptions) -> VfsResult<Arc<dyn VfsFile>>;

    async fn create_dir(&self, path: &ProviderPath, options: CreateDirOptions) -> VfsResult<()>;

    async fn remove(&self, path: &ProviderPath, options: RemoveOptions)
    -> VfsResult<RemoveOutcome>;

    async fn rename(
        &self,
        source: &ProviderPath,
        target: &ProviderPath,
        options: RenameOptions,
    ) -> VfsResult<()>;

    async fn copy(
        &self,
        source: &ProviderPath,
        target: &ProviderPath,
        options: CopyOptions,
    ) -> VfsResult<()>;

    /// Creates a symbolic link at `link_path` pointing at `target`.
    async fn symlink(
        &self,
        _target: &ProviderPath,
        link_path: &ProviderPath,
        options: SymlinkOptions,
    ) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::Symlink, &self.descriptor().id)?;
        Err(VfsError::new(
            VfsErrorCode::Unsupported,
            VfsOperation::Symlink,
            self.descriptor().id.clone(),
        )
        .with_path(link_path.clone()))
    }

    /// Updates the mutable permission flags of an entry.
    async fn set_permissions(
        &self,
        path: &ProviderPath,
        options: SetPermissionsOptions,
    ) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::SetPermissions, &self.descriptor().id)?;
        Err(VfsError::new(
            VfsErrorCode::Unsupported,
            VfsOperation::SetPermissions,
            self.descriptor().id.clone(),
        )
        .with_path(path.clone()))
    }

    /// Moves an entry to the provider's trash and returns a restorable handle.
    async fn trash(&self, path: &ProviderPath, options: TrashOptions) -> VfsResult<TrashedEntry> {
        options
            .context
            .cancellation
            .check(VfsOperation::Trash, &self.descriptor().id)?;
        Err(VfsError::new(
            VfsErrorCode::Unsupported,
            VfsOperation::Trash,
            self.descriptor().id.clone(),
        )
        .with_path(path.clone()))
    }

    /// Restores an entry previously returned by [`VfsProvider::trash`].
    async fn restore(&self, entry: TrashedEntry, options: TrashOptions) -> VfsResult<ProviderPath> {
        options
            .context
            .cancellation
            .check(VfsOperation::Restore, &self.descriptor().id)?;
        Err(VfsError::new(
            VfsErrorCode::Unsupported,
            VfsOperation::Restore,
            self.descriptor().id.clone(),
        )
        .with_path(entry.original_path))
    }

    async fn watch(
        &self,
        request: WatchRequest,
    ) -> VfsResult<BoxStream<'static, VfsResult<EventBatch>>>;

    /// Resolves `path` to a host-native path.
    ///
    /// Contract:
    ///
    /// - A native path is data-local only. It is never resource identity, never
    ///   written to the wire or to persistence, and never compared for equality;
    ///   callers must keep using [`ProviderPath`]/[`VfsPath`] as the source of
    ///   truth.
    /// - The provider returns [`VfsErrorCode::Unsupported`] when the path has no
    ///   data-local representation (virtual, archive, remote, or subtree
    ///   resources) instead of inventing a placeholder path that a host tool
    ///   could act on.
    /// - A returned path may become stale the moment it is returned; the caller
    ///   owns any race and must re-validate through the provider before
    ///   mutating through the native path.
    /// - Hosts must not follow native paths across a provider boundary that the
    ///   provider itself would reject (for example a symlink escape).
    async fn native_path(
        &self,
        _path: &ProviderPath,
        context: OperationContext,
    ) -> VfsResult<NativePath> {
        context
            .cancellation
            .check(VfsOperation::NativePath, &self.descriptor().id)?;
        Err(VfsError::new(
            VfsErrorCode::Unsupported,
            VfsOperation::NativePath,
            self.descriptor().id.clone(),
        ))
    }
}
