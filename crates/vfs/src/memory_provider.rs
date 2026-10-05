use crate::{
    Atomicity, CaseSensitivity, CollisionPolicy, CopyOptions, CreateDirOptions, CreateDisposition,
    CreateParents, DirCursor, DirEntry, DirPage, DirPageRequest, DirectoryCapabilities, EntryKind,
    EntryMetadata, EntryName, EntryPermissions, EventBatch, FileAccess, LinkCapabilities,
    MutationCapabilities, OpenOptions, OperationContext, PathEncoding, ProviderCapabilities,
    ProviderDescriptor, ProviderFileKey, ProviderId, ProviderLimits, ProviderPath,
    ReadCapabilities, RemoveKind, RemoveOptions, RemoveOutcome, RenameOptions, StatOptions,
    SupportLevel, TrashCapabilities, VfsError, VfsErrorCode, VfsEvent, VfsEventKind, VfsFile,
    VfsOperation, VfsProvider, VfsResult, VfsVersion, WatchCapabilities, WatchDepth, WatchRequest,
    WriteAtOptions, WriteCapabilities,
};
use async_trait::async_trait;
use futures::{
    StreamExt as _,
    channel::mpsc::{self, UnboundedSender},
    stream::{self, BoxStream},
};
use parking_lot::Mutex;
use std::{
    collections::{BTreeMap, VecDeque},
    num::{NonZeroU32, NonZeroU64},
    sync::Arc,
    time::SystemTime,
};

const DEFAULT_JOURNAL_CAPACITY: usize = 1_024;

#[derive(Clone)]
pub struct MemoryProvider {
    descriptor: Arc<ProviderDescriptor>,
    state: Arc<Mutex<MemoryProviderState>>,
    journal_capacity: usize,
}

struct MemoryProviderState {
    nodes: BTreeMap<ProviderPath, MemoryNode>,
    next_file_key: u64,
    next_version: u64,
    sequence: u64,
    journal: VecDeque<EventBatch>,
    subscriptions: Vec<MemoryWatchSubscription>,
}

#[derive(Clone)]
struct MemoryNode {
    kind: EntryKind,
    contents: Vec<u8>,
    file_key: ProviderFileKey,
    content_version: VfsVersion,
    structure_version: VfsVersion,
    modified_at: SystemTime,
}

struct MemoryWatchSubscription {
    path: ProviderPath,
    depth: WatchDepth,
    sender: UnboundedSender<VfsResult<EventBatch>>,
}

impl MemoryProvider {
    pub fn new(id: impl Into<Arc<str>>, path_encoding: PathEncoding) -> Self {
        Self::with_journal_capacity(id, path_encoding, DEFAULT_JOURNAL_CAPACITY)
    }

