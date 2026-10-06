use anyhow::Result as TransportResult;
use async_trait::async_trait;
use futures::{StreamExt as _, stream::BoxStream};
use parking_lot::Mutex;
use proto::{
    MountIdV2, ProviderPathV2, VfsCancelOperationRequestV2, VfsCloseHandleRequestV2,
    VfsCopyRequestV2, VfsCreateDirectoryRequestV2, VfsDirectoryEntryV2, VfsErrorV2,
    VfsFileLengthRequestV2, VfsFileLengthResponseV2, VfsHandleOperationKindV2,
    VfsHandleOperationRequestV2, VfsMountArchiveRequestV2, VfsMountArchiveResponseV2,
    VfsNegotiateRequestV2, VfsNegotiateResponseV2, VfsOpenRequestV2, VfsOpenResponseV2,
    VfsOperationResponseV2, VfsProviderCapabilitiesV2, VfsProviderDescriptorV2, VfsReadAtRequestV2,
    VfsReadAtResponseV2, VfsReadDirectoryRequestV2, VfsReadDirectoryResponseV2, VfsReleaseHandleV2,
    VfsRemoveRequestV2, VfsRemoveResponseV2, VfsRenameRequestV2, VfsRenewHandleRequestV2,
    VfsRenewHandleResponseV2, VfsSetLengthRequestV2, VfsStatManyItemV2, VfsStatManyRequestV2,
    VfsStatManyResponseV2, VfsStatRequestV2, VfsStatResponseV2, VfsWatchRequestV2,
    VfsWatchResponseV2, VfsWriteAtRequestV2, VfsWriteAtResponseV2,
};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    future::Future,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use vfs::{
    ArchiveLimits, ArchiveProvider, CancellationToken, CopyOptions, CreateDirOptions,
    CreateParents, DirCursor, DirPage, DirPageRequest, EntryKind, EventBatch, MountId, OpenOptions,
    OperationContext, OperationId, ProviderCapabilities, ProviderDescriptor, ProviderId,
    ProviderPath, RemoveOptions, RemoveOutcome, RemovedEntryCount, RenameOptions, SnapshotBudgets,
    StatOptions, SymbolicLinkMode, VfsError, VfsErrorCode, VfsFile, VfsManager, VfsOperation,
    VfsProvider, VfsResult, VfsSnapshot, VfsVersion, WatchRequest, WriteAtOptions,
};

pub const REMOTE_VFS_PROTOCOL_VERSION: u32 = 1;
const DEFAULT_HANDLE_LEASE: Duration = Duration::from_secs(30);
const DEFAULT_RESULT_JOURNAL_CAPACITY: usize = 4_096;
const MAXIMUM_CONTROL_MESSAGE_BYTES: usize = 8 * 1_024 * 1_024;
const MAXIMUM_ARCHIVE_MOUNTS: usize = 128;

pub type VfsAuthorizer = Arc<dyn Fn(&ProviderPath, VfsOperation) -> VfsResult<()> + Send + Sync>;

#[async_trait]
pub trait RemoteVfsTransport: Send + Sync {
    async fn negotiate(
        &self,
        request: VfsNegotiateRequestV2,
    ) -> TransportResult<VfsNegotiateResponseV2>;
    async fn mount_archive(
        &self,
        request: VfsMountArchiveRequestV2,
    ) -> TransportResult<VfsMountArchiveResponseV2>;
    async fn stat(&self, request: VfsStatRequestV2) -> TransportResult<VfsStatResponseV2>;
    async fn stat_many(
        &self,
        request: VfsStatManyRequestV2,
    ) -> TransportResult<VfsStatManyResponseV2>;
    async fn read_directory(
        &self,
        request: VfsReadDirectoryRequestV2,
    ) -> TransportResult<VfsReadDirectoryResponseV2>;
    async fn open(&self, request: VfsOpenRequestV2) -> TransportResult<VfsOpenResponseV2>;
    async fn file_length(
        &self,
        request: VfsFileLengthRequestV2,
    ) -> TransportResult<VfsFileLengthResponseV2>;
    async fn read_at(&self, request: VfsReadAtRequestV2) -> TransportResult<VfsReadAtResponseV2>;
    async fn write_at(&self, request: VfsWriteAtRequestV2)
    -> TransportResult<VfsWriteAtResponseV2>;
    async fn set_length(
        &self,
        request: VfsSetLengthRequestV2,
    ) -> TransportResult<VfsOperationResponseV2>;
    async fn handle_operation(
        &self,
        request: VfsHandleOperationRequestV2,
    ) -> TransportResult<VfsOperationResponseV2>;
    async fn create_directory(
        &self,
        request: VfsCreateDirectoryRequestV2,
    ) -> TransportResult<VfsOperationResponseV2>;
    async fn remove(&self, request: VfsRemoveRequestV2) -> TransportResult<VfsRemoveResponseV2>;
    async fn rename(&self, request: VfsRenameRequestV2) -> TransportResult<VfsOperationResponseV2>;
    async fn copy(&self, request: VfsCopyRequestV2) -> TransportResult<VfsOperationResponseV2>;
    async fn renew_handle(
        &self,
        request: VfsRenewHandleRequestV2,
    ) -> TransportResult<VfsRenewHandleResponseV2>;
    async fn cancel_operation(
        &self,
        request: VfsCancelOperationRequestV2,
    ) -> TransportResult<VfsOperationResponseV2>;
    async fn watch(
        &self,
        request: VfsWatchRequestV2,
    ) -> TransportResult<BoxStream<'static, TransportResult<VfsWatchResponseV2>>>;
    fn release_handle(&self, request: VfsReleaseHandleV2);
}

#[derive(Clone)]
pub struct VfsService {
    inner: Arc<VfsServiceState>,
}

struct VfsServiceState {
    mounts_by_worktree: Mutex<BTreeMap<u64, VfsSnapshot>>,
    archive_mounts: Mutex<BTreeMap<ArchiveMountKey, VfsSnapshot>>,
    manager: VfsManager,
    authorizers_by_mount: Mutex<BTreeMap<MountId, VfsAuthorizer>>,
    handles: Mutex<BTreeMap<u64, ServiceHandle>>,
    operations: Arc<Mutex<BTreeMap<(MountId, OperationId), CancellationToken>>>,
    result_journal: Mutex<ResultJournal>,
    next_handle_id: AtomicU64,
    handle_lease: Duration,
    capability_extensions: Arc<[proto::VfsCapabilityExtensionV2]>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ArchiveMountKey {
    source_mount_id: MountId,
    source_path: ProviderPath,
    source_version: VfsVersion,
    nested_depth: u8,
}

struct ResultJournal {
    entries: BTreeMap<(MountId, OperationId), JournalRecord>,
    insertion_order: VecDeque<(MountId, OperationId)>,
    capacity: usize,
}

struct JournalRecord {
    request_fingerprint: [u8; 32],
    response: JournalResponse,
}

#[derive(Clone)]
enum JournalResponse {
    Open(VfsOpenResponseV2),
    Write(VfsWriteAtResponseV2),
    Operation {
        kind: &'static str,
        response: VfsOperationResponseV2,
    },
    Remove(VfsRemoveResponseV2),
}

trait JournalValue: Clone {
    const KIND: &'static str;
    fn from_journal(response: &JournalResponse) -> Option<Self>;
    fn into_journal(self) -> JournalResponse;
}

struct ServiceHandle {
    mount_id: MountId,
    path: ProviderPath,
    file: Arc<dyn VfsFile>,
    expires_at: Instant,
}

struct OperationGuard {
    key: (MountId, OperationId),
    operations: Arc<Mutex<BTreeMap<(MountId, OperationId), CancellationToken>>>,
    context: OperationContext,
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        self.operations.lock().remove(&self.key);
    }
}

impl ResultJournal {
    fn replay<Value: JournalValue>(
        &self,
        key: &(MountId, OperationId),
        request_fingerprint: &[u8; 32],
        operation: VfsOperation,
    ) -> VfsResult<Option<Value>> {
        let Some(record) = self.entries.get(key) else {
            return Ok(None);
        };
        if record.request_fingerprint != *request_fingerprint {
            return Err(service_error(
                VfsErrorCode::Conflict,
                operation,
                "operation ID retry payload does not match the original request",
            ));
        }
        Value::from_journal(&record.response)
            .map(Some)
            .ok_or_else(|| {
                service_error(
                    VfsErrorCode::Conflict,
                    operation,
                    format!(
                        "operation ID was previously used for a different mutation; expected {}",
                        Value::KIND
                    ),
                )
            })
    }

    fn record<Value: JournalValue>(
        &mut self,
        key: (MountId, OperationId),
        request_fingerprint: [u8; 32],
        value: Value,
    ) {
        if self.entries.contains_key(&key) {
            return;
        }
        self.entries.insert(
            key,
            JournalRecord {
                request_fingerprint,
                response: value.into_journal(),
            },
        );
        self.insertion_order.push_back(key);
        while self.entries.len() > self.capacity {
            let Some(oldest_key) = self.insertion_order.pop_front() else {
                break;
            };
            self.entries.remove(&oldest_key);
        }
    }

    fn replay_operation(
        &self,
        key: &(MountId, OperationId),
        request_fingerprint: &[u8; 32],
        kind: &'static str,
        operation: VfsOperation,
    ) -> VfsResult<Option<VfsOperationResponseV2>> {
        let Some(record) = self.entries.get(key) else {
            return Ok(None);
        };
        if record.request_fingerprint != *request_fingerprint {
            return Err(service_error(
                VfsErrorCode::Conflict,
                operation,
                "operation ID retry payload does not match the original request",
            ));
        }
        match &record.response {
            JournalResponse::Operation {
                kind: previous_kind,
                response,
            } if *previous_kind == kind => Ok(Some(response.clone())),
            _ => Err(service_error(
                VfsErrorCode::Conflict,
                operation,
                format!(
                    "operation ID was previously used for a different mutation; expected {kind}"
                ),
            )),
        }
    }

    fn record_operation(
        &mut self,
        key: (MountId, OperationId),
        request_fingerprint: [u8; 32],
        kind: &'static str,
        response: VfsOperationResponseV2,
    ) {
        if self.entries.contains_key(&key) {
            return;
        }
        self.entries.insert(
            key,
            JournalRecord {
                request_fingerprint,
                response: JournalResponse::Operation { kind, response },
            },
        );
        self.insertion_order.push_back(key);
        while self.entries.len() > self.capacity {
            let Some(oldest_key) = self.insertion_order.pop_front() else {
                break;
            };
            self.entries.remove(&oldest_key);
        }
    }
}

impl JournalValue for VfsOpenResponseV2 {
    const KIND: &'static str = "open";

    fn from_journal(response: &JournalResponse) -> Option<Self> {
        match response {
            JournalResponse::Open(response) => Some(response.clone()),
            _ => None,
        }
    }

    fn into_journal(self) -> JournalResponse {
        JournalResponse::Open(self)
    }
}

impl JournalValue for VfsWriteAtResponseV2 {
    const KIND: &'static str = "write";

    fn from_journal(response: &JournalResponse) -> Option<Self> {
        match response {
            JournalResponse::Write(response) => Some(response.clone()),
            _ => None,
        }
    }

    fn into_journal(self) -> JournalResponse {
        JournalResponse::Write(self)
    }
}

impl JournalValue for VfsRemoveResponseV2 {
    const KIND: &'static str = "remove";

    fn from_journal(response: &JournalResponse) -> Option<Self> {
        match response {
            JournalResponse::Remove(response) => Some(response.clone()),
            _ => None,
        }
    }

    fn into_journal(self) -> JournalResponse {
        JournalResponse::Remove(self)
    }
}

impl Default for VfsService {
    fn default() -> Self {
        Self::new(DEFAULT_HANDLE_LEASE)
    }
}

impl VfsService {
    pub fn new(handle_lease: Duration) -> Self {
        Self::with_capability_extensions(handle_lease, Arc::from([]))
    }

    pub fn with_capability_extensions(
        handle_lease: Duration,
        capability_extensions: Arc<[proto::VfsCapabilityExtensionV2]>,
    ) -> Self {
        Self {
            inner: Arc::new(VfsServiceState {
                mounts_by_worktree: Mutex::new(BTreeMap::new()),
                archive_mounts: Mutex::new(BTreeMap::new()),
                manager: VfsManager::default(),
                authorizers_by_mount: Mutex::new(BTreeMap::new()),
                handles: Mutex::new(BTreeMap::new()),
                operations: Arc::new(Mutex::new(BTreeMap::new())),
                result_journal: Mutex::new(ResultJournal {
                    entries: BTreeMap::new(),
                    insertion_order: VecDeque::new(),
                    capacity: DEFAULT_RESULT_JOURNAL_CAPACITY,
                }),
                next_handle_id: AtomicU64::new(1),
                handle_lease,
                capability_extensions,
            }),
        }
    }

    pub fn register_provider(
        &self,
        worktree_id: u64,
        provider: Arc<dyn VfsProvider>,
    ) -> VfsResult<MountId> {
        let snapshot = VfsSnapshot::mount(provider, SnapshotBudgets::default())?;
        self.register_snapshot(worktree_id, snapshot)
    }

    pub fn register_snapshot(&self, worktree_id: u64, snapshot: VfsSnapshot) -> VfsResult<MountId> {
        self.register_snapshot_with_authorizer(worktree_id, snapshot, Arc::new(|_, _| Ok(())))
    }

    pub fn register_snapshot_with_authorizer(
        &self,
        worktree_id: u64,
        snapshot: VfsSnapshot,
        authorizer: VfsAuthorizer,
    ) -> VfsResult<MountId> {
        let mut mounts_by_worktree = self.inner.mounts_by_worktree.lock();
        if let Some(existing) = mounts_by_worktree.get(&worktree_id) {
            let mount_id = existing.registry().mount_id();
            self.inner
                .authorizers_by_mount
                .lock()
                .insert(mount_id, authorizer);
            return Ok(mount_id);
        }
        let mount_id = snapshot.registry().mount_id();
        self.inner.manager.register(snapshot.clone())?;
        self.inner
            .authorizers_by_mount
            .lock()
            .insert(mount_id, authorizer.clone());
        mounts_by_worktree.insert(worktree_id, snapshot);
        drop(mounts_by_worktree);
        Ok(mount_id)
    }

    pub fn release_all_handles(&self) {
        self.inner.handles.lock().clear();
    }

    pub fn open_handle_count(&self) -> usize {
        self.prune_expired_handles();
        self.inner.handles.lock().len()
    }

    pub fn handle_lease(&self) -> Duration {
        self.inner.handle_lease
    }

