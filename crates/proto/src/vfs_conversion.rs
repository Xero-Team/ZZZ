use crate::{
    ExactPathV2, MountIdV2, NativePathRootKindV2, NativePathRootV2, NativePathV2, PathEncodingV2,
    ProviderPathV2, ResourceIdV2, VfsAtomicityV2, VfsCaseSensitivityV2, VfsCollisionPolicyV2,
    VfsCreateDispositionV2, VfsDirectoryCapabilitiesV2, VfsDirectoryEntryV2, VfsEntryKindV2,
    VfsEntryMetadataV2, VfsEntryPermissionsV2, VfsErrorCodeV2, VfsErrorV2, VfsEventBatchV2,
    VfsEventV2, VfsFileAccessV2, VfsLinkCapabilitiesV2, VfsMutationCapabilitiesV2, VfsOperationV2,
    VfsPathComponentV2, VfsPathV2, VfsProviderCapabilitiesV2, VfsProviderDescriptorV2,
    VfsProviderLimitsV2, VfsReadCapabilitiesV2, VfsRemoveKindV2, VfsSupportLevelV2, VfsTimestampV2,
    VfsTrashCapabilitiesV2, VfsWatchCapabilitiesV2, VfsWatchDepthV2, VfsWriteCapabilitiesV2,
};
use std::{
    num::{NonZeroU32, NonZeroU64},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use vfs::{
    Atomicity, CaseSensitivity, CollisionPolicy, CreateDisposition, DirEntry,
    DirectoryCapabilities, DisplayComponent, EntryKind, EntryMetadata, EntryName, EntryPermissions,
    EventBatch, ExactComponent, FileAccess, LinkCapabilities, LookupKey, MountId,
    MutationCapabilities, NativePath, NativePathRoot, PathEncoding, ProviderCapabilities,
    ProviderDescriptor, ProviderFileKey, ProviderId, ProviderLimits, ProviderPath,
    ReadCapabilities, RemoveKind, ResourceId, SupportLevel, TrashCapabilities, VfsError,
    VfsErrorCode, VfsEvent, VfsEventKind, VfsOperation, VfsPath, VfsVersion, WatchCapabilities,
    WatchDepth, WindowsDrive, WindowsDriveForm, WindowsUncForm, WriteCapabilities,
};

#[derive(Debug, thiserror::Error)]
pub enum VfsWireError {
    #[error("unknown VFS path encoding {0}")]
    UnknownPathEncoding(i32),
    #[error("unknown native path root kind {0}")]
    UnknownNativeRootKind(i32),
    #[error("unknown VFS support level {0}")]
    UnknownSupportLevel(i32),
    #[error("unknown VFS atomicity {0}")]
    UnknownAtomicity(i32),
    #[error("unknown VFS case sensitivity {0}")]
    UnknownCaseSensitivity(i32),
    #[error("unknown VFS entry kind {0}")]
    UnknownEntryKind(i32),
    #[error("unknown VFS error code {0}")]
    UnknownErrorCode(i32),
    #[error("unknown VFS operation {0}")]
    UnknownOperation(i32),
    #[error("unknown VFS file access {0}")]
    UnknownFileAccess(i32),
    #[error("unknown VFS create disposition {0}")]
    UnknownCreateDisposition(i32),
    #[error("unknown VFS remove kind {0}")]
    UnknownRemoveKind(i32),
    #[error("unknown VFS collision policy {0}")]
    UnknownCollisionPolicy(i32),
    #[error("unknown VFS watch depth {0}")]
    UnknownWatchDepth(i32),
    #[error("missing {0}")]
    MissingField(&'static str),
    #[error("provider path carries a native root")]
    ProviderPathHasNativeRoot,
    #[error("Windows drive root must contain exactly one ASCII byte")]
    InvalidWindowsDrive,
    #[error("native root contains fields that do not match its kind")]
    InvalidNativeRoot,
    #[error("VFS provider limit {0} must be non-zero")]
    ZeroProviderLimit(&'static str),
    #[error("VFS timestamp is outside the supported SystemTime range")]
    InvalidTimestamp,
    #[error("VFS event has no kind")]
    MissingEventKind,
    #[error("invalid VFS protobuf payload")]
    InvalidProtobuf(#[from] crate::DecodeError),
    #[error(transparent)]
    InvalidPath(#[from] vfs::PathError),
}

impl ExactPathV2 {
    pub fn from_provider_path(path: &ProviderPath) -> Self {
        Self {
            encoding: encode_path_encoding(path.encoding()) as i32,
            root: Some(relative_root()),
            components: path
                .components()
                .map(|component| component.as_bytes().to_vec())
                .collect(),
        }
    }

    pub fn to_provider_path(&self) -> Result<ProviderPath, VfsWireError> {
        let encoding = decode_path_encoding(self.encoding)?;
        let root = self
            .root
            .as_ref()
            .ok_or(VfsWireError::MissingField("exact path root"))?;
        let root_kind = decode_root_kind(root.kind)?;
        if root_kind != NativePathRootKindV2::Relative || !root_has_no_payload_or_flags(root) {
            return Err(VfsWireError::ProviderPathHasNativeRoot);
        }
        ProviderPath::from_byte_components(encoding, &self.components).map_err(Into::into)
    }

    pub fn from_native_path(path: &NativePath) -> Self {
        Self {
            encoding: encode_path_encoding(path.provider_path().encoding()) as i32,
            root: Some(encode_native_root(path.root())),
            components: path
                .provider_path()
                .components()
                .map(|component| component.as_bytes().to_vec())
                .collect(),
        }
    }

    pub fn to_native_path(&self) -> Result<NativePath, VfsWireError> {
        let encoding = decode_path_encoding(self.encoding)?;
        let root = self
            .root
            .as_ref()
            .ok_or(VfsWireError::MissingField("exact path root"))?;
        let native_root = decode_native_root(root)?;
        let provider_path = ProviderPath::from_byte_components(encoding, &self.components)?;
        NativePath::new(native_root, provider_path).map_err(Into::into)
    }
}

impl MountIdV2 {
    pub fn from_mount_id(mount_id: MountId) -> Self {
        Self {
            value: mount_id.get(),
        }
    }

    pub fn to_mount_id(&self) -> MountId {
        MountId::new(self.value)
    }
}

impl ResourceIdV2 {
    pub fn from_resource_id(resource_id: ResourceId) -> Self {
        Self {
            mount_id: Some(MountIdV2::from_mount_id(resource_id.mount_id())),
            node_id: resource_id.node_id(),
            generation: resource_id.generation(),
        }
    }

    pub fn to_resource_id(&self) -> Result<ResourceId, VfsWireError> {
        let mount_id = self
            .mount_id
            .as_ref()
            .ok_or(VfsWireError::MissingField("resource mount id"))?
            .to_mount_id();
        Ok(ResourceId::new(mount_id, self.node_id, self.generation))
    }
}

impl ProviderPathV2 {
    pub fn from_provider_path(path: &ProviderPath) -> Self {
        Self {
            path: Some(ExactPathV2::from_provider_path(path)),
        }
    }

    pub fn to_provider_path(&self) -> Result<ProviderPath, VfsWireError> {
        self.path
            .as_ref()
            .ok_or(VfsWireError::MissingField("provider path"))?
            .to_provider_path()
    }
}

impl VfsPathV2 {
    pub fn from_vfs_path(path: &VfsPath) -> Self {
        Self {
            mount_id: Some(MountIdV2::from_mount_id(path.mount_id())),
            relative_path: Some(ExactPathV2::from_provider_path(path.provider_path())),
        }
    }

    pub fn to_vfs_path(&self) -> Result<VfsPath, VfsWireError> {
        let mount_id = self
            .mount_id
            .as_ref()
            .ok_or(VfsWireError::MissingField("VFS mount id"))?
            .to_mount_id();
        let path = self
            .relative_path
            .as_ref()
            .ok_or(VfsWireError::MissingField("VFS relative path"))?
            .to_provider_path()?;
        Ok(VfsPath::new(mount_id, path))
    }
}

impl NativePathV2 {
    pub fn from_native_path(path: &NativePath) -> Self {
        Self {
            path: Some(ExactPathV2::from_native_path(path)),
        }
    }

    pub fn to_native_path(&self) -> Result<NativePath, VfsWireError> {
        self.path
            .as_ref()
            .ok_or(VfsWireError::MissingField("native path"))?
            .to_native_path()
    }
}

impl VfsPathComponentV2 {
    pub fn from_entry_name(entry_name: &EntryName) -> Self {
        Self {
            encoding: encode_path_encoding(entry_name.exact().encoding()) as i32,
            exact: entry_name.exact().as_bytes().to_vec(),
            display: entry_name.display().as_str().to_owned(),
            lookup_key: entry_name.lookup_key().as_bytes().to_vec(),
        }
    }

    pub fn to_entry_name(&self) -> Result<EntryName, VfsWireError> {
        let encoding = decode_path_encoding(self.encoding)?;
        Ok(EntryName::new(
            ExactComponent::new(encoding, self.exact.clone())?,
            DisplayComponent::new(self.display.clone()),
            LookupKey::new(self.lookup_key.clone()),
        ))
    }
}

impl VfsProviderDescriptorV2 {
    pub fn from_provider_descriptor(descriptor: &ProviderDescriptor) -> Self {
        Self {
            id: descriptor.id.as_str().to_owned(),
            display_name: descriptor.display_name.to_string(),
            path_encoding: encode_path_encoding(descriptor.path_encoding) as i32,
            case_sensitivity: encode_case_sensitivity(descriptor.case_sensitivity) as i32,
        }
    }

    pub fn to_provider_descriptor(&self) -> Result<ProviderDescriptor, VfsWireError> {
        Ok(ProviderDescriptor {
            id: ProviderId::new(self.id.clone()),
            display_name: self.display_name.clone().into(),
            path_encoding: decode_path_encoding(self.path_encoding)?,
            case_sensitivity: decode_case_sensitivity(self.case_sensitivity)?,
        })
    }
}

impl VfsProviderCapabilitiesV2 {
    pub fn from_provider_capabilities(capabilities: &ProviderCapabilities) -> Self {
        Self {
            read: Some(VfsReadCapabilitiesV2 {
                whole_file: encode_support_level(capabilities.read.whole_file) as i32,
                stream: encode_support_level(capabilities.read.stream) as i32,
                positioned: encode_support_level(capabilities.read.positioned) as i32,
                atomic_snapshot: encode_support_level(capabilities.read.atomic_snapshot) as i32,
            }),
            write: Some(VfsWriteCapabilitiesV2 {
                positioned: encode_support_level(capabilities.write.positioned) as i32,
                append: encode_support_level(capabilities.write.append) as i32,
                truncate: encode_support_level(capabilities.write.truncate) as i32,
                set_len: encode_support_level(capabilities.write.set_len) as i32,
                flush: encode_support_level(capabilities.write.flush) as i32,
                sync: encode_support_level(capabilities.write.sync) as i32,
                conditional: encode_support_level(capabilities.write.conditional) as i32,
                maximum_atomicity: encode_atomicity(capabilities.write.maximum_atomicity) as i32,
            }),
            directories: Some(VfsDirectoryCapabilitiesV2 {
                paged: encode_support_level(capabilities.directories.paged) as i32,
                recursive_list: encode_support_level(capabilities.directories.recursive_list)
                    as i32,
                stat_many: encode_support_level(capabilities.directories.stat_many) as i32,
            }),
            mutations: Some(VfsMutationCapabilitiesV2 {
                create_directory: encode_support_level(capabilities.mutations.create_directory)
                    as i32,
                remove: encode_support_level(capabilities.mutations.remove) as i32,
                rename: encode_support_level(capabilities.mutations.rename) as i32,
                copy: encode_support_level(capabilities.mutations.copy) as i32,
                cross_provider_copy: encode_support_level(
                    capabilities.mutations.cross_provider_copy,
                ) as i32,
                idempotency: encode_support_level(capabilities.mutations.idempotency) as i32,
                maximum_rename_atomicity: encode_atomicity(
                    capabilities.mutations.maximum_rename_atomicity,
                ) as i32,
                maximum_delete_atomicity: encode_atomicity(
                    capabilities.mutations.maximum_delete_atomicity,
                ) as i32,
            }),
            watch: Some(VfsWatchCapabilitiesV2 {
                watch: encode_support_level(capabilities.watch.watch) as i32,
                recursive: encode_support_level(capabilities.watch.recursive) as i32,
                resumable_journal: encode_support_level(capabilities.watch.resumable_journal)
                    as i32,
            }),
            links: Some(VfsLinkCapabilitiesV2 {
                symbolic_links: encode_support_level(capabilities.links.symbolic_links) as i32,
                hard_links: encode_support_level(capabilities.links.hard_links) as i32,
                permissions: encode_support_level(capabilities.links.permissions) as i32,
                extended_attributes: encode_support_level(capabilities.links.extended_attributes)
                    as i32,
                native_path: encode_support_level(capabilities.links.native_path) as i32,
            }),
            trash: Some(VfsTrashCapabilitiesV2 {
                trash: encode_support_level(capabilities.trash.trash) as i32,
                restore: encode_support_level(capabilities.trash.restore) as i32,
            }),
            stable_file_key: encode_support_level(capabilities.stable_file_key) as i32,
            case_sensitivity: encode_case_sensitivity(capabilities.case_sensitivity) as i32,
            limits: Some(VfsProviderLimitsV2 {
                maximum_page_size: capabilities.limits.maximum_page_size.get(),
                maximum_stat_batch: capabilities.limits.maximum_stat_batch.get(),
                maximum_range_size: capabilities.limits.maximum_range_size.get(),
                maximum_request_size: capabilities.limits.maximum_request_size.get(),
                maximum_open_handles: capabilities.limits.maximum_open_handles.get(),
            }),
            extensions: Vec::new(),
        }
    }

    pub fn to_provider_capabilities(&self) -> Result<ProviderCapabilities, VfsWireError> {
        let read = self
            .read
            .as_ref()
            .ok_or(VfsWireError::MissingField("VFS read capabilities"))?;
        let write = self
            .write
            .as_ref()
            .ok_or(VfsWireError::MissingField("VFS write capabilities"))?;
        let directories = self
            .directories
            .as_ref()
            .ok_or(VfsWireError::MissingField("VFS directory capabilities"))?;
        let mutations = self
            .mutations
            .as_ref()
            .ok_or(VfsWireError::MissingField("VFS mutation capabilities"))?;
        let watch = self
            .watch
            .as_ref()
            .ok_or(VfsWireError::MissingField("VFS watch capabilities"))?;
        let links = self
            .links
            .as_ref()
            .ok_or(VfsWireError::MissingField("VFS link capabilities"))?;
        let trash = self
            .trash
            .as_ref()
            .ok_or(VfsWireError::MissingField("VFS trash capabilities"))?;
        let limits = self
            .limits
            .as_ref()
            .ok_or(VfsWireError::MissingField("VFS provider limits"))?;
        Ok(ProviderCapabilities {
            read: ReadCapabilities {
                whole_file: decode_support_level(read.whole_file)?,
                stream: decode_support_level(read.stream)?,
                positioned: decode_support_level(read.positioned)?,
                atomic_snapshot: decode_support_level(read.atomic_snapshot)?,
            },
            write: WriteCapabilities {
                positioned: decode_support_level(write.positioned)?,
                append: decode_support_level(write.append)?,
                truncate: decode_support_level(write.truncate)?,
                set_len: decode_support_level(write.set_len)?,
                flush: decode_support_level(write.flush)?,
                sync: decode_support_level(write.sync)?,
                conditional: decode_support_level(write.conditional)?,
                maximum_atomicity: decode_atomicity(write.maximum_atomicity)?,
            },
            directories: DirectoryCapabilities {
                paged: decode_support_level(directories.paged)?,
                recursive_list: decode_support_level(directories.recursive_list)?,
                stat_many: decode_support_level(directories.stat_many)?,
            },
            mutations: MutationCapabilities {
                create_directory: decode_support_level(mutations.create_directory)?,
                remove: decode_support_level(mutations.remove)?,
                rename: decode_support_level(mutations.rename)?,
                copy: decode_support_level(mutations.copy)?,
                cross_provider_copy: decode_support_level(mutations.cross_provider_copy)?,
                idempotency: decode_support_level(mutations.idempotency)?,
                maximum_rename_atomicity: decode_atomicity(mutations.maximum_rename_atomicity)?,
                maximum_delete_atomicity: decode_atomicity(mutations.maximum_delete_atomicity)?,
            },
            watch: WatchCapabilities {
                watch: decode_support_level(watch.watch)?,
                recursive: decode_support_level(watch.recursive)?,
                resumable_journal: decode_support_level(watch.resumable_journal)?,
            },
            links: LinkCapabilities {
                symbolic_links: decode_support_level(links.symbolic_links)?,
                hard_links: decode_support_level(links.hard_links)?,
                permissions: decode_support_level(links.permissions)?,
                extended_attributes: decode_support_level(links.extended_attributes)?,
                native_path: decode_support_level(links.native_path)?,
            },
            trash: TrashCapabilities {
                trash: decode_support_level(trash.trash)?,
                restore: decode_support_level(trash.restore)?,
            },
            stable_file_key: decode_support_level(self.stable_file_key)?,
            case_sensitivity: decode_case_sensitivity(self.case_sensitivity)?,
            limits: ProviderLimits {
                maximum_page_size: non_zero_u32(limits.maximum_page_size, "maximum page size")?,
                maximum_stat_batch: non_zero_u32(limits.maximum_stat_batch, "maximum stat batch")?,
                maximum_range_size: non_zero_u64(limits.maximum_range_size, "maximum range size")?,
                maximum_request_size: non_zero_u64(
                    limits.maximum_request_size,
                    "maximum request size",
                )?,
                maximum_open_handles: non_zero_u32(
                    limits.maximum_open_handles,
                    "maximum open handles",
                )?,
            },
        })
    }
}

impl VfsTimestampV2 {
    pub fn from_system_time(time: SystemTime) -> Result<Self, VfsWireError> {
        match time.duration_since(UNIX_EPOCH) {
            Ok(duration) => Ok(Self {
                seconds: i64::try_from(duration.as_secs())
                    .map_err(|_| VfsWireError::InvalidTimestamp)?,
                nanos: duration.subsec_nanos(),
            }),
            Err(error) => {
                let duration = error.duration();
                let seconds = i64::try_from(duration.as_secs())
                    .map_err(|_| VfsWireError::InvalidTimestamp)?;
                if duration.subsec_nanos() == 0 {
                    Ok(Self {
                        seconds: -seconds,
                        nanos: 0,
                    })
                } else {
                    Ok(Self {
                        seconds: seconds
                            .checked_add(1)
                            .and_then(|seconds| seconds.checked_neg())
                            .ok_or(VfsWireError::InvalidTimestamp)?,
                        nanos: 1_000_000_000 - duration.subsec_nanos(),
                    })
                }
            }
        }
    }

    pub fn to_system_time(&self) -> Result<SystemTime, VfsWireError> {
        if self.nanos >= 1_000_000_000 {
            return Err(VfsWireError::InvalidTimestamp);
        }
        if self.seconds >= 0 {
            let seconds =
                u64::try_from(self.seconds).map_err(|_| VfsWireError::InvalidTimestamp)?;
            UNIX_EPOCH
                .checked_add(Duration::new(seconds, self.nanos))
                .ok_or(VfsWireError::InvalidTimestamp)
        } else if self.nanos == 0 {
            UNIX_EPOCH
                .checked_sub(Duration::from_secs(self.seconds.unsigned_abs()))
                .ok_or(VfsWireError::InvalidTimestamp)
        } else {
            let seconds = self
                .seconds
                .checked_add(1)
                .ok_or(VfsWireError::InvalidTimestamp)?
                .unsigned_abs();
            UNIX_EPOCH
                .checked_sub(Duration::new(seconds, 1_000_000_000 - self.nanos))
                .ok_or(VfsWireError::InvalidTimestamp)
        }
    }
}

impl VfsEntryMetadataV2 {
    pub fn from_entry_metadata(metadata: &EntryMetadata) -> Result<Self, VfsWireError> {
        Ok(Self {
            name: metadata
                .name
                .as_ref()
                .map(VfsPathComponentV2::from_entry_name),
            kind: encode_entry_kind(metadata.kind) as i32,
            size: metadata.size,
            modified_at: metadata
                .modified_at
                .map(VfsTimestampV2::from_system_time)
                .transpose()?,
            created_at: metadata
                .created_at
                .map(VfsTimestampV2::from_system_time)
                .transpose()?,
            permissions: Some(VfsEntryPermissionsV2 {
                writable: metadata.permissions.writable,
                executable: metadata.permissions.executable,
                private: metadata.permissions.private,
                hidden: metadata.permissions.hidden,
            }),
            provider_file_key: metadata
                .provider_file_key
                .as_ref()
                .map(|file_key| file_key.as_bytes().to_vec()),
            content_version: metadata.content_version.as_bytes().to_vec(),
            structure_version: metadata.structure_version.as_bytes().to_vec(),
            symbolic_link_target: metadata
                .symbolic_link_target
                .as_ref()
                .map(ProviderPathV2::from_provider_path),
            symbolic_link_target_kind: metadata
                .symbolic_link_target_kind
                .map_or(VfsEntryKindV2::Unspecified as i32, |kind| {
                    encode_entry_kind(kind) as i32
                }),
            case_sensitivity: encode_case_sensitivity(metadata.case_sensitivity) as i32,
        })
    }

    pub fn to_entry_metadata(&self) -> Result<EntryMetadata, VfsWireError> {
        let permissions = self
            .permissions
            .as_ref()
            .ok_or(VfsWireError::MissingField("VFS entry permissions"))?;
        let symbolic_link_target_kind =
            match VfsEntryKindV2::try_from(self.symbolic_link_target_kind) {
                Ok(VfsEntryKindV2::Unspecified) => None,
                Ok(kind) => Some(decode_entry_kind(kind as i32)?),
                Err(_) => {
                    return Err(VfsWireError::UnknownEntryKind(
                        self.symbolic_link_target_kind,
                    ));
                }
            };
        Ok(EntryMetadata {
            name: self
                .name
                .as_ref()
                .map(VfsPathComponentV2::to_entry_name)
                .transpose()?,
            kind: decode_entry_kind(self.kind)?,
            size: self.size,
            modified_at: self
                .modified_at
                .as_ref()
                .map(VfsTimestampV2::to_system_time)
                .transpose()?,
            created_at: self
                .created_at
                .as_ref()
                .map(VfsTimestampV2::to_system_time)
                .transpose()?,
            permissions: EntryPermissions {
                writable: permissions.writable,
                executable: permissions.executable,
                private: permissions.private,
                hidden: permissions.hidden,
            },
            provider_file_key: self.provider_file_key.clone().map(ProviderFileKey::new),
            content_version: VfsVersion::new(self.content_version.clone()),
            structure_version: VfsVersion::new(self.structure_version.clone()),
            symbolic_link_target: self
                .symbolic_link_target
                .as_ref()
                .map(ProviderPathV2::to_provider_path)
                .transpose()?,
            symbolic_link_target_kind,
            case_sensitivity: decode_case_sensitivity(self.case_sensitivity)?,
        })
    }
}

impl VfsDirectoryEntryV2 {
    pub fn from_dir_entry(entry: &DirEntry) -> Result<Self, VfsWireError> {
        Ok(Self {
            path: Some(ProviderPathV2::from_provider_path(&entry.path)),
            metadata: Some(VfsEntryMetadataV2::from_entry_metadata(&entry.metadata)?),
        })
    }

    pub fn to_dir_entry(&self) -> Result<DirEntry, VfsWireError> {
        Ok(DirEntry {
            path: self
                .path
                .as_ref()
                .ok_or(VfsWireError::MissingField("VFS directory entry path"))?
                .to_provider_path()?,
            metadata: self
                .metadata
                .as_ref()
                .ok_or(VfsWireError::MissingField("VFS directory entry metadata"))?
                .to_entry_metadata()?,
        })
    }
}

impl VfsErrorV2 {
    pub fn from_vfs_error(error: &VfsError) -> Self {
        Self {
            code: encode_error_code(error.code()) as i32,
            operation: encode_operation(error.operation()) as i32,
            provider_id: error.provider().as_str().to_owned(),
            path: error.path().map(ProviderPathV2::from_provider_path),
            detail: error.detail().unwrap_or_default().to_owned(),
        }
    }

    pub fn to_vfs_error(&self) -> Result<VfsError, VfsWireError> {
        let mut error = VfsError::new(
            decode_error_code(self.code)?,
            decode_operation(self.operation)?,
            ProviderId::new(self.provider_id.clone()),
        );
        if let Some(path) = &self.path {
            error = error.with_path(path.to_provider_path()?);
        }
        if !self.detail.is_empty() {
            error = error.with_detail(self.detail.clone());
        }
        Ok(error)
    }
}

impl VfsEventV2 {
    pub fn from_vfs_event(event: &VfsEvent) -> Self {
        let kind = match &event.kind {
            VfsEventKind::Created => crate::vfs_event_v2::Kind::Created(true),
            VfsEventKind::Modified => crate::vfs_event_v2::Kind::Modified(true),
            VfsEventKind::Removed => crate::vfs_event_v2::Kind::Removed(true),
            VfsEventKind::Renamed { old_path } => {
                crate::vfs_event_v2::Kind::RenamedFrom(ProviderPathV2::from_provider_path(old_path))
            }
            VfsEventKind::Overflow { rescan_root } => {
                crate::vfs_event_v2::Kind::OverflowRescanRoot(ProviderPathV2::from_provider_path(
                    rescan_root,
                ))
            }
        };
        Self {
            path: Some(ProviderPathV2::from_provider_path(&event.path)),
            kind: Some(kind),
        }
    }

    pub fn to_vfs_event(&self) -> Result<VfsEvent, VfsWireError> {
        let kind = match self.kind.as_ref().ok_or(VfsWireError::MissingEventKind)? {
            crate::vfs_event_v2::Kind::Created(_) => VfsEventKind::Created,
            crate::vfs_event_v2::Kind::Modified(_) => VfsEventKind::Modified,
            crate::vfs_event_v2::Kind::Removed(_) => VfsEventKind::Removed,
            crate::vfs_event_v2::Kind::RenamedFrom(path) => VfsEventKind::Renamed {
                old_path: path.to_provider_path()?,
            },
            crate::vfs_event_v2::Kind::OverflowRescanRoot(path) => VfsEventKind::Overflow {
                rescan_root: path.to_provider_path()?,
            },
        };
        Ok(VfsEvent {
            path: self
                .path
                .as_ref()
                .ok_or(VfsWireError::MissingField("VFS event path"))?
                .to_provider_path()?,
            kind,
        })
    }
}

impl VfsEventBatchV2 {
    pub fn from_event_batch(batch: &EventBatch) -> Self {
        Self {
            first_sequence: batch.first_sequence,
            last_sequence: batch.last_sequence,
            events: batch
                .events
                .iter()
                .map(VfsEventV2::from_vfs_event)
                .collect(),
        }
    }

    pub fn to_event_batch(&self) -> Result<EventBatch, VfsWireError> {
        Ok(EventBatch {
            first_sequence: self.first_sequence,
            last_sequence: self.last_sequence,
            events: self
                .events
                .iter()
                .map(VfsEventV2::to_vfs_event)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

fn encode_support_level(level: SupportLevel) -> VfsSupportLevelV2 {
    match level {
        SupportLevel::Unsupported => VfsSupportLevelV2::Unsupported,
        SupportLevel::Emulated => VfsSupportLevelV2::Emulated,
        SupportLevel::Native => VfsSupportLevelV2::Native,
    }
}

fn decode_support_level(level: i32) -> Result<SupportLevel, VfsWireError> {
    match VfsSupportLevelV2::try_from(level) {
        Ok(VfsSupportLevelV2::Unsupported) => Ok(SupportLevel::Unsupported),
        Ok(VfsSupportLevelV2::Emulated) => Ok(SupportLevel::Emulated),
        Ok(VfsSupportLevelV2::Native) => Ok(SupportLevel::Native),
        Ok(VfsSupportLevelV2::Unspecified) | Err(_) => {
            Err(VfsWireError::UnknownSupportLevel(level))
        }
    }
}

pub fn encode_atomicity(atomicity: Atomicity) -> VfsAtomicityV2 {
    match atomicity {
        Atomicity::BestEffort => VfsAtomicityV2::BestEffort,
        Atomicity::AtomicWithinFile => VfsAtomicityV2::AtomicWithinFile,
        Atomicity::AtomicWithinDirectory => VfsAtomicityV2::AtomicWithinDirectory,
        Atomicity::AtomicWithinMount => VfsAtomicityV2::AtomicWithinMount,
    }
}

pub fn decode_atomicity(atomicity: i32) -> Result<Atomicity, VfsWireError> {
    match VfsAtomicityV2::try_from(atomicity) {
        Ok(VfsAtomicityV2::BestEffort) => Ok(Atomicity::BestEffort),
        Ok(VfsAtomicityV2::AtomicWithinFile) => Ok(Atomicity::AtomicWithinFile),
        Ok(VfsAtomicityV2::AtomicWithinDirectory) => Ok(Atomicity::AtomicWithinDirectory),
        Ok(VfsAtomicityV2::AtomicWithinMount) => Ok(Atomicity::AtomicWithinMount),
        Ok(VfsAtomicityV2::Unspecified) | Err(_) => Err(VfsWireError::UnknownAtomicity(atomicity)),
    }
}

fn encode_case_sensitivity(case_sensitivity: CaseSensitivity) -> VfsCaseSensitivityV2 {
    match case_sensitivity {
        CaseSensitivity::Sensitive => VfsCaseSensitivityV2::Sensitive,
        CaseSensitivity::Insensitive => VfsCaseSensitivityV2::Insensitive,
        CaseSensitivity::PerDirectory => VfsCaseSensitivityV2::PerDirectory,
    }
}

fn decode_case_sensitivity(case_sensitivity: i32) -> Result<CaseSensitivity, VfsWireError> {
    match VfsCaseSensitivityV2::try_from(case_sensitivity) {
        Ok(VfsCaseSensitivityV2::Sensitive) => Ok(CaseSensitivity::Sensitive),
        Ok(VfsCaseSensitivityV2::Insensitive) => Ok(CaseSensitivity::Insensitive),
        Ok(VfsCaseSensitivityV2::PerDirectory) => Ok(CaseSensitivity::PerDirectory),
        Ok(VfsCaseSensitivityV2::Unspecified) | Err(_) => {
            Err(VfsWireError::UnknownCaseSensitivity(case_sensitivity))
        }
    }
}

fn encode_entry_kind(kind: EntryKind) -> VfsEntryKindV2 {
    match kind {
        EntryKind::File => VfsEntryKindV2::File,
        EntryKind::Directory => VfsEntryKindV2::Directory,
        EntryKind::SymbolicLink => VfsEntryKindV2::SymbolicLink,
        EntryKind::Special => VfsEntryKindV2::Special,
    }
}

fn decode_entry_kind(kind: i32) -> Result<EntryKind, VfsWireError> {
    match VfsEntryKindV2::try_from(kind) {
        Ok(VfsEntryKindV2::File) => Ok(EntryKind::File),
        Ok(VfsEntryKindV2::Directory) => Ok(EntryKind::Directory),
        Ok(VfsEntryKindV2::SymbolicLink) => Ok(EntryKind::SymbolicLink),
        Ok(VfsEntryKindV2::Special) => Ok(EntryKind::Special),
        Ok(VfsEntryKindV2::Unspecified) | Err(_) => Err(VfsWireError::UnknownEntryKind(kind)),
    }
}

fn encode_error_code(code: VfsErrorCode) -> VfsErrorCodeV2 {
    match code {
        VfsErrorCode::NotFound => VfsErrorCodeV2::NotFound,
        VfsErrorCode::AlreadyExists => VfsErrorCodeV2::AlreadyExists,
        VfsErrorCode::NotDirectory => VfsErrorCodeV2::NotDirectory,
        VfsErrorCode::IsDirectory => VfsErrorCodeV2::IsDirectory,
        VfsErrorCode::PermissionDenied => VfsErrorCodeV2::PermissionDenied,
        VfsErrorCode::ReadOnly => VfsErrorCodeV2::ReadOnly,
        VfsErrorCode::Unsupported => VfsErrorCodeV2::Unsupported,
        VfsErrorCode::InvalidPath => VfsErrorCodeV2::InvalidPath,
        VfsErrorCode::InvalidArgument => VfsErrorCodeV2::InvalidArgument,
        VfsErrorCode::Conflict => VfsErrorCodeV2::Conflict,
        VfsErrorCode::NameCollision => VfsErrorCodeV2::NameCollision,
        VfsErrorCode::StaleVersion => VfsErrorCodeV2::StaleVersion,
        VfsErrorCode::Disconnected => VfsErrorCodeV2::Disconnected,
        VfsErrorCode::Cancelled => VfsErrorCodeV2::Cancelled,
        VfsErrorCode::Timeout => VfsErrorCodeV2::Timeout,
        VfsErrorCode::Quota => VfsErrorCodeV2::Quota,
        VfsErrorCode::TooLarge => VfsErrorCodeV2::TooLarge,
        VfsErrorCode::CorruptData => VfsErrorCodeV2::CorruptData,
        VfsErrorCode::WatchOverflow => VfsErrorCodeV2::WatchOverflow,
        VfsErrorCode::Internal => VfsErrorCodeV2::Internal,
    }
}

fn decode_error_code(code: i32) -> Result<VfsErrorCode, VfsWireError> {
    match VfsErrorCodeV2::try_from(code) {
        Ok(VfsErrorCodeV2::NotFound) => Ok(VfsErrorCode::NotFound),
        Ok(VfsErrorCodeV2::AlreadyExists) => Ok(VfsErrorCode::AlreadyExists),
        Ok(VfsErrorCodeV2::NotDirectory) => Ok(VfsErrorCode::NotDirectory),
        Ok(VfsErrorCodeV2::IsDirectory) => Ok(VfsErrorCode::IsDirectory),
        Ok(VfsErrorCodeV2::PermissionDenied) => Ok(VfsErrorCode::PermissionDenied),
        Ok(VfsErrorCodeV2::ReadOnly) => Ok(VfsErrorCode::ReadOnly),
        Ok(VfsErrorCodeV2::Unsupported) => Ok(VfsErrorCode::Unsupported),
        Ok(VfsErrorCodeV2::InvalidPath) => Ok(VfsErrorCode::InvalidPath),
        Ok(VfsErrorCodeV2::InvalidArgument) => Ok(VfsErrorCode::InvalidArgument),
        Ok(VfsErrorCodeV2::Conflict) => Ok(VfsErrorCode::Conflict),
        Ok(VfsErrorCodeV2::NameCollision) => Ok(VfsErrorCode::NameCollision),
        Ok(VfsErrorCodeV2::StaleVersion) => Ok(VfsErrorCode::StaleVersion),
        Ok(VfsErrorCodeV2::Disconnected) => Ok(VfsErrorCode::Disconnected),
        Ok(VfsErrorCodeV2::Cancelled) => Ok(VfsErrorCode::Cancelled),
        Ok(VfsErrorCodeV2::Timeout) => Ok(VfsErrorCode::Timeout),
        Ok(VfsErrorCodeV2::Quota) => Ok(VfsErrorCode::Quota),
        Ok(VfsErrorCodeV2::TooLarge) => Ok(VfsErrorCode::TooLarge),
        Ok(VfsErrorCodeV2::CorruptData) => Ok(VfsErrorCode::CorruptData),
        Ok(VfsErrorCodeV2::WatchOverflow) => Ok(VfsErrorCode::WatchOverflow),
        Ok(VfsErrorCodeV2::Internal) => Ok(VfsErrorCode::Internal),
        Ok(VfsErrorCodeV2::Unspecified) | Err(_) => Err(VfsWireError::UnknownErrorCode(code)),
    }
}

fn encode_operation(operation: VfsOperation) -> VfsOperationV2 {
    match operation {
        VfsOperation::Stat => VfsOperationV2::Stat,
        VfsOperation::ReadDirectory => VfsOperationV2::ReadDirectory,
        VfsOperation::Open => VfsOperationV2::Open,
        VfsOperation::Read => VfsOperationV2::Read,
        VfsOperation::Write => VfsOperationV2::Write,
        VfsOperation::SetLength => VfsOperationV2::SetLength,
        VfsOperation::Flush => VfsOperationV2::Flush,
        VfsOperation::Sync => VfsOperationV2::Sync,
        VfsOperation::CreateDirectory => VfsOperationV2::CreateDirectory,
        VfsOperation::Remove => VfsOperationV2::Remove,
        VfsOperation::Rename => VfsOperationV2::Rename,
        VfsOperation::Copy => VfsOperationV2::Copy,
        VfsOperation::Watch => VfsOperationV2::Watch,
        VfsOperation::NativePath => VfsOperationV2::NativePath,
    }
}

fn decode_operation(operation: i32) -> Result<VfsOperation, VfsWireError> {
    match VfsOperationV2::try_from(operation) {
        Ok(VfsOperationV2::Stat) => Ok(VfsOperation::Stat),
        Ok(VfsOperationV2::ReadDirectory) => Ok(VfsOperation::ReadDirectory),
        Ok(VfsOperationV2::Open) => Ok(VfsOperation::Open),
        Ok(VfsOperationV2::Read) => Ok(VfsOperation::Read),
        Ok(VfsOperationV2::Write) => Ok(VfsOperation::Write),
        Ok(VfsOperationV2::SetLength) => Ok(VfsOperation::SetLength),
        Ok(VfsOperationV2::Flush) => Ok(VfsOperation::Flush),
        Ok(VfsOperationV2::Sync) => Ok(VfsOperation::Sync),
        Ok(VfsOperationV2::CreateDirectory) => Ok(VfsOperation::CreateDirectory),
        Ok(VfsOperationV2::Remove) => Ok(VfsOperation::Remove),
        Ok(VfsOperationV2::Rename) => Ok(VfsOperation::Rename),
        Ok(VfsOperationV2::Copy) => Ok(VfsOperation::Copy),
        Ok(VfsOperationV2::Watch) => Ok(VfsOperation::Watch),
        Ok(VfsOperationV2::NativePath) => Ok(VfsOperation::NativePath),
        Ok(VfsOperationV2::Unspecified) | Err(_) => Err(VfsWireError::UnknownOperation(operation)),
    }
}

fn non_zero_u32(value: u32, name: &'static str) -> Result<NonZeroU32, VfsWireError> {
    NonZeroU32::new(value).ok_or(VfsWireError::ZeroProviderLimit(name))
}

fn non_zero_u64(value: u64, name: &'static str) -> Result<NonZeroU64, VfsWireError> {
    NonZeroU64::new(value).ok_or(VfsWireError::ZeroProviderLimit(name))
}

pub fn encode_file_access(access: FileAccess) -> VfsFileAccessV2 {
    match access {
        FileAccess::Read => VfsFileAccessV2::Read,
        FileAccess::Write => VfsFileAccessV2::Write,
        FileAccess::ReadWrite => VfsFileAccessV2::ReadWrite,
    }
}

pub fn decode_file_access(access: i32) -> Result<FileAccess, VfsWireError> {
    match VfsFileAccessV2::try_from(access) {
        Ok(VfsFileAccessV2::Read) => Ok(FileAccess::Read),
        Ok(VfsFileAccessV2::Write) => Ok(FileAccess::Write),
        Ok(VfsFileAccessV2::ReadWrite) => Ok(FileAccess::ReadWrite),
        Ok(VfsFileAccessV2::Unspecified) | Err(_) => Err(VfsWireError::UnknownFileAccess(access)),
    }
}

pub fn encode_create_disposition(disposition: CreateDisposition) -> VfsCreateDispositionV2 {
    match disposition {
        CreateDisposition::OpenExisting => VfsCreateDispositionV2::OpenExisting,
        CreateDisposition::CreateNew => VfsCreateDispositionV2::CreateNew,
        CreateDisposition::OpenOrCreate => VfsCreateDispositionV2::OpenOrCreate,
        CreateDisposition::TruncateExisting => VfsCreateDispositionV2::TruncateExisting,
    }
}

pub fn decode_create_disposition(disposition: i32) -> Result<CreateDisposition, VfsWireError> {
    match VfsCreateDispositionV2::try_from(disposition) {
        Ok(VfsCreateDispositionV2::OpenExisting) => Ok(CreateDisposition::OpenExisting),
        Ok(VfsCreateDispositionV2::CreateNew) => Ok(CreateDisposition::CreateNew),
        Ok(VfsCreateDispositionV2::OpenOrCreate) => Ok(CreateDisposition::OpenOrCreate),
        Ok(VfsCreateDispositionV2::TruncateExisting) => Ok(CreateDisposition::TruncateExisting),
        Ok(VfsCreateDispositionV2::Unspecified) | Err(_) => {
            Err(VfsWireError::UnknownCreateDisposition(disposition))
        }
    }
}

pub fn encode_remove_kind(kind: RemoveKind) -> VfsRemoveKindV2 {
    match kind {
        RemoveKind::File => VfsRemoveKindV2::File,
        RemoveKind::EmptyDirectory => VfsRemoveKindV2::EmptyDirectory,
        RemoveKind::Recursive => VfsRemoveKindV2::Recursive,
    }
}

pub fn decode_remove_kind(kind: i32) -> Result<RemoveKind, VfsWireError> {
    match VfsRemoveKindV2::try_from(kind) {
        Ok(VfsRemoveKindV2::File) => Ok(RemoveKind::File),
        Ok(VfsRemoveKindV2::EmptyDirectory) => Ok(RemoveKind::EmptyDirectory),
        Ok(VfsRemoveKindV2::Recursive) => Ok(RemoveKind::Recursive),
        Ok(VfsRemoveKindV2::Unspecified) | Err(_) => Err(VfsWireError::UnknownRemoveKind(kind)),
    }
}

pub fn encode_collision_policy(policy: CollisionPolicy) -> VfsCollisionPolicyV2 {
    match policy {
        CollisionPolicy::Fail => VfsCollisionPolicyV2::Fail,
        CollisionPolicy::Replace => VfsCollisionPolicyV2::Replace,
    }
}

pub fn decode_collision_policy(policy: i32) -> Result<CollisionPolicy, VfsWireError> {
    match VfsCollisionPolicyV2::try_from(policy) {
        Ok(VfsCollisionPolicyV2::Fail) => Ok(CollisionPolicy::Fail),
        Ok(VfsCollisionPolicyV2::Replace) => Ok(CollisionPolicy::Replace),
        Ok(VfsCollisionPolicyV2::Unspecified) | Err(_) => {
            Err(VfsWireError::UnknownCollisionPolicy(policy))
        }
    }
}

pub fn encode_watch_depth(depth: WatchDepth) -> VfsWatchDepthV2 {
    match depth {
        WatchDepth::DirectChildren => VfsWatchDepthV2::DirectChildren,
        WatchDepth::Recursive => VfsWatchDepthV2::Recursive,
    }
}

pub fn decode_watch_depth(depth: i32) -> Result<WatchDepth, VfsWireError> {
    match VfsWatchDepthV2::try_from(depth) {
        Ok(VfsWatchDepthV2::DirectChildren) => Ok(WatchDepth::DirectChildren),
        Ok(VfsWatchDepthV2::Recursive) => Ok(WatchDepth::Recursive),
        Ok(VfsWatchDepthV2::Unspecified) | Err(_) => Err(VfsWireError::UnknownWatchDepth(depth)),
    }
}

pub fn encode_path_encoding(encoding: PathEncoding) -> PathEncodingV2 {
    match encoding {
        PathEncoding::UnixBytes => PathEncodingV2::UnixBytes,
        PathEncoding::WindowsWtf8 => PathEncodingV2::WindowsWtf8,
        PathEncoding::PortableUtf8 => PathEncodingV2::PortableUtf8,
    }
}

pub fn decode_path_encoding(encoding: i32) -> Result<PathEncoding, VfsWireError> {
    match PathEncodingV2::try_from(encoding) {
        Ok(PathEncodingV2::UnixBytes) => Ok(PathEncoding::UnixBytes),
        Ok(PathEncodingV2::WindowsWtf8) => Ok(PathEncoding::WindowsWtf8),
        Ok(PathEncodingV2::PortableUtf8) => Ok(PathEncoding::PortableUtf8),
        Ok(PathEncodingV2::Unspecified) | Err(_) => {
            Err(VfsWireError::UnknownPathEncoding(encoding))
        }
    }
}

fn relative_root() -> NativePathRootV2 {
    NativePathRootV2 {
        kind: NativePathRootKindV2::Relative as i32,
        ..Default::default()
    }
}

fn encode_native_root(root: &NativePathRoot) -> NativePathRootV2 {
    match root {
        NativePathRoot::Relative => relative_root(),
        NativePathRoot::Posix => NativePathRootV2 {
            kind: NativePathRootKindV2::Posix as i32,
            ..Default::default()
        },
        NativePathRoot::WindowsRootRelative => NativePathRootV2 {
            kind: NativePathRootKindV2::WindowsRootRelative as i32,
            ..Default::default()
        },
        NativePathRoot::WindowsDrive { drive, form } => NativePathRootV2 {
            kind: NativePathRootKindV2::WindowsDrive as i32,
            drive: vec![drive.as_ascii()],
            absolute: form.is_absolute(),
            verbatim: form.is_verbatim(),
            ..Default::default()
        },
        NativePathRoot::WindowsUnc {
            server,
            share,
            form,
        } => NativePathRootV2 {
            kind: NativePathRootKindV2::WindowsUnc as i32,
            server: server.as_bytes().to_vec(),
            share: share.as_bytes().to_vec(),
            verbatim: *form == WindowsUncForm::Verbatim,
            ..Default::default()
        },
        NativePathRoot::WindowsDevice { device } => NativePathRootV2 {
            kind: NativePathRootKindV2::WindowsDevice as i32,
            namespace: device.as_bytes().to_vec(),
            ..Default::default()
        },
        NativePathRoot::WindowsVerbatim { namespace } => NativePathRootV2 {
            kind: NativePathRootKindV2::WindowsVerbatim as i32,
            namespace: namespace.as_bytes().to_vec(),
            verbatim: true,
            ..Default::default()
        },
    }
}

fn decode_native_root(root: &NativePathRootV2) -> Result<NativePathRoot, VfsWireError> {
    match decode_root_kind(root.kind)? {
        NativePathRootKindV2::Relative if root_has_no_payload_or_flags(root) => {
            Ok(NativePathRoot::Relative)
        }
        NativePathRootKindV2::Posix if root_has_no_payload_or_flags(root) => {
            Ok(NativePathRoot::Posix)
        }
        NativePathRootKindV2::WindowsRootRelative if root_has_no_payload_or_flags(root) => {
            Ok(NativePathRoot::WindowsRootRelative)
        }
        NativePathRootKindV2::WindowsDrive
            if root.server.is_empty() && root.share.is_empty() && root.namespace.is_empty() =>
        {
            let [drive] = root.drive.as_slice() else {
                return Err(VfsWireError::InvalidWindowsDrive);
            };
            Ok(NativePathRoot::WindowsDrive {
                drive: WindowsDrive::new(*drive).map_err(|_| VfsWireError::InvalidWindowsDrive)?,
                form: match (root.absolute, root.verbatim) {
                    (false, false) => WindowsDriveForm::Relative,
                    (true, false) => WindowsDriveForm::Absolute,
                    (false, true) => WindowsDriveForm::VerbatimRelative,
                    (true, true) => WindowsDriveForm::VerbatimAbsolute,
                },
            })
        }
        NativePathRootKindV2::WindowsUnc
            if root.drive.is_empty() && root.namespace.is_empty() && !root.absolute =>
        {
            Ok(NativePathRoot::WindowsUnc {
                server: ExactComponent::new(PathEncoding::WindowsWtf8, root.server.clone())?,
                share: ExactComponent::new(PathEncoding::WindowsWtf8, root.share.clone())?,
                form: if root.verbatim {
                    WindowsUncForm::Verbatim
                } else {
                    WindowsUncForm::Standard
                },
            })
        }
        NativePathRootKindV2::WindowsDevice
            if root.drive.is_empty()
                && root.server.is_empty()
                && root.share.is_empty()
                && !root.absolute
                && !root.verbatim =>
        {
            Ok(NativePathRoot::WindowsDevice {
                device: ExactComponent::new(PathEncoding::WindowsWtf8, root.namespace.clone())?,
            })
        }
        NativePathRootKindV2::WindowsVerbatim
            if root.drive.is_empty()
                && root.server.is_empty()
                && root.share.is_empty()
                && !root.absolute
                && root.verbatim =>
        {
            Ok(NativePathRoot::WindowsVerbatim {
                namespace: ExactComponent::new(PathEncoding::WindowsWtf8, root.namespace.clone())?,
            })
        }
        NativePathRootKindV2::Unspecified => Err(VfsWireError::UnknownNativeRootKind(root.kind)),
        _ => Err(VfsWireError::InvalidNativeRoot),
    }
}

fn decode_root_kind(kind: i32) -> Result<NativePathRootKindV2, VfsWireError> {
    match NativePathRootKindV2::try_from(kind) {
        Ok(NativePathRootKindV2::Unspecified) | Err(_) => {
            Err(VfsWireError::UnknownNativeRootKind(kind))
        }
        Ok(kind) => Ok(kind),
    }
}

fn root_has_no_payload_or_flags(root: &NativePathRootV2) -> bool {
    root_has_no_payload(root) && !root.absolute && !root.verbatim
}

fn root_has_no_payload(root: &NativePathRootV2) -> bool {
    root.drive.is_empty()
        && root.server.is_empty()
        && root.share.is_empty()
        && root.namespace.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message as _;
    use serde::Deserialize;
    use std::collections::BTreeSet;

    #[derive(Debug, Deserialize)]
    struct PathCorpus {
        cases: Vec<PathCorpusCase>,
    }

    #[derive(Debug, Deserialize)]
    struct PathCorpusCase {
        id: String,
        encoding: String,
        form: String,
        root_kind: String,
        components_hex: Vec<String>,
        expected: String,
    }

    #[test]
    fn fixed_path_corpus_round_trips_through_protobuf() {
        let parsed = serde_json::from_str::<PathCorpus>(include_str!(
            "../../vfs/test_data/path_corpus.json"
        ));
        let Ok(corpus) = parsed else {
            panic!("path corpus must parse: {parsed:?}");
        };
        let mut round_tripped = BTreeSet::new();
        let mut rejected = BTreeSet::new();

        for case in corpus.cases {
            let encoding = encoding_from_fixture(&case.encoding);
            let Ok(encoding) = encoding else {
                panic!("fixture encoding must be known: {case:?}");
            };
            let components = case
                .components_hex
                .iter()
                .map(|component| decode_hex(component))
                .collect::<Result<Vec<_>, _>>();
            let Ok(components) = components else {
                panic!("fixture components must be hex: {case:?}");
            };

            let result = match case.form.as_str() {
                "provider_relative" | "archive_member" => {
                    round_trip_provider_fixture(&case, encoding, components)
                }
                "native" => round_trip_native_fixture(&case, encoding, components),
                _ => panic!("unknown fixture form: {case:?}"),
            };

            if case.expected == "valid" {
                assert!(
                    result.is_ok(),
                    "valid fixture failed: {}: {result:?}",
                    case.id
                );
                round_tripped.insert(case.id);
            } else {
                assert!(result.is_err(), "invalid fixture passed: {}", case.id);
                rejected.insert(case.id);
            }
        }

        assert_eq!(round_tripped.len(), 10);
        assert_eq!(rejected.len(), 6);
    }

    #[test]
    fn resource_and_entry_name_wire_types_round_trip() {
        let resource_id = ResourceId::new(MountId::new(17), 42, 3);
        let wire_resource = ResourceIdV2::from_resource_id(resource_id);
        let round_tripped_resource = wire_resource.to_resource_id();
        let Ok(round_tripped_resource) = round_tripped_resource else {
            panic!("resource id must round trip: {round_tripped_resource:?}");
        };
        assert_eq!(round_tripped_resource, resource_id);

        let exact = ExactComponent::new(PathEncoding::UnixBytes, b"name".to_vec());
        let Ok(exact) = exact else {
            panic!("test component must be valid: {exact:?}");
        };
        let entry_name = EntryName::new(
            exact,
            DisplayComponent::new("Name"),
            LookupKey::new(b"name".to_vec()),
        );
        let wire_entry_name = VfsPathComponentV2::from_entry_name(&entry_name);
        assert_eq!(wire_entry_name.to_entry_name().ok(), Some(entry_name));
    }

    #[test]
    fn provider_metadata_error_and_event_wire_types_round_trip() {
        use vfs::{MemoryProvider, VfsProvider as _};

        let provider = MemoryProvider::new("wire-provider", PathEncoding::PortableUtf8);
        let descriptor = provider.descriptor().clone();
        let capabilities = provider.capabilities();
        let wire_descriptor = VfsProviderDescriptorV2::from_provider_descriptor(&descriptor);
        let wire_capabilities =
            VfsProviderCapabilitiesV2::from_provider_capabilities(&capabilities);
        assert_eq!(
            wire_descriptor.to_provider_descriptor().ok(),
            Some(descriptor)
        );
        assert_eq!(
            wire_capabilities.to_provider_capabilities().ok(),
            Some(capabilities)
        );

        let path = ProviderPath::from_byte_components(
            PathEncoding::PortableUtf8,
            [b"entry.rs".as_slice()],
        );
        let Ok(path) = path else {
            panic!("test path must be valid: {path:?}");
        };
        let Some(exact_name) = path.file_name().cloned() else {
            panic!("test path must have a file name");
        };
        let metadata = EntryMetadata {
            name: Some(EntryName::from_exact(exact_name)),
            kind: EntryKind::File,
            size: 42,
            modified_at: UNIX_EPOCH.checked_sub(Duration::from_millis(500)),
            created_at: UNIX_EPOCH.checked_add(Duration::from_secs(3)),
            permissions: EntryPermissions {
                writable: true,
                executable: false,
                private: true,
                hidden: false,
            },
            provider_file_key: Some(ProviderFileKey::new(b"file-key".to_vec())),
            content_version: VfsVersion::new(b"content".to_vec()),
            structure_version: VfsVersion::new(b"structure".to_vec()),
            symbolic_link_target: None,
            symbolic_link_target_kind: None,
            case_sensitivity: CaseSensitivity::Sensitive,
        };
        let wire_metadata = VfsEntryMetadataV2::from_entry_metadata(&metadata);
        let Ok(wire_metadata) = wire_metadata else {
            panic!("metadata must encode: {wire_metadata:?}");
        };
        assert_eq!(wire_metadata.to_entry_metadata().ok(), Some(metadata));

        let error = VfsError::new(
            VfsErrorCode::StaleVersion,
            VfsOperation::Write,
            ProviderId::new("wire-provider"),
        )
        .with_path(path.clone())
        .with_detail("version mismatch");
        let round_tripped_error = VfsErrorV2::from_vfs_error(&error).to_vfs_error();
        let Ok(round_tripped_error) = round_tripped_error else {
            panic!("error must round trip: {round_tripped_error:?}");
        };
        assert_eq!(round_tripped_error.code(), error.code());
        assert_eq!(round_tripped_error.operation(), error.operation());
        assert_eq!(round_tripped_error.provider(), error.provider());
        assert_eq!(round_tripped_error.path(), error.path());
        assert_eq!(round_tripped_error.detail(), error.detail());

        let collision = VfsError::new(
            VfsErrorCode::NameCollision,
            VfsOperation::Open,
            ProviderId::new("archive-wire-provider"),
        )
        .with_path(path.clone());
        let collision = VfsErrorV2::from_vfs_error(&collision).to_vfs_error();
        assert!(
            collision
                .as_ref()
                .is_ok_and(|error| error.code() == VfsErrorCode::NameCollision)
        );

        let old_path =
            ProviderPath::from_byte_components(PathEncoding::PortableUtf8, [b"old.rs".as_slice()]);
        let Ok(old_path) = old_path else {
            panic!("old test path must be valid: {old_path:?}");
        };
        let batch = EventBatch {
            first_sequence: 7,
            last_sequence: 8,
            events: vec![
                VfsEvent {
                    path: path.clone(),
                    kind: VfsEventKind::Renamed { old_path },
                },
                VfsEvent {
                    path,
                    kind: VfsEventKind::Overflow {
                        rescan_root: ProviderPath::root(PathEncoding::PortableUtf8),
                    },
                },
            ],
        };
        assert_eq!(
            VfsEventBatchV2::from_event_batch(&batch)
                .to_event_batch()
                .ok(),
            Some(batch)
        );

        let mut invalid_limits = wire_capabilities;
        let Some(limits) = invalid_limits.limits.as_mut() else {
            panic!("wire capabilities must contain limits");
        };
        limits.maximum_page_size = 0;
        assert!(matches!(
            invalid_limits.to_provider_capabilities(),
            Err(VfsWireError::ZeroProviderLimit("maximum page size"))
        ));
    }

    fn round_trip_provider_fixture(
        case: &PathCorpusCase,
        encoding: PathEncoding,
        components: Vec<Vec<u8>>,
    ) -> Result<(), VfsWireError> {
        if case.root_kind != "relative" {
            let wire = ProviderPathV2 {
                path: Some(ExactPathV2 {
                    encoding: encode_path_encoding(encoding) as i32,
                    root: Some(NativePathRootV2 {
                        kind: NativePathRootKindV2::WindowsDrive as i32,
                        drive: vec![b'C'],
                        absolute: true,
                        ..Default::default()
                    }),
                    components,
                }),
            };
            wire.to_provider_path()?;
            return Ok(());
        }

        let path = ProviderPath::from_byte_components(encoding, &components)?;
        let wire = ProviderPathV2::from_provider_path(&path);
        let decoded = protobuf_round_trip_provider_path(&wire)?;
        let round_tripped = decoded.to_provider_path()?;
        assert_eq!(round_tripped, path);
        Ok(())
    }

    fn round_trip_native_fixture(
        case: &PathCorpusCase,
        encoding: PathEncoding,
        mut components: Vec<Vec<u8>>,
    ) -> Result<(), VfsWireError> {
        if encoding != PathEncoding::WindowsWtf8 {
            return Err(VfsWireError::InvalidNativeRoot);
        }
        let root = match case.root_kind.as_str() {
            "windows_drive" | "windows_verbatim_drive" => {
                let drive = take_first_component(&mut components)?;
                let [drive] = drive.as_slice() else {
                    return Err(VfsWireError::InvalidWindowsDrive);
                };
                NativePathRoot::WindowsDrive {
                    drive: WindowsDrive::new(*drive)
                        .map_err(|_| VfsWireError::InvalidWindowsDrive)?,
                    form: if case.root_kind == "windows_verbatim_drive" {
                        WindowsDriveForm::VerbatimAbsolute
                    } else {
                        WindowsDriveForm::Absolute
                    },
                }
            }
            "windows_unc" => NativePathRoot::WindowsUnc {
                server: ExactComponent::new(
                    PathEncoding::WindowsWtf8,
                    take_first_component(&mut components)?,
                )?,
                share: ExactComponent::new(
                    PathEncoding::WindowsWtf8,
                    take_first_component(&mut components)?,
                )?,
                form: WindowsUncForm::Standard,
            },
            _ => return Err(VfsWireError::InvalidNativeRoot),
        };
        let provider_path = ProviderPath::from_byte_components(encoding, &components)?;
        let native_path = NativePath::new(root, provider_path)?;
        let wire = NativePathV2::from_native_path(&native_path);
        let decoded = protobuf_round_trip_native_path(&wire)?;
        let round_tripped = decoded.to_native_path()?;
        assert_eq!(round_tripped, native_path);
        Ok(())
    }

    fn protobuf_round_trip_provider_path(
        wire: &ProviderPathV2,
    ) -> Result<ProviderPathV2, VfsWireError> {
        ProviderPathV2::decode(wire.encode_to_vec().as_slice()).map_err(Into::into)
    }

    fn protobuf_round_trip_native_path(wire: &NativePathV2) -> Result<NativePathV2, VfsWireError> {
        NativePathV2::decode(wire.encode_to_vec().as_slice()).map_err(Into::into)
    }

    fn take_first_component(components: &mut Vec<Vec<u8>>) -> Result<Vec<u8>, VfsWireError> {
        if components.is_empty() {
            return Err(VfsWireError::MissingField("native root component"));
        }
        Ok(components.remove(0))
    }

    fn encoding_from_fixture(encoding: &str) -> Result<PathEncoding, VfsWireError> {
        match encoding {
            "unix_bytes" => Ok(PathEncoding::UnixBytes),
            "windows_wtf8" => Ok(PathEncoding::WindowsWtf8),
            "portable_utf8" => Ok(PathEncoding::PortableUtf8),
            _ => Err(VfsWireError::UnknownPathEncoding(-1)),
        }
    }

    fn decode_hex(hex: &str) -> Result<Vec<u8>, VfsWireError> {
        if !hex.len().is_multiple_of(2) {
            return Err(VfsWireError::InvalidNativeRoot);
        }
        let mut bytes = Vec::with_capacity(hex.len() / 2);
        for pair in hex.as_bytes().chunks_exact(2) {
            let high = hex_digit(pair[0])?;
            let low = hex_digit(pair[1])?;
            bytes.push((high << 4) | low);
        }
        Ok(bytes)
    }

    fn hex_digit(byte: u8) -> Result<u8, VfsWireError> {
        match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            b'A'..=b'F' => Ok(byte - b'A' + 10),
            _ => Err(VfsWireError::InvalidNativeRoot),
        }
    }
}