    pub fn with_journal_capacity(
        id: impl Into<Arc<str>>,
        path_encoding: PathEncoding,
        journal_capacity: usize,
    ) -> Self {
        let descriptor = Arc::new(ProviderDescriptor {
            id: ProviderId::new(id),
            display_name: Arc::from("Memory"),
            path_encoding,
            case_sensitivity: CaseSensitivity::Sensitive,
        });
        let root_path = ProviderPath::root(path_encoding);
        let root_node = MemoryNode {
            kind: EntryKind::Directory,
            contents: Vec::new(),
            file_key: ProviderFileKey::new(1_u64.to_be_bytes().to_vec()),
            content_version: version_from_u64(1),
            structure_version: version_from_u64(1),
            modified_at: SystemTime::now(),
        };
        Self {
            descriptor,
            state: Arc::new(Mutex::new(MemoryProviderState {
                nodes: BTreeMap::from([(root_path, root_node)]),
                next_file_key: 2,
                next_version: 2,
                sequence: 0,
                journal: VecDeque::new(),
                subscriptions: Vec::new(),
            })),
            journal_capacity: journal_capacity.max(1),
        }
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

    fn error(&self, code: VfsErrorCode, operation: VfsOperation) -> VfsError {
        VfsError::new(code, operation, self.descriptor.id.clone())
    }

    fn metadata_for(&self, path: &ProviderPath, node: &MemoryNode) -> EntryMetadata {
        EntryMetadata {
            name: path.file_name().cloned().map(EntryName::from_exact),
            kind: node.kind,
            size: node.contents.len() as u64,
            modified_at: Some(node.modified_at),
            created_at: None,
            permissions: EntryPermissions {
                writable: true,
                executable: false,
                private: false,
                hidden: false,
            },
            provider_file_key: Some(node.file_key.clone()),
            content_version: node.content_version.clone(),
            structure_version: node.structure_version.clone(),
            symbolic_link_target: None,
            case_sensitivity: CaseSensitivity::Sensitive,
        }
    }

    fn check_expected_version(
        &self,
        path: &ProviderPath,
        node: &MemoryNode,
        expected_version: Option<&VfsVersion>,
        operation: VfsOperation,
    ) -> VfsResult<()> {
        let current_version = if node.kind == EntryKind::Directory {
            &node.structure_version
        } else {
            &node.content_version
        };
        if expected_version.is_some_and(|expected| expected != current_version) {
            return Err(self
                .error(VfsErrorCode::StaleVersion, operation)
                .with_path(path.clone()));
        }
        Ok(())
    }

    fn create_node(
        &self,
        state: &mut MemoryProviderState,
        kind: EntryKind,
        operation: VfsOperation,
    ) -> VfsResult<MemoryNode> {
        let file_key = self.next_file_key(state, operation)?;
        let version = self.next_version(state, operation)?;
        Ok(MemoryNode {
            kind,
            contents: Vec::new(),
            file_key,
            content_version: version.clone(),
            structure_version: version,
            modified_at: SystemTime::now(),
        })
    }

    fn ensure_parent_directory(
        &self,
        state: &MemoryProviderState,
        path: &ProviderPath,
        operation: VfsOperation,
    ) -> VfsResult<()> {
        let Some(parent) = path.parent() else {
            return Err(self
                .error(VfsErrorCode::InvalidPath, operation)
                .with_path(path.clone())
                .with_detail("provider root cannot be created or replaced"));
        };
        match state.nodes.get(&parent) {
            Some(node) if node.kind == EntryKind::Directory => Ok(()),
            Some(_) => Err(self
                .error(VfsErrorCode::NotDirectory, operation)
                .with_path(parent)),
            None => Err(self
                .error(VfsErrorCode::NotFound, operation)
                .with_path(parent)),
        }
    }

    fn bump_parent_structure_version(
        &self,
        state: &mut MemoryProviderState,
        path: &ProviderPath,
        operation: VfsOperation,
    ) -> VfsResult<()> {
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        let version = self.next_version(state, operation)?;
        if let Some(parent_node) = state.nodes.get_mut(&parent) {
            parent_node.structure_version = version;
            parent_node.modified_at = SystemTime::now();
        }
        Ok(())
    }

    fn emit_events(
        &self,
        state: &mut MemoryProviderState,
        events: Vec<VfsEvent>,
        operation: VfsOperation,
    ) -> VfsResult<()> {
        state.sequence = state.sequence.checked_add(1).ok_or_else(|| {
            self.error(VfsErrorCode::Internal, operation)
                .with_detail("watch sequence exhausted")
        })?;
        let batch = EventBatch {
            first_sequence: state.sequence,
            last_sequence: state.sequence,
            events,
        };
        state.journal.push_back(batch.clone());
        while state.journal.len() > self.journal_capacity {
            state.journal.pop_front();
        }
        state.subscriptions.retain(|subscription| {
            if !batch.events.iter().any(|event| subscription.matches(event)) {
                return true;
            }
            subscription
                .sender
                .unbounded_send(Ok(batch.clone()))
                .is_ok()
        });
        Ok(())
    }

    fn next_file_key(
        &self,
        state: &mut MemoryProviderState,
        operation: VfsOperation,
    ) -> VfsResult<ProviderFileKey> {
        let file_key = ProviderFileKey::new(state.next_file_key.to_be_bytes().to_vec());
        state.next_file_key = state.next_file_key.checked_add(1).ok_or_else(|| {
            self.error(VfsErrorCode::Internal, operation)
                .with_detail("provider file key space exhausted")
        })?;
        Ok(file_key)
    }

    fn next_version(
        &self,
        state: &mut MemoryProviderState,
        operation: VfsOperation,
    ) -> VfsResult<VfsVersion> {
        let version = version_from_u64(state.next_version);
        state.next_version = state.next_version.checked_add(1).ok_or_else(|| {
            self.error(VfsErrorCode::Internal, operation)
                .with_detail("provider version space exhausted")
        })?;
        Ok(version)
    }

    fn remove_subtree(state: &mut MemoryProviderState, path: &ProviderPath) -> u64 {
        let paths = state
            .nodes
            .keys()
            .filter(|candidate| *candidate == path || candidate.starts_with(path))
            .cloned()
            .collect::<Vec<_>>();
        let removed_count = paths.len() as u64;
        for path in paths {
            state.nodes.remove(&path);
        }
        removed_count
    }
}

impl MemoryWatchSubscription {
    fn matches(&self, event: &VfsEvent) -> bool {
        watch_matches(&self.path, self.depth, event)
    }
}

fn watch_matches(path: &ProviderPath, depth: WatchDepth, event: &VfsEvent) -> bool {
    if matches!(event.kind, VfsEventKind::Overflow { .. }) {
        return true;
    }
    let matches_path = |candidate: &ProviderPath| match depth {
        WatchDepth::Recursive => candidate == path || candidate.starts_with(path),
        WatchDepth::DirectChildren => {
            candidate == path || candidate.parent().as_ref() == Some(path)
        }
    };
    matches_path(&event.path)
        || matches!(&event.kind, VfsEventKind::Renamed { old_path } if matches_path(old_path))
}

#[async_trait]
impl VfsProvider for MemoryProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            read: ReadCapabilities {
                whole_file: SupportLevel::Native,
                stream: SupportLevel::Emulated,
                positioned: SupportLevel::Native,
                atomic_snapshot: SupportLevel::Native,
            },
            write: WriteCapabilities {
                positioned: SupportLevel::Native,
                append: SupportLevel::Emulated,
                truncate: SupportLevel::Native,
                set_len: SupportLevel::Native,
                flush: SupportLevel::Native,
                sync: SupportLevel::Native,
                conditional: SupportLevel::Native,
                maximum_atomicity: Atomicity::AtomicWithinMount,
            },
            directories: DirectoryCapabilities {
                paged: SupportLevel::Native,
                recursive_list: SupportLevel::Emulated,
                stat_many: SupportLevel::Emulated,
            },
            mutations: MutationCapabilities {
                create_directory: SupportLevel::Native,
                remove: SupportLevel::Native,
                rename: SupportLevel::Native,
                copy: SupportLevel::Native,
                cross_provider_copy: SupportLevel::Unsupported,
                idempotency: SupportLevel::Emulated,
                maximum_rename_atomicity: Atomicity::AtomicWithinMount,
                maximum_delete_atomicity: Atomicity::AtomicWithinMount,
            },
            watch: WatchCapabilities {
                watch: SupportLevel::Native,
                recursive: SupportLevel::Native,
                resumable_journal: SupportLevel::Native,
            },
            links: LinkCapabilities {
                symbolic_links: SupportLevel::Unsupported,
                hard_links: SupportLevel::Unsupported,
                permissions: SupportLevel::Emulated,
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
                maximum_range_size: NonZeroU64::new(1024 * 1024).unwrap_or(NonZeroU64::MIN),
                maximum_request_size: NonZeroU64::new(8 * 1024 * 1024).unwrap_or(NonZeroU64::MIN),
                maximum_open_handles: NonZeroU32::new(65_536).unwrap_or(NonZeroU32::MIN),
            },
        }
    }

    async fn stat(&self, path: &ProviderPath, options: StatOptions) -> VfsResult<EntryMetadata> {
        options
            .context
            .cancellation
            .check(VfsOperation::Stat, &self.descriptor.id)?;
        self.validate_path(path, VfsOperation::Stat)?;
        let state = self.state.lock();
        let node = state.nodes.get(path).ok_or_else(|| {
            self.error(VfsErrorCode::NotFound, VfsOperation::Stat)
                .with_path(path.clone())
        })?;
        Ok(self.metadata_for(path, node))
    }

    async fn read_dir(&self, path: &ProviderPath, request: DirPageRequest) -> VfsResult<DirPage> {
        request
            .context
            .cancellation
            .check(VfsOperation::ReadDirectory, &self.descriptor.id)?;
        self.validate_path(path, VfsOperation::ReadDirectory)?;
        if request.limit > self.capabilities().limits.maximum_page_size {
            return Err(self
                .error(VfsErrorCode::TooLarge, VfsOperation::ReadDirectory)
                .with_path(path.clone()));
        }
        let start = decode_cursor(request.cursor.as_ref()).map_err(|detail| {
            self.error(VfsErrorCode::InvalidArgument, VfsOperation::ReadDirectory)
                .with_path(path.clone())
                .with_detail(detail)
        })?;
        let state = self.state.lock();
        let directory = state.nodes.get(path).ok_or_else(|| {
            self.error(VfsErrorCode::NotFound, VfsOperation::ReadDirectory)
                .with_path(path.clone())
        })?;
        if directory.kind != EntryKind::Directory {
            return Err(self
                .error(VfsErrorCode::NotDirectory, VfsOperation::ReadDirectory)
                .with_path(path.clone()));
        }
        let child_count = state
            .nodes
            .iter()
            .filter(|(candidate, _)| candidate.parent().as_ref() == Some(path))
            .count();
        if start > child_count {
            return Err(self
                .error(VfsErrorCode::InvalidArgument, VfsOperation::ReadDirectory)
                .with_path(path.clone())
                .with_detail("directory cursor is past the end of the listing"));
        }
        let page_limit = request.limit.get() as usize;
        let entries = state
            .nodes
            .iter()
            .filter(|(candidate, _)| candidate.parent().as_ref() == Some(path))
            .skip(start)
            .take(page_limit)
            .map(|(path, node)| DirEntry {
                path: path.clone(),
                metadata: self.metadata_for(path, node),
            })
            .collect::<Vec<_>>();
        let next_index = start.saturating_add(entries.len());
        let next_cursor = (next_index < child_count).then(|| encode_cursor(next_index));
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
        self.validate_path(path, VfsOperation::Open)?;
        let mut state = self.state.lock();
        let existing = state.nodes.get(path).cloned();
        match (existing, options.create) {
            (None, CreateDisposition::OpenExisting | CreateDisposition::TruncateExisting) => {
                return Err(self
                    .error(VfsErrorCode::NotFound, VfsOperation::Open)
                    .with_path(path.clone()));
            }
            (Some(_), CreateDisposition::CreateNew) => {
                return Err(self
                    .error(VfsErrorCode::AlreadyExists, VfsOperation::Open)
                    .with_path(path.clone()));
            }
            (None, CreateDisposition::CreateNew | CreateDisposition::OpenOrCreate) => {
                if !options.access.can_write() {
                    return Err(self
                        .error(VfsErrorCode::InvalidArgument, VfsOperation::Open)
                        .with_path(path.clone())
                        .with_detail("creating a file requires write access"));
                }
                self.ensure_parent_directory(&state, path, VfsOperation::Open)?;
                let node = self.create_node(&mut state, EntryKind::File, VfsOperation::Open)?;
                state.nodes.insert(path.clone(), node);
                self.bump_parent_structure_version(&mut state, path, VfsOperation::Open)?;
                self.emit_events(
                    &mut state,
                    vec![VfsEvent {
                        path: path.clone(),
                        kind: VfsEventKind::Created,
                    }],
                    VfsOperation::Open,
                )?;
            }
            (Some(node), CreateDisposition::TruncateExisting) => {
                if node.kind == EntryKind::Directory {
                    return Err(self
                        .error(VfsErrorCode::IsDirectory, VfsOperation::Open)
                        .with_path(path.clone()));
                }
                if !options.access.can_write() {
                    return Err(self
                        .error(VfsErrorCode::InvalidArgument, VfsOperation::Open)
                        .with_path(path.clone())
                        .with_detail("truncating a file requires write access"));
                }
                self.check_expected_version(
                    path,
                    &node,
                    options.expected_version.as_ref(),
                    VfsOperation::Open,
                )?;
                let version = self.next_version(&mut state, VfsOperation::Open)?;
                let node = state.nodes.get_mut(path).ok_or_else(|| {
                    self.error(VfsErrorCode::NotFound, VfsOperation::Open)
                        .with_path(path.clone())
                })?;
                node.contents.clear();
                node.content_version = version;
                node.modified_at = SystemTime::now();
                self.emit_events(
                    &mut state,
                    vec![VfsEvent {
                        path: path.clone(),
                        kind: VfsEventKind::Modified,
                    }],
                    VfsOperation::Open,
                )?;
            }
            (Some(node), CreateDisposition::OpenExisting | CreateDisposition::OpenOrCreate) => {
                if node.kind == EntryKind::Directory {
                    return Err(self
                        .error(VfsErrorCode::IsDirectory, VfsOperation::Open)
                        .with_path(path.clone()));
                }
                self.check_expected_version(
                    path,
                    &node,
                    options.expected_version.as_ref(),
                    VfsOperation::Open,
                )?;
            }
        }
        Ok(Arc::new(MemoryFile {
            provider: self.clone(),
            path: path.clone(),
            access: options.access,
        }))
    }

    async fn create_dir(&self, path: &ProviderPath, options: CreateDirOptions) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::CreateDirectory, &self.descriptor.id)?;
        self.validate_path(path, VfsOperation::CreateDirectory)?;
        if path.is_root() {
            return Err(self
                .error(VfsErrorCode::AlreadyExists, VfsOperation::CreateDirectory)
                .with_path(path.clone()));
        }
        let mut state = self.state.lock();
        if state.nodes.contains_key(path) {
            return Err(self
                .error(VfsErrorCode::AlreadyExists, VfsOperation::CreateDirectory)
                .with_path(path.clone()));
        }

        let paths_to_create = match options.parents {
            CreateParents::No => {
                self.ensure_parent_directory(&state, path, VfsOperation::CreateDirectory)?;
                vec![path.clone()]
            }
            CreateParents::Yes => {
                let mut paths = Vec::new();
                let mut current = ProviderPath::root(path.encoding());
                for component in path.components().cloned() {
                    current = current.join_component(component).map_err(|error| {
                        self.error(VfsErrorCode::InvalidPath, VfsOperation::CreateDirectory)
                            .with_path(path.clone())
                            .with_detail(error.to_string())
                    })?;
                    if !state.nodes.contains_key(&current) {
                        paths.push(current.clone());
                    }
                }
                paths
            }
        };

        let mut events = Vec::new();
        for directory_path in paths_to_create {
            self.ensure_parent_directory(&state, &directory_path, VfsOperation::CreateDirectory)?;
            let node = self.create_node(
                &mut state,
                EntryKind::Directory,
                VfsOperation::CreateDirectory,
            )?;
            state.nodes.insert(directory_path.clone(), node);
            self.bump_parent_structure_version(
                &mut state,
                &directory_path,
                VfsOperation::CreateDirectory,
            )?;
            events.push(VfsEvent {
                path: directory_path,
                kind: VfsEventKind::Created,
            });
        }
        if !events.is_empty() {
            self.emit_events(&mut state, events, VfsOperation::CreateDirectory)?;
        }
        Ok(())
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
        self.validate_path(path, VfsOperation::Remove)?;
        if path.is_root() {
            return Err(self
                .error(VfsErrorCode::InvalidPath, VfsOperation::Remove)
                .with_path(path.clone()));
        }
        let mut state = self.state.lock();
        let node = state.nodes.get(path).cloned().ok_or_else(|| {
            self.error(VfsErrorCode::NotFound, VfsOperation::Remove)
                .with_path(path.clone())
        })?;
        self.check_expected_version(
            path,
            &node,
            options.expected_version.as_ref(),
            VfsOperation::Remove,
        )?;
        let has_children = state
            .nodes
            .keys()
            .any(|candidate| candidate != path && candidate.starts_with(path));
        match options.kind {
            RemoveKind::File if node.kind == EntryKind::Directory => {
                return Err(self
                    .error(VfsErrorCode::IsDirectory, VfsOperation::Remove)
                    .with_path(path.clone()));
            }
            RemoveKind::EmptyDirectory if node.kind != EntryKind::Directory => {
                return Err(self
                    .error(VfsErrorCode::NotDirectory, VfsOperation::Remove)
                    .with_path(path.clone()));
            }
            RemoveKind::EmptyDirectory if has_children => {
                return Err(self
                    .error(VfsErrorCode::Conflict, VfsOperation::Remove)
                    .with_path(path.clone())
                    .with_detail("directory is not empty"));
            }
            RemoveKind::File | RemoveKind::EmptyDirectory | RemoveKind::Recursive => {}
        }
        let removed_entries = Self::remove_subtree(&mut state, path);
        self.bump_parent_structure_version(&mut state, path, VfsOperation::Remove)?;
        self.emit_events(
            &mut state,
            vec![VfsEvent {
                path: path.clone(),
                kind: VfsEventKind::Removed,
            }],
            VfsOperation::Remove,
        )?;
        Ok(RemoveOutcome { removed_entries })
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
        self.validate_path(source, VfsOperation::Rename)?;
        self.validate_path(target, VfsOperation::Rename)?;
        if source.is_root() || target.is_root() || target.starts_with(source) {
            return Err(self
                .error(VfsErrorCode::InvalidPath, VfsOperation::Rename)
                .with_path(target.clone()));
        }
        let mut state = self.state.lock();
        let source_node = state.nodes.get(source).cloned().ok_or_else(|| {
            self.error(VfsErrorCode::NotFound, VfsOperation::Rename)
                .with_path(source.clone())
        })?;
        self.check_expected_version(
            source,
            &source_node,
            options.expected_version.as_ref(),
            VfsOperation::Rename,
        )?;
        self.ensure_parent_directory(&state, target, VfsOperation::Rename)?;
        if state.nodes.contains_key(target) {
            match options.collision {
                CollisionPolicy::Fail => {
                    return Err(self
                        .error(VfsErrorCode::AlreadyExists, VfsOperation::Rename)
                        .with_path(target.clone()));
                }
                CollisionPolicy::Replace => {
                    Self::remove_subtree(&mut state, target);
                }
            }
        }
        let moved_nodes = state
            .nodes
            .iter()
            .filter(|(candidate, _)| *candidate == source || candidate.starts_with(source))
            .map(|(path, node)| (path.clone(), node.clone()))
            .collect::<Vec<_>>();
        for (path, _) in &moved_nodes {
            state.nodes.remove(path);
        }
        for (path, node) in moved_nodes {
            let suffix = path.strip_prefix(source).map_err(|error| {
                self.error(VfsErrorCode::Internal, VfsOperation::Rename)
                    .with_detail(error.to_string())
            })?;
            let target_path = target.join(&suffix).map_err(|error| {
                self.error(VfsErrorCode::Internal, VfsOperation::Rename)
                    .with_detail(error.to_string())
            })?;
            state.nodes.insert(target_path, node);
        }
        self.bump_parent_structure_version(&mut state, source, VfsOperation::Rename)?;
        self.bump_parent_structure_version(&mut state, target, VfsOperation::Rename)?;
        self.emit_events(
            &mut state,
            vec![VfsEvent {
                path: target.clone(),
                kind: VfsEventKind::Renamed {
                    old_path: source.clone(),
                },
            }],
            VfsOperation::Rename,
        )?;
        Ok(())
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
        self.validate_path(source, VfsOperation::Copy)?;
        self.validate_path(target, VfsOperation::Copy)?;
        if source.is_root() || target.is_root() || target.starts_with(source) {
            return Err(self
                .error(VfsErrorCode::InvalidPath, VfsOperation::Copy)
                .with_path(target.clone()));
        }
        let mut state = self.state.lock();
        let source_node = state.nodes.get(source).cloned().ok_or_else(|| {
            self.error(VfsErrorCode::NotFound, VfsOperation::Copy)
                .with_path(source.clone())
        })?;
        self.check_expected_version(
            source,
            &source_node,
            options.expected_version.as_ref(),
            VfsOperation::Copy,
        )?;
        self.ensure_parent_directory(&state, target, VfsOperation::Copy)?;
        if state.nodes.contains_key(target) {
            match options.collision {
                CollisionPolicy::Fail => {
                    return Err(self
                        .error(VfsErrorCode::AlreadyExists, VfsOperation::Copy)
                        .with_path(target.clone()));
                }
                CollisionPolicy::Replace => {
                    Self::remove_subtree(&mut state, target);
                }
            }
        }
        let copied_nodes = state
            .nodes
            .iter()
            .filter(|(candidate, _)| *candidate == source || candidate.starts_with(source))
            .map(|(path, node)| (path.clone(), node.clone()))
            .collect::<Vec<_>>();
        for (path, mut node) in copied_nodes {
            let suffix = path.strip_prefix(source).map_err(|error| {
                self.error(VfsErrorCode::Internal, VfsOperation::Copy)
                    .with_detail(error.to_string())
            })?;
            let target_path = target.join(&suffix).map_err(|error| {
                self.error(VfsErrorCode::Internal, VfsOperation::Copy)
                    .with_detail(error.to_string())
            })?;
            node.file_key = self.next_file_key(&mut state, VfsOperation::Copy)?;
            let version = self.next_version(&mut state, VfsOperation::Copy)?;
            node.content_version = version.clone();
            node.structure_version = version;
            node.modified_at = SystemTime::now();
            state.nodes.insert(target_path, node);
        }
        self.bump_parent_structure_version(&mut state, target, VfsOperation::Copy)?;
        self.emit_events(
            &mut state,
            vec![VfsEvent {
                path: target.clone(),
                kind: VfsEventKind::Created,
            }],
            VfsOperation::Copy,
        )?;
        Ok(())
    }

    async fn watch(
        &self,
        request: WatchRequest,
    ) -> VfsResult<BoxStream<'static, VfsResult<EventBatch>>> {
        request
            .context
            .cancellation
            .check(VfsOperation::Watch, &self.descriptor.id)?;
        self.validate_path(&request.path, VfsOperation::Watch)?;
        let (sender, receiver) = mpsc::unbounded();
        let mut state = self.state.lock();
        let mut initial_batches = Vec::new();
        if let Some(resume_after_sequence) = request.resume_after_sequence {
            let oldest_sequence = state.journal.front().map_or_else(
                || state.sequence.saturating_add(1),
                |batch| batch.first_sequence,
            );
            if resume_after_sequence.saturating_add(1) < oldest_sequence {
                initial_batches.push(Ok(EventBatch {
                    first_sequence: state.sequence,
                    last_sequence: state.sequence,
                    events: vec![VfsEvent {
                        path: request.path.clone(),
                        kind: VfsEventKind::Overflow {
                            rescan_root: request.path.clone(),
                        },
                    }],
                }));
            } else {
                initial_batches.extend(
                    state
                        .journal
                        .iter()
                        .filter(|batch| batch.last_sequence > resume_after_sequence)
                        .filter(|batch| {
                            batch
                                .events
                                .iter()
                                .any(|event| watch_matches(&request.path, request.depth, event))
                        })
                        .cloned()
                        .map(Ok),
                );
            }
        }
        state.subscriptions.push(MemoryWatchSubscription {
            path: request.path,
            depth: request.depth,
            sender,
        });
        Ok(Box::pin(stream::iter(initial_batches).chain(receiver)))
    }
}