    pub async fn negotiate(&self, request: VfsNegotiateRequestV2) -> VfsNegotiateResponseV2 {
        let snapshot = self
            .inner
            .mounts_by_worktree
            .lock()
            .get(&request.worktree_id)
            .cloned();
        let Some(snapshot) = snapshot else {
            return negotiation_error(service_error(
                VfsErrorCode::NotFound,
                VfsOperation::Stat,
                "worktree VFS mount is not registered",
            ));
        };
        if request.minimum_protocol_version > REMOTE_VFS_PROTOCOL_VERSION
            || request.maximum_protocol_version < REMOTE_VFS_PROTOCOL_VERSION
        {
            return negotiation_error(service_error(
                VfsErrorCode::Unsupported,
                VfsOperation::Stat,
                "no compatible VFS protocol version",
            ));
        }
        let descriptor = snapshot.provider().descriptor();
        if !request.supported_path_encodings.iter().any(|encoding| {
            proto::decode_path_encoding(*encoding).ok() == Some(descriptor.path_encoding)
        }) {
            return negotiation_error(service_error(
                VfsErrorCode::Unsupported,
                VfsOperation::Stat,
                "client does not support the provider path encoding",
            ));
        }
        let mut capabilities = VfsProviderCapabilitiesV2::from_provider_capabilities(
            &snapshot.provider().capabilities(),
        );
        capabilities.extensions = self.inner.capability_extensions.to_vec();
        VfsNegotiateResponseV2 {
            protocol_version: REMOTE_VFS_PROTOCOL_VERSION,
            mount_id: Some(MountIdV2::from_mount_id(snapshot.registry().mount_id())),
            descriptor: Some(VfsProviderDescriptorV2::from_provider_descriptor(
                descriptor,
            )),
            capabilities: Some(capabilities),
            error: None,
        }
    }

    pub async fn mount_archive(
        &self,
        request: VfsMountArchiveRequestV2,
    ) -> VfsMountArchiveResponseV2 {
        match self.mount_archive_inner(request).await {
            Ok(snapshot) => self.archive_mount_response(&snapshot),
            Err(error) => archive_mount_error(error),
        }
    }

    async fn mount_archive_inner(
        &self,
        request: VfsMountArchiveRequestV2,
    ) -> VfsResult<VfsSnapshot> {
        let source_mount_id =
            required_mount_id(request.source_mount_id.as_ref(), VfsOperation::Open)?;
        let source_path = request
            .source_path
            .as_ref()
            .ok_or_else(|| {
                service_error(
                    VfsErrorCode::InvalidArgument,
                    VfsOperation::Open,
                    "archive mount request is missing its source path",
                )
            })?
            .to_provider_path()
            .map_err(|error| wire_error(VfsOperation::Open, error))?;
        let source_authorizer = self
            .inner
            .authorizers_by_mount
            .lock()
            .get(&source_mount_id)
            .cloned()
            .ok_or_else(|| {
                service_error(
                    VfsErrorCode::NotFound,
                    VfsOperation::Open,
                    "archive source mount authorization is not registered",
                )
            })?;
        let nested_depth = u8::try_from(request.nested_depth).map_err(|_| {
            service_error(
                VfsErrorCode::TooLarge,
                VfsOperation::Open,
                "archive nesting depth exceeds the protocol range",
            )
        })?;
        self.authorize(source_mount_id, &source_path, VfsOperation::Open)?;
        let source_snapshot = self.mount(source_mount_id, VfsOperation::Open)?;
        let operation =
            self.operation(source_mount_id, request.operation_id, VfsOperation::Open)?;
        let source_version = VfsVersion::new(request.source_version);
        let metadata = source_snapshot
            .provider()
            .stat(
                &source_path,
                StatOptions {
                    symbolic_link_mode: SymbolicLinkMode::DoNotFollow,
                    context: operation.context.clone(),
                },
            )
            .await?;
        if metadata.kind != EntryKind::File {
            return Err(service_error(
                VfsErrorCode::InvalidArgument,
                VfsOperation::Open,
                "archive source is not a regular file",
            )
            .with_path(source_path));
        }
        if metadata.content_version != source_version {
            return Err(service_error(
                VfsErrorCode::StaleVersion,
                VfsOperation::Open,
                "archive source version changed before mount",
            )
            .with_path(source_path));
        }
        let key = ArchiveMountKey {
            source_mount_id,
            source_path: source_path.clone(),
            source_version: source_version.clone(),
            nested_depth,
        };
        if let Some(snapshot) = self.inner.archive_mounts.lock().get(&key).cloned() {
            return Ok(snapshot);
        }

        let stale_keys = self
            .inner
            .archive_mounts
            .lock()
            .keys()
            .filter(|existing| {
                existing.source_mount_id == source_mount_id
                    && existing.source_path == source_path
                    && existing.nested_depth == nested_depth
                    && existing.source_version != source_version
            })
            .cloned()
            .collect::<Vec<_>>();
        for stale_key in stale_keys {
            let stale_snapshot = self.inner.archive_mounts.lock().remove(&stale_key);
            if let Some(stale_snapshot) = stale_snapshot {
                self.remove_registered_mount(stale_snapshot.registry().mount_id())?;
            }
        }
        if self.inner.archive_mounts.lock().len() >= MAXIMUM_ARCHIVE_MOUNTS {
            return Err(service_error(
                VfsErrorCode::Quota,
                VfsOperation::Open,
                "remote archive mount limit reached",
            ));
        }

        let provider = ArchiveProvider::mount_from_provider(
            format!("archive-{}-{}", source_mount_id.get(), request.operation_id),
            source_snapshot.provider().clone(),
            source_path.clone(),
            nested_depth,
            ArchiveLimits::default(),
            operation.context.clone(),
        )
        .await?;
        if provider.source_version() != &source_version {
            return Err(service_error(
                VfsErrorCode::StaleVersion,
                VfsOperation::Open,
                "archive source version changed during mount",
            ));
        }
        let snapshot = VfsSnapshot::mount(Arc::new(provider), SnapshotBudgets::default())?;
        self.inner.manager.register(snapshot.clone())?;
        self.inner.authorizers_by_mount.lock().insert(
            snapshot.registry().mount_id(),
            Arc::new(move |_, _| source_authorizer(&source_path, VfsOperation::Open)),
        );

        let mut archive_mounts = self.inner.archive_mounts.lock();
        if let Some(existing) = archive_mounts.get(&key).cloned() {
            drop(archive_mounts);
            self.remove_registered_mount(snapshot.registry().mount_id())?;
            return Ok(existing);
        }
        archive_mounts.insert(key, snapshot.clone());
        Ok(snapshot)
    }

    fn archive_mount_response(&self, snapshot: &VfsSnapshot) -> VfsMountArchiveResponseV2 {
        let mut capabilities = VfsProviderCapabilitiesV2::from_provider_capabilities(
            &snapshot.provider().capabilities(),
        );
        capabilities.extensions = self.inner.capability_extensions.to_vec();
        VfsMountArchiveResponseV2 {
            protocol_version: REMOTE_VFS_PROTOCOL_VERSION,
            mount_id: Some(MountIdV2::from_mount_id(snapshot.registry().mount_id())),
            descriptor: Some(VfsProviderDescriptorV2::from_provider_descriptor(
                snapshot.provider().descriptor(),
            )),
            capabilities: Some(capabilities),
            error: None,
        }
    }

    fn remove_registered_mount(&self, mount_id: MountId) -> VfsResult<()> {
        self.inner.manager.unmount(mount_id)?;
        self.inner.authorizers_by_mount.lock().remove(&mount_id);
        self.inner
            .handles
            .lock()
            .retain(|_, handle| handle.mount_id != mount_id);
        let cancellations = {
            let mut operations = self.inner.operations.lock();
            let keys = operations
                .keys()
                .filter(|(operation_mount_id, _)| *operation_mount_id == mount_id)
                .copied()
                .collect::<Vec<_>>();
            keys.into_iter()
                .filter_map(|key| operations.remove(&key))
                .collect::<Vec<_>>()
        };
        for cancellation in cancellations {
            cancellation.cancel();
        }
        let mut journal = self.inner.result_journal.lock();
        journal
            .entries
            .retain(|(operation_mount_id, _), _| *operation_mount_id != mount_id);
        journal
            .insertion_order
            .retain(|(operation_mount_id, _)| *operation_mount_id != mount_id);
        Ok(())
    }

    pub fn archive_mount_count(&self) -> usize {
        self.inner.archive_mounts.lock().len()
    }

    pub async fn stat(&self, request: VfsStatRequestV2) -> VfsStatResponseV2 {
        let result = self.stat_inner(request).await;
        match result {
            Ok(metadata) => match proto::VfsEntryMetadataV2::from_entry_metadata(&metadata) {
                Ok(metadata) => VfsStatResponseV2 {
                    metadata: Some(metadata),
                    error: None,
                },
                Err(error) => VfsStatResponseV2 {
                    metadata: None,
                    error: Some(VfsErrorV2::from_vfs_error(&wire_error(
                        VfsOperation::Stat,
                        error,
                    ))),
                },
            },
            Err(error) => VfsStatResponseV2 {
                metadata: None,
                error: Some(VfsErrorV2::from_vfs_error(&error)),
            },
        }
    }

    async fn stat_inner(&self, request: VfsStatRequestV2) -> VfsResult<vfs::EntryMetadata> {
        let (snapshot, mount_id) = self.snapshot(request.mount_id.as_ref(), VfsOperation::Stat)?;
        let path = required_path(request.path.as_ref(), VfsOperation::Stat)?;
        self.authorize(mount_id, &path, VfsOperation::Stat)?;
        let operation = self.operation(mount_id, request.operation_id, VfsOperation::Stat)?;
        snapshot
            .provider()
            .stat(
                &path,
                StatOptions {
                    symbolic_link_mode: if request.follow_symbolic_link {
                        SymbolicLinkMode::Follow
                    } else {
                        SymbolicLinkMode::DoNotFollow
                    },
                    context: operation.context.clone(),
                },
            )
            .await
    }

    pub async fn stat_many(&self, request: VfsStatManyRequestV2) -> VfsStatManyResponseV2 {
        if proto::Message::encoded_len(&request) > MAXIMUM_CONTROL_MESSAGE_BYTES {
            return VfsStatManyResponseV2 {
                items: Vec::new(),
                error: Some(VfsErrorV2::from_vfs_error(&service_error(
                    VfsErrorCode::TooLarge,
                    VfsOperation::Stat,
                    "stat-many request exceeds the control-message limit",
                ))),
            };
        }
        let result = async {
            let (snapshot, mount_id) =
                self.snapshot(request.mount_id.as_ref(), VfsOperation::Stat)?;
            if request.paths.len()
                > snapshot
                    .provider()
                    .capabilities()
                    .limits
                    .maximum_stat_batch
                    .get() as usize
            {
                return Err(service_error(
                    VfsErrorCode::TooLarge,
                    VfsOperation::Stat,
                    "stat-many request exceeds the negotiated batch limit",
                ));
            }
            let operation = self.operation(mount_id, request.operation_id, VfsOperation::Stat)?;
            let mut items = Vec::with_capacity(request.paths.len());
            for wire_path in request.paths {
                let path = match wire_path.to_provider_path() {
                    Ok(path) => path,
                    Err(error) => {
                        items.push(VfsStatManyItemV2 {
                            path: Some(wire_path),
                            metadata: None,
                            error: Some(VfsErrorV2::from_vfs_error(&wire_error(
                                VfsOperation::Stat,
                                error,
                            ))),
                        });
                        continue;
                    }
                };
                let item_result = self
                    .authorize(mount_id, &path, VfsOperation::Stat)
                    .and_then(|()| {
                        operation
                            .context
                            .cancellation
                            .check(VfsOperation::Stat, &snapshot.provider().descriptor().id)
                    });
                let metadata = match item_result {
                    Ok(()) => {
                        snapshot
                            .provider()
                            .stat(
                                &path,
                                StatOptions {
                                    symbolic_link_mode: if request.follow_symbolic_link {
                                        SymbolicLinkMode::Follow
                                    } else {
                                        SymbolicLinkMode::DoNotFollow
                                    },
                                    context: operation.context.clone(),
                                },
                            )
                            .await
                    }
                    Err(error) => Err(error),
                };
                let (metadata, error) = match metadata {
                    Ok(metadata) => match proto::VfsEntryMetadataV2::from_entry_metadata(&metadata)
                    {
                        Ok(metadata) => (Some(metadata), None),
                        Err(error) => (
                            None,
                            Some(VfsErrorV2::from_vfs_error(&wire_error(
                                VfsOperation::Stat,
                                error,
                            ))),
                        ),
                    },
                    Err(error) => (None, Some(VfsErrorV2::from_vfs_error(&error))),
                };
                items.push(VfsStatManyItemV2 {
                    path: Some(ProviderPathV2::from_provider_path(&path)),
                    metadata,
                    error,
                });
            }
            Ok(items)
        }
        .await;
        match result {
            Ok(items) => {
                let response = VfsStatManyResponseV2 { items, error: None };
                if proto::Message::encoded_len(&response) > MAXIMUM_CONTROL_MESSAGE_BYTES {
                    VfsStatManyResponseV2 {
                        items: Vec::new(),
                        error: Some(VfsErrorV2::from_vfs_error(&service_error(
                            VfsErrorCode::TooLarge,
                            VfsOperation::Stat,
                            "stat-many response exceeds the control-message limit",
                        ))),
                    }
                } else {
                    response
                }
            }
            Err(error) => VfsStatManyResponseV2 {
                items: Vec::new(),
                error: Some(VfsErrorV2::from_vfs_error(&error)),
            },
        }
    }

    pub async fn read_directory(
        &self,
        request: VfsReadDirectoryRequestV2,
    ) -> VfsReadDirectoryResponseV2 {
        match self.read_directory_inner(request).await {
            Ok(page) => {
                let entries = page
                    .entries
                    .iter()
                    .map(VfsDirectoryEntryV2::from_dir_entry)
                    .collect::<Result<Vec<_>, _>>();
                match entries {
                    Ok(entries) => {
                        let response = VfsReadDirectoryResponseV2 {
                            entries,
                            next_cursor: page.next_cursor.map(|cursor| cursor.as_bytes().to_vec()),
                            error: None,
                        };
                        if proto::Message::encoded_len(&response) > MAXIMUM_CONTROL_MESSAGE_BYTES {
                            directory_error(service_error(
                                VfsErrorCode::TooLarge,
                                VfsOperation::ReadDirectory,
                                "directory response exceeds the control-message limit",
                            ))
                        } else {
                            response
                        }
                    }
                    Err(error) => directory_error(wire_error(VfsOperation::ReadDirectory, error)),
                }
            }
            Err(error) => directory_error(error),
        }
    }

    async fn read_directory_inner(&self, request: VfsReadDirectoryRequestV2) -> VfsResult<DirPage> {
        let (snapshot, mount_id) =
            self.snapshot(request.mount_id.as_ref(), VfsOperation::ReadDirectory)?;
        let path = required_path(request.path.as_ref(), VfsOperation::ReadDirectory)?;
        self.authorize(mount_id, &path, VfsOperation::ReadDirectory)?;
        let limit = NonZeroU32::new(request.limit).ok_or_else(|| {
            service_error(
                VfsErrorCode::InvalidArgument,
                VfsOperation::ReadDirectory,
                "directory page limit must be non-zero",
            )
        })?;
        let operation =
            self.operation(mount_id, request.operation_id, VfsOperation::ReadDirectory)?;
        snapshot
            .provider()
            .read_dir(
                &path,
                DirPageRequest {
                    cursor: request.cursor.map(DirCursor::new),
                    limit,
                    context: operation.context.clone(),
                },
            )
            .await
    }

