use crate::{
    Atomicity, CollisionPolicy, CopyOptions, CreateDirOptions, CreateDisposition, CreateParents,
    DirPageRequest, EntryKind, EntryMetadata, ExactComponent, FileAccess, MountId, OpenOptions,
    OperationContext, ProviderId, ProviderPath, RemoveKind, RemoveOptions, RenameOptions,
    StatOptions, SymbolicLinkMode, VfsError, VfsErrorCode, VfsOperation, VfsPath, VfsProvider,
    VfsResult, VfsVersion, WriteAtOptions,
};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeSet, VecDeque},
    error::Error,
    fmt,
    num::NonZeroU64,
    sync::Arc,
};

const DEFAULT_TRANSFER_CHUNK_SIZE: u64 = 256 * 1024;
const STAGING_PREFIX: &str = ".zzz-vfs-transfer";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VfsTransferKind {
    Copy,
    Move,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VfsTransferPhase {
    InspectSource,
    PrepareTarget,
    CopyData,
    VerifyTarget,
    CommitTarget,
    DeleteSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VfsTransferVerification {
    ProviderNative,
    LengthVersionAndSha256,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VfsTransferProgress {
    pub phase: VfsTransferPhase,
    pub source: VfsPath,
    pub target: VfsPath,
    pub bytes_copied: u64,
    pub total_bytes: u64,
    pub entries_copied: usize,
    pub total_entries: usize,
}

pub type VfsTransferProgressCallback = Arc<dyn Fn(VfsTransferProgress) + Send + Sync>;

#[derive(Clone)]
pub struct VfsTransferOptions {
    pub collision: CollisionPolicy,
    pub expected_source_version: Option<VfsVersion>,
    pub chunk_size: NonZeroU64,
    pub context: OperationContext,
    pub progress: Option<VfsTransferProgressCallback>,
}

impl Default for VfsTransferOptions {
    fn default() -> Self {
        Self {
            collision: CollisionPolicy::Fail,
            expected_source_version: None,
            chunk_size: NonZeroU64::new(DEFAULT_TRANSFER_CHUNK_SIZE).unwrap_or(NonZeroU64::MIN),
            context: OperationContext::default(),
            progress: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VfsTransferOutcome {
    pub kind: VfsTransferKind,
    pub source: VfsPath,
    pub target: VfsPath,
    pub used_native_fast_path: bool,
    pub verification: Option<VfsTransferVerification>,
    pub bytes_copied: u64,
    pub entries_copied: usize,
    pub target_verified: bool,
    pub target_committed: bool,
    pub source_removed: bool,
    pub materialized_paths: Vec<VfsPath>,
    pub removed_source_paths: Vec<VfsPath>,
    pub staging_path: Option<VfsPath>,
    pub staging_owned: bool,
    pub staging_cleanup_completed: bool,
}

impl VfsTransferOutcome {
    fn new(kind: VfsTransferKind, source: VfsPath, target: VfsPath) -> Self {
        Self {
            kind,
            source,
            target,
            used_native_fast_path: false,
            verification: None,
            bytes_copied: 0,
            entries_copied: 0,
            target_verified: false,
            target_committed: false,
            source_removed: false,
            materialized_paths: Vec::new(),
            removed_source_paths: Vec::new(),
            staging_path: None,
            staging_owned: false,
            staging_cleanup_completed: false,
        }
    }
}

pub struct VfsTransferError {
    error: Box<VfsError>,
    phase: VfsTransferPhase,
    outcome: Box<VfsTransferOutcome>,
    cleanup_error: Option<Box<VfsError>>,
}

impl VfsTransferError {
    fn new(error: VfsError, phase: VfsTransferPhase, outcome: VfsTransferOutcome) -> Self {
        Self {
            error: Box::new(error),
            phase,
            outcome: Box::new(outcome),
            cleanup_error: None,
        }
    }

    pub fn error(&self) -> &VfsError {
        self.error.as_ref()
    }

    pub fn phase(&self) -> VfsTransferPhase {
        self.phase
    }

    pub fn outcome(&self) -> &VfsTransferOutcome {
        self.outcome.as_ref()
    }

    pub fn cleanup_error(&self) -> Option<&VfsError> {
        self.cleanup_error.as_deref()
    }
}

impl fmt::Debug for VfsTransferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VfsTransferError")
            .field("error", &self.error)
            .field("phase", &self.phase)
            .field("outcome", &self.outcome)
            .field("cleanup_error", &self.cleanup_error)
            .finish()
    }
}

impl fmt::Display for VfsTransferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "VFS transfer failed during {:?}: {}",
            self.phase, self.error
        )?;
        if let Some(cleanup_error) = &self.cleanup_error {
            write!(formatter, "; staging cleanup also failed: {cleanup_error}")?;
        }
        Ok(())
    }
}

impl Error for VfsTransferError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.error.as_ref())
    }
}

#[derive(Clone)]
struct SourceEntry {
    source_path: ProviderPath,
    relative_path: ProviderPath,
    metadata: EntryMetadata,
}

struct SourceTree {
    entries: Vec<SourceEntry>,
    total_bytes: u64,
}

struct CrossCopyContext<'a> {
    source_provider: &'a dyn VfsProvider,
    target_provider: &'a dyn VfsProvider,
    source: &'a VfsPath,
    target: &'a VfsPath,
    source_tree: &'a SourceTree,
    options: &'a VfsTransferOptions,
}

struct TransferRequest<'a> {
    kind: VfsTransferKind,
    source: &'a VfsPath,
    target: &'a VfsPath,
    options: VfsTransferOptions,
    outcome: VfsTransferOutcome,
}

impl CrossCopyContext<'_> {
    fn report_progress(&self, phase: VfsTransferPhase, outcome: &VfsTransferOutcome) {
        report_progress(
            self.options,
            VfsTransferProgress {
                phase,
                source: self.source.clone(),
                target: self.target.clone(),
                bytes_copied: outcome.bytes_copied,
                total_bytes: self.source_tree.total_bytes,
                entries_copied: outcome.entries_copied,
                total_entries: self.source_tree.entries.len(),
            },
        );
    }
}

impl crate::VfsManager {
    pub async fn copy_path(
        &self,
        source: &VfsPath,
        target: &VfsPath,
        options: VfsTransferOptions,
    ) -> Result<VfsTransferOutcome, VfsTransferError> {
        self.transfer_path(VfsTransferKind::Copy, source, target, options)
            .await
    }

    pub async fn move_path(
        &self,
        source: &VfsPath,
        target: &VfsPath,
        options: VfsTransferOptions,
    ) -> Result<VfsTransferOutcome, VfsTransferError> {
        self.transfer_path(VfsTransferKind::Move, source, target, options)
            .await
    }

    async fn transfer_path(
        &self,
        kind: VfsTransferKind,
        source: &VfsPath,
        target: &VfsPath,
        options: VfsTransferOptions,
    ) -> Result<VfsTransferOutcome, VfsTransferError> {
        let outcome = VfsTransferOutcome::new(kind, source.clone(), target.clone());
        let operation = match kind {
            VfsTransferKind::Copy => VfsOperation::Copy,
            VfsTransferKind::Move => VfsOperation::Rename,
        };
        options
            .context
            .cancellation
            .check(operation, &ProviderId::new("vfs-transfer"))
            .map_err(|error| {
                VfsTransferError::new(error, VfsTransferPhase::InspectSource, outcome.clone())
            })?;
        let source_snapshot = self.snapshot(source.mount_id()).map_err(|error| {
            VfsTransferError::new(error, VfsTransferPhase::InspectSource, outcome.clone())
        })?;
        let target_snapshot = self.snapshot(target.mount_id()).map_err(|error| {
            VfsTransferError::new(error, VfsTransferPhase::PrepareTarget, outcome.clone())
        })?;
        let source_provider = source_snapshot.provider().clone();
        let target_provider = target_snapshot.provider().clone();
        let request = TransferRequest {
            kind,
            source,
            target,
            options,
            outcome,
        };
        if source.mount_id() == target.mount_id() || Arc::ptr_eq(&source_provider, &target_provider)
        {
            return transfer_same_provider(source_provider.as_ref(), request).await;
        }
        transfer_cross_provider(source_provider, target_provider, request).await
    }
}