struct MemoryFile {
    provider: MemoryProvider,
    path: ProviderPath,
    access: FileAccess,
}

impl MemoryFile {
    fn check_read_access(&self) -> VfsResult<()> {
        if self.access.can_read() {
            Ok(())
        } else {
            Err(self
                .provider
                .error(VfsErrorCode::PermissionDenied, VfsOperation::Read)
                .with_path(self.path.clone()))
        }
    }

    fn check_write_access(&self, operation: VfsOperation) -> VfsResult<()> {
        if self.access.can_write() {
            Ok(())
        } else {
            Err(self
                .provider
                .error(VfsErrorCode::PermissionDenied, operation)
                .with_path(self.path.clone()))
        }
    }
}

#[async_trait]
impl VfsFile for MemoryFile {
    async fn len(&self, context: OperationContext) -> VfsResult<u64> {
        context
            .cancellation
            .check(VfsOperation::Stat, &self.provider.descriptor.id)?;
        let state = self.provider.state.lock();
        let node = state.nodes.get(&self.path).ok_or_else(|| {
            self.provider
                .error(VfsErrorCode::NotFound, VfsOperation::Stat)
                .with_path(self.path.clone())
        })?;
        Ok(node.contents.len() as u64)
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
        self.check_read_access()?;
        let state = self.provider.state.lock();
        let node = state.nodes.get(&self.path).ok_or_else(|| {
            self.provider
                .error(VfsErrorCode::NotFound, VfsOperation::Read)
                .with_path(self.path.clone())
        })?;
        let Ok(offset) = usize::try_from(offset) else {
            return Ok(0);
        };
        let Some(contents) = node.contents.get(offset..) else {
            return Ok(0);
        };
        let read_len = contents.len().min(buffer.len());
        if let (Some(target), Some(source)) = (buffer.get_mut(..read_len), contents.get(..read_len))
        {
            target.copy_from_slice(source);
        }
        Ok(read_len)
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
        self.check_write_access(VfsOperation::Write)?;
        let offset = usize::try_from(offset).map_err(|error| {
            self.provider
                .error(VfsErrorCode::TooLarge, VfsOperation::Write)
                .with_path(self.path.clone())
                .with_source(error)
        })?;
        let end = offset.checked_add(buffer.len()).ok_or_else(|| {
            self.provider
                .error(VfsErrorCode::TooLarge, VfsOperation::Write)
                .with_path(self.path.clone())
        })?;
        let mut state = self.provider.state.lock();
        let node = state.nodes.get(&self.path).cloned().ok_or_else(|| {
            self.provider
                .error(VfsErrorCode::NotFound, VfsOperation::Write)
                .with_path(self.path.clone())
        })?;
        self.provider.check_expected_version(
            &self.path,
            &node,
            options.expected_version.as_ref(),
            VfsOperation::Write,
        )?;
        let version = self
            .provider
            .next_version(&mut state, VfsOperation::Write)?;
        let node = state.nodes.get_mut(&self.path).ok_or_else(|| {
            self.provider
                .error(VfsErrorCode::NotFound, VfsOperation::Write)
                .with_path(self.path.clone())
        })?;
        if node.contents.len() < end {
            node.contents.resize(end, 0);
        }
        if let Some(target) = node.contents.get_mut(offset..end) {
            target.copy_from_slice(buffer);
        }
        node.content_version = version;
        node.modified_at = SystemTime::now();
        self.provider.emit_events(
            &mut state,
            vec![VfsEvent {
                path: self.path.clone(),
                kind: VfsEventKind::Modified,
            }],
            VfsOperation::Write,
        )?;
        Ok(buffer.len())
    }