    pub async fn open(&self, request: VfsOpenRequestV2) -> VfsOpenResponseV2 {
        let request_fingerprint = request_fingerprint(&request);
        let journal_key = match mutation_key(
            request.mount_id.as_ref(),
            request.operation_id,
            VfsOperation::Open,
        ) {
            Ok(key) => key,
            Err(error) => {
                return VfsOpenResponseV2 {
                    handle_id: 0,
                    lease_duration_millis: 0,
                    error: Some(VfsErrorV2::from_vfs_error(&error)),
                };
            }
        };
        match self.inner.result_journal.lock().replay(
            &journal_key,
            &request_fingerprint,
            VfsOperation::Open,
        ) {
            Ok(Some(response)) => return response,
            Ok(None) => {}
            Err(error) => {
                return VfsOpenResponseV2 {
                    handle_id: 0,
                    lease_duration_millis: 0,
                    error: Some(VfsErrorV2::from_vfs_error(&error)),
                };
            }
        }
        let response = match self.open_inner(request).await {
            Ok(handle_id) => VfsOpenResponseV2 {
                handle_id,
                lease_duration_millis: self.lease_millis(),
                error: None,
            },
            Err(error) => VfsOpenResponseV2 {
                handle_id: 0,
                lease_duration_millis: 0,
                error: Some(VfsErrorV2::from_vfs_error(&error)),
            },
        };
        self.inner
            .result_journal
            .lock()
            .record(journal_key, request_fingerprint, response.clone());
        response
    }

    async fn open_inner(&self, request: VfsOpenRequestV2) -> VfsResult<u64> {
        let (snapshot, mount_id) = self.snapshot(request.mount_id.as_ref(), VfsOperation::Open)?;
        let path = required_path(request.path.as_ref(), VfsOperation::Open)?;
        self.authorize(mount_id, &path, VfsOperation::Open)?;
        let operation = self.operation(mount_id, request.operation_id, VfsOperation::Open)?;
        self.prune_expired_handles();
        if self.inner.handles.lock().len()
            >= snapshot
                .provider()
                .capabilities()
                .limits
                .maximum_open_handles
                .get() as usize
        {
            return Err(service_error(
                VfsErrorCode::Quota,
                VfsOperation::Open,
                "maximum remote VFS handle count reached",
            ));
        }
        let file = snapshot
            .provider()
            .open(
                &path,
                OpenOptions {
                    access: proto::decode_file_access(request.access)
                        .map_err(|error| wire_error(VfsOperation::Open, error))?,
                    create: proto::decode_create_disposition(request.create)
                        .map_err(|error| wire_error(VfsOperation::Open, error))?,
                    expected_version: request.expected_version.map(VfsVersion::new),
                    context: operation.context.clone(),
                },
            )
            .await?;
        let handle_id = self.next_handle_id()?;
        self.inner.handles.lock().insert(
            handle_id,
            ServiceHandle {
                mount_id,
                path,
                file,
                expires_at: Instant::now() + self.inner.handle_lease,
            },
        );
        Ok(handle_id)
    }

    pub async fn file_length(&self, request: VfsFileLengthRequestV2) -> VfsFileLengthResponseV2 {
        let result = async {
            let mount_id = required_mount_id(request.mount_id.as_ref(), VfsOperation::Read)?;
            let operation = self.operation(mount_id, request.operation_id, VfsOperation::Read)?;
            self.handle(mount_id, request.handle_id, VfsOperation::Read)?
                .len(operation.context.clone())
                .await
        }
        .await;
        match result {
            Ok(length) => VfsFileLengthResponseV2 {
                length,
                error: None,
            },
            Err(error) => VfsFileLengthResponseV2 {
                length: 0,
                error: Some(VfsErrorV2::from_vfs_error(&error)),
            },
        }
    }

    pub async fn read_at(&self, request: VfsReadAtRequestV2) -> VfsReadAtResponseV2 {
        let result = async {
            let mount_id = required_mount_id(request.mount_id.as_ref(), VfsOperation::Read)?;
            let snapshot = self.mount(mount_id, VfsOperation::Read)?;
            if u64::from(request.length)
                > snapshot
                    .provider()
                    .capabilities()
                    .limits
                    .maximum_range_size
                    .get()
            {
                return Err(service_error(
                    VfsErrorCode::TooLarge,
                    VfsOperation::Read,
                    "remote read exceeds negotiated range limit",
                ));
            }
            let operation = self.operation(mount_id, request.operation_id, VfsOperation::Read)?;
            let file = self.handle(mount_id, request.handle_id, VfsOperation::Read)?;
            let mut data = vec![0; request.length as usize];
            let read = file
                .read_at(request.offset, &mut data, operation.context.clone())
                .await?;
            data.truncate(read);
            Ok(data)
        }
        .await;
        match result {
            Ok(data) => VfsReadAtResponseV2 { data, error: None },
            Err(error) => VfsReadAtResponseV2 {
                data: Vec::new(),
                error: Some(VfsErrorV2::from_vfs_error(&error)),
            },
        }
    }

    pub async fn write_at(&self, request: VfsWriteAtRequestV2) -> VfsWriteAtResponseV2 {
        let request_fingerprint = request_fingerprint(&request);
        let journal_key = match mutation_key(
            request.mount_id.as_ref(),
            request.operation_id,
            VfsOperation::Write,
        ) {
            Ok(key) => key,
            Err(error) => {
                return VfsWriteAtResponseV2 {
                    written: 0,
                    error: Some(VfsErrorV2::from_vfs_error(&error)),
                };
            }
        };
        match self.inner.result_journal.lock().replay(
            &journal_key,
            &request_fingerprint,
            VfsOperation::Write,
        ) {
            Ok(Some(response)) => return response,
            Ok(None) => {}
            Err(error) => {
                return VfsWriteAtResponseV2 {
                    written: 0,
                    error: Some(VfsErrorV2::from_vfs_error(&error)),
                };
            }
        }
        let result = async {
            let mount_id = required_mount_id(request.mount_id.as_ref(), VfsOperation::Write)?;
            let snapshot = self.mount(mount_id, VfsOperation::Write)?;
            if request.data.len() as u64
                > snapshot
                    .provider()
                    .capabilities()
                    .limits
                    .maximum_range_size
                    .get()
            {
                return Err(service_error(
                    VfsErrorCode::TooLarge,
                    VfsOperation::Write,
                    "remote write exceeds negotiated range limit",
                ));
            }
            let operation = self.operation(mount_id, request.operation_id, VfsOperation::Write)?;
            self.handle(mount_id, request.handle_id, VfsOperation::Write)?
                .write_at(
                    request.offset,
                    &request.data,
                    WriteAtOptions {
                        expected_version: request.expected_version.map(VfsVersion::new),
                        context: operation.context.clone(),
                    },
                )
                .await
        }
        .await;
        let response = match result {
            Ok(written) => match u32::try_from(written) {
                Ok(written) => VfsWriteAtResponseV2 {
                    written,
                    error: None,
                },
                Err(error) => VfsWriteAtResponseV2 {
                    written: 0,
                    error: Some(VfsErrorV2::from_vfs_error(
                        &service_error(
                            VfsErrorCode::TooLarge,
                            VfsOperation::Write,
                            "provider reported a write larger than the protocol limit",
                        )
                        .with_source(error),
                    )),
                },
            },
            Err(error) => VfsWriteAtResponseV2 {
                written: 0,
                error: Some(VfsErrorV2::from_vfs_error(&error)),
            },
        };
        self.inner
            .result_journal
            .lock()
            .record(journal_key, request_fingerprint, response.clone());
        response
    }

    pub async fn set_length(&self, request: VfsSetLengthRequestV2) -> VfsOperationResponseV2 {
        let request_fingerprint = request_fingerprint(&request);
        let journal_key = match mutation_key(
            request.mount_id.as_ref(),
            request.operation_id,
            VfsOperation::SetLength,
        ) {
            Ok(key) => key,
            Err(error) => return operation_response(Err(error)),
        };
        match self.inner.result_journal.lock().replay_operation(
            &journal_key,
            &request_fingerprint,
            "set length",
            VfsOperation::SetLength,
        ) {
            Ok(Some(response)) => return response,
            Ok(None) => {}
            Err(error) => return operation_response(Err(error)),
        }
        let result = async {
            let mount_id = required_mount_id(request.mount_id.as_ref(), VfsOperation::SetLength)?;
            let operation =
                self.operation(mount_id, request.operation_id, VfsOperation::SetLength)?;
            self.handle(mount_id, request.handle_id, VfsOperation::SetLength)?
                .set_len(
                    request.length,
                    WriteAtOptions {
                        expected_version: request.expected_version.map(VfsVersion::new),
                        context: operation.context.clone(),
                    },
                )
                .await
        }
        .await;
        let response = operation_response(result);
        self.inner.result_journal.lock().record_operation(
            journal_key,
            request_fingerprint,
            "set length",
            response.clone(),
        );
        response
    }

    pub async fn handle_operation(
        &self,
        request: VfsHandleOperationRequestV2,
    ) -> VfsOperationResponseV2 {
        let operation_kind = VfsHandleOperationKindV2::try_from(request.kind);
        let operation = match operation_kind {
            Ok(VfsHandleOperationKindV2::Flush) => VfsOperation::Flush,
            Ok(VfsHandleOperationKindV2::Sync) => VfsOperation::Sync,
            Ok(VfsHandleOperationKindV2::Unspecified) | Err(_) => {
                return operation_response(Err(service_error(
                    VfsErrorCode::InvalidArgument,
                    VfsOperation::Flush,
                    "unknown handle operation",
                )));
            }
        };
        let result = async {
            let mount_id = required_mount_id(request.mount_id.as_ref(), operation)?;
            let operation_guard = self.operation(mount_id, request.operation_id, operation)?;
            let file = self.handle(mount_id, request.handle_id, operation)?;
            match operation {
                VfsOperation::Flush => file.flush(operation_guard.context.clone()).await,
                VfsOperation::Sync => file.sync(operation_guard.context.clone()).await,
                _ => Err(service_error(
                    VfsErrorCode::Internal,
                    operation,
                    "invalid handle operation dispatch",
                )),
            }
        }
        .await;
        operation_response(result)
    }

    pub async fn create_directory(
        &self,
        request: VfsCreateDirectoryRequestV2,
    ) -> VfsOperationResponseV2 {
        let request_fingerprint = request_fingerprint(&request);
        let journal_key = match mutation_key(
            request.mount_id.as_ref(),
            request.operation_id,
            VfsOperation::CreateDirectory,
        ) {
            Ok(key) => key,
            Err(error) => return operation_response(Err(error)),
        };
        match self.inner.result_journal.lock().replay_operation(
            &journal_key,
            &request_fingerprint,
            "create directory",
            VfsOperation::CreateDirectory,
        ) {
            Ok(Some(response)) => return response,
            Ok(None) => {}
            Err(error) => return operation_response(Err(error)),
        }
        let result = async {
            let (snapshot, mount_id) =
                self.snapshot(request.mount_id.as_ref(), VfsOperation::CreateDirectory)?;
            let path = required_path(request.path.as_ref(), VfsOperation::CreateDirectory)?;
            self.authorize(mount_id, &path, VfsOperation::CreateDirectory)?;
            let operation = self.operation(
                mount_id,
                request.operation_id,
                VfsOperation::CreateDirectory,
            )?;
            snapshot
                .provider()
                .create_dir(
                    &path,
                    CreateDirOptions {
                        parents: if request.parents {
                            CreateParents::Yes
                        } else {
                            CreateParents::No
                        },
                        context: operation.context.clone(),
                    },
                )
                .await
        }
        .await;
        let response = operation_response(result);
        self.inner.result_journal.lock().record_operation(
            journal_key,
            request_fingerprint,
            "create directory",
            response.clone(),
        );
        response
    }

    pub async fn remove(&self, request: VfsRemoveRequestV2) -> VfsRemoveResponseV2 {
        let request_fingerprint = request_fingerprint(&request);
        let journal_key = match mutation_key(
            request.mount_id.as_ref(),
            request.operation_id,
            VfsOperation::Remove,
        ) {
            Ok(key) => key,
            Err(error) => {
                return VfsRemoveResponseV2 {
                    removed_entries: None,
                    error: Some(VfsErrorV2::from_vfs_error(&error)),
                };
            }
        };
        match self.inner.result_journal.lock().replay(
            &journal_key,
            &request_fingerprint,
            VfsOperation::Remove,
        ) {
            Ok(Some(response)) => return response,
            Ok(None) => {}
            Err(error) => {
                return VfsRemoveResponseV2 {
                    removed_entries: None,
                    error: Some(VfsErrorV2::from_vfs_error(&error)),
                };
            }
        }
        let result = async {
            let (snapshot, mount_id) =
                self.snapshot(request.mount_id.as_ref(), VfsOperation::Remove)?;
            let path = required_path(request.path.as_ref(), VfsOperation::Remove)?;
            self.authorize(mount_id, &path, VfsOperation::Remove)?;
            let operation = self.operation(mount_id, request.operation_id, VfsOperation::Remove)?;
            snapshot
                .provider()
                .remove(
                    &path,
                    RemoveOptions {
                        kind: proto::decode_remove_kind(request.kind)
                            .map_err(|error| wire_error(VfsOperation::Remove, error))?,
                        expected_version: request.expected_version.map(VfsVersion::new),
                        context: operation.context.clone(),
                    },
                )
                .await
        }
        .await;
        let response = match result {
            Ok(outcome) => VfsRemoveResponseV2 {
                removed_entries: match outcome.removed_entries {
                    RemovedEntryCount::Exact(count) => Some(count),
                    RemovedEntryCount::Unknown => None,
                },
                error: None,
            },
            Err(error) => VfsRemoveResponseV2 {
                removed_entries: None,
                error: Some(VfsErrorV2::from_vfs_error(&error)),
            },
        };
        self.inner
            .result_journal
            .lock()
            .record(journal_key, request_fingerprint, response.clone());
        response
    }