async fn transfer_same_provider(
    provider: &dyn VfsProvider,
    request: TransferRequest<'_>,
) -> Result<VfsTransferOutcome, VfsTransferError> {
    let TransferRequest {
        kind,
        source,
        target,
        options,
        mut outcome,
    } = request;
    let operation = match kind {
        VfsTransferKind::Copy => VfsOperation::Copy,
        VfsTransferKind::Move => VfsOperation::Rename,
    };
    options
        .context
        .cancellation
        .check(operation, &provider.descriptor().id)
        .map_err(|error| {
            VfsTransferError::new(error, VfsTransferPhase::CommitTarget, outcome.clone())
        })?;
    let metadata = provider
        .stat(
            source.provider_path(),
            StatOptions {
                symbolic_link_mode: SymbolicLinkMode::DoNotFollow,
                context: options.context.clone(),
            },
        )
        .await
        .map_err(|error| {
            VfsTransferError::new(error, VfsTransferPhase::InspectSource, outcome.clone())
        })?;
    check_expected_source_version(
        provider,
        source.provider_path(),
        &metadata,
        options.expected_source_version.as_ref(),
        operation,
    )
    .map_err(|error| {
        VfsTransferError::new(error, VfsTransferPhase::InspectSource, outcome.clone())
    })?;
    report_progress(
        &options,
        VfsTransferProgress {
            phase: VfsTransferPhase::CommitTarget,
            source: source.clone(),
            target: target.clone(),
            bytes_copied: 0,
            total_bytes: metadata.size,
            entries_copied: 0,
            total_entries: 1,
        },
    );
    let result = match kind {
        VfsTransferKind::Copy => {
            provider
                .copy(
                    source.provider_path(),
                    target.provider_path(),
                    CopyOptions {
                        collision: options.collision,
                        expected_version: options.expected_source_version.clone(),
                        context: options.context.clone(),
                    },
                )
                .await
        }
        VfsTransferKind::Move => {
            provider
                .rename(
                    source.provider_path(),
                    target.provider_path(),
                    RenameOptions {
                        collision: options.collision,
                        required_atomicity: Atomicity::BestEffort,
                        expected_version: options.expected_source_version.clone(),
                        context: options.context.clone(),
                    },
                )
                .await
        }
    };
    result.map_err(|error| {
        VfsTransferError::new(error, VfsTransferPhase::CommitTarget, outcome.clone())
    })?;
    outcome.used_native_fast_path = true;
    outcome.verification = Some(VfsTransferVerification::ProviderNative);
    outcome.bytes_copied = metadata.size;
    outcome.entries_copied = 1;
    outcome.target_verified = true;
    outcome.target_committed = true;
    outcome.source_removed = kind == VfsTransferKind::Move;
    outcome.materialized_paths.push(target.clone());
    if kind == VfsTransferKind::Move {
        outcome.removed_source_paths.push(source.clone());
    }
    report_progress(
        &options,
        VfsTransferProgress {
            phase: VfsTransferPhase::CommitTarget,
            source: source.clone(),
            target: target.clone(),
            bytes_copied: outcome.bytes_copied,
            total_bytes: outcome.bytes_copied,
            entries_copied: 1,
            total_entries: 1,
        },
    );
    Ok(outcome)
}

async fn transfer_cross_provider(
    source_provider: Arc<dyn VfsProvider>,
    target_provider: Arc<dyn VfsProvider>,
    request: TransferRequest<'_>,
) -> Result<VfsTransferOutcome, VfsTransferError> {
    let TransferRequest {
        kind,
        source,
        target,
        options,
        mut outcome,
    } = request;
    if source.provider_path().encoding() != target.provider_path().encoding() {
        let error = transfer_error(
            VfsErrorCode::Unsupported,
            VfsOperation::Copy,
            source_provider.descriptor().id.clone(),
            source.provider_path(),
            "cross-provider transfer requires identical exact path encodings",
        );
        return Err(VfsTransferError::new(
            error,
            VfsTransferPhase::PrepareTarget,
            outcome,
        ));
    }
    if target.provider_path().is_root() {
        let error = transfer_error(
            VfsErrorCode::InvalidPath,
            VfsOperation::Copy,
            target_provider.descriptor().id.clone(),
            target.provider_path(),
            "cross-provider transfer target cannot be a mount root",
        );
        return Err(VfsTransferError::new(
            error,
            VfsTransferPhase::PrepareTarget,
            outcome,
        ));
    }
    let source_tree = scan_source_tree(source_provider.as_ref(), source.provider_path(), &options)
        .await
        .map_err(|error| {
            VfsTransferError::new(error, VfsTransferPhase::InspectSource, outcome.clone())
        })?;
    let final_materialized_paths = source_tree
        .entries
        .iter()
        .map(|entry| {
            target
                .provider_path()
                .join(&entry.relative_path)
                .map(|path| VfsPath::new(target.mount_id(), path))
                .map_err(|error| {
                    transfer_error(
                        VfsErrorCode::InvalidPath,
                        VfsOperation::Copy,
                        target_provider.descriptor().id.clone(),
                        target.provider_path(),
                        &error.to_string(),
                    )
                })
        })
        .collect::<VfsResult<Vec<_>>>()
        .map_err(|error| {
            VfsTransferError::new(error, VfsTransferPhase::PrepareTarget, outcome.clone())
        })?;
    let Some(root_entry) = source_tree.entries.first() else {
        let error = transfer_error(
            VfsErrorCode::Internal,
            VfsOperation::Copy,
            source_provider.descriptor().id.clone(),
            source.provider_path(),
            "source scan returned no root entry",
        );
        return Err(VfsTransferError::new(
            error,
            VfsTransferPhase::InspectSource,
            outcome,
        ));
    };
    check_expected_source_version(
        source_provider.as_ref(),
        source.provider_path(),
        &root_entry.metadata,
        options.expected_source_version.as_ref(),
        VfsOperation::Copy,
    )
    .map_err(|error| {
        VfsTransferError::new(error, VfsTransferPhase::InspectSource, outcome.clone())
    })?;
    preflight_target(
        target_provider.as_ref(),
        target.provider_path(),
        options.collision,
        options.context.clone(),
    )
    .await
    .map_err(|error| {
        VfsTransferError::new(error, VfsTransferPhase::PrepareTarget, outcome.clone())
    })?;
    let staging_path = staging_path(target.provider_path(), options.context.operation_id.get())
        .map_err(|error| {
            VfsTransferError::new(error, VfsTransferPhase::PrepareTarget, outcome.clone())
        })?;
    outcome.staging_path = Some(VfsPath::new(target.mount_id(), staging_path.clone()));
    report_progress(
        &options,
        VfsTransferProgress {
            phase: VfsTransferPhase::CopyData,
            source: source.clone(),
            target: target.clone(),
            bytes_copied: 0,
            total_bytes: source_tree.total_bytes,
            entries_copied: 0,
            total_entries: source_tree.entries.len(),
        },
    );

    let copy_context = CrossCopyContext {
        source_provider: source_provider.as_ref(),
        target_provider: target_provider.as_ref(),
        source,
        target,
        source_tree: &source_tree,
        options: &options,
    };
    let copy_result = copy_source_tree(&copy_context, &staging_path, &mut outcome).await;
    if let Err(error) = copy_result {
        return Err(cleanup_staging_after_failure(
            target_provider.as_ref(),
            error,
            VfsTransferPhase::CopyData,
            outcome,
        )
        .await);
    }

    report_progress(
        &options,
        VfsTransferProgress {
            phase: VfsTransferPhase::VerifyTarget,
            source: source.clone(),
            target: target.clone(),
            bytes_copied: outcome.bytes_copied,
            total_bytes: source_tree.total_bytes,
            entries_copied: outcome.entries_copied,
            total_entries: source_tree.entries.len(),
        },
    );
    if let Err(error) = verify_source_tree(
        source_provider.as_ref(),
        &source_tree,
        options.context.clone(),
    )
    .await
    {
        return Err(cleanup_staging_after_failure(
            target_provider.as_ref(),
            error,
            VfsTransferPhase::VerifyTarget,
            outcome,
        )
        .await);
    }
    outcome.target_verified = true;
    outcome.verification = Some(VfsTransferVerification::LengthVersionAndSha256);

    report_progress(
        &options,
        VfsTransferProgress {
            phase: VfsTransferPhase::CommitTarget,
            source: source.clone(),
            target: target.clone(),
            bytes_copied: outcome.bytes_copied,
            total_bytes: source_tree.total_bytes,
            entries_copied: outcome.entries_copied,
            total_entries: source_tree.entries.len(),
        },
    );
    let staging_metadata = target_provider
        .stat(
            &staging_path,
            StatOptions {
                symbolic_link_mode: SymbolicLinkMode::DoNotFollow,
                context: options.context.clone(),
            },
        )
        .await;
    let staging_metadata = match staging_metadata {
        Ok(metadata) => metadata,
        Err(error) => {
            return Err(cleanup_staging_after_failure(
                target_provider.as_ref(),
                error,
                VfsTransferPhase::CommitTarget,
                outcome,
            )
            .await);
        }
    };
    let staging_version = version_for_metadata(&staging_metadata);
    let commit_result = target_provider
        .rename(
            &staging_path,
            target.provider_path(),
            RenameOptions {
                collision: options.collision,
                required_atomicity: Atomicity::AtomicWithinDirectory,
                expected_version: Some(staging_version),
                context: options.context.clone(),
            },
        )
        .await;
    if let Err(error) = commit_result {
        return Err(cleanup_staging_after_failure(
            target_provider.as_ref(),
            error,
            VfsTransferPhase::CommitTarget,
            outcome,
        )
        .await);
    }
    outcome.target_committed = true;
    outcome.staging_path = None;
    outcome.staging_owned = false;
    outcome.materialized_paths = final_materialized_paths;

    if kind == VfsTransferKind::Move {
        report_progress(
            &options,
            VfsTransferProgress {
                phase: VfsTransferPhase::DeleteSource,
                source: source.clone(),
                target: target.clone(),
                bytes_copied: outcome.bytes_copied,
                total_bytes: source_tree.total_bytes,
                entries_copied: outcome.entries_copied,
                total_entries: source_tree.entries.len(),
            },
        );
        if let Err(error) = delete_source_tree(
            source_provider.as_ref(),
            source.mount_id(),
            &source_tree,
            options.context.clone(),
            &mut outcome,
        )
        .await
        {
            return Err(VfsTransferError::new(
                error,
                VfsTransferPhase::DeleteSource,
                outcome,
            ));
        }
        outcome.source_removed = true;
    }
    report_progress(
        &options,
        VfsTransferProgress {
            phase: if kind == VfsTransferKind::Move {
                VfsTransferPhase::DeleteSource
            } else {
                VfsTransferPhase::CommitTarget
            },
            source: source.clone(),
            target: target.clone(),
            bytes_copied: outcome.bytes_copied,
            total_bytes: source_tree.total_bytes,
            entries_copied: outcome.entries_copied,
            total_entries: source_tree.entries.len(),
        },
    );
    Ok(outcome)
}