    async fn set_len(&self, len: u64, options: WriteAtOptions) -> VfsResult<()> {
        options
            .context
            .cancellation
            .check(VfsOperation::SetLength, &self.provider.descriptor.id)?;
        self.check_write_access(VfsOperation::SetLength)?;
        let len = usize::try_from(len).map_err(|error| {
            self.provider
                .error(VfsErrorCode::TooLarge, VfsOperation::SetLength)
                .with_path(self.path.clone())
                .with_source(error)
        })?;
        let mut state = self.provider.state.lock();
        let node = state.nodes.get(&self.path).cloned().ok_or_else(|| {
            self.provider
                .error(VfsErrorCode::NotFound, VfsOperation::SetLength)
                .with_path(self.path.clone())
        })?;
        self.provider.check_expected_version(
            &self.path,
            &node,
            options.expected_version.as_ref(),
            VfsOperation::SetLength,
        )?;
        let version = self
            .provider
            .next_version(&mut state, VfsOperation::SetLength)?;
        let node = state.nodes.get_mut(&self.path).ok_or_else(|| {
            self.provider
                .error(VfsErrorCode::NotFound, VfsOperation::SetLength)
                .with_path(self.path.clone())
        })?;
        node.contents.resize(len, 0);
        node.content_version = version;
        node.modified_at = SystemTime::now();
        self.provider.emit_events(
            &mut state,
            vec![VfsEvent {
                path: self.path.clone(),
                kind: VfsEventKind::Modified,
            }],
            VfsOperation::SetLength,
        )?;
        Ok(())
    }