    pub async fn rename(&self, request: VfsRenameRequestV2) -> VfsOperationResponseV2 {
        let request_fingerprint = request_fingerprint(&request);
        let journal_key = match mutation_key(
            request.mount_id.as_ref(),
            request.operation_id,
            VfsOperation::Rename,
        ) {
            Ok(key) => key,
            Err(error) => return operation_response(Err(error)),
        };
        match self.inner.result_journal.lock().replay_operation(
            &journal_key,
            &request_fingerprint,
            "rename",
            VfsOperation::Rename,
        ) {
            Ok(Some(response)) => return response,
            Ok(None) => {}
            Err(error) => return operation_response(Err(error)),
        }
        let result = async {
            let (snapshot, mount_id) =
                self.snapshot(request.mount_id.as_ref(), VfsOperation::Rename)?;
            let source = required_path(request.source.as_ref(), VfsOperation::Rename)?;
            let target = required_path(request.target.as_ref(), VfsOperation::Rename)?;
            self.authorize(mount_id, &source, VfsOperation::Rename)?;
            self.authorize(mount_id, &target, VfsOperation::Rename)?;
            let operation = self.operation(mount_id, request.operation_id, VfsOperation::Rename)?;
            snapshot
                .provider()
                .rename(
                    &source,
                    &target,
                    RenameOptions {
                        collision: proto::decode_collision_policy(request.collision)
                            .map_err(|error| wire_error(VfsOperation::Rename, error))?,
                        required_atomicity: proto::decode_atomicity(request.required_atomicity)
                            .map_err(|error| wire_error(VfsOperation::Rename, error))?,
                        expected_version: request.expected_version.map(VfsVersion::new),
                        context: operation.context.clone(),
                    },
                )
                .await
        }
        .await;
        let response = operation_response(result);
        self.inner.result_journal.lock().record_operation(
            journal_key,
            request_fingerprint,
            "rename",
            response.clone(),
        );
        response
    }

    pub async fn copy(&self, request: VfsCopyRequestV2) -> VfsOperationResponseV2 {
        let request_fingerprint = request_fingerprint(&request);
        let journal_key = match mutation_key(
            request.mount_id.as_ref(),
            request.operation_id,
            VfsOperation::Copy,
        ) {
            Ok(key) => key,
            Err(error) => return operation_response(Err(error)),
        };
        match self.inner.result_journal.lock().replay_operation(
            &journal_key,
            &request_fingerprint,
            "copy",
            VfsOperation::Copy,
        ) {
            Ok(Some(response)) => return response,
            Ok(None) => {}
            Err(error) => return operation_response(Err(error)),
        }
        let result = async {
            let (snapshot, mount_id) =
                self.snapshot(request.mount_id.as_ref(), VfsOperation::Copy)?;
            let source = required_path(request.source.as_ref(), VfsOperation::Copy)?;
            let target = required_path(request.target.as_ref(), VfsOperation::Copy)?;
            self.authorize(mount_id, &source, VfsOperation::Copy)?;
            self.authorize(mount_id, &target, VfsOperation::Copy)?;
            let operation = self.operation(mount_id, request.operation_id, VfsOperation::Copy)?;
            snapshot
                .provider()
                .copy(
                    &source,
                    &target,
                    CopyOptions {
                        collision: proto::decode_collision_policy(request.collision)
                            .map_err(|error| wire_error(VfsOperation::Copy, error))?,
                        expected_version: request.expected_version.map(VfsVersion::new),
                        context: operation.context.clone(),
                    },
                )
                .await
        }
        .await;
        let response = operation_response(result);
        self.inner.result_journal.lock().record_operation(
            journal_key,
            request_fingerprint,
            "copy",
            response.clone(),
        );
        response
    }

    pub async fn renew_handle(&self, request: VfsRenewHandleRequestV2) -> VfsRenewHandleResponseV2 {
        let result =
            required_mount_id(request.mount_id.as_ref(), VfsOperation::Open).and_then(|mount_id| {
                self.handle(mount_id, request.handle_id, VfsOperation::Open)
                    .map(|_| ())
            });
        match result {
            Ok(()) => VfsRenewHandleResponseV2 {
                lease_duration_millis: self.lease_millis(),
                error: None,
            },
            Err(error) => VfsRenewHandleResponseV2 {
                lease_duration_millis: 0,
                error: Some(VfsErrorV2::from_vfs_error(&error)),
            },
        }
    }

    pub async fn close_handle(&self, request: VfsCloseHandleRequestV2) -> VfsOperationResponseV2 {
        let result =
            required_mount_id(request.mount_id.as_ref(), VfsOperation::Open).and_then(|mount_id| {
                let mut handles = self.inner.handles.lock();
                let belongs_to_mount = handles
                    .get(&request.handle_id)
                    .is_some_and(|handle| handle.mount_id == mount_id);
                if belongs_to_mount {
                    handles.remove(&request.handle_id);
                    Ok(())
                } else {
                    Err(service_error(
                        VfsErrorCode::NotFound,
                        VfsOperation::Open,
                        "remote VFS handle is closed or belongs to another mount",
                    ))
                }
            });
        operation_response(result)
    }

    pub async fn cancel_operation(
        &self,
        request: VfsCancelOperationRequestV2,
    ) -> VfsOperationResponseV2 {
        let result =
            required_mount_id(request.mount_id.as_ref(), VfsOperation::Read).map(|mount_id| {
                if let Some(cancellation) = self
                    .inner
                    .operations
                    .lock()
                    .get(&(mount_id, OperationId::new(request.operation_id)))
                    .cloned()
                {
                    cancellation.cancel();
                }
            });
        operation_response(result)
    }

    pub async fn watch(
        &self,
        request: VfsWatchRequestV2,
    ) -> VfsResult<BoxStream<'static, VfsResult<VfsWatchResponseV2>>> {
        let (snapshot, mount_id) = self.snapshot(request.mount_id.as_ref(), VfsOperation::Watch)?;
        let path = required_path(request.path.as_ref(), VfsOperation::Watch)?;
        self.authorize(mount_id, &path, VfsOperation::Watch)?;
        let operation = self.operation(mount_id, request.operation_id, VfsOperation::Watch)?;
        let stream = snapshot
            .provider()
            .watch(WatchRequest {
                path,
                depth: proto::decode_watch_depth(request.depth)
                    .map_err(|error| wire_error(VfsOperation::Watch, error))?,
                resume_after_sequence: request.resume_after_sequence,
                context: operation.context.clone(),
            })
            .await?;
        let provider_id = snapshot.provider().descriptor().id.clone();
        Ok(Box::pin(stream.map(move |batch| {
            operation
                .context
                .cancellation
                .check(VfsOperation::Watch, &provider_id)?;
            batch.map(|batch| VfsWatchResponseV2 {
                batch: Some(proto::VfsEventBatchV2::from_event_batch(&batch)),
                error: None,
            })
        })))
    }

    pub fn release_handle(&self, request: VfsReleaseHandleV2) {
        let Some(mount_id) = request.mount_id.as_ref().map(MountIdV2::to_mount_id) else {
            return;
        };
        let mut handles = self.inner.handles.lock();
        if handles
            .get(&request.handle_id)
            .is_some_and(|handle| handle.mount_id == mount_id)
        {
            handles.remove(&request.handle_id);
        }
    }

    fn snapshot(
        &self,
        mount_id: Option<&MountIdV2>,
        operation: VfsOperation,
    ) -> VfsResult<(VfsSnapshot, MountId)> {
        let mount_id = required_mount_id(mount_id, operation)?;
        Ok((self.mount(mount_id, operation)?, mount_id))
    }

    fn mount(&self, mount_id: MountId, operation: VfsOperation) -> VfsResult<VfsSnapshot> {
        self.inner.manager.snapshot(mount_id).map_err(|error| {
            service_error(
                error.code(),
                operation,
                "remote VFS mount is not registered",
            )
        })
    }

    fn authorize(
        &self,
        mount_id: MountId,
        path: &ProviderPath,
        operation: VfsOperation,
    ) -> VfsResult<()> {
        let authorizer = self
            .inner
            .authorizers_by_mount
            .lock()
            .get(&mount_id)
            .cloned()
            .ok_or_else(|| {
                service_error(
                    VfsErrorCode::NotFound,
                    operation,
                    "remote VFS mount authorization is not registered",
                )
            })?;
        authorizer(path, operation)
    }

    fn operation(
        &self,
        mount_id: MountId,
        operation_id: u64,
        operation: VfsOperation,
    ) -> VfsResult<OperationGuard> {
        let operation_id = OperationId::new(operation_id);
        let key = (mount_id, operation_id);
        let cancellation = CancellationToken::default();
        if self
            .inner
            .operations
            .lock()
            .insert(key, cancellation.clone())
            .is_some()
        {
            return Err(service_error(
                VfsErrorCode::Conflict,
                operation,
                "operation ID is already active",
            ));
        }
        Ok(OperationGuard {
            key,
            operations: self.inner.operations.clone(),
            context: OperationContext {
                operation_id,
                cancellation,
            },
        })
    }

    fn handle(
        &self,
        mount_id: MountId,
        handle_id: u64,
        operation: VfsOperation,
    ) -> VfsResult<Arc<dyn VfsFile>> {
        self.prune_expired_handles();
        let (path, file) = {
            let mut handles = self.inner.handles.lock();
            let handle = handles.get_mut(&handle_id).ok_or_else(|| {
                service_error(
                    VfsErrorCode::NotFound,
                    operation,
                    "remote VFS handle is closed or expired",
                )
            })?;
            if handle.mount_id != mount_id {
                return Err(service_error(
                    VfsErrorCode::PermissionDenied,
                    operation,
                    "remote VFS handle belongs to another mount",
                ));
            }
            handle.expires_at = Instant::now() + self.inner.handle_lease;
            (handle.path.clone(), handle.file.clone())
        };
        self.authorize(mount_id, &path, operation)?;
        Ok(file)
    }

    pub fn prune_expired_handles(&self) {
        let now = Instant::now();
        self.inner
            .handles
            .lock()
            .retain(|_, handle| handle.expires_at > now);
    }

    fn next_handle_id(&self) -> VfsResult<u64> {
        self.inner
            .next_handle_id
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |handle_id| {
                handle_id.checked_add(1)
            })
            .map_err(|_| {
                service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::Open,
                    "remote VFS handle ID space exhausted",
                )
            })
    }

    fn lease_millis(&self) -> u64 {
        u64::try_from(self.inner.handle_lease.as_millis()).unwrap_or(u64::MAX)
    }
}

#[derive(Clone)]
pub struct LoopbackVfsTransport {
    service: VfsService,
    read_at_requests: Arc<AtomicU64>,
    requested_read_bytes: Arc<AtomicU64>,
}