async fn scan_source_tree(
    provider: &dyn VfsProvider,
    root: &ProviderPath,
    options: &VfsTransferOptions,
) -> VfsResult<SourceTree> {
    let root_metadata = provider
        .stat(
            root,
            StatOptions {
                symbolic_link_mode: SymbolicLinkMode::DoNotFollow,
                context: options.context.clone(),
            },
        )
        .await?;
    validate_transfer_kind(provider, root, &root_metadata)?;
    let mut pending = VecDeque::from([(root.clone(), root_metadata)]);
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    let mut total_bytes = 0_u64;
    while let Some((path, metadata)) = pending.pop_front() {
        options
            .context
            .cancellation
            .check(VfsOperation::Copy, &provider.descriptor().id)?;
        if !seen.insert(path.clone()) {
            return Err(transfer_error(
                VfsErrorCode::NameCollision,
                VfsOperation::Copy,
                provider.descriptor().id.clone(),
                &path,
                "source provider returned a duplicate path",
            ));
        }
        validate_transfer_kind(provider, &path, &metadata)?;
        let relative_path = path.strip_prefix(root).map_err(|error| {
            transfer_error(
                VfsErrorCode::CorruptData,
                VfsOperation::Copy,
                provider.descriptor().id.clone(),
                &path,
                &error.to_string(),
            )
        })?;
        if metadata.kind == EntryKind::File {
            total_bytes = total_bytes.checked_add(metadata.size).ok_or_else(|| {
                transfer_error(
                    VfsErrorCode::TooLarge,
                    VfsOperation::Copy,
                    provider.descriptor().id.clone(),
                    &path,
                    "source tree byte count overflowed",
                )
            })?;
        }
        entries.push(SourceEntry {
            source_path: path.clone(),
            relative_path,
            metadata: metadata.clone(),
        });
        if metadata.kind != EntryKind::Directory {
            continue;
        }
        let mut cursor = None;
        let mut seen_cursors = BTreeSet::new();
        loop {
            let page = provider
                .read_dir(
                    &path,
                    DirPageRequest {
                        cursor: cursor.clone(),
                        limit: provider.capabilities().limits.maximum_page_size,
                        context: options.context.clone(),
                    },
                )
                .await?;
            for entry in page.entries {
                if entry.path.parent().as_ref() != Some(&path) {
                    return Err(transfer_error(
                        VfsErrorCode::CorruptData,
                        VfsOperation::ReadDirectory,
                        provider.descriptor().id.clone(),
                        &entry.path,
                        "source provider returned a non-child directory entry",
                    ));
                }
                pending.push_back((entry.path, entry.metadata));
            }
            let Some(next_cursor) = page.next_cursor else {
                break;
            };
            if !seen_cursors.insert(next_cursor.as_bytes().to_vec()) {
                return Err(transfer_error(
                    VfsErrorCode::CorruptData,
                    VfsOperation::ReadDirectory,
                    provider.descriptor().id.clone(),
                    &path,
                    "source provider repeated a directory cursor",
                ));
            }
            cursor = Some(next_cursor);
        }
    }
    entries.sort_by(|left, right| {
        left.relative_path
            .component_count()
            .cmp(&right.relative_path.component_count())
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    Ok(SourceTree {
        entries,
        total_bytes,
    })
}

async fn preflight_target(
    provider: &dyn VfsProvider,
    target: &ProviderPath,
    collision: CollisionPolicy,
    context: OperationContext,
) -> VfsResult<()> {
    let Some(parent) = target.parent() else {
        return Err(transfer_error(
            VfsErrorCode::InvalidPath,
            VfsOperation::Copy,
            provider.descriptor().id.clone(),
            target,
            "transfer target must have a parent directory",
        ));
    };
    let parent_metadata = provider
        .stat(
            &parent,
            StatOptions {
                symbolic_link_mode: SymbolicLinkMode::Follow,
                context: context.clone(),
            },
        )
        .await?;
    if parent_metadata.kind != EntryKind::Directory {
        return Err(transfer_error(
            VfsErrorCode::NotDirectory,
            VfsOperation::Copy,
            provider.descriptor().id.clone(),
            &parent,
            "transfer target parent is not a directory",
        ));
    }
    let existing = provider
        .stat(
            target,
            StatOptions {
                symbolic_link_mode: SymbolicLinkMode::DoNotFollow,
                context,
            },
        )
        .await;
    match existing {
        Ok(_) if collision == CollisionPolicy::Fail => Err(transfer_error(
            VfsErrorCode::AlreadyExists,
            VfsOperation::Copy,
            provider.descriptor().id.clone(),
            target,
            "transfer target already exists",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.code() == VfsErrorCode::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn staging_path(target: &ProviderPath, operation_id: u64) -> VfsResult<ProviderPath> {
    let Some(parent) = target.parent() else {
        return Err(transfer_error(
            VfsErrorCode::InvalidPath,
            VfsOperation::Copy,
            ProviderId::new("vfs-transfer"),
            target,
            "transfer target must have a parent directory",
        ));
    };
    let component = ExactComponent::new(
        target.encoding(),
        format!("{STAGING_PREFIX}-{operation_id}").into_bytes(),
    )
    .map_err(|error| {
        transfer_error(
            VfsErrorCode::InvalidPath,
            VfsOperation::Copy,
            ProviderId::new("vfs-transfer"),
            target,
            &error.to_string(),
        )
    })?;
    parent.join_component(component).map_err(|error| {
        transfer_error(
            VfsErrorCode::InvalidPath,
            VfsOperation::Copy,
            ProviderId::new("vfs-transfer"),
            target,
            &error.to_string(),
        )
    })
}

async fn copy_source_tree(
    transfer: &CrossCopyContext<'_>,
    staging_root: &ProviderPath,
    outcome: &mut VfsTransferOutcome,
) -> VfsResult<()> {
    for entry in &transfer.source_tree.entries {
        transfer.options.context.cancellation.check(
            VfsOperation::Copy,
            &transfer.source_provider.descriptor().id,
        )?;
        let staging_path = staging_root.join(&entry.relative_path).map_err(|error| {
            transfer_error(
                VfsErrorCode::InvalidPath,
                VfsOperation::Copy,
                transfer.target_provider.descriptor().id.clone(),
                staging_root,
                &error.to_string(),
            )
        })?;
        match entry.metadata.kind {
            EntryKind::Directory => {
                transfer
                    .target_provider
                    .create_dir(
                        &staging_path,
                        CreateDirOptions {
                            parents: CreateParents::No,
                            context: transfer.options.context.clone(),
                        },
                    )
                    .await?;
                if entry.relative_path.is_root() {
                    outcome.staging_owned = true;
                }
            }
            EntryKind::File => {
                copy_file(transfer, entry, &staging_path, outcome).await?;
            }
            EntryKind::SymbolicLink | EntryKind::Special => {
                return Err(transfer_error(
                    VfsErrorCode::Unsupported,
                    VfsOperation::Copy,
                    transfer.source_provider.descriptor().id.clone(),
                    &entry.source_path,
                    "cross-provider transfer does not copy links or special entries",
                ));
            }
        }
        outcome.entries_copied = outcome.entries_copied.checked_add(1).ok_or_else(|| {
            transfer_error(
                VfsErrorCode::TooLarge,
                VfsOperation::Copy,
                transfer.target_provider.descriptor().id.clone(),
                &staging_path,
                "transfer entry count overflowed",
            )
        })?;
        outcome
            .materialized_paths
            .push(VfsPath::new(transfer.target.mount_id(), staging_path));
        transfer.report_progress(VfsTransferPhase::CopyData, outcome);
    }
    Ok(())
}

async fn copy_file(
    transfer: &CrossCopyContext<'_>,
    entry: &SourceEntry,
    staging_path: &ProviderPath,
    outcome: &mut VfsTransferOutcome,
) -> VfsResult<()> {
    let source_file = transfer
        .source_provider
        .open(
            &entry.source_path,
            OpenOptions {
                access: FileAccess::Read,
                create: CreateDisposition::OpenExisting,
                expected_version: Some(entry.metadata.content_version.clone()),
                context: transfer.options.context.clone(),
            },
        )
        .await?;
    let target_file = transfer
        .target_provider
        .open(
            staging_path,
            OpenOptions {
                access: FileAccess::Write,
                create: CreateDisposition::CreateNew,
                expected_version: None,
                context: transfer.options.context.clone(),
            },
        )
        .await?;
    if entry.relative_path.is_root() {
        outcome.staging_owned = true;
    }
    let chunk_size = transfer_chunk_size(
        transfer.source_provider,
        transfer.target_provider,
        transfer.options,
    )?;
    let mut buffer = vec![0; chunk_size];
    let mut source_digest = Sha256::new();
    let mut offset = 0_u64;
    while offset < entry.metadata.size {
        transfer.options.context.cancellation.check(
            VfsOperation::Copy,
            &transfer.source_provider.descriptor().id,
        )?;
        let remaining = entry.metadata.size.saturating_sub(offset);
        let requested = usize::try_from(remaining.min(chunk_size as u64)).map_err(|error| {
            transfer_error(
                VfsErrorCode::TooLarge,
                VfsOperation::Copy,
                transfer.source_provider.descriptor().id.clone(),
                &entry.source_path,
                &error.to_string(),
            )
        })?;
        let read = source_file
            .read_at(
                offset,
                &mut buffer[..requested],
                transfer.options.context.clone(),
            )
            .await?;
        if read == 0 {
            return Err(transfer_error(
                VfsErrorCode::StaleVersion,
                VfsOperation::Copy,
                transfer.source_provider.descriptor().id.clone(),
                &entry.source_path,
                "source file ended before its captured length",
            ));
        }
        source_digest.update(&buffer[..read]);
        write_all_at(
            transfer.target_provider,
            target_file.as_ref(),
            staging_path,
            offset,
            &buffer[..read],
            transfer.options.context.clone(),
        )
        .await?;
        let read_u64 = u64::try_from(read).map_err(|error| {
            transfer_error(
                VfsErrorCode::TooLarge,
                VfsOperation::Copy,
                transfer.source_provider.descriptor().id.clone(),
                &entry.source_path,
                &error.to_string(),
            )
        })?;
        offset = offset.checked_add(read_u64).ok_or_else(|| {
            transfer_error(
                VfsErrorCode::TooLarge,
                VfsOperation::Copy,
                transfer.source_provider.descriptor().id.clone(),
                &entry.source_path,
                "source offset overflowed",
            )
        })?;
        outcome.bytes_copied = outcome.bytes_copied.checked_add(read_u64).ok_or_else(|| {
            transfer_error(
                VfsErrorCode::TooLarge,
                VfsOperation::Copy,
                transfer.source_provider.descriptor().id.clone(),
                &entry.source_path,
                "transfer byte count overflowed",
            )
        })?;
        transfer.report_progress(VfsTransferPhase::CopyData, outcome);
    }
    let mut extra = [0_u8; 1];
    if source_file
        .read_at(offset, &mut extra, transfer.options.context.clone())
        .await?
        != 0
    {
        return Err(transfer_error(
            VfsErrorCode::StaleVersion,
            VfsOperation::Copy,
            transfer.source_provider.descriptor().id.clone(),
            &entry.source_path,
            "source file grew beyond its captured length",
        ));
    }
    target_file.flush(transfer.options.context.clone()).await?;
    target_file.sync(transfer.options.context.clone()).await?;
    let source_digest: [u8; 32] = source_digest.finalize().into();
    let target_digest = digest_file(
        transfer.target_provider,
        staging_path,
        entry.metadata.size,
        chunk_size,
        transfer.options.context.clone(),
    )
    .await?;
    if target_digest != source_digest {
        return Err(transfer_error(
            VfsErrorCode::CorruptData,
            VfsOperation::Copy,
            transfer.target_provider.descriptor().id.clone(),
            staging_path,
            "target digest does not match the source digest",
        ));
    }
    let current_source = transfer
        .source_provider
        .stat(
            &entry.source_path,
            StatOptions {
                symbolic_link_mode: SymbolicLinkMode::DoNotFollow,
                context: transfer.options.context.clone(),
            },
        )
        .await?;
    if current_source.content_version != entry.metadata.content_version
        || current_source.size != entry.metadata.size
    {
        return Err(transfer_error(
            VfsErrorCode::StaleVersion,
            VfsOperation::Copy,
            transfer.source_provider.descriptor().id.clone(),
            &entry.source_path,
            "source version changed during transfer",
        ));
    }
    Ok(())
}

async fn write_all_at(
    provider: &dyn VfsProvider,
    file: &dyn crate::VfsFile,
    path: &ProviderPath,
    offset: u64,
    bytes: &[u8],
    context: OperationContext,
) -> VfsResult<()> {
    let mut written = 0_usize;
    while written < bytes.len() {
        context
            .cancellation
            .check(VfsOperation::Write, &provider.descriptor().id)?;
        let write_offset = offset
            .checked_add(u64::try_from(written).map_err(|error| {
                transfer_error(
                    VfsErrorCode::TooLarge,
                    VfsOperation::Write,
                    provider.descriptor().id.clone(),
                    path,
                    &error.to_string(),
                )
            })?)
            .ok_or_else(|| {
                transfer_error(
                    VfsErrorCode::TooLarge,
                    VfsOperation::Write,
                    provider.descriptor().id.clone(),
                    path,
                    "target offset overflowed",
                )
            })?;
        let count = file
            .write_at(
                write_offset,
                &bytes[written..],
                WriteAtOptions {
                    expected_version: None,
                    context: context.clone(),
                },
            )
            .await?;
        if count == 0 {
            return Err(transfer_error(
                VfsErrorCode::CorruptData,
                VfsOperation::Write,
                provider.descriptor().id.clone(),
                path,
                "target provider reported a zero-length write",
            ));
        }
        written = written.checked_add(count).ok_or_else(|| {
            transfer_error(
                VfsErrorCode::TooLarge,
                VfsOperation::Write,
                provider.descriptor().id.clone(),
                path,
                "target write count overflowed",
            )
        })?;
        if written > bytes.len() {
            return Err(transfer_error(
                VfsErrorCode::CorruptData,
                VfsOperation::Write,
                provider.descriptor().id.clone(),
                path,
                "target provider wrote more bytes than requested",
            ));
        }
    }
    Ok(())
}

async fn digest_file(
    provider: &dyn VfsProvider,
    path: &ProviderPath,
    expected_size: u64,
    chunk_size: usize,
    context: OperationContext,
) -> VfsResult<[u8; 32]> {
    let metadata = provider
        .stat(
            path,
            StatOptions {
                symbolic_link_mode: SymbolicLinkMode::DoNotFollow,
                context: context.clone(),
            },
        )
        .await?;
    if metadata.kind != EntryKind::File || metadata.size != expected_size {
        return Err(transfer_error(
            VfsErrorCode::CorruptData,
            VfsOperation::Read,
            provider.descriptor().id.clone(),
            path,
            "target length or kind does not match the source",
        ));
    }
    let file = provider
        .open(
            path,
            OpenOptions {
                access: FileAccess::Read,
                create: CreateDisposition::OpenExisting,
                expected_version: Some(metadata.content_version.clone()),
                context: context.clone(),
            },
        )
        .await?;
    let mut buffer = vec![0; chunk_size];
    let mut digest = Sha256::new();
    let mut offset = 0_u64;
    while offset < expected_size {
        let remaining = expected_size.saturating_sub(offset);
        let requested = usize::try_from(remaining.min(chunk_size as u64)).map_err(|error| {
            transfer_error(
                VfsErrorCode::TooLarge,
                VfsOperation::Read,
                provider.descriptor().id.clone(),
                path,
                &error.to_string(),
            )
        })?;
        let read = file
            .read_at(offset, &mut buffer[..requested], context.clone())
            .await?;
        if read == 0 {
            return Err(transfer_error(
                VfsErrorCode::CorruptData,
                VfsOperation::Read,
                provider.descriptor().id.clone(),
                path,
                "target file ended before its expected length",
            ));
        }
        digest.update(&buffer[..read]);
        offset = offset
            .checked_add(u64::try_from(read).map_err(|error| {
                transfer_error(
                    VfsErrorCode::TooLarge,
                    VfsOperation::Read,
                    provider.descriptor().id.clone(),
                    path,
                    &error.to_string(),
                )
            })?)
            .ok_or_else(|| {
                transfer_error(
                    VfsErrorCode::TooLarge,
                    VfsOperation::Read,
                    provider.descriptor().id.clone(),
                    path,
                    "target read offset overflowed",
                )
            })?;
    }
    let current = provider
        .stat(
            path,
            StatOptions {
                symbolic_link_mode: SymbolicLinkMode::DoNotFollow,
                context,
            },
        )
        .await?;
    if current.content_version != metadata.content_version || current.size != expected_size {
        return Err(transfer_error(
            VfsErrorCode::StaleVersion,
            VfsOperation::Read,
            provider.descriptor().id.clone(),
            path,
            "target version changed during digest verification",
        ));
    }
    Ok(digest.finalize().into())
}

async fn verify_source_tree(
    provider: &dyn VfsProvider,
    source_tree: &SourceTree,
    context: OperationContext,
) -> VfsResult<()> {
    for entry in &source_tree.entries {
        context
            .cancellation
            .check(VfsOperation::Copy, &provider.descriptor().id)?;
        let current = provider
            .stat(
                &entry.source_path,
                StatOptions {
                    symbolic_link_mode: SymbolicLinkMode::DoNotFollow,
                    context: context.clone(),
                },
            )
            .await?;
        let version_matches = match entry.metadata.kind {
            EntryKind::File => {
                current.content_version == entry.metadata.content_version
                    && current.size == entry.metadata.size
            }
            EntryKind::Directory => current.structure_version == entry.metadata.structure_version,
            EntryKind::SymbolicLink | EntryKind::Special => false,
        };
        if current.kind != entry.metadata.kind || !version_matches {
            return Err(transfer_error(
                VfsErrorCode::StaleVersion,
                VfsOperation::Copy,
                provider.descriptor().id.clone(),
                &entry.source_path,
                "source tree changed during transfer",
            ));
        }
    }
    Ok(())
}

async fn delete_source_tree(
    provider: &dyn VfsProvider,
    mount_id: MountId,
    source_tree: &SourceTree,
    context: OperationContext,
    outcome: &mut VfsTransferOutcome,
) -> VfsResult<()> {
    for entry in source_tree
        .entries
        .iter()
        .filter(|entry| entry.metadata.kind == EntryKind::File)
    {
        provider
            .remove(
                &entry.source_path,
                RemoveOptions {
                    kind: RemoveKind::File,
                    expected_version: Some(entry.metadata.content_version.clone()),
                    context: context.clone(),
                },
            )
            .await?;
        outcome
            .removed_source_paths
            .push(VfsPath::new(mount_id, entry.source_path.clone()));
    }
    let mut directories = source_tree
        .entries
        .iter()
        .filter(|entry| entry.metadata.kind == EntryKind::Directory)
        .collect::<Vec<_>>();
    directories.sort_by(|left, right| {
        right
            .relative_path
            .component_count()
            .cmp(&left.relative_path.component_count())
            .then_with(|| right.relative_path.cmp(&left.relative_path))
    });
    for entry in directories {
        provider
            .remove(
                &entry.source_path,
                RemoveOptions {
                    kind: RemoveKind::EmptyDirectory,
                    expected_version: None,
                    context: context.clone(),
                },
            )
            .await?;
        outcome
            .removed_source_paths
            .push(VfsPath::new(mount_id, entry.source_path.clone()));
    }
    Ok(())
}

async fn cleanup_staging_after_failure(
    provider: &dyn VfsProvider,
    error: VfsError,
    phase: VfsTransferPhase,
    mut outcome: VfsTransferOutcome,
) -> VfsTransferError {
    let Some(staging) = outcome.staging_path.as_ref() else {
        return VfsTransferError::new(error, phase, outcome);
    };
    if !outcome.staging_owned {
        return VfsTransferError::new(error, phase, outcome);
    }
    let cleanup = provider
        .remove(
            staging.provider_path(),
            RemoveOptions {
                kind: RemoveKind::Recursive,
                expected_version: None,
                context: OperationContext::default(),
            },
        )
        .await;
    let mut transfer_error = VfsTransferError::new(error, phase, outcome.clone());
    match cleanup {
        Ok(_) => {
            outcome.staging_cleanup_completed = true;
            outcome.staging_owned = false;
            outcome.staging_path = None;
            outcome.materialized_paths.clear();
            transfer_error.outcome = Box::new(outcome);
        }
        Err(cleanup_error) if cleanup_error.code() == VfsErrorCode::NotFound => {
            outcome.staging_cleanup_completed = true;
            outcome.staging_owned = false;
            outcome.staging_path = None;
            outcome.materialized_paths.clear();
            transfer_error.outcome = Box::new(outcome);
        }
        Err(cleanup_error) => {
            transfer_error.cleanup_error = Some(Box::new(cleanup_error));
        }
    }
    transfer_error
}

fn transfer_chunk_size(
    source_provider: &dyn VfsProvider,
    target_provider: &dyn VfsProvider,
    options: &VfsTransferOptions,
) -> VfsResult<usize> {
    let maximum = options
        .chunk_size
        .get()
        .min(
            source_provider
                .capabilities()
                .limits
                .maximum_range_size
                .get(),
        )
        .min(
            target_provider
                .capabilities()
                .limits
                .maximum_range_size
                .get(),
        );
    usize::try_from(maximum).map_err(|error| {
        VfsError::new(
            VfsErrorCode::TooLarge,
            VfsOperation::Copy,
            source_provider.descriptor().id.clone(),
        )
        .with_source(error)
    })
}

fn validate_transfer_kind(
    provider: &dyn VfsProvider,
    path: &ProviderPath,
    metadata: &EntryMetadata,
) -> VfsResult<()> {
    match metadata.kind {
        EntryKind::File | EntryKind::Directory => Ok(()),
        EntryKind::SymbolicLink | EntryKind::Special => Err(transfer_error(
            VfsErrorCode::Unsupported,
            VfsOperation::Copy,
            provider.descriptor().id.clone(),
            path,
            "cross-provider transfer does not copy links or special entries",
        )),
    }
}

fn check_expected_source_version(
    provider: &dyn VfsProvider,
    path: &ProviderPath,
    metadata: &EntryMetadata,
    expected: Option<&VfsVersion>,
    operation: VfsOperation,
) -> VfsResult<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    if expected == &version_for_metadata(metadata) {
        Ok(())
    } else {
        Err(transfer_error(
            VfsErrorCode::StaleVersion,
            operation,
            provider.descriptor().id.clone(),
            path,
            "source version does not match the expected version",
        ))
    }
}

fn version_for_metadata(metadata: &EntryMetadata) -> VfsVersion {
    if metadata.kind == EntryKind::Directory {
        metadata.structure_version.clone()
    } else {
        metadata.content_version.clone()
    }
}

fn report_progress(options: &VfsTransferOptions, progress_update: VfsTransferProgress) {
    if let Some(progress) = &options.progress {
        progress(progress_update);
    }
}

fn transfer_error(
    code: VfsErrorCode,
    operation: VfsOperation,
    provider: ProviderId,
    path: &ProviderPath,
    detail: &str,
) -> VfsError {
    VfsError::new(code, operation, provider)
        .with_path(path.clone())
        .with_detail(detail.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MemoryProvider, PathEncoding, ReadOnlyProvider, SnapshotBudgets, VfsManager,
        load_provider_bytes,
    };
    use futures::executor::block_on;
    use parking_lot::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    fn path(encoding: PathEncoding, components: &[&[u8]]) -> ProviderPath {
        ProviderPath::from_byte_components(encoding, components.iter().copied())
            .unwrap_or_else(|error| panic!("fixture path must be valid: {error}"))
    }

    async fn create_directory(provider: &dyn VfsProvider, directory: &ProviderPath) {
        provider
            .create_dir(
                directory,
                CreateDirOptions {
                    parents: CreateParents::Yes,
                    context: OperationContext::default(),
                },
            )
            .await
            .unwrap_or_else(|error| panic!("fixture directory creation failed: {error}"));
    }

    async fn create_file(provider: &dyn VfsProvider, file_path: &ProviderPath, contents: &[u8]) {
        let file = provider
            .open(
                file_path,
                OpenOptions {
                    access: FileAccess::Write,
                    create: CreateDisposition::CreateNew,
                    expected_version: None,
                    context: OperationContext::default(),
                },
            )
            .await
            .unwrap_or_else(|error| panic!("fixture file creation failed: {error}"));
        write_all_at(
            provider,
            file.as_ref(),
            file_path,
            0,
            contents,
            OperationContext::default(),
        )
        .await
        .unwrap_or_else(|error| panic!("fixture file write failed: {error}"));
    }

    async fn replace_file_bytes(
        provider: &dyn VfsProvider,
        file_path: &ProviderPath,
        offset: u64,
        contents: &[u8],
    ) -> VfsResult<()> {
        let file = provider
            .open(
                file_path,
                OpenOptions {
                    access: FileAccess::Write,
                    create: CreateDisposition::OpenExisting,
                    expected_version: None,
                    context: OperationContext::default(),
                },
            )
            .await?;
        write_all_at(
            provider,
            file.as_ref(),
            file_path,
            offset,
            contents,
            OperationContext::default(),
        )
        .await
    }

    async fn read_file(provider: &dyn VfsProvider, file_path: &ProviderPath) -> Vec<u8> {
        load_provider_bytes(provider, file_path, OperationContext::default())
            .await
            .unwrap_or_else(|error| panic!("fixture file read failed: {error}"))
            .bytes
    }

    fn mount(
        manager: &VfsManager,
        provider: Arc<dyn VfsProvider>,
    ) -> (MountId, Arc<dyn VfsProvider>) {
        let snapshot = manager
            .mount(provider.clone(), SnapshotBudgets::default())
            .unwrap_or_else(|error| panic!("fixture mount failed: {error}"));
        (snapshot.registry().mount_id(), provider)
    }

    #[test]
    fn same_provider_copy_and_move_use_native_fast_paths() {
        let result = block_on(async {
            let manager = VfsManager::default();
            let provider = Arc::new(MemoryProvider::new(
                "same-provider-transfer",
                PathEncoding::PortableUtf8,
            ));
            let (mount_id, provider) = mount(&manager, provider);
            let source_path = path(PathEncoding::PortableUtf8, &[b"source.bin"]);
            let copied_path = path(PathEncoding::PortableUtf8, &[b"copied.bin"]);
            let moved_path = path(PathEncoding::PortableUtf8, &[b"moved.bin"]);
            create_file(provider.as_ref(), &source_path, b"native-fast-path").await;
            let copy = manager
                .copy_path(
                    &VfsPath::new(mount_id, source_path.clone()),
                    &VfsPath::new(mount_id, copied_path.clone()),
                    VfsTransferOptions::default(),
                )
                .await?;
            assert!(copy.used_native_fast_path);
            assert_eq!(
                copy.verification,
                Some(VfsTransferVerification::ProviderNative)
            );
            assert_eq!(
                read_file(provider.as_ref(), &copied_path).await,
                b"native-fast-path"
            );
            let moved = manager
                .move_path(
                    &VfsPath::new(mount_id, copied_path.clone()),
                    &VfsPath::new(mount_id, moved_path.clone()),
                    VfsTransferOptions::default(),
                )
                .await?;
            assert!(moved.used_native_fast_path);
            assert!(moved.source_removed);
            assert_eq!(
                read_file(provider.as_ref(), &moved_path).await,
                b"native-fast-path"
            );
            assert_eq!(
                provider
                    .stat(&copied_path, StatOptions::default())
                    .await
                    .as_ref()
                    .err()
                    .map(VfsError::code),
                Some(VfsErrorCode::NotFound)
            );
            Ok::<(), VfsTransferError>(())
        });
        assert!(result.is_ok(), "same-provider transfer failed: {result:?}");
    }

    #[test]
    fn cross_provider_directory_copy_streams_verifies_and_reports_progress() {
        let result = block_on(async {
            let manager = VfsManager::default();
            let source_provider = Arc::new(MemoryProvider::new(
                "cross-copy-source",
                PathEncoding::PortableUtf8,
            ));
            let target_provider = Arc::new(MemoryProvider::new(
                "cross-copy-target",
                PathEncoding::PortableUtf8,
            ));
            let (source_mount, source_provider) = mount(&manager, source_provider);
            let (target_mount, target_provider) = mount(&manager, target_provider);
            let source_root = path(PathEncoding::PortableUtf8, &[b"tree"]);
            create_directory(source_provider.as_ref(), &source_root).await;
            create_directory(
                source_provider.as_ref(),
                &path(PathEncoding::PortableUtf8, &[b"tree", b"nested"]),
            )
            .await;
            create_file(
                source_provider.as_ref(),
                &path(PathEncoding::PortableUtf8, &[b"tree", b"alpha.bin"]),
                b"abcdefgh",
            )
            .await;
            create_file(
                source_provider.as_ref(),
                &path(
                    PathEncoding::PortableUtf8,
                    &[b"tree", b"nested", b"beta.bin"],
                ),
                b"ijklmnop",
            )
            .await;
            let progress = Arc::new(Mutex::new(Vec::new()));
            let progress_callback = {
                let progress = progress.clone();
                Arc::new(move |update: VfsTransferProgress| progress.lock().push(update))
                    as VfsTransferProgressCallback
            };
            let target_root = path(PathEncoding::PortableUtf8, &[b"copied-tree"]);
            let outcome = manager
                .copy_path(
                    &VfsPath::new(source_mount, source_root),
                    &VfsPath::new(target_mount, target_root.clone()),
                    VfsTransferOptions {
                        chunk_size: NonZeroU64::new(4).unwrap_or(NonZeroU64::MIN),
                        progress: Some(progress_callback),
                        ..VfsTransferOptions::default()
                    },
                )
                .await?;
            assert!(!outcome.used_native_fast_path);
            assert_eq!(
                outcome.verification,
                Some(VfsTransferVerification::LengthVersionAndSha256)
            );
            assert!(outcome.target_verified);
            assert!(outcome.target_committed);
            assert_eq!(outcome.bytes_copied, 16);
            assert_eq!(outcome.entries_copied, 4);
            assert_eq!(
                read_file(
                    target_provider.as_ref(),
                    &path(PathEncoding::PortableUtf8, &[b"copied-tree", b"alpha.bin"],),
                )
                .await,
                b"abcdefgh"
            );
            assert_eq!(
                read_file(
                    target_provider.as_ref(),
                    &path(
                        PathEncoding::PortableUtf8,
                        &[b"copied-tree", b"nested", b"beta.bin"],
                    ),
                )
                .await,
                b"ijklmnop"
            );
            let progress = progress.lock();
            assert!(
                progress
                    .iter()
                    .any(|update| update.phase == VfsTransferPhase::CopyData)
            );
            assert_eq!(progress.last().map(|update| update.bytes_copied), Some(16));
            Ok::<(), VfsTransferError>(())
        });
        assert!(result.is_ok(), "cross-provider copy failed: {result:?}");
    }

    #[test]
    fn cancelled_cross_provider_copy_cleans_staging_and_preserves_source() {
        let result = block_on(async {
            let manager = VfsManager::default();
            let source_provider = Arc::new(MemoryProvider::new(
                "cancelled-copy-source",
                PathEncoding::PortableUtf8,
            ));
            let target_provider = Arc::new(MemoryProvider::new(
                "cancelled-copy-target",
                PathEncoding::PortableUtf8,
            ));
            let (source_mount, source_provider) = mount(&manager, source_provider);
            let (target_mount, target_provider) = mount(&manager, target_provider);
            let source_path = path(PathEncoding::PortableUtf8, &[b"source.bin"]);
            let target_path = path(PathEncoding::PortableUtf8, &[b"target.bin"]);
            create_file(source_provider.as_ref(), &source_path, b"abcdefgh").await;
            let cancellation = crate::CancellationToken::default();
            let progress_callback = {
                let cancellation = cancellation.clone();
                Arc::new(move |update: VfsTransferProgress| {
                    if update.bytes_copied >= 4 {
                        cancellation.cancel();
                    }
                }) as VfsTransferProgressCallback
            };
            let transfer = manager
                .copy_path(
                    &VfsPath::new(source_mount, source_path.clone()),
                    &VfsPath::new(target_mount, target_path.clone()),
                    VfsTransferOptions {
                        chunk_size: NonZeroU64::new(4).unwrap_or(NonZeroU64::MIN),
                        context: OperationContext {
                            operation_id: crate::OperationId::new(7001),
                            cancellation,
                        },
                        progress: Some(progress_callback),
                        ..VfsTransferOptions::default()
                    },
                )
                .await;
            let error = transfer
                .err()
                .ok_or_else(|| String::from("cancelled transfer unexpectedly succeeded"))?;
            assert_eq!(error.phase(), VfsTransferPhase::CopyData);
            assert_eq!(error.error().code(), VfsErrorCode::Cancelled);
            assert!(error.outcome().staging_cleanup_completed);
            assert!(!error.outcome().target_committed);
            assert_eq!(
                read_file(source_provider.as_ref(), &source_path).await,
                b"abcdefgh"
            );
            assert_eq!(
                target_provider
                    .stat(&target_path, StatOptions::default())
                    .await
                    .as_ref()
                    .err()
                    .map(VfsError::code),
                Some(VfsErrorCode::NotFound)
            );
            Ok::<(), String>(())
        });
        assert!(result.is_ok(), "cancelled copy contract failed: {result:?}");
    }

    #[test]
    fn disconnected_cross_provider_copy_cleans_staging_and_reports_disconnect() {
        let result = block_on(async {
            let manager = VfsManager::default();
            let source_memory =
                MemoryProvider::new("disconnected-copy-memory", PathEncoding::PortableUtf8);
            let source_path = path(PathEncoding::PortableUtf8, &[b"source.bin"]);
            create_file(&source_memory, &source_path, b"abcdefgh").await;
            let source_provider: Arc<dyn VfsProvider> = Arc::new(
                TransferFaultProvider::disconnecting_reads(source_memory.clone(), 1),
            );
            let target_provider = Arc::new(MemoryProvider::new(
                "disconnected-copy-target",
                PathEncoding::PortableUtf8,
            ));
            let (source_mount, _) = mount(&manager, source_provider);
            let (target_mount, target_provider) = mount(&manager, target_provider);
            let target_path = path(PathEncoding::PortableUtf8, &[b"target.bin"]);
            let transfer = manager
                .copy_path(
                    &VfsPath::new(source_mount, source_path.clone()),
                    &VfsPath::new(target_mount, target_path.clone()),
                    VfsTransferOptions {
                        chunk_size: NonZeroU64::new(4).unwrap_or(NonZeroU64::MIN),
                        ..VfsTransferOptions::default()
                    },
                )
                .await;
            let error = transfer
                .err()
                .ok_or_else(|| String::from("disconnected transfer unexpectedly succeeded"))?;
            assert_eq!(error.phase(), VfsTransferPhase::CopyData);
            assert_eq!(error.error().code(), VfsErrorCode::Disconnected);
            assert!(error.outcome().staging_cleanup_completed);
            assert_eq!(read_file(&source_memory, &source_path).await, b"abcdefgh");
            assert_eq!(
                target_provider
                    .stat(&target_path, StatOptions::default())
                    .await
                    .as_ref()
                    .err()
                    .map(VfsError::code),
                Some(VfsErrorCode::NotFound)
            );
            Ok::<(), String>(())
        });
        assert!(result.is_ok(), "disconnect recovery failed: {result:?}");
    }

    #[test]
    fn source_change_during_copy_returns_stale_and_rolls_back_staging() {
        let result = block_on(async {
            let manager = VfsManager::default();
            let source_provider = Arc::new(MemoryProvider::new(
                "stale-copy-source",
                PathEncoding::PortableUtf8,
            ));
            let target_provider = Arc::new(MemoryProvider::new(
                "stale-copy-target",
                PathEncoding::PortableUtf8,
            ));
            let (source_mount, source_provider_trait) = mount(&manager, source_provider.clone());
            let (target_mount, target_provider_trait) = mount(&manager, target_provider.clone());
            let source_path = path(PathEncoding::PortableUtf8, &[b"source.bin"]);
            let target_path = path(PathEncoding::PortableUtf8, &[b"target.bin"]);
            create_file(source_provider.as_ref(), &source_path, b"abcdefgh").await;
            let changed = Arc::new(AtomicBool::new(false));
            let progress_callback = {
                let changed = changed.clone();
                let source_provider = source_provider.clone();
                let source_path = source_path.clone();
                Arc::new(move |update: VfsTransferProgress| {
                    if update.bytes_copied < 4 || changed.swap(true, Ordering::SeqCst) {
                        return;
                    }
                    let source_provider = source_provider.clone();
                    let source_path = source_path.clone();
                    let thread = std::thread::spawn(move || {
                        block_on(replace_file_bytes(
                            source_provider.as_ref(),
                            &source_path,
                            4,
                            b"WXYZ",
                        ))
                    });
                    match thread.join() {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => panic!("source mutation failed: {error}"),
                        Err(_) => panic!("source mutation thread panicked"),
                    }
                }) as VfsTransferProgressCallback
            };
            let transfer = manager
                .copy_path(
                    &VfsPath::new(source_mount, source_path.clone()),
                    &VfsPath::new(target_mount, target_path.clone()),
                    VfsTransferOptions {
                        chunk_size: NonZeroU64::new(4).unwrap_or(NonZeroU64::MIN),
                        progress: Some(progress_callback),
                        ..VfsTransferOptions::default()
                    },
                )
                .await;
            let error = transfer
                .err()
                .ok_or_else(|| String::from("stale transfer unexpectedly succeeded"))?;
            assert_eq!(error.error().code(), VfsErrorCode::StaleVersion);
            assert!(error.outcome().staging_cleanup_completed);
            assert_eq!(
                read_file(source_provider_trait.as_ref(), &source_path).await,
                b"abcdWXYZ"
            );
            assert_eq!(
                target_provider_trait
                    .stat(&target_path, StatOptions::default())
                    .await
                    .as_ref()
                    .err()
                    .map(VfsError::code),
                Some(VfsErrorCode::NotFound)
            );
            Ok::<(), String>(())
        });
        assert!(result.is_ok(), "stale source contract failed: {result:?}");
    }

    #[test]
    fn cross_provider_move_reports_committed_target_when_source_delete_is_read_only() {
        let result = block_on(async {
            let manager = VfsManager::default();
            let source_memory = Arc::new(MemoryProvider::new(
                "readonly-move-memory",
                PathEncoding::PortableUtf8,
            ));
            let source_path = path(PathEncoding::PortableUtf8, &[b"source.bin"]);
            create_file(source_memory.as_ref(), &source_path, b"preserve-source").await;
            let source_provider: Arc<dyn VfsProvider> = Arc::new(ReadOnlyProvider::new(
                "readonly-move-source",
                source_memory.clone(),
            ));
            let target_provider = Arc::new(MemoryProvider::new(
                "readonly-move-target",
                PathEncoding::PortableUtf8,
            ));
            let (source_mount, _) = mount(&manager, source_provider);
            let (target_mount, target_provider) = mount(&manager, target_provider);
            let target_path = path(PathEncoding::PortableUtf8, &[b"target.bin"]);
            let transfer = manager
                .move_path(
                    &VfsPath::new(source_mount, source_path.clone()),
                    &VfsPath::new(target_mount, target_path.clone()),
                    VfsTransferOptions::default(),
                )
                .await;
            let error = transfer
                .err()
                .ok_or_else(|| String::from("read-only source move unexpectedly succeeded"))?;
            assert_eq!(error.phase(), VfsTransferPhase::DeleteSource);
            assert_eq!(error.error().code(), VfsErrorCode::ReadOnly);
            assert!(error.outcome().target_committed);
            assert!(error.outcome().target_verified);
            assert!(!error.outcome().source_removed);
            assert!(error.outcome().removed_source_paths.is_empty());
            assert_eq!(
                read_file(source_memory.as_ref(), &source_path).await,
                b"preserve-source"
            );
            assert_eq!(
                read_file(target_provider.as_ref(), &target_path).await,
                b"preserve-source"
            );
            Ok::<(), String>(())
        });
        assert!(result.is_ok(), "partial move outcome failed: {result:?}");
    }

    #[test]
    fn cross_provider_move_reports_exact_paths_after_partial_source_deletion() {
        let result = block_on(async {
            let manager = VfsManager::default();
            let source_memory =
                MemoryProvider::new("partial-delete-memory", PathEncoding::PortableUtf8);
            let source_root = path(PathEncoding::PortableUtf8, &[b"source"]);
            let first_file = path(PathEncoding::PortableUtf8, &[b"source", b"a.bin"]);
            let second_file = path(PathEncoding::PortableUtf8, &[b"source", b"b.bin"]);
            create_directory(&source_memory, &source_root).await;
            create_file(&source_memory, &first_file, b"first").await;
            create_file(&source_memory, &second_file, b"second").await;
            let source_provider: Arc<dyn VfsProvider> = Arc::new(
                TransferFaultProvider::failing_removes(source_memory.clone(), 1),
            );
            let target_provider = Arc::new(MemoryProvider::new(
                "partial-delete-target",
                PathEncoding::PortableUtf8,
            ));
            let (source_mount, _) = mount(&manager, source_provider);
            let (target_mount, target_provider) = mount(&manager, target_provider);
            let target_root = path(PathEncoding::PortableUtf8, &[b"target"]);
            let transfer = manager
                .move_path(
                    &VfsPath::new(source_mount, source_root.clone()),
                    &VfsPath::new(target_mount, target_root.clone()),
                    VfsTransferOptions::default(),
                )
                .await;
            let error = transfer
                .err()
                .ok_or_else(|| String::from("partial-delete move unexpectedly succeeded"))?;
            assert_eq!(error.phase(), VfsTransferPhase::DeleteSource);
            assert_eq!(error.error().code(), VfsErrorCode::PermissionDenied);
            assert!(error.outcome().target_committed);
            assert_eq!(
                error.outcome().removed_source_paths,
                vec![VfsPath::new(source_mount, first_file.clone())]
            );
            assert_eq!(
                source_memory
                    .stat(&first_file, StatOptions::default())
                    .await
                    .as_ref()
                    .err()
                    .map(VfsError::code),
                Some(VfsErrorCode::NotFound)
            );
            assert_eq!(read_file(&source_memory, &second_file).await, b"second");
            assert_eq!(
                read_file(
                    target_provider.as_ref(),
                    &path(PathEncoding::PortableUtf8, &[b"target", b"a.bin"]),
                )
                .await,
                b"first"
            );
            assert_eq!(
                read_file(
                    target_provider.as_ref(),
                    &path(PathEncoding::PortableUtf8, &[b"target", b"b.bin"]),
                )
                .await,
                b"second"
            );
            Ok::<(), String>(())
        });
        assert!(result.is_ok(), "partial-delete outcome failed: {result:?}");
    }

    #[test]
    fn cross_provider_transfer_rejects_incompatible_exact_path_encodings() {
        let result = block_on(async {
            let manager = VfsManager::default();
            let source_provider = Arc::new(MemoryProvider::new(
                "encoding-source",
                PathEncoding::PortableUtf8,
            ));
            let target_provider = Arc::new(MemoryProvider::new(
                "encoding-target",
                PathEncoding::UnixBytes,
            ));
            let (source_mount, source_provider) = mount(&manager, source_provider);
            let (target_mount, _) = mount(&manager, target_provider);
            let source_path = path(PathEncoding::PortableUtf8, &[b"source.bin"]);
            create_file(source_provider.as_ref(), &source_path, b"data").await;
            let transfer = manager
                .copy_path(
                    &VfsPath::new(source_mount, source_path),
                    &VfsPath::new(
                        target_mount,
                        path(PathEncoding::UnixBytes, &[b"target.bin"]),
                    ),
                    VfsTransferOptions::default(),
                )
                .await;
            let error = transfer
                .err()
                .ok_or_else(|| String::from("encoding mismatch unexpectedly succeeded"))?;
            assert_eq!(error.phase(), VfsTransferPhase::PrepareTarget);
            assert_eq!(error.error().code(), VfsErrorCode::Unsupported);
            Ok::<(), String>(())
        });
        assert!(result.is_ok(), "encoding rejection failed: {result:?}");
    }

    #[derive(Clone)]
    struct TransferFaultProvider {
        inner: MemoryProvider,
        disconnect_after_reads: Option<usize>,
        fail_after_removes: Option<usize>,
        read_count: Arc<AtomicUsize>,
        remove_count: Arc<AtomicUsize>,
    }

    impl TransferFaultProvider {
        fn disconnecting_reads(inner: MemoryProvider, successful_reads: usize) -> Self {
            Self {
                inner,
                disconnect_after_reads: Some(successful_reads),
                fail_after_removes: None,
                read_count: Arc::new(AtomicUsize::new(0)),
                remove_count: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn failing_removes(inner: MemoryProvider, successful_removes: usize) -> Self {
            Self {
                inner,
                disconnect_after_reads: None,
                fail_after_removes: Some(successful_removes),
                read_count: Arc::new(AtomicUsize::new(0)),
                remove_count: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    #[async_trait::async_trait]
    impl VfsProvider for TransferFaultProvider {
        fn descriptor(&self) -> &crate::ProviderDescriptor {
            self.inner.descriptor()
        }

        fn capabilities(&self) -> crate::ProviderCapabilities {
            self.inner.capabilities()
        }

        async fn stat(
            &self,
            requested_path: &ProviderPath,
            options: StatOptions,
        ) -> VfsResult<EntryMetadata> {
            self.inner.stat(requested_path, options).await
        }

        async fn read_dir(
            &self,
            requested_path: &ProviderPath,
            request: DirPageRequest,
        ) -> VfsResult<crate::DirPage> {
            self.inner.read_dir(requested_path, request).await
        }

        async fn open(
            &self,
            requested_path: &ProviderPath,
            options: OpenOptions,
        ) -> VfsResult<Arc<dyn crate::VfsFile>> {
            let inner = self.inner.open(requested_path, options).await?;
            Ok(Arc::new(TransferFaultFile {
                inner,
                provider_id: self.descriptor().id.clone(),
                path: requested_path.clone(),
                disconnect_after_reads: self.disconnect_after_reads,
                read_count: self.read_count.clone(),
            }))
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
        ) -> VfsResult<crate::RemoveOutcome> {
            if self.fail_after_removes.is_some_and(|successful_removes| {
                self.remove_count.fetch_add(1, Ordering::SeqCst) >= successful_removes
            }) {
                return Err(VfsError::new(
                    VfsErrorCode::PermissionDenied,
                    VfsOperation::Remove,
                    self.descriptor().id.clone(),
                )
                .with_path(requested_path.clone()));
            }
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
            request: crate::WatchRequest,
        ) -> VfsResult<futures::stream::BoxStream<'static, VfsResult<crate::EventBatch>>> {
            self.inner.watch(request).await
        }
    }

    struct TransferFaultFile {
        inner: Arc<dyn crate::VfsFile>,
        provider_id: ProviderId,
        path: ProviderPath,
        disconnect_after_reads: Option<usize>,
        read_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl crate::VfsFile for TransferFaultFile {
        async fn len(&self, context: OperationContext) -> VfsResult<u64> {
            self.inner.len(context).await
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
            if self.disconnect_after_reads.is_some_and(|successful_reads| {
                self.read_count.fetch_add(1, Ordering::SeqCst) >= successful_reads
            }) {
                return Err(VfsError::new(
                    VfsErrorCode::Disconnected,
                    VfsOperation::Read,
                    self.provider_id.clone(),
                )
                .with_path(self.path.clone()));
            }
            self.inner.read_at(offset, buffer, context).await
        }

        async fn write_at(
            &self,
            offset: u64,
            buffer: &[u8],
            options: WriteAtOptions,
        ) -> VfsResult<usize> {
            self.inner.write_at(offset, buffer, options).await
        }

        async fn set_len(&self, len: u64, options: WriteAtOptions) -> VfsResult<()> {
            self.inner.set_len(len, options).await
        }

        async fn flush(&self, context: OperationContext) -> VfsResult<()> {
            self.inner.flush(context).await
        }

        async fn sync(&self, context: OperationContext) -> VfsResult<()> {
            self.inner.sync(context).await
        }
    }
}