    async fn flush(&self, context: OperationContext) -> VfsResult<()> {
        context
            .cancellation
            .check(VfsOperation::Flush, &self.provider.descriptor.id)?;
        self.check_write_access(VfsOperation::Flush)
    }

    async fn sync(&self, context: OperationContext) -> VfsResult<()> {
        context
            .cancellation
            .check(VfsOperation::Sync, &self.provider.descriptor.id)?;
        self.check_write_access(VfsOperation::Sync)
    }
}

fn version_from_u64(version: u64) -> VfsVersion {
    VfsVersion::new(version.to_be_bytes().to_vec())
}

fn encode_cursor(index: usize) -> DirCursor {
    DirCursor::new((index as u64).to_be_bytes().to_vec())
}

fn decode_cursor(cursor: Option<&DirCursor>) -> Result<usize, &'static str> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    let Ok(bytes) = <[u8; 8]>::try_from(cursor.as_bytes()) else {
        return Err("directory cursor has invalid length");
    };
    usize::try_from(u64::from_be_bytes(bytes)).map_err(|_| "directory cursor exceeds host size")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::run_provider_conformance;
    use futures::executor::block_on;

    #[test]
    fn memory_provider_passes_shared_conformance() {
        let provider: Arc<dyn VfsProvider> = Arc::new(MemoryProvider::new(
            "memory-conformance",
            PathEncoding::PortableUtf8,
        ));
        let result = block_on(run_provider_conformance(provider));
        assert!(
            result.is_ok(),
            "memory provider conformance failed: {result:?}"
        );
    }

    #[test]
    fn memory_watch_resume_reports_overflow_after_journal_truncation() {
        let provider =
            MemoryProvider::with_journal_capacity("memory-overflow", PathEncoding::PortableUtf8, 2);
        let result = block_on(async {
            for name in [b"one".as_slice(), b"two".as_slice(), b"three".as_slice()] {
                let path = ProviderPath::from_byte_components(PathEncoding::PortableUtf8, [name])
                    .map_err(|error| error.to_string())?;
                provider
                    .create_dir(&path, CreateDirOptions::default())
                    .await
                    .map_err(|error| error.to_string())?;
            }
            let mut watch = provider
                .watch(WatchRequest {
                    path: ProviderPath::root(PathEncoding::PortableUtf8),
                    depth: WatchDepth::Recursive,
                    resume_after_sequence: Some(0),
                    context: OperationContext::default(),
                })
                .await
                .map_err(|error| error.to_string())?;
            let batch = watch
                .next()
                .await
                .ok_or_else(|| String::from("watch ended before overflow"))?
                .map_err(|error| error.to_string())?;
            if batch
                .events
                .iter()
                .any(|event| matches!(event.kind, VfsEventKind::Overflow { .. }))
            {
                Ok(())
            } else {
                Err(String::from("watch did not report overflow"))
            }
        });
        assert!(
            result.is_ok(),
            "overflow recovery contract failed: {result:?}"
        );
    }
}