impl LoopbackVfsTransport {
    pub fn new(service: VfsService) -> Self {
        Self {
            service,
            read_at_requests: Arc::new(AtomicU64::new(0)),
            requested_read_bytes: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn read_at_request_count(&self) -> u64 {
        self.read_at_requests.load(Ordering::Relaxed)
    }

    pub fn requested_read_bytes(&self) -> u64 {
        self.requested_read_bytes.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl RemoteVfsTransport for LoopbackVfsTransport {
    async fn negotiate(
        &self,
        request: VfsNegotiateRequestV2,
    ) -> TransportResult<VfsNegotiateResponseV2> {
        Ok(self.service.negotiate(request).await)
    }

    async fn mount_archive(
        &self,
        request: VfsMountArchiveRequestV2,
    ) -> TransportResult<VfsMountArchiveResponseV2> {
        Ok(self.service.mount_archive(request).await)
    }

    async fn stat(&self, request: VfsStatRequestV2) -> TransportResult<VfsStatResponseV2> {
        Ok(self.service.stat(request).await)
    }

    async fn stat_many(
        &self,
        request: VfsStatManyRequestV2,
    ) -> TransportResult<VfsStatManyResponseV2> {
        Ok(self.service.stat_many(request).await)
    }

    async fn read_directory(
        &self,
        request: VfsReadDirectoryRequestV2,
    ) -> TransportResult<VfsReadDirectoryResponseV2> {
        Ok(self.service.read_directory(request).await)
    }

    async fn open(&self, request: VfsOpenRequestV2) -> TransportResult<VfsOpenResponseV2> {
        Ok(self.service.open(request).await)
    }

    async fn file_length(
        &self,
        request: VfsFileLengthRequestV2,
    ) -> TransportResult<VfsFileLengthResponseV2> {
        Ok(self.service.file_length(request).await)
    }

    async fn read_at(&self, request: VfsReadAtRequestV2) -> TransportResult<VfsReadAtResponseV2> {
        self.read_at_requests.fetch_add(1, Ordering::Relaxed);
        self.requested_read_bytes
            .fetch_add(u64::from(request.length), Ordering::Relaxed);
        Ok(self.service.read_at(request).await)
    }

    async fn write_at(
        &self,
        request: VfsWriteAtRequestV2,
    ) -> TransportResult<VfsWriteAtResponseV2> {
        Ok(self.service.write_at(request).await)
    }

    async fn set_length(
        &self,
        request: VfsSetLengthRequestV2,
    ) -> TransportResult<VfsOperationResponseV2> {
        Ok(self.service.set_length(request).await)
    }

    async fn handle_operation(
        &self,
        request: VfsHandleOperationRequestV2,
    ) -> TransportResult<VfsOperationResponseV2> {
        Ok(self.service.handle_operation(request).await)
    }

    async fn create_directory(
        &self,
        request: VfsCreateDirectoryRequestV2,
    ) -> TransportResult<VfsOperationResponseV2> {
        Ok(self.service.create_directory(request).await)
    }

    async fn remove(&self, request: VfsRemoveRequestV2) -> TransportResult<VfsRemoveResponseV2> {
        Ok(self.service.remove(request).await)
    }

    async fn rename(&self, request: VfsRenameRequestV2) -> TransportResult<VfsOperationResponseV2> {
        Ok(self.service.rename(request).await)
    }

    async fn copy(&self, request: VfsCopyRequestV2) -> TransportResult<VfsOperationResponseV2> {
        Ok(self.service.copy(request).await)
    }

    async fn renew_handle(
        &self,
        request: VfsRenewHandleRequestV2,
    ) -> TransportResult<VfsRenewHandleResponseV2> {
        Ok(self.service.renew_handle(request).await)
    }

    async fn cancel_operation(
        &self,
        request: VfsCancelOperationRequestV2,
    ) -> TransportResult<VfsOperationResponseV2> {
        Ok(self.service.cancel_operation(request).await)
    }

    async fn watch(
        &self,
        request: VfsWatchRequestV2,
    ) -> TransportResult<BoxStream<'static, TransportResult<VfsWatchResponseV2>>> {
        let stream = self
            .service
            .watch(request)
            .await
            .map_err(anyhow::Error::new)?;
        Ok(Box::pin(
            stream.map(|response| response.map_err(anyhow::Error::new)),
        ))
    }

    fn release_handle(&self, request: VfsReleaseHandleV2) {
        self.service.release_handle(request);
    }
}

#[cfg(feature = "gpui")]
#[derive(Clone)]
pub struct ProtoVfsTransport {
    client: crate::AnyProtoClient,
}

#[cfg(feature = "gpui")]
impl ProtoVfsTransport {
    pub fn new(client: crate::AnyProtoClient) -> Self {
        Self { client }
    }
}

#[cfg(feature = "gpui")]
#[async_trait]
impl RemoteVfsTransport for ProtoVfsTransport {
    async fn negotiate(
        &self,
        request: VfsNegotiateRequestV2,
    ) -> TransportResult<VfsNegotiateResponseV2> {
        self.client.request(request).await
    }

    async fn mount_archive(
        &self,
        request: VfsMountArchiveRequestV2,
    ) -> TransportResult<VfsMountArchiveResponseV2> {
        self.client.request(request).await
    }

    async fn stat(&self, request: VfsStatRequestV2) -> TransportResult<VfsStatResponseV2> {
        self.client.request(request).await
    }

    async fn stat_many(
        &self,
        request: VfsStatManyRequestV2,
    ) -> TransportResult<VfsStatManyResponseV2> {
        self.client.request(request).await
    }

    async fn read_directory(
        &self,
        request: VfsReadDirectoryRequestV2,
    ) -> TransportResult<VfsReadDirectoryResponseV2> {
        self.client.request(request).await
    }

    async fn open(&self, request: VfsOpenRequestV2) -> TransportResult<VfsOpenResponseV2> {
        self.client.request(request).await
    }

    async fn file_length(
        &self,
        request: VfsFileLengthRequestV2,
    ) -> TransportResult<VfsFileLengthResponseV2> {
        self.client.request(request).await
    }

    async fn read_at(&self, request: VfsReadAtRequestV2) -> TransportResult<VfsReadAtResponseV2> {
        self.client.request(request).await
    }

    async fn write_at(
        &self,
        request: VfsWriteAtRequestV2,
    ) -> TransportResult<VfsWriteAtResponseV2> {
        self.client.request(request).await
    }

    async fn set_length(
        &self,
        request: VfsSetLengthRequestV2,
    ) -> TransportResult<VfsOperationResponseV2> {
        self.client.request(request).await
    }

    async fn handle_operation(
        &self,
        request: VfsHandleOperationRequestV2,
    ) -> TransportResult<VfsOperationResponseV2> {
        self.client.request(request).await
    }

    async fn create_directory(
        &self,
        request: VfsCreateDirectoryRequestV2,
    ) -> TransportResult<VfsOperationResponseV2> {
        self.client.request(request).await
    }

    async fn remove(&self, request: VfsRemoveRequestV2) -> TransportResult<VfsRemoveResponseV2> {
        self.client.request(request).await
    }

    async fn rename(&self, request: VfsRenameRequestV2) -> TransportResult<VfsOperationResponseV2> {
        self.client.request(request).await
    }

    async fn copy(&self, request: VfsCopyRequestV2) -> TransportResult<VfsOperationResponseV2> {
        self.client.request(request).await
    }

    async fn renew_handle(
        &self,
        request: VfsRenewHandleRequestV2,
    ) -> TransportResult<VfsRenewHandleResponseV2> {
        self.client.request(request).await
    }

    async fn cancel_operation(
        &self,
        request: VfsCancelOperationRequestV2,
    ) -> TransportResult<VfsOperationResponseV2> {
        self.client.request(request).await
    }

    async fn watch(
        &self,
        request: VfsWatchRequestV2,
    ) -> TransportResult<BoxStream<'static, TransportResult<VfsWatchResponseV2>>> {
        let stream = self.client.request_stream(request).await?;
        Ok(Box::pin(stream))
    }

    fn release_handle(&self, request: VfsReleaseHandleV2) {
        if let Err(error) = self.client.send(request) {
            tracing::debug!(%error, "failed to release remote VFS handle");
        }
    }
}

#[derive(Clone)]
pub struct RemoteProviderProxy {
    project_id: u64,
    mount_id: MountId,
    descriptor: ProviderDescriptor,
    capabilities: ProviderCapabilities,
    capability_extensions: Arc<[proto::VfsCapabilityExtensionV2]>,
    transport: Arc<dyn RemoteVfsTransport>,
}

impl RemoteProviderProxy {
    pub async fn connect(
        project_id: u64,
        worktree_id: u64,
        transport: Arc<dyn RemoteVfsTransport>,
    ) -> VfsResult<Self> {
        let response = transport
            .negotiate(VfsNegotiateRequestV2 {
                project_id,
                worktree_id,
                minimum_protocol_version: REMOTE_VFS_PROTOCOL_VERSION,
                maximum_protocol_version: REMOTE_VFS_PROTOCOL_VERSION,
                supported_path_encodings: vec![
                    proto::PathEncodingV2::UnixBytes as i32,
                    proto::PathEncodingV2::WindowsWtf8 as i32,
                    proto::PathEncodingV2::PortableUtf8 as i32,
                ],
            })
            .await
            .map_err(|error| transport_error(VfsOperation::Stat, error))?;
        Self::from_mount_response(
            project_id,
            response.protocol_version,
            response.mount_id,
            response.descriptor,
            response.capabilities,
            response.error,
            transport,
            VfsOperation::Stat,
        )
    }

    pub async fn mount_archive(
        &self,
        source_path: &ProviderPath,
        source_version: VfsVersion,
        nested_depth: u8,
        context: OperationContext,
    ) -> VfsResult<Self> {
        let response = cancellable_transport_request(
            &self.transport,
            self.project_id,
            self.mount_id,
            &context,
            VfsOperation::Open,
            self.transport.mount_archive(VfsMountArchiveRequestV2 {
                project_id: self.project_id,
                source_mount_id: self.mount(),
                source_path: Some(ProviderPathV2::from_provider_path(source_path)),
                source_version: source_version.as_bytes().to_vec(),
                nested_depth: u32::from(nested_depth),
                operation_id: context.operation_id.get(),
            }),
        )
        .await?;
        Self::from_mount_response(
            self.project_id,
            response.protocol_version,
            response.mount_id,
            response.descriptor,
            response.capabilities,
            response.error,
            self.transport.clone(),
            VfsOperation::Open,
        )
    }

    #[expect(clippy::too_many_arguments)]
    fn from_mount_response(
        project_id: u64,
        protocol_version: u32,
        mount_id: Option<MountIdV2>,
        descriptor: Option<VfsProviderDescriptorV2>,
        capabilities: Option<VfsProviderCapabilitiesV2>,
        error: Option<VfsErrorV2>,
        transport: Arc<dyn RemoteVfsTransport>,
        operation: VfsOperation,
    ) -> VfsResult<Self> {
        response_error(error, operation)?;
        if protocol_version != REMOTE_VFS_PROTOCOL_VERSION {
            return Err(proxy_error(
                VfsErrorCode::Unsupported,
                operation,
                "server selected an unsupported VFS protocol version",
            ));
        }
        let mount_id = mount_id
            .as_ref()
            .ok_or_else(|| missing_response_field(operation, "mount ID"))?
            .to_mount_id();
        let descriptor = descriptor
            .as_ref()
            .ok_or_else(|| missing_response_field(operation, "provider descriptor"))?
            .to_provider_descriptor()
            .map_err(|error| wire_error(operation, error))?;
        let wire_capabilities = capabilities
            .as_ref()
            .ok_or_else(|| missing_response_field(operation, "provider capabilities"))?;
        let mut capabilities = wire_capabilities
            .to_provider_capabilities()
            .map_err(|error| wire_error(operation, error))?;
        let has_mutation = capabilities.mutations.create_directory.is_supported()
            || capabilities.mutations.remove.is_supported()
            || capabilities.mutations.rename.is_supported()
            || capabilities.mutations.copy.is_supported();
        capabilities.mutations.idempotency = if has_mutation {
            vfs::SupportLevel::Native
        } else {
            vfs::SupportLevel::Unsupported
        };
        capabilities.links.native_path = vfs::SupportLevel::Unsupported;
        Ok(Self {
            project_id,
            mount_id,
            descriptor,
            capabilities,
            capability_extensions: wire_capabilities.extensions.clone().into(),
            transport,
        })
    }

    pub fn capability_extensions(&self) -> &[proto::VfsCapabilityExtensionV2] {
        &self.capability_extensions
    }

    pub async fn stat_many(
        &self,
        paths: &[ProviderPath],
        options: StatOptions,
    ) -> VfsResult<Vec<VfsResult<vfs::EntryMetadata>>> {
        let response = cancellable_transport_request(
            &self.transport,
            self.project_id,
            self.mount_id,
            &options.context,
            VfsOperation::Stat,
            self.transport.stat_many(VfsStatManyRequestV2 {
                project_id: self.project_id,
                mount_id: self.mount(),
                paths: paths
                    .iter()
                    .map(ProviderPathV2::from_provider_path)
                    .collect(),
                operation_id: options.context.operation_id.get(),
                follow_symbolic_link: options.symbolic_link_mode == SymbolicLinkMode::Follow,
            }),
        )
        .await?;
        response_error(response.error, VfsOperation::Stat)?;
        if response.items.len() != paths.len() {
            return Err(missing_response_field(
                VfsOperation::Stat,
                "one stat-many result per requested path",
            ));
        }
        Ok(response
            .items
            .into_iter()
            .zip(paths)
            .map(|(item, expected_path)| {
                let response_path = item
                    .path
                    .as_ref()
                    .ok_or_else(|| missing_response_field(VfsOperation::Stat, "stat-many path"))?
                    .to_provider_path()
                    .map_err(|error| wire_error(VfsOperation::Stat, error))?;
                if &response_path != expected_path {
                    return Err(proxy_error(
                        VfsErrorCode::CorruptData,
                        VfsOperation::Stat,
                        "stat-many response path does not match the request order",
                    ));
                }
                if let Some(error) = item.error {
                    return Err(error
                        .to_vfs_error()
                        .map_err(|error| wire_error(VfsOperation::Stat, error))?);
                }
                item.metadata
                    .as_ref()
                    .ok_or_else(|| {
                        missing_response_field(VfsOperation::Stat, "stat-many metadata")
                    })?
                    .to_entry_metadata()
                    .map_err(|error| wire_error(VfsOperation::Stat, error))
            })
            .collect())
    }

    fn mount(&self) -> Option<MountIdV2> {
        Some(MountIdV2::from_mount_id(self.mount_id))
    }

    fn request_path(&self, path: &ProviderPath) -> Option<ProviderPathV2> {
        Some(ProviderPathV2::from_provider_path(path))
    }
}

#[async_trait]
impl VfsProvider for RemoteProviderProxy {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }

    async fn stat(
        &self,
        path: &ProviderPath,
        options: StatOptions,
    ) -> VfsResult<vfs::EntryMetadata> {
        options
            .context
            .cancellation
            .check(VfsOperation::Stat, &self.descriptor.id)?;
        let response = cancellable_transport_request(
            &self.transport,
            self.project_id,
            self.mount_id,
            &options.context,
            VfsOperation::Stat,
            self.transport.stat(VfsStatRequestV2 {
                project_id: self.project_id,
                mount_id: self.mount(),
                path: self.request_path(path),
                operation_id: options.context.operation_id.get(),
                follow_symbolic_link: options.symbolic_link_mode == SymbolicLinkMode::Follow,
            }),
        )
        .await?;
        response_error(response.error, VfsOperation::Stat)?;
        response
            .metadata
            .as_ref()
            .ok_or_else(|| missing_response_field(VfsOperation::Stat, "metadata"))?
            .to_entry_metadata()
            .map_err(|error| wire_error(VfsOperation::Stat, error))
    }

    async fn read_dir(&self, path: &ProviderPath, request: DirPageRequest) -> VfsResult<DirPage> {
        request
            .context
            .cancellation
            .check(VfsOperation::ReadDirectory, &self.descriptor.id)?;
        let response = cancellable_transport_request(
            &self.transport,
            self.project_id,
            self.mount_id,
            &request.context,
            VfsOperation::ReadDirectory,
            self.transport.read_directory(VfsReadDirectoryRequestV2 {
                project_id: self.project_id,
                mount_id: self.mount(),
                path: self.request_path(path),
                cursor: request.cursor.map(|cursor| cursor.as_bytes().to_vec()),
                limit: request.limit.get(),
                operation_id: request.context.operation_id.get(),
            }),
        )
        .await?;
        response_error(response.error, VfsOperation::ReadDirectory)?;
        Ok(DirPage {
            entries: response
                .entries
                .iter()
                .map(VfsDirectoryEntryV2::to_dir_entry)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| wire_error(VfsOperation::ReadDirectory, error))?,
            next_cursor: response.next_cursor.map(DirCursor::new),
        })
    }

    async fn open(&self, path: &ProviderPath, options: OpenOptions) -> VfsResult<Arc<dyn VfsFile>> {
        options
            .context
            .cancellation
            .check(VfsOperation::Open, &self.descriptor.id)?;
        let response = cancellable_transport_request(
            &self.transport,
            self.project_id,
            self.mount_id,
            &options.context,
            VfsOperation::Open,
            self.transport.open(VfsOpenRequestV2 {
                project_id: self.project_id,
                mount_id: self.mount(),
                path: self.request_path(path),
                access: proto::encode_file_access(options.access) as i32,
                create: proto::encode_create_disposition(options.create) as i32,
                expected_version: options
                    .expected_version
                    .map(|version| version.as_bytes().to_vec()),
                operation_id: options.context.operation_id.get(),
            }),
        )
        .await?;
        response_error(response.error, VfsOperation::Open)?;
        if response.handle_id == 0 {
            return Err(missing_response_field(VfsOperation::Open, "handle ID"));
        }
        Ok(Arc::new(RemoteVfsFile {
            project_id: self.project_id,
            mount_id: self.mount_id,
            handle_id: response.handle_id,
            provider_id: self.descriptor.id.clone(),
            transport: self.transport.clone(),
        }))
    }

    async fn create_dir(&self, path: &ProviderPath, options: CreateDirOptions) -> VfsResult<()> {
        let response = cancellable_transport_request(
            &self.transport,
            self.project_id,
            self.mount_id,
            &options.context,
            VfsOperation::CreateDirectory,
            self.transport
                .create_directory(VfsCreateDirectoryRequestV2 {
                    project_id: self.project_id,
                    mount_id: self.mount(),
                    path: self.request_path(path),
                    parents: options.parents == CreateParents::Yes,
                    operation_id: options.context.operation_id.get(),
                }),
        )
        .await?;
        response_error(response.error, VfsOperation::CreateDirectory)
    }

    async fn remove(
        &self,
        path: &ProviderPath,
        options: RemoveOptions,
    ) -> VfsResult<RemoveOutcome> {
        let response = cancellable_transport_request(
            &self.transport,
            self.project_id,
            self.mount_id,
            &options.context,
            VfsOperation::Remove,
            self.transport.remove(VfsRemoveRequestV2 {
                project_id: self.project_id,
                mount_id: self.mount(),
                path: self.request_path(path),
                kind: proto::encode_remove_kind(options.kind) as i32,
                expected_version: options
                    .expected_version
                    .map(|version| version.as_bytes().to_vec()),
                operation_id: options.context.operation_id.get(),
            }),
        )
        .await?;
        response_error(response.error, VfsOperation::Remove)?;
        Ok(RemoveOutcome {
            removed_entries: response
                .removed_entries
                .map_or(RemovedEntryCount::Unknown, RemovedEntryCount::Exact),
        })
    }

