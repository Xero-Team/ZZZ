use anyhow::Result as TransportResult;
use async_trait::async_trait;
use futures::{StreamExt as _, stream::BoxStream};
use parking_lot::Mutex;
use proto::{
    MountIdV2, ProviderPathV2, VfsCancelOperationRequestV2, VfsCloseHandleRequestV2,
    VfsCopyRequestV2, VfsCreateDirectoryRequestV2, VfsDirectoryEntryV2, VfsErrorV2,
    VfsFileLengthRequestV2, VfsFileLengthResponseV2, VfsHandleOperationKindV2,
    VfsHandleOperationRequestV2, VfsNegotiateRequestV2, VfsNegotiateResponseV2, VfsOpenRequestV2,
    VfsOpenResponseV2, VfsOperationResponseV2, VfsProviderCapabilitiesV2, VfsProviderDescriptorV2,
    VfsReadAtRequestV2, VfsReadAtResponseV2, VfsReadDirectoryRequestV2, VfsReadDirectoryResponseV2,
    VfsRemoveRequestV2, VfsRemoveResponseV2, VfsRenameRequestV2, VfsRenewHandleRequestV2,
    VfsRenewHandleResponseV2, VfsSetLengthRequestV2, VfsStatRequestV2, VfsStatResponseV2,
    VfsWatchRequestV2, VfsWatchResponseV2, VfsWriteAtRequestV2, VfsWriteAtResponseV2,
};
use std::{
    collections::BTreeMap,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use vfs::{
    CancellationToken, CopyOptions, CreateDirOptions, CreateParents, DirCursor, DirPage,
    DirPageRequest, EventBatch, MountId, OpenOptions, OperationContext, OperationId,
    ProviderCapabilities, ProviderDescriptor, ProviderId, ProviderPath, RemoveOptions,
    RemoveOutcome, RemovedEntryCount, RenameOptions, SnapshotBudgets, StatOptions,
    SymbolicLinkMode, VfsError, VfsErrorCode, VfsFile, VfsOperation, VfsProvider, VfsResult,
    VfsSnapshot, VfsVersion, WatchRequest, WriteAtOptions,
};

pub const REMOTE_VFS_PROTOCOL_VERSION: u32 = 1;
const DEFAULT_HANDLE_LEASE: Duration = Duration::from_secs(30);

#[async_trait]
pub trait RemoteVfsTransport: Send + Sync {
    async fn negotiate(
        &self,
        request: VfsNegotiateRequestV2,
    ) -> TransportResult<VfsNegotiateResponseV2>;
    async fn stat(&self, request: VfsStatRequestV2) -> TransportResult<VfsStatResponseV2>;
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
    fn release_handle(&self, request: VfsCloseHandleRequestV2);
}

#[derive(Clone)]
pub struct VfsService {
    inner: Arc<VfsServiceState>,
}

struct VfsServiceState {
    mounts_by_worktree: Mutex<BTreeMap<u64, VfsSnapshot>>,
    mounts_by_id: Mutex<BTreeMap<MountId, VfsSnapshot>>,
    handles: Mutex<BTreeMap<u64, ServiceHandle>>,
    operations: Arc<Mutex<BTreeMap<(MountId, OperationId), CancellationToken>>>,
    next_handle_id: AtomicU64,
    handle_lease: Duration,
}

struct ServiceHandle {
    mount_id: MountId,
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

impl Default for VfsService {
    fn default() -> Self {
        Self::new(DEFAULT_HANDLE_LEASE)
    }
}

impl VfsService {
    pub fn new(handle_lease: Duration) -> Self {
        Self {
            inner: Arc::new(VfsServiceState {
                mounts_by_worktree: Mutex::new(BTreeMap::new()),
                mounts_by_id: Mutex::new(BTreeMap::new()),
                handles: Mutex::new(BTreeMap::new()),
                operations: Arc::new(Mutex::new(BTreeMap::new())),
                next_handle_id: AtomicU64::new(1),
                handle_lease,
            }),
        }
    }

    pub fn register_provider(
        &self,
        worktree_id: u64,
        provider: Arc<dyn VfsProvider>,
    ) -> VfsResult<MountId> {
        let snapshot = VfsSnapshot::mount(provider, SnapshotBudgets::default())?;
        Ok(self.register_snapshot(worktree_id, snapshot))
    }

    pub fn register_snapshot(&self, worktree_id: u64, snapshot: VfsSnapshot) -> MountId {
        let mut mounts_by_worktree = self.inner.mounts_by_worktree.lock();
        if let Some(existing) = mounts_by_worktree.get(&worktree_id) {
            return existing.registry().mount_id();
        }
        let mount_id = snapshot.registry().mount_id();
        mounts_by_worktree.insert(worktree_id, snapshot.clone());
        drop(mounts_by_worktree);
        self.inner.mounts_by_id.lock().insert(mount_id, snapshot);
        mount_id
    }

    pub fn release_all_handles(&self) {
        self.inner.handles.lock().clear();
    }

    pub fn open_handle_count(&self) -> usize {
        self.prune_expired_handles();
        self.inner.handles.lock().len()
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
        VfsNegotiateResponseV2 {
            protocol_version: REMOTE_VFS_PROTOCOL_VERSION,
            mount_id: Some(MountIdV2::from_mount_id(snapshot.registry().mount_id())),
            descriptor: Some(VfsProviderDescriptorV2::from_provider_descriptor(
                descriptor,
            )),
            capabilities: Some(VfsProviderCapabilitiesV2::from_provider_capabilities(
                &snapshot.provider().capabilities(),
            )),
            error: None,
        }
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
                    Ok(entries) => VfsReadDirectoryResponseV2 {
                        entries,
                        next_cursor: page.next_cursor.map(|cursor| cursor.as_bytes().to_vec()),
                        error: None,
                    },
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
        match self.open_inner(request).await {
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
        }
    }

    async fn open_inner(&self, request: VfsOpenRequestV2) -> VfsResult<u64> {
        let (snapshot, mount_id) = self.snapshot(request.mount_id.as_ref(), VfsOperation::Open)?;
        let path = required_path(request.path.as_ref(), VfsOperation::Open)?;
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
        match result {
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
        }
    }

    pub async fn set_length(&self, request: VfsSetLengthRequestV2) -> VfsOperationResponseV2 {
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
        operation_response(result)
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
        let result = async {
            let (snapshot, mount_id) =
                self.snapshot(request.mount_id.as_ref(), VfsOperation::CreateDirectory)?;
            let path = required_path(request.path.as_ref(), VfsOperation::CreateDirectory)?;
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
        operation_response(result)
    }

    pub async fn remove(&self, request: VfsRemoveRequestV2) -> VfsRemoveResponseV2 {
        let result = async {
            let (snapshot, mount_id) =
                self.snapshot(request.mount_id.as_ref(), VfsOperation::Remove)?;
            let path = required_path(request.path.as_ref(), VfsOperation::Remove)?;
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
        match result {
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
        }
    }

    pub async fn rename(&self, request: VfsRenameRequestV2) -> VfsOperationResponseV2 {
        let result = async {
            let (snapshot, mount_id) =
                self.snapshot(request.mount_id.as_ref(), VfsOperation::Rename)?;
            let source = required_path(request.source.as_ref(), VfsOperation::Rename)?;
            let target = required_path(request.target.as_ref(), VfsOperation::Rename)?;
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
        operation_response(result)
    }

    pub async fn copy(&self, request: VfsCopyRequestV2) -> VfsOperationResponseV2 {
        let result = async {
            let (snapshot, mount_id) =
                self.snapshot(request.mount_id.as_ref(), VfsOperation::Copy)?;
            let source = required_path(request.source.as_ref(), VfsOperation::Copy)?;
            let target = required_path(request.target.as_ref(), VfsOperation::Copy)?;
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
        operation_response(result)
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

    pub fn release_handle(&self, request: VfsCloseHandleRequestV2) {
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
        self.inner
            .mounts_by_id
            .lock()
            .get(&mount_id)
            .cloned()
            .ok_or_else(|| {
                service_error(
                    VfsErrorCode::NotFound,
                    operation,
                    "remote VFS mount is not registered",
                )
            })
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
        Ok(handle.file.clone())
    }

    fn prune_expired_handles(&self) {
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
}

impl LoopbackVfsTransport {
    pub fn new(service: VfsService) -> Self {
        Self { service }
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

    async fn stat(&self, request: VfsStatRequestV2) -> TransportResult<VfsStatResponseV2> {
        Ok(self.service.stat(request).await)
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

    fn release_handle(&self, request: VfsCloseHandleRequestV2) {
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

    async fn stat(&self, request: VfsStatRequestV2) -> TransportResult<VfsStatResponseV2> {
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

    fn release_handle(&self, request: VfsCloseHandleRequestV2) {
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
        response_error(response.error)?;
        if response.protocol_version != REMOTE_VFS_PROTOCOL_VERSION {
            return Err(proxy_error(
                VfsErrorCode::Unsupported,
                VfsOperation::Stat,
                "server selected an unsupported VFS protocol version",
            ));
        }
        let mount_id = response
            .mount_id
            .as_ref()
            .ok_or_else(|| missing_response_field(VfsOperation::Stat, "mount ID"))?
            .to_mount_id();
        let descriptor = response
            .descriptor
            .as_ref()
            .ok_or_else(|| missing_response_field(VfsOperation::Stat, "provider descriptor"))?
            .to_provider_descriptor()
            .map_err(|error| wire_error(VfsOperation::Stat, error))?;
        let capabilities = response
            .capabilities
            .as_ref()
            .ok_or_else(|| missing_response_field(VfsOperation::Stat, "provider capabilities"))?
            .to_provider_capabilities()
            .map_err(|error| wire_error(VfsOperation::Stat, error))?;
        Ok(Self {
            project_id,
            mount_id,
            descriptor,
            capabilities,
            transport,
        })
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
        let response = self
            .transport
            .stat(VfsStatRequestV2 {
                project_id: self.project_id,
                mount_id: self.mount(),
                path: self.request_path(path),
                operation_id: options.context.operation_id.get(),
                follow_symbolic_link: options.symbolic_link_mode == SymbolicLinkMode::Follow,
            })
            .await
            .map_err(|error| transport_error(VfsOperation::Stat, error))?;
        response_error(response.error)?;
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
        let response = self
            .transport
            .read_directory(VfsReadDirectoryRequestV2 {
                project_id: self.project_id,
                mount_id: self.mount(),
                path: self.request_path(path),
                cursor: request.cursor.map(|cursor| cursor.as_bytes().to_vec()),
                limit: request.limit.get(),
                operation_id: request.context.operation_id.get(),
            })
            .await
            .map_err(|error| transport_error(VfsOperation::ReadDirectory, error))?;
        response_error(response.error)?;
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
        let response = self
            .transport
            .open(VfsOpenRequestV2 {
                project_id: self.project_id,
                mount_id: self.mount(),
                path: self.request_path(path),
                access: proto::encode_file_access(options.access) as i32,
                create: proto::encode_create_disposition(options.create) as i32,
                expected_version: options
                    .expected_version
                    .map(|version| version.as_bytes().to_vec()),
                operation_id: options.context.operation_id.get(),
            })
            .await
            .map_err(|error| transport_error(VfsOperation::Open, error))?;
        response_error(response.error)?;
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
        let response = self
            .transport
            .create_directory(VfsCreateDirectoryRequestV2 {
                project_id: self.project_id,
                mount_id: self.mount(),
                path: self.request_path(path),
                parents: options.parents == CreateParents::Yes,
                operation_id: options.context.operation_id.get(),
            })
            .await
            .map_err(|error| transport_error(VfsOperation::CreateDirectory, error))?;
        response_error(response.error)
    }

    async fn remove(
        &self,
        path: &ProviderPath,
        options: RemoveOptions,
    ) -> VfsResult<RemoveOutcome> {
        let response = self
            .transport
            .remove(VfsRemoveRequestV2 {
                project_id: self.project_id,
                mount_id: self.mount(),
                path: self.request_path(path),
                kind: proto::encode_remove_kind(options.kind) as i32,
                expected_version: options
                    .expected_version
                    .map(|version| version.as_bytes().to_vec()),
                operation_id: options.context.operation_id.get(),
            })
            .await
            .map_err(|error| transport_error(VfsOperation::Remove, error))?;
        response_error(response.error)?;
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
        let response = self
            .transport
            .rename(VfsRenameRequestV2 {
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
            })
            .await
            .map_err(|error| transport_error(VfsOperation::Rename, error))?;
        response_error(response.error)
    }

    async fn copy(
        &self,
        source: &ProviderPath,
        target: &ProviderPath,
        options: CopyOptions,
    ) -> VfsResult<()> {
        let response = self
            .transport
            .copy(VfsCopyRequestV2 {
                project_id: self.project_id,
                mount_id: self.mount(),
                source: self.request_path(source),
                target: self.request_path(target),
                collision: proto::encode_collision_policy(options.collision) as i32,
                expected_version: options
                    .expected_version
                    .map(|version| version.as_bytes().to_vec()),
                operation_id: options.context.operation_id.get(),
            })
            .await
            .map_err(|error| transport_error(VfsOperation::Copy, error))?;
        response_error(response.error)
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
        Ok(Box::pin(stream.map(|response| {
            let response = response.map_err(|error| transport_error(VfsOperation::Watch, error))?;
            response_error(response.error)?;
            response
                .batch
                .as_ref()
                .ok_or_else(|| missing_response_field(VfsOperation::Watch, "event batch"))?
                .to_event_batch()
                .map_err(|error| wire_error(VfsOperation::Watch, error))
        })))
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
        self.transport.release_handle(VfsCloseHandleRequestV2 {
            project_id: self.project_id,
            mount_id: Some(MountIdV2::from_mount_id(self.mount_id)),
            handle_id: self.handle_id,
            operation_id: 0,
        });
    }
}

#[async_trait]
impl VfsFile for RemoteVfsFile {
    async fn len(&self, context: OperationContext) -> VfsResult<u64> {
        context
            .cancellation
            .check(VfsOperation::Read, &self.provider_id)?;
        let response = self
            .transport
            .file_length(VfsFileLengthRequestV2 {
                project_id: self.project_id,
                mount_id: Some(MountIdV2::from_mount_id(self.mount_id)),
                handle_id: self.handle_id,
                operation_id: context.operation_id.get(),
            })
            .await
            .map_err(|error| transport_error(VfsOperation::Read, error))?;
        response_error(response.error)?;
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
        let response = self
            .transport
            .read_at(VfsReadAtRequestV2 {
                project_id: self.project_id,
                mount_id: Some(MountIdV2::from_mount_id(self.mount_id)),
                handle_id: self.handle_id,
                offset,
                length,
                operation_id: context.operation_id.get(),
            })
            .await
            .map_err(|error| transport_error(VfsOperation::Read, error))?;
        response_error(response.error)?;
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
        let response = self
            .transport
            .write_at(VfsWriteAtRequestV2 {
                project_id: self.project_id,
                mount_id: Some(MountIdV2::from_mount_id(self.mount_id)),
                handle_id: self.handle_id,
                offset,
                data: buffer.to_vec(),
                expected_version: options
                    .expected_version
                    .map(|version| version.as_bytes().to_vec()),
                operation_id: options.context.operation_id.get(),
            })
            .await
            .map_err(|error| transport_error(VfsOperation::Write, error))?;
        response_error(response.error)?;
        Ok(response.written as usize)
    }

    async fn set_len(&self, len: u64, options: WriteAtOptions) -> VfsResult<()> {
        let response = self
            .transport
            .set_length(VfsSetLengthRequestV2 {
                project_id: self.project_id,
                mount_id: Some(MountIdV2::from_mount_id(self.mount_id)),
                handle_id: self.handle_id,
                length: len,
                expected_version: options
                    .expected_version
                    .map(|version| version.as_bytes().to_vec()),
                operation_id: options.context.operation_id.get(),
            })
            .await
            .map_err(|error| transport_error(VfsOperation::SetLength, error))?;
        response_error(response.error)
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
        let response = self
            .transport
            .handle_operation(VfsHandleOperationRequestV2 {
                project_id: self.project_id,
                mount_id: Some(MountIdV2::from_mount_id(self.mount_id)),
                handle_id: self.handle_id,
                operation_id: context.operation_id.get(),
                kind: kind as i32,
            })
            .await
            .map_err(|error| transport_error(operation, error))?;
        response_error(response.error)
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

fn directory_error(error: VfsError) -> VfsReadDirectoryResponseV2 {
    VfsReadDirectoryResponseV2 {
        entries: Vec::new(),
        next_cursor: None,
        error: Some(VfsErrorV2::from_vfs_error(&error)),
    }
}

fn response_error(error: Option<VfsErrorV2>) -> VfsResult<()> {
    match error {
        Some(error) => Err(error
            .to_vfs_error()
            .map_err(|error| wire_error(VfsOperation::Stat, error))?),
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
    use futures::executor::block_on;
    use vfs::{MemoryProvider, test_support::run_provider_conformance};

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
}