    async fn rename(
        &self,
        source: &ProviderPath,
        target: &ProviderPath,
        options: RenameOptions,
    ) -> VfsResult<()> {
        let response = cancellable_transport_request(
            &self.transport,
            self.project_id,
            self.mount_id,
            &options.context,
            VfsOperation::Rename,
            self.transport.rename(VfsRenameRequestV2 {
                project_id: self.project_id,
                mount_id: self.mount(),
                source: self.request_path(source),
                target: self.request_path(target),
                collision: proto::encode_collision_policy(options.collision) as i32,
                required_atomicity: proto::encode_atomicity(options.required_atomicity) as i32,
                expected_version: options
                    .expected_version
                    .map(|version| version.as_bytes().to_vec()),
                operation_id: options.context.operation_id.get(),
            }),
        )
        .await?;
        response_error(response.error, VfsOperation::Rename)
    }

    async fn copy(
        &self,
        source: &ProviderPath,
        target: &ProviderPath,
        options: CopyOptions,
    ) -> VfsResult<()> {
        let response = cancellable_transport_request(
            &self.transport,
            self.project_id,
            self.mount_id,
            &options.context,
            VfsOperation::Copy,
            self.transport.copy(VfsCopyRequestV2 {
                project_id: self.project_id,
                mount_id: self.mount(),
                source: self.request_path(source),
                target: self.request_path(target),
                collision: proto::encode_collision_policy(options.collision) as i32,
                expected_version: options
                    .expected_version
                    .map(|version| version.as_bytes().to_vec()),
                operation_id: options.context.operation_id.get(),
            }),
        )
        .await?;
        response_error(response.error, VfsOperation::Copy)
    }

    async fn watch(
        &self,
        request: WatchRequest,
    ) -> VfsResult<BoxStream<'static, VfsResult<EventBatch>>> {
        let stream = self
            .transport
            .watch(VfsWatchRequestV2 {
                project_id: self.project_id,
                mount_id: self.mount(),
                path: self.request_path(&request.path),
                depth: proto::encode_watch_depth(request.depth) as i32,
                resume_after_sequence: request.resume_after_sequence,
                operation_id: request.context.operation_id.get(),
            })
            .await
            .map_err(|error| transport_error(VfsOperation::Watch, error))?;
        let transport = self.transport.clone();
        let project_id = self.project_id;
        let mount_id = self.mount_id;
        let context = request.context;
        Ok(Box::pin(futures::stream::unfold(
            (stream, transport, context, false),
            move |(mut stream, transport, context, finished)| async move {
                if finished {
                    return None;
                }
                let next_response = Box::pin(stream.next());
                let cancellation = context.cancellation.clone();
                let cancelled = Box::pin(cancellation.cancelled());
                match futures::future::select(next_response, cancelled).await {
                    futures::future::Either::Left((Some(response), _)) => {
                        let response = response
                            .map_err(|error| transport_error(VfsOperation::Watch, error))
                            .and_then(|response| {
                                response_error(response.error, VfsOperation::Watch)?;
                                response
                                    .batch
                                    .as_ref()
                                    .ok_or_else(|| {
                                        missing_response_field(VfsOperation::Watch, "event batch")
                                    })?
                                    .to_event_batch()
                                    .map_err(|error| wire_error(VfsOperation::Watch, error))
                            });
                        Some((response, (stream, transport, context, false)))
                    }
                    futures::future::Either::Left((None, _)) => None,
                    futures::future::Either::Right(((), _)) => {
                        if let Err(error) = transport
                            .cancel_operation(VfsCancelOperationRequestV2 {
                                project_id,
                                mount_id: Some(MountIdV2::from_mount_id(mount_id)),
                                operation_id: context.operation_id.get(),
                            })
                            .await
                        {
                            tracing::debug!(%error, "failed to cancel remote VFS watch");
                        }
                        Some((
                            Err(proxy_error(
                                VfsErrorCode::Cancelled,
                                VfsOperation::Watch,
                                "remote VFS watch was cancelled",
                            )),
                            (stream, transport, context, true),
                        ))
                    }
                }
            },
        )))
    }
}

struct RemoteVfsFile {
    project_id: u64,
    mount_id: MountId,
    handle_id: u64,
    provider_id: ProviderId,
    transport: Arc<dyn RemoteVfsTransport>,
}

impl Drop for RemoteVfsFile {
    fn drop(&mut self) {
        self.transport.release_handle(VfsReleaseHandleV2 {
            project_id: self.project_id,
            mount_id: Some(MountIdV2::from_mount_id(self.mount_id)),
            handle_id: self.handle_id,
        });
    }
}

#[async_trait]
impl VfsFile for RemoteVfsFile {
    async fn len(&self, context: OperationContext) -> VfsResult<u64> {
        context
            .cancellation
            .check(VfsOperation::Read, &self.provider_id)?;
        let response = cancellable_transport_request(
            &self.transport,
            self.project_id,
            self.mount_id,
            &context,
            VfsOperation::Read,
            self.transport.file_length(VfsFileLengthRequestV2 {
                project_id: self.project_id,
                mount_id: Some(MountIdV2::from_mount_id(self.mount_id)),
                handle_id: self.handle_id,
                operation_id: context.operation_id.get(),
            }),
        )
        .await?;
        response_error(response.error, VfsOperation::Read)?;
        Ok(response.length)
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
        let length = u32::try_from(buffer.len()).map_err(|error| {
            proxy_error(
                VfsErrorCode::TooLarge,
                VfsOperation::Read,
                error.to_string(),
            )
        })?;
        let response = cancellable_transport_request(
            &self.transport,
            self.project_id,
            self.mount_id,
            &context,
            VfsOperation::Read,
            self.transport.read_at(VfsReadAtRequestV2 {
                project_id: self.project_id,
                mount_id: Some(MountIdV2::from_mount_id(self.mount_id)),
                handle_id: self.handle_id,
                offset,
                length,
                operation_id: context.operation_id.get(),
            }),
        )
        .await?;
        response_error(response.error, VfsOperation::Read)?;
        if response.data.len() > buffer.len() {
            return Err(proxy_error(
                VfsErrorCode::CorruptData,
                VfsOperation::Read,
                "remote read returned more bytes than requested",
            ));
        }
        let read = response.data.len();
        if let Some(target) = buffer.get_mut(..read) {
            target.copy_from_slice(&response.data);
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
            .check(VfsOperation::Write, &self.provider_id)?;
        let response = cancellable_transport_request(
            &self.transport,
            self.project_id,
            self.mount_id,
            &options.context,
            VfsOperation::Write,
            self.transport.write_at(VfsWriteAtRequestV2 {
                project_id: self.project_id,
                mount_id: Some(MountIdV2::from_mount_id(self.mount_id)),
                handle_id: self.handle_id,
                offset,
                data: buffer.to_vec(),
                expected_version: options
                    .expected_version
                    .map(|version| version.as_bytes().to_vec()),
                operation_id: options.context.operation_id.get(),
            }),
        )
        .await?;
        response_error(response.error, VfsOperation::Write)?;
        Ok(response.written as usize)
    }

    async fn set_len(&self, len: u64, options: WriteAtOptions) -> VfsResult<()> {
        let response = cancellable_transport_request(
            &self.transport,
            self.project_id,
            self.mount_id,
            &options.context,
            VfsOperation::SetLength,
            self.transport.set_length(VfsSetLengthRequestV2 {
                project_id: self.project_id,
                mount_id: Some(MountIdV2::from_mount_id(self.mount_id)),
                handle_id: self.handle_id,
                length: len,
                expected_version: options
                    .expected_version
                    .map(|version| version.as_bytes().to_vec()),
                operation_id: options.context.operation_id.get(),
            }),
        )
        .await?;
        response_error(response.error, VfsOperation::SetLength)
    }

    async fn flush(&self, context: OperationContext) -> VfsResult<()> {
        self.handle_operation(VfsOperation::Flush, context).await
    }

    async fn sync(&self, context: OperationContext) -> VfsResult<()> {
        self.handle_operation(VfsOperation::Sync, context).await
    }
}

impl RemoteVfsFile {
    async fn handle_operation(
        &self,
        operation: VfsOperation,
        context: OperationContext,
    ) -> VfsResult<()> {
        let kind = match operation {
            VfsOperation::Flush => VfsHandleOperationKindV2::Flush,
            VfsOperation::Sync => VfsHandleOperationKindV2::Sync,
            _ => {
                return Err(proxy_error(
                    VfsErrorCode::Internal,
                    operation,
                    "invalid remote handle operation",
                ));
            }
        };
        let response = cancellable_transport_request(
            &self.transport,
            self.project_id,
            self.mount_id,
            &context,
            operation,
            self.transport
                .handle_operation(VfsHandleOperationRequestV2 {
                    project_id: self.project_id,
                    mount_id: Some(MountIdV2::from_mount_id(self.mount_id)),
                    handle_id: self.handle_id,
                    operation_id: context.operation_id.get(),
                    kind: kind as i32,
                }),
        )
        .await?;
        response_error(response.error, operation)
    }
}

fn required_mount_id(mount_id: Option<&MountIdV2>, operation: VfsOperation) -> VfsResult<MountId> {
    mount_id.map(MountIdV2::to_mount_id).ok_or_else(|| {
        service_error(
            VfsErrorCode::InvalidArgument,
            operation,
            "missing remote VFS mount ID",
        )
    })
}

fn mutation_key(
    mount_id: Option<&MountIdV2>,
    operation_id: u64,
    operation: VfsOperation,
) -> VfsResult<(MountId, OperationId)> {
    Ok((
        required_mount_id(mount_id, operation)?,
        OperationId::new(operation_id),
    ))
}

fn request_fingerprint<MessageType: proto::Message>(request: &MessageType) -> [u8; 32] {
    Sha256::digest(request.encode_to_vec()).into()
}

fn required_path(
    path: Option<&ProviderPathV2>,
    operation: VfsOperation,
) -> VfsResult<ProviderPath> {
    path.ok_or_else(|| {
        service_error(
            VfsErrorCode::InvalidArgument,
            operation,
            "missing remote VFS path",
        )
    })?
    .to_provider_path()
    .map_err(|error| wire_error(operation, error))
}

fn operation_response(result: VfsResult<()>) -> VfsOperationResponseV2 {
    VfsOperationResponseV2 {
        error: result.err().as_ref().map(VfsErrorV2::from_vfs_error),
    }
}

fn negotiation_error(error: VfsError) -> VfsNegotiateResponseV2 {
    VfsNegotiateResponseV2 {
        protocol_version: 0,
        mount_id: None,
        descriptor: None,
        capabilities: None,
        error: Some(VfsErrorV2::from_vfs_error(&error)),
    }
}

fn archive_mount_error(error: VfsError) -> VfsMountArchiveResponseV2 {
    VfsMountArchiveResponseV2 {
        protocol_version: 0,
        mount_id: None,
        descriptor: None,
        capabilities: None,
        error: Some(VfsErrorV2::from_vfs_error(&error)),
    }
}

fn directory_error(error: VfsError) -> VfsReadDirectoryResponseV2 {
    VfsReadDirectoryResponseV2 {
        entries: Vec::new(),
        next_cursor: None,
        error: Some(VfsErrorV2::from_vfs_error(&error)),
    }
}

async fn cancellable_transport_request<Response, RequestFuture>(
    transport: &Arc<dyn RemoteVfsTransport>,
    project_id: u64,
    mount_id: MountId,
    context: &OperationContext,
    operation: VfsOperation,
    request: RequestFuture,
) -> VfsResult<Response>
where
    RequestFuture: Future<Output = TransportResult<Response>>,
{
    context
        .cancellation
        .check(operation, &ProviderId::new("remote-vfs-proxy"))?;
    let request = Box::pin(request);
    let cancelled = Box::pin(context.cancellation.cancelled());
    match futures::future::select(request, cancelled).await {
        futures::future::Either::Left((response, _)) => {
            response.map_err(|error| transport_error(operation, error))
        }
        futures::future::Either::Right(((), _)) => {
            if let Err(error) = transport
                .cancel_operation(VfsCancelOperationRequestV2 {
                    project_id,
                    mount_id: Some(MountIdV2::from_mount_id(mount_id)),
                    operation_id: context.operation_id.get(),
                })
                .await
            {
                tracing::debug!(%error, "failed to send remote VFS cancellation");
            }
            Err(proxy_error(
                VfsErrorCode::Cancelled,
                operation,
                "remote VFS operation was cancelled",
            ))
        }
    }
}

fn response_error(error: Option<VfsErrorV2>, operation: VfsOperation) -> VfsResult<()> {
    match error {
        Some(error) => Err(error
            .to_vfs_error()
            .map_err(|error| wire_error(operation, error))?),
        None => Ok(()),
    }
}

fn wire_error(operation: VfsOperation, error: proto::VfsWireError) -> VfsError {
    proxy_error(VfsErrorCode::CorruptData, operation, error.to_string())
}

fn transport_error(operation: VfsOperation, error: anyhow::Error) -> VfsError {
    proxy_error(VfsErrorCode::Disconnected, operation, error.to_string())
}

fn missing_response_field(operation: VfsOperation, field: &str) -> VfsError {
    proxy_error(
        VfsErrorCode::CorruptData,
        operation,
        format!("remote VFS response is missing {field}"),
    )
}

fn service_error(
    code: VfsErrorCode,
    operation: VfsOperation,
    detail: impl Into<Arc<str>>,
) -> VfsError {
    VfsError::new(code, operation, ProviderId::new("remote-vfs-service")).with_detail(detail)
}

fn proxy_error(
    code: VfsErrorCode,
    operation: VfsOperation,
    detail: impl Into<Arc<str>>,
) -> VfsError {
    VfsError::new(code, operation, ProviderId::new("remote-vfs-proxy")).with_detail(detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_zip::{Compression, ZipEntryBuilder, base::write::ZipFileWriter};
    use futures::executor::block_on;
    use proto::Message as _;
    use std::{
        collections::BTreeSet,
        num::NonZeroUsize,
        sync::atomic::{AtomicBool, Ordering as AtomicOrdering},
    };
    use vfs::{
        ArchiveLimits, ArchiveProvider, MemoryProvider, WatchDepth,
        test_support::{
            FaultInjectingTransport, FrameFault, FrameOutcome, TransportDirection, TransportEvent,
            run_provider_conformance,
        },
    };

    async fn zip_fixture(member_contents: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut writer = ZipFileWriter::new(&mut bytes);
        writer
            .write_entry_whole(
                ZipEntryBuilder::new("dir/member.txt".into(), Compression::Deflate),
                member_contents,
            )
            .await
            .expect("archive member fixture should write");
        writer
            .write_entry_whole(
                ZipEntryBuilder::new("padding.bin".into(), Compression::Stored),
                &vec![0x5a; 2 * 1024 * 1024],
            )
            .await
            .expect("archive padding fixture should write");
        writer.close().await.expect("archive fixture should close");
        bytes
    }

    async fn read_provider_bytes(
        provider: &dyn VfsProvider,
        path: &ProviderPath,
    ) -> VfsResult<Vec<u8>> {
        let file = provider.open(path, OpenOptions::default()).await?;
        let length =
            usize::try_from(file.len(OperationContext::default()).await?).map_err(|_| {
                VfsError::new(
                    VfsErrorCode::TooLarge,
                    VfsOperation::Read,
                    ProviderId::new("archive-parity-test"),
                )
            })?;
        let mut bytes = vec![0; length];
        let read = file
            .read_at(0, &mut bytes, OperationContext::default())
            .await?;
        bytes.truncate(read);
        Ok(bytes)
    }

    #[test]
    fn loopback_remote_provider_passes_shared_conformance() {
        let result = block_on(async {
            let service = VfsService::default();
            service.register_provider(
                7,
                Arc::new(MemoryProvider::new(
                    "loopback-remote",
                    vfs::PathEncoding::PortableUtf8,
                )),
            )?;
            let proxy =
                RemoteProviderProxy::connect(0, 7, Arc::new(LoopbackVfsTransport::new(service)))
                    .await?;
            run_provider_conformance(Arc::new(proxy)).await
        });
        assert!(
            result.is_ok(),
            "loopback remote provider conformance failed: {result:?}"
        );
    }

    #[test]
    fn loopback_remote_provider_loads_media_bytes() {
        let result = block_on(async {
            let provider = Arc::new(MemoryProvider::new(
                "loopback-media",
                vfs::PathEncoding::PortableUtf8,
            ));
            let path = ProviderPath::from_byte_components(
                vfs::PathEncoding::PortableUtf8,
                [b"image.png".as_slice()],
            )
            .map_err(|error| {
                VfsError::new(
                    VfsErrorCode::InvalidPath,
                    VfsOperation::Read,
                    ProviderId::new("loopback-media-test"),
                )
                .with_detail(error.to_string())
            })?;
            let expected = b"\x89PNG\r\n\x1a\nprovider-media";
            let file = provider
                .open(
                    &path,
                    vfs::OpenOptions {
                        access: vfs::FileAccess::ReadWrite,
                        create: vfs::CreateDisposition::CreateNew,
                        expected_version: None,
                        context: vfs::OperationContext::default(),
                    },
                )
                .await?;
            file.write_at(0, expected, vfs::WriteAtOptions::default())
                .await?;

            let service = VfsService::default();
            service.register_provider(8, provider)?;
            let proxy =
                RemoteProviderProxy::connect(0, 8, Arc::new(LoopbackVfsTransport::new(service)))
                    .await?;
            let loaded =
                vfs::load_provider_bytes(&proxy, &path, vfs::OperationContext::default()).await?;
            if loaded.bytes != expected || loaded.metadata.size != expected.len() as u64 {
                return Err(VfsError::new(
                    VfsErrorCode::CorruptData,
                    VfsOperation::Read,
                    ProviderId::new("loopback-media-test"),
                ));
            }
            Ok::<_, VfsError>(())
        });
        assert!(result.is_ok(), "loopback media load failed: {result:?}");
    }

    #[test]
    fn loopback_remote_archive_mount_is_data_local_and_matches_local_tree() {
        let result = block_on(async {
            let source_provider = Arc::new(MemoryProvider::new(
                "archive-source",
                vfs::PathEncoding::PortableUtf8,
            ));
            let source_path = ProviderPath::from_byte_components(
                vfs::PathEncoding::PortableUtf8,
                [b"bundle.zip".as_slice()],
            )
            .map_err(|error| {
                VfsError::new(
                    VfsErrorCode::InvalidPath,
                    VfsOperation::Open,
                    ProviderId::new("archive-parity-test"),
                )
                .with_detail(error.to_string())
            })?;
            let initial_archive = zip_fixture(b"remote archive member").await;
            let source_file = source_provider
                .open(
                    &source_path,
                    OpenOptions {
                        access: vfs::FileAccess::ReadWrite,
                        create: vfs::CreateDisposition::CreateNew,
                        ..OpenOptions::default()
                    },
                )
                .await?;
            source_file
                .write_at(0, &initial_archive, WriteAtOptions::default())
                .await?;
            let source_metadata = source_provider
                .stat(&source_path, StatOptions::default())
                .await?;

            let local_source = source_provider
                .open(&source_path, OpenOptions::default())
                .await?;
            let local_archive = ArchiveProvider::mount(
                "local-archive",
                local_source,
                source_metadata.content_version.clone(),
                1,
                ArchiveLimits::default(),
                OperationContext::default(),
            )
            .await?;

            let service = VfsService::default();
            let source_snapshot =
                VfsSnapshot::mount(source_provider.clone(), SnapshotBudgets::default())?;
            let source_authorized = Arc::new(AtomicBool::new(true));
            service.register_snapshot_with_authorizer(23, source_snapshot, {
                let source_authorized = source_authorized.clone();
                Arc::new(move |path, operation| {
                    if source_authorized.load(AtomicOrdering::Acquire) {
                        Ok(())
                    } else {
                        Err(VfsError::new(
                            VfsErrorCode::PermissionDenied,
                            operation,
                            ProviderId::new("archive-source-policy"),
                        )
                        .with_path(path.clone()))
                    }
                })
            })?;
            let transport = Arc::new(LoopbackVfsTransport::new(service.clone()));
            let source_proxy = RemoteProviderProxy::connect(0, 23, transport.clone()).await?;
            let remote_archive = source_proxy
                .mount_archive(
                    &source_path,
                    source_metadata.content_version.clone(),
                    1,
                    OperationContext::default(),
                )
                .await?;
            let root = ProviderPath::root(vfs::PathEncoding::UnixBytes);
            let local_root = local_archive
                .read_dir(&root, DirPageRequest::default())
                .await?;
            let remote_root = remote_archive
                .read_dir(&root, DirPageRequest::default())
                .await?;
            assert_eq!(
                local_root
                    .entries
                    .iter()
                    .map(|entry| entry.path.clone())
                    .collect::<BTreeSet<_>>(),
                remote_root
                    .entries
                    .iter()
                    .map(|entry| entry.path.clone())
                    .collect::<BTreeSet<_>>()
            );
            assert_eq!(transport.read_at_request_count(), 0);
            assert_eq!(transport.requested_read_bytes(), 0);

            let member_path = ProviderPath::from_byte_components(
                vfs::PathEncoding::UnixBytes,
                [b"dir".as_slice(), b"member.txt".as_slice()],
            )
            .map_err(|error| {
                VfsError::new(
                    VfsErrorCode::InvalidPath,
                    VfsOperation::Open,
                    ProviderId::new("archive-parity-test"),
                )
                .with_detail(error.to_string())
            })?;
            let local_bytes = read_provider_bytes(&local_archive, &member_path).await?;
            let remote_bytes = read_provider_bytes(&remote_archive, &member_path).await?;
            assert_eq!(local_bytes, remote_bytes);
            assert_eq!(remote_bytes, b"remote archive member");
            assert!(
                transport.requested_read_bytes()
                    < u64::try_from(initial_archive.len()).unwrap_or(u64::MAX),
                "remote member read must not transfer the whole archive"
            );
            let held_member = remote_archive
                .open(&member_path, OpenOptions::default())
                .await?;
            source_authorized.store(false, AtomicOrdering::Release);
            assert_eq!(
                remote_archive
                    .stat(&root, StatOptions::default())
                    .await
                    .expect_err("child archive mount should revalidate source authorization")
                    .code(),
                VfsErrorCode::PermissionDenied
            );
            let mut held_byte = [0; 1];
            assert_eq!(
                held_member
                    .read_at(0, &mut held_byte, OperationContext::default())
                    .await
                    .expect_err("open archive handle should revalidate source authorization")
                    .code(),
                VfsErrorCode::PermissionDenied
            );
            source_authorized.store(true, AtomicOrdering::Release);

            let replacement_archive = zip_fixture(b"replacement").await;
            source_file
                .set_len(
                    u64::try_from(replacement_archive.len()).unwrap_or(u64::MAX),
                    WriteAtOptions::default(),
                )
                .await?;
            source_file
                .write_at(0, &replacement_archive, WriteAtOptions::default())
                .await?;
            let replacement_metadata = source_provider
                .stat(&source_path, StatOptions::default())
                .await?;
            assert_eq!(
                remote_archive
                    .stat(&root, StatOptions::default())
                    .await
                    .expect_err("changed archive source should mark the child mount stale")
                    .code(),
                VfsErrorCode::StaleVersion
            );
            let replacement = source_proxy
                .mount_archive(
                    &source_path,
                    replacement_metadata.content_version,
                    1,
                    OperationContext::default(),
                )
                .await?;
            assert_eq!(service.archive_mount_count(), 1);
            assert_eq!(
                remote_archive
                    .stat(&root, StatOptions::default())
                    .await
                    .expect_err("stale archive mount should be invalidated")
                    .code(),
                VfsErrorCode::NotFound
            );
            assert_eq!(
                read_provider_bytes(&replacement, &member_path).await?,
                b"replacement"
            );
            VfsResult::Ok(())
        });
        assert!(result.is_ok(), "remote archive parity failed: {result:?}");
    }

    #[test]
    fn remote_handle_drop_releases_the_server_handle() {
        let result = block_on(async {
            let service = VfsService::default();
            service.register_provider(
                9,
                Arc::new(MemoryProvider::new(
                    "loopback-handles",
                    vfs::PathEncoding::PortableUtf8,
                )),
            )?;
            let proxy = RemoteProviderProxy::connect(
                0,
                9,
                Arc::new(LoopbackVfsTransport::new(service.clone())),
            )
            .await?;
            let path = ProviderPath::from_byte_components(
                vfs::PathEncoding::PortableUtf8,
                [b"handle.txt".as_slice()],
            )
            .map_err(|error| wire_error(VfsOperation::Open, error.into()))?;
            let file = proxy
                .open(
                    &path,
                    OpenOptions {
                        access: vfs::FileAccess::ReadWrite,
                        create: vfs::CreateDisposition::CreateNew,
                        expected_version: None,
                        context: OperationContext::default(),
                    },
                )
                .await?;
            if service.open_handle_count() != 1 {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::Open,
                    "server did not retain the open handle",
                ));
            }
            drop(file);
            if service.open_handle_count() != 0 {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::Open,
                    "dropping the remote file did not release its server handle",
                ));
            }
            Ok::<_, VfsError>(())
        });
        assert!(result.is_ok(), "remote handle release failed: {result:?}");
    }

    #[test]
    fn mutation_result_journal_replays_only_identical_requests() {
        let result = block_on(async {
            let service = VfsService::default();
            service.register_provider(
                11,
                Arc::new(MemoryProvider::new(
                    "loopback-idempotency",
                    vfs::PathEncoding::PortableUtf8,
                )),
            )?;
            let proxy =
                RemoteProviderProxy::connect(0, 11, Arc::new(LoopbackVfsTransport::new(service)))
                    .await?;
            let first_path = ProviderPath::from_byte_components(
                vfs::PathEncoding::PortableUtf8,
                [b"first".as_slice()],
            )
            .map_err(|error| wire_error(VfsOperation::CreateDirectory, error.into()))?;
            let second_path = ProviderPath::from_byte_components(
                vfs::PathEncoding::PortableUtf8,
                [b"second".as_slice()],
            )
            .map_err(|error| wire_error(VfsOperation::CreateDirectory, error.into()))?;
            let options = CreateDirOptions {
                parents: CreateParents::No,
                context: OperationContext {
                    operation_id: OperationId::new(77),
                    cancellation: CancellationToken::default(),
                },
            };
            proxy.create_dir(&first_path, options.clone()).await?;
            proxy.create_dir(&first_path, options.clone()).await?;
            let mismatch = proxy.create_dir(&second_path, options).await;
            if !mismatch
                .as_ref()
                .err()
                .is_some_and(|error| error.code() == VfsErrorCode::Conflict)
            {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::CreateDirectory,
                    "idempotency journal accepted a mismatched retry payload",
                ));
            }
            Ok::<_, VfsError>(())
        });
        assert!(
            result.is_ok(),
            "idempotency journal test failed: {result:?}"
        );
    }

    #[test]
    fn active_remote_request_observes_cancellation() {
        let result = block_on(async {
            let service = VfsService::default();
            let transport: Arc<dyn RemoteVfsTransport> =
                Arc::new(LoopbackVfsTransport::new(service));
            let context = OperationContext {
                operation_id: OperationId::new(91),
                cancellation: CancellationToken::default(),
            };
            let cancellation = context.cancellation.clone();
            let request = cancellable_transport_request(
                &transport,
                0,
                MountId::new(1),
                &context,
                VfsOperation::Read,
                futures::future::pending::<TransportResult<()>>(),
            );
            let cancel = async move {
                futures::future::poll_fn(|_| {
                    cancellation.cancel();
                    std::task::Poll::Ready(())
                })
                .await
            };
            let (request_result, ()) = futures::future::join(request, cancel).await;
            if !request_result
                .as_ref()
                .err()
                .is_some_and(|error| error.code() == VfsErrorCode::Cancelled)
            {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::Read,
                    "active remote request did not observe cancellation",
                ));
            }
            Ok::<_, VfsError>(())
        });
        assert!(
            result.is_ok(),
            "active cancellation test failed: {result:?}"
        );
    }

    #[test]
    fn active_remote_watch_observes_cancellation() {
        let result = block_on(async {
            let service = VfsService::default();
            service.register_provider(
                31,
                Arc::new(MemoryProvider::new(
                    "loopback-watch-cancellation",
                    vfs::PathEncoding::PortableUtf8,
                )),
            )?;
            let proxy =
                RemoteProviderProxy::connect(0, 31, Arc::new(LoopbackVfsTransport::new(service)))
                    .await?;
            let cancellation = CancellationToken::default();
            let mut watch = proxy
                .watch(WatchRequest {
                    path: ProviderPath::root(vfs::PathEncoding::PortableUtf8),
                    depth: WatchDepth::Recursive,
                    resume_after_sequence: None,
                    context: OperationContext {
                        operation_id: OperationId::new(92),
                        cancellation: cancellation.clone(),
                    },
                })
                .await?;
            cancellation.cancel();
            let cancelled = watch.next().await;
            if !cancelled
                .as_ref()
                .and_then(|result| result.as_ref().err())
                .is_some_and(|error| error.code() == VfsErrorCode::Cancelled)
            {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::Watch,
                    "active remote watch did not observe cancellation",
                ));
            }
            Ok::<_, VfsError>(())
        });
        assert!(
            result.is_ok(),
            "active watch cancellation test failed: {result:?}"
        );
    }

    #[test]
    fn response_loss_retry_replays_the_committed_mutation() {
        let result = block_on(async {
            let provider = Arc::new(MemoryProvider::new(
                "loopback-response-loss",
                vfs::PathEncoding::PortableUtf8,
            ));
            let service = VfsService::default();
            let mount_id = service.register_provider(13, provider.clone())?;
            let path = ProviderPath::from_byte_components(
                vfs::PathEncoding::PortableUtf8,
                [b"committed-once".as_slice()],
            )
            .map_err(|error| wire_error(VfsOperation::CreateDirectory, error.into()))?;
            let request = VfsCreateDirectoryRequestV2 {
                project_id: 0,
                mount_id: Some(MountIdV2::from_mount_id(mount_id)),
                path: Some(ProviderPathV2::from_provider_path(&path)),
                parents: false,
                operation_id: 501,
            };
            let Some(maximum_chunk_size) = NonZeroUsize::new(3) else {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::CreateDirectory,
                    "fault chunk size must be non-zero",
                ));
            };
            let mut faults = FaultInjectingTransport::new([
                FrameFault {
                    direction: TransportDirection::ClientToServer,
                    sequence: 0,
                    delay: Duration::from_millis(2),
                    maximum_chunk_size: Some(maximum_chunk_size),
                    outcome: FrameOutcome::Deliver,
                },
                FrameFault {
                    direction: TransportDirection::ServerToClient,
                    sequence: 0,
                    delay: Duration::ZERO,
                    maximum_chunk_size: None,
                    outcome: FrameOutcome::Drop,
                },
            ])
            .map_err(anyhow::Error::new)
            .map_err(|error| transport_error(VfsOperation::CreateDirectory, error))?;

            let first_request_events =
                faults.transmit(TransportDirection::ClientToServer, request.encode_to_vec());
            let first_request_bytes = delivered_frame_bytes(&first_request_events)?;
            if first_request_events.len() <= 1 {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::CreateDirectory,
                    "request was not fragmented by the fault transport",
                ));
            }
            if !first_request_events.first().is_some_and(|event| {
                matches!(
                    event,
                    TransportEvent::Chunk { delay, .. }
                        if *delay == Duration::from_millis(2)
                )
            }) {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::CreateDirectory,
                    "fault transport did not preserve the injected latency",
                ));
            }
            let first_request = VfsCreateDirectoryRequestV2::decode(first_request_bytes.as_slice())
                .map_err(anyhow::Error::new)
                .map_err(|error| transport_error(VfsOperation::CreateDirectory, error))?;
            let first_response = service.create_directory(first_request).await;
            let first_response_events = faults.transmit(
                TransportDirection::ServerToClient,
                first_response.encode_to_vec(),
            );
            if !matches!(
                first_response_events.as_slice(),
                [TransportEvent::Dropped { .. }]
            ) {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::CreateDirectory,
                    "fault transport did not drop the committed response",
                ));
            }

            faults.reconnect();
            let retry_request_events =
                faults.transmit(TransportDirection::ClientToServer, request.encode_to_vec());
            let retry_request = VfsCreateDirectoryRequestV2::decode(
                delivered_frame_bytes(&retry_request_events)?.as_slice(),
            )
            .map_err(anyhow::Error::new)
            .map_err(|error| transport_error(VfsOperation::CreateDirectory, error))?;
            let retry_response = service.create_directory(retry_request).await;
            let retry_response_events = faults.transmit(
                TransportDirection::ServerToClient,
                retry_response.encode_to_vec(),
            );
            let retry_response = VfsOperationResponseV2::decode(
                delivered_frame_bytes(&retry_response_events)?.as_slice(),
            )
            .map_err(anyhow::Error::new)
            .map_err(|error| transport_error(VfsOperation::CreateDirectory, error))?;
            response_error(retry_response.error, VfsOperation::CreateDirectory)?;
            provider.stat(&path, StatOptions::default()).await?;
            if faults.remaining_fault_count() != 0 {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::CreateDirectory,
                    "fault script was not fully consumed",
                ));
            }
            Ok::<_, VfsError>(())
        });
        assert!(
            result.is_ok(),
            "response-loss idempotency test failed: {result:?}"
        );
    }

    #[test]
    fn remote_watch_resume_replays_history_or_reports_overflow() {
        let result = block_on(async {
            let provider = Arc::new(MemoryProvider::with_journal_capacity(
                "loopback-watch-resume",
                vfs::PathEncoding::PortableUtf8,
                2,
            ));
            let service = VfsService::default();
            service.register_provider(17, provider)?;
            let proxy =
                RemoteProviderProxy::connect(0, 17, Arc::new(LoopbackVfsTransport::new(service)))
                    .await?;
            let root = ProviderPath::root(vfs::PathEncoding::PortableUtf8);
            let snapshot = VfsSnapshot::mount(Arc::new(proxy.clone()), SnapshotBudgets::default())?;
            snapshot.load_directory(&root).await?;
            let mut watch = proxy
                .watch(WatchRequest {
                    path: root.clone(),
                    depth: WatchDepth::Recursive,
                    resume_after_sequence: None,
                    context: OperationContext::default(),
                })
                .await?;
            let first = ProviderPath::from_byte_components(
                vfs::PathEncoding::PortableUtf8,
                [b"first".as_slice()],
            )
            .map_err(|error| wire_error(VfsOperation::CreateDirectory, error.into()))?;
            proxy
                .create_dir(&first, CreateDirOptions::default())
                .await?;
            let first_batch = watch.next().await.ok_or_else(|| {
                service_error(
                    VfsErrorCode::Disconnected,
                    VfsOperation::Watch,
                    "initial remote watch ended",
                )
            })??;
            snapshot.apply_event_batch(first_batch.clone()).await?;
            drop(watch);

            let second = ProviderPath::from_byte_components(
                vfs::PathEncoding::PortableUtf8,
                [b"second".as_slice()],
            )
            .map_err(|error| wire_error(VfsOperation::CreateDirectory, error.into()))?;
            proxy
                .create_dir(&second, CreateDirOptions::default())
                .await?;
            let mut resumed = proxy
                .watch(WatchRequest {
                    path: root.clone(),
                    depth: WatchDepth::Recursive,
                    resume_after_sequence: Some(first_batch.last_sequence),
                    context: OperationContext::default(),
                })
                .await?;
            let resumed_batch = resumed.next().await.ok_or_else(|| {
                service_error(
                    VfsErrorCode::Disconnected,
                    VfsOperation::Watch,
                    "resumed remote watch ended",
                )
            })??;
            if !resumed_batch
                .events
                .iter()
                .any(|event| event.path == second)
            {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::Watch,
                    "remote watch did not replay retained history",
                ));
            }
            snapshot.apply_event_batch(resumed_batch).await?;
            drop(resumed);

            for name in [
                b"third".as_slice(),
                b"fourth".as_slice(),
                b"fifth".as_slice(),
            ] {
                let path =
                    ProviderPath::from_byte_components(vfs::PathEncoding::PortableUtf8, [name])
                        .map_err(|error| wire_error(VfsOperation::CreateDirectory, error.into()))?;
                proxy.create_dir(&path, CreateDirOptions::default()).await?;
            }
            let mut truncated = proxy
                .watch(WatchRequest {
                    path: root.clone(),
                    depth: WatchDepth::Recursive,
                    resume_after_sequence: Some(first_batch.last_sequence),
                    context: OperationContext::default(),
                })
                .await?;
            let overflow = truncated.next().await.ok_or_else(|| {
                service_error(
                    VfsErrorCode::Disconnected,
                    VfsOperation::Watch,
                    "truncated remote watch ended",
                )
            })??;
            if !overflow.events.iter().any(|event| {
                matches!(
                    &event.kind,
                    vfs::VfsEventKind::Overflow { rescan_root } if rescan_root == &root
                )
            }) {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::Watch,
                    "truncated remote journal did not report overflow",
                ));
            }
            snapshot.apply_event_batch(overflow).await?;
            let actual_paths = snapshot
                .registry()
                .children(&root)
                .into_iter()
                .map(|record| record.path)
                .collect::<BTreeSet<_>>();
            let expected_paths = ["first", "second", "third", "fourth", "fifth"]
                .into_iter()
                .map(|name| {
                    ProviderPath::from_byte_components(
                        vfs::PathEncoding::PortableUtf8,
                        [name.as_bytes()],
                    )
                    .map_err(|error| wire_error(VfsOperation::Watch, error.into()))
                })
                .collect::<VfsResult<BTreeSet<_>>>()?;
            if actual_paths != expected_paths {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::Watch,
                    "remote overflow rescan did not converge to the provider tree",
                ));
            }
            Ok::<_, VfsError>(())
        });
        assert!(
            result.is_ok(),
            "remote watch resume test failed: {result:?}"
        );
    }

    #[test]
    fn remote_service_revalidates_mount_path_authorization() {
        let result = block_on(async {
            let provider = Arc::new(MemoryProvider::new(
                "loopback-authorization",
                vfs::PathEncoding::PortableUtf8,
            ));
            let private_path = ProviderPath::from_byte_components(
                vfs::PathEncoding::PortableUtf8,
                [b"private".as_slice()],
            )
            .map_err(|error| wire_error(VfsOperation::Stat, error.into()))?;
            provider
                .create_dir(&private_path, CreateDirOptions::default())
                .await?;
            let public_path = ProviderPath::from_byte_components(
                vfs::PathEncoding::PortableUtf8,
                [b"public.txt".as_slice()],
            )
            .map_err(|error| wire_error(VfsOperation::Open, error.into()))?;
            let public_file = provider
                .open(
                    &public_path,
                    OpenOptions {
                        access: vfs::FileAccess::ReadWrite,
                        create: vfs::CreateDisposition::CreateNew,
                        ..OpenOptions::default()
                    },
                )
                .await?;
            public_file
                .write_at(0, b"public", WriteAtOptions::default())
                .await?;
            let snapshot = VfsSnapshot::mount(provider, SnapshotBudgets::default())?;
            let service = VfsService::default();
            service.register_snapshot_with_authorizer(
                19,
                snapshot.clone(),
                Arc::new(|path, operation| {
                    if path
                        .file_name()
                        .is_some_and(|name| name.as_bytes() == b"private")
                    {
                        Err(service_error(
                            VfsErrorCode::PermissionDenied,
                            operation,
                            "fixture path is private",
                        ))
                    } else {
                        Ok(())
                    }
                }),
            )?;
            let proxy = RemoteProviderProxy::connect(
                0,
                19,
                Arc::new(LoopbackVfsTransport::new(service.clone())),
            )
            .await?;
            let denied = proxy.stat(&private_path, StatOptions::default()).await;
            if !denied
                .as_ref()
                .err()
                .is_some_and(|error| error.code() == VfsErrorCode::PermissionDenied)
            {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::Stat,
                    "remote service did not enforce its mount authorizer",
                ));
            }
            let open_file = proxy.open(&public_path, OpenOptions::default()).await?;
            service.register_snapshot_with_authorizer(
                19,
                snapshot.clone(),
                Arc::new(|path, operation| {
                    Err(service_error(
                        VfsErrorCode::PermissionDenied,
                        operation,
                        format!("fixture revoked access to {}", path.display()),
                    ))
                }),
            )?;
            let mut byte = [0; 1];
            let denied_read = open_file
                .read_at(0, &mut byte, OperationContext::default())
                .await;
            if !denied_read
                .as_ref()
                .err()
                .is_some_and(|error| error.code() == VfsErrorCode::PermissionDenied)
            {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::Read,
                    "remote handle did not revalidate its path authorization",
                ));
            }
            service.register_snapshot_with_authorizer(19, snapshot, Arc::new(|_, _| Ok(())))?;
            proxy.stat(&private_path, StatOptions::default()).await?;
            open_file
                .read_at(0, &mut byte, OperationContext::default())
                .await?;
            Ok::<_, VfsError>(())
        });
        assert!(
            result.is_ok(),
            "remote authorization test failed: {result:?}"
        );
    }

    #[test]
    fn negotiation_preserves_unknown_capability_extensions() {
        let result = block_on(async {
            let extension = proto::VfsCapabilityExtensionV2 {
                id: 77,
                payload: b"future-capability".to_vec(),
            };
            let service = VfsService::with_capability_extensions(
                DEFAULT_HANDLE_LEASE,
                vec![extension.clone()].into(),
            );
            service.register_provider(
                23,
                Arc::new(MemoryProvider::new(
                    "loopback-capability-extension",
                    vfs::PathEncoding::PortableUtf8,
                )),
            )?;
            let proxy =
                RemoteProviderProxy::connect(0, 23, Arc::new(LoopbackVfsTransport::new(service)))
                    .await?;
            if proxy.capability_extensions() != [extension] {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::Stat,
                    "unknown capability extension was not preserved",
                ));
            }
            Ok::<_, VfsError>(())
        });
        assert!(
            result.is_ok(),
            "capability extension negotiation failed: {result:?}"
        );
    }

    #[test]
    fn remote_stat_many_preserves_per_path_results_and_batch_limits() {
        let result = block_on(async {
            let service = VfsService::default();
            service.register_provider(
                29,
                Arc::new(MemoryProvider::new(
                    "loopback-stat-many",
                    vfs::PathEncoding::PortableUtf8,
                )),
            )?;
            let proxy =
                RemoteProviderProxy::connect(0, 29, Arc::new(LoopbackVfsTransport::new(service)))
                    .await?;
            let existing = ProviderPath::from_byte_components(
                vfs::PathEncoding::PortableUtf8,
                [b"existing".as_slice()],
            )
            .map_err(|error| wire_error(VfsOperation::Stat, error.into()))?;
            let missing = ProviderPath::from_byte_components(
                vfs::PathEncoding::PortableUtf8,
                [b"missing".as_slice()],
            )
            .map_err(|error| wire_error(VfsOperation::Stat, error.into()))?;
            proxy
                .create_dir(&existing, CreateDirOptions::default())
                .await?;
            let results = proxy
                .stat_many(&[existing, missing], StatOptions::default())
                .await?;
            if results.len() != 2
                || results.first().is_none_or(Result::is_err)
                || !results
                    .get(1)
                    .and_then(|result| result.as_ref().err())
                    .is_some_and(|error| error.code() == VfsErrorCode::NotFound)
            {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::Stat,
                    "stat-many did not preserve per-path results",
                ));
            }
            let root = ProviderPath::root(vfs::PathEncoding::PortableUtf8);
            let oversized = vec![root; 4_097];
            let oversized_result = proxy.stat_many(&oversized, StatOptions::default()).await;
            if !oversized_result
                .as_ref()
                .err()
                .is_some_and(|error| error.code() == VfsErrorCode::TooLarge)
            {
                return Err(service_error(
                    VfsErrorCode::Internal,
                    VfsOperation::Stat,
                    "stat-many batch limit was not enforced",
                ));
            }
            Ok::<_, VfsError>(())
        });
        assert!(result.is_ok(), "remote stat-many test failed: {result:?}");
    }

    fn delivered_frame_bytes(events: &[TransportEvent]) -> VfsResult<Vec<u8>> {
        let mut bytes = Vec::new();
        for event in events {
            match event {
                TransportEvent::Chunk { bytes: chunk, .. } => bytes.extend_from_slice(chunk),
                TransportEvent::Dropped { .. } | TransportEvent::Disconnected { .. } => {
                    return Err(service_error(
                        VfsErrorCode::Disconnected,
                        VfsOperation::Read,
                        "fault transport did not deliver the frame",
                    ));
                }
            }
        }
        Ok(bytes)
    }
}
