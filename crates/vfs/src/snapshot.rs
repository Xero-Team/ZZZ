use crate::{
    CreateDisposition, DirEntry, DirPageRequest, EntryKind, EntryMetadata, EventBatch, FileAccess,
    LookupKey, MountId, OpenOptions, OperationContext, ProviderFileKey, ProviderPath, ResourceId,
    StatOptions, VfsError, VfsErrorCode, VfsEventKind, VfsOperation, VfsProvider, VfsResult,
    VfsVersion,
};
use async_lock::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use parking_lot::Mutex;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceFreshness {
    Loading,
    Fresh,
    Stale,
    Tombstone,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectoryLoadState {
    Unloaded,
    Loading,
    Loaded,
    Stale,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SnapshotRescanState {
    #[default]
    Idle,
    Running,
    Required,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotBudgets {
    pub metadata_bytes: usize,
    pub directory_bytes: usize,
    pub range_bytes: usize,
}

impl Default for SnapshotBudgets {
    fn default() -> Self {
        Self {
            metadata_bytes: 48 * 1_024 * 1_024,
            directory_bytes: 80 * 1_024 * 1_024,
            range_bytes: 128 * 1_024 * 1_024,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResourceRecord {
    pub id: ResourceId,
    pub path: ProviderPath,
    pub provider_file_key: Option<ProviderFileKey>,
    pub metadata: Option<EntryMetadata>,
    pub freshness: ResourceFreshness,
    pub directory_state: Option<DirectoryLoadState>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResourceChangeKind {
    Created,
    Modified,
    Removed,
    Renamed { old_path: ProviderPath },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceChange {
    pub resource_id: ResourceId,
    pub path: ProviderPath,
    pub kind: ResourceChangeKind,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SnapshotDelta {
    pub changes: Vec<ResourceChange>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SnapshotStats {
    pub identity_count: usize,
    pub metadata_count: usize,
    pub loaded_directory_count: usize,
    pub estimated_identity_bytes: usize,
    pub estimated_metadata_bytes: usize,
    pub estimated_directory_bytes: usize,
    pub cached_range_count: usize,
    pub cached_range_bytes: usize,
    pub last_sequence: u64,
    pub rescan_state: SnapshotRescanState,
}

#[derive(Clone)]
pub struct ResourceRegistry {
    inner: Arc<Mutex<ResourceRegistryState>>,
}

struct ResourceRegistryState {
    mount_id: MountId,
    root_path: ProviderPath,
    next_node_id: u64,
    records: BTreeMap<ResourceId, RegistryRecord>,
    ids_by_path: BTreeMap<ProviderPath, ResourceId>,
    ids_by_file_key: BTreeMap<ProviderFileKey, BTreeSet<ResourceId>>,
    access_clock: u64,
    last_sequence: u64,
    rescan_state: SnapshotRescanState,
    budgets: SnapshotBudgets,
}

struct RegistryRecord {
    path: ProviderPath,
    provider_file_key: Option<ProviderFileKey>,
    metadata: Option<EntryMetadata>,
    freshness: ResourceFreshness,
    directory_state: Option<DirectoryLoadState>,
    children: Option<BTreeSet<ResourceId>>,
    last_access: u64,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct RangeCacheKey {
    resource_id: ResourceId,
    source_version: VfsVersion,
    offset: u64,
    length: usize,
}

struct RangeCacheEntry {
    bytes: Arc<[u8]>,
    last_access: u64,
}

struct RangeCacheState {
    entries: BTreeMap<RangeCacheKey, RangeCacheEntry>,
    bytes: usize,
    access_clock: u64,
    budget: usize,
}

#[derive(Clone)]
pub struct VfsSnapshot {
    provider: Arc<dyn VfsProvider>,
    registry: ResourceRegistry,
    directory_load_locks: Arc<Mutex<BTreeMap<ProviderPath, Weak<AsyncMutex<()>>>>>,
    event_reconciliation_lock: Arc<AsyncRwLock<()>>,
    range_cache: Arc<Mutex<RangeCacheState>>,
}

impl ResourceRegistry {
    pub fn new(mount_id: MountId, root_path: ProviderPath, budgets: SnapshotBudgets) -> Self {
        let root_id = ResourceId::new(mount_id, 1, 0);
        let root_record = RegistryRecord {
            path: root_path.clone(),
            provider_file_key: None,
            metadata: None,
            freshness: ResourceFreshness::Stale,
            directory_state: Some(DirectoryLoadState::Unloaded),
            children: None,
            last_access: 1,
        };
        Self {
            inner: Arc::new(Mutex::new(ResourceRegistryState {
                mount_id,
                root_path: root_path.clone(),
                next_node_id: 2,
                records: BTreeMap::from([(root_id, root_record)]),
                ids_by_path: BTreeMap::from([(root_path, root_id)]),
                ids_by_file_key: BTreeMap::new(),
                access_clock: 1,
                last_sequence: 0,
                rescan_state: SnapshotRescanState::Idle,
                budgets,
            })),
        }
    }

    pub fn mount_id(&self) -> MountId {
        self.inner.lock().mount_id
    }

    pub fn id_for_path(&self, path: &ProviderPath) -> Option<ResourceId> {
        let mut state = self.inner.lock();
        let id = state.ids_by_path.get(path).copied()?;
        touch_record(&mut state, id);
        Some(id)
    }

    pub fn record(&self, id: ResourceId) -> Option<ResourceRecord> {
        let mut state = self.inner.lock();
        touch_record(&mut state, id);
        let record = state.records.get(&id)?;
        Some(ResourceRecord {
            id,
            path: record.path.clone(),
            provider_file_key: record.provider_file_key.clone(),
            metadata: record.metadata.clone(),
            freshness: record.freshness,
            directory_state: record.directory_state,
        })
    }

    pub fn stats(&self) -> SnapshotStats {
        let state = self.inner.lock();
        snapshot_stats(&state)
    }

    pub fn children(&self, path: &ProviderPath) -> Vec<ResourceRecord> {
        let mut state = self.inner.lock();
        let Some(directory_id) = state.ids_by_path.get(path).copied() else {
            return Vec::new();
        };
        let child_ids = state
            .records
            .get(&directory_id)
            .and_then(|record| record.children.clone())
            .unwrap_or_default();
        child_ids
            .into_iter()
            .filter_map(|id| {
                let access_clock = next_access_clock(&mut state);
                let record = state.records.get_mut(&id)?;
                record.last_access = access_clock;
                Some(ResourceRecord {
                    id,
                    path: record.path.clone(),
                    provider_file_key: record.provider_file_key.clone(),
                    metadata: record.metadata.clone(),
                    freshness: record.freshness,
                    directory_state: record.directory_state,
                })
            })
            .collect()
    }

    fn root_path(&self) -> ProviderPath {
        self.inner.lock().root_path.clone()
    }

    fn mark_directory_loading(&self, path: &ProviderPath) -> VfsResult<()> {
        let mut state = self.inner.lock();
        let id = state.ids_by_path.get(path).copied().ok_or_else(|| {
            registry_error(
                &state,
                VfsErrorCode::NotFound,
                VfsOperation::ReadDirectory,
                Some(path.clone()),
            )
        })?;
        let Some(record) = state.records.get_mut(&id) else {
            return Err(registry_error(
                &state,
                VfsErrorCode::Internal,
                VfsOperation::ReadDirectory,
                Some(path.clone()),
            ));
        };
        record.directory_state = Some(DirectoryLoadState::Loading);
        record.freshness = ResourceFreshness::Loading;
        Ok(())
    }

    fn mark_directory_stale(&self, path: &ProviderPath) {
        let mut state = self.inner.lock();
        let Some(id) = state.ids_by_path.get(path).copied() else {
            return;
        };
        if let Some(record) = state.records.get_mut(&id) {
            record.freshness = ResourceFreshness::Stale;
            record.directory_state = record.directory_state.map(|_| DirectoryLoadState::Stale);
        }
    }

    fn intern_metadata(
        &self,
        path: ProviderPath,
        metadata: EntryMetadata,
    ) -> VfsResult<(ResourceId, Option<ResourceChange>)> {
        let mut state = self.inner.lock();
        let result = intern_record(&mut state, path, metadata, None)?;
        enforce_budgets(&mut state);
        Ok(result)
    }

    fn reconcile_directory(
        &self,
        directory_path: &ProviderPath,
        directory_metadata: EntryMetadata,
        mut entries: Vec<DirEntry>,
    ) -> VfsResult<SnapshotDelta> {
        let mut state = self.inner.lock();
        validate_directory_entries(&state, directory_path, &entries)?;
        let (directory_id, directory_change) =
            intern_record(&mut state, directory_path.clone(), directory_metadata, None)?;
        let existing_children = state
            .records
            .get(&directory_id)
            .and_then(|record| record.children.clone())
            .unwrap_or_default();
        entries.sort_by(|left, right| {
            let left_has_existing_path = state.ids_by_path.contains_key(&left.path);
            let right_has_existing_path = state.ids_by_path.contains_key(&right.path);
            right_has_existing_path
                .cmp(&left_has_existing_path)
                .then(left.path.cmp(&right.path))
        });
        let mut available_existing_children = existing_children.clone();
        let mut current_children = BTreeSet::new();
        let mut changes = Vec::new();
        if let Some(change) = directory_change {
            changes.push(change);
        }

        for entry in entries {
            let (resource_id, change) = intern_record(
                &mut state,
                entry.path,
                entry.metadata,
                Some(&available_existing_children),
            )?;
            available_existing_children.remove(&resource_id);
            current_children.insert(resource_id);
            if let Some(change) = change {
                changes.push(change);
            }
        }

        for removed_id in existing_children.difference(&current_children).copied() {
            let removed_identity = state
                .records
                .get(&removed_id)
                .map(|record| record.path.clone());
            if let Some(path) = removed_identity {
                if state.ids_by_path.get(&path) == Some(&removed_id) {
                    state.ids_by_path.remove(&path);
                }
            }
            if let Some(record) = state.records.get_mut(&removed_id) {
                record.freshness = ResourceFreshness::Tombstone;
                record.metadata = None;
                record.directory_state = record.directory_state.map(|_| DirectoryLoadState::Stale);
                record.children = None;
                changes.push(ResourceChange {
                    resource_id: removed_id,
                    path: record.path.clone(),
                    kind: ResourceChangeKind::Removed,
                });
            }
        }

        let access_clock = next_access_clock(&mut state);
        let missing_directory_error = registry_error(
            &state,
            VfsErrorCode::Internal,
            VfsOperation::ReadDirectory,
            Some(directory_path.clone()),
        );
        let Some(directory_record) = state.records.get_mut(&directory_id) else {
            return Err(missing_directory_error);
        };
        directory_record.children = Some(current_children);
        directory_record.directory_state = Some(DirectoryLoadState::Loaded);
        directory_record.freshness = ResourceFreshness::Fresh;
        directory_record.last_access = access_clock;

        changes.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then(left.resource_id.cmp(&right.resource_id))
        });
        enforce_budgets(&mut state);
        Ok(SnapshotDelta { changes })
    }

    fn last_sequence(&self) -> u64 {
        self.inner.lock().last_sequence
    }

    fn set_rescan_state(&self, rescan_state: SnapshotRescanState) {
        self.inner.lock().rescan_state = rescan_state;
    }

    fn loaded_directory_paths_under(&self, root: &ProviderPath) -> Vec<ProviderPath> {
        let state = self.inner.lock();
        let mut paths = state
            .records
            .values()
            .filter(|record| record.path.starts_with(root) && record.children.is_some())
            .map(|record| record.path.clone())
            .collect::<Vec<_>>();
        if !paths.iter().any(|path| path == root) {
            paths.push(root.clone());
        }
        paths.sort_by(|left, right| {
            left.component_count()
                .cmp(&right.component_count())
                .then(left.cmp(right))
        });
        paths
    }

    fn prune_tombstone_file_keys(&self) {
        let mut state = self.inner.lock();
        let tombstones = state
            .records
            .iter()
            .filter(|(_, record)| record.freshness == ResourceFreshness::Tombstone)
            .filter_map(|(resource_id, record)| {
                record
                    .provider_file_key
                    .clone()
                    .map(|file_key| (file_key, *resource_id))
            })
            .collect::<Vec<_>>();
        for (file_key, resource_id) in tombstones {
            remove_file_key_mapping(&mut state, &file_key, resource_id);
        }
    }

    fn complete_rescan(&self, sequence: u64) {
        let mut state = self.inner.lock();
        state.last_sequence = sequence;
        state.rescan_state = SnapshotRescanState::Idle;
    }
}

impl RangeCacheState {
    fn get(&mut self, key: &RangeCacheKey) -> Option<Arc<[u8]>> {
        self.access_clock = self.access_clock.saturating_add(1);
        let entry = self.entries.get_mut(key)?;
        entry.last_access = self.access_clock;
        Some(entry.bytes.clone())
    }

    fn insert(&mut self, key: RangeCacheKey, bytes: Arc<[u8]>) {
        self.access_clock = self.access_clock.saturating_add(1);
        let inserted_bytes = bytes.len();
        let entry = RangeCacheEntry {
            bytes,
            last_access: self.access_clock,
        };
        if let Some(previous_entry) = self.entries.insert(key, entry) {
            self.bytes = self.bytes.saturating_sub(previous_entry.bytes.len());
        }
        self.bytes = self.bytes.saturating_add(inserted_bytes);
        self.enforce_budget();
    }

    fn enforce_budget(&mut self) {
        if self.bytes > self.budget {
            let mut candidates = self
                .entries
                .iter()
                .map(|(key, entry)| (entry.last_access, key.clone()))
                .collect::<Vec<_>>();
            candidates.sort_unstable();
            for (_, key) in candidates {
                if self.bytes <= self.budget {
                    break;
                }
                if let Some(removed) = self.entries.remove(&key) {
                    self.bytes = self.bytes.saturating_sub(removed.bytes.len());
                }
            }
        }
    }
}

impl VfsSnapshot {
    pub fn mount(provider: Arc<dyn VfsProvider>, budgets: SnapshotBudgets) -> VfsResult<Self> {
        static NEXT_MOUNT_ID: AtomicU64 = AtomicU64::new(1);
        let mount_id = NEXT_MOUNT_ID
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |mount_id| {
                mount_id.checked_add(1)
            })
            .map(MountId::new)
            .map_err(|_| {
                VfsError::new(
                    VfsErrorCode::Internal,
                    VfsOperation::Stat,
                    provider.descriptor().id.clone(),
                )
                .with_detail("mount ID space exhausted")
            })?;
        Ok(Self::new(mount_id, provider, budgets))
    }

    pub fn new(
        mount_id: MountId,
        provider: Arc<dyn VfsProvider>,
        budgets: SnapshotBudgets,
    ) -> Self {
        let root_path = ProviderPath::root(provider.descriptor().path_encoding);
        Self {
            provider,
            registry: ResourceRegistry::new(mount_id, root_path, budgets),
            directory_load_locks: Arc::new(Mutex::new(BTreeMap::new())),
            event_reconciliation_lock: Arc::new(AsyncRwLock::new(())),
            range_cache: Arc::new(Mutex::new(RangeCacheState {
                entries: BTreeMap::new(),
                bytes: 0,
                access_clock: 0,
                budget: budgets.range_bytes,
            })),
        }
    }

    pub fn provider(&self) -> &Arc<dyn VfsProvider> {
        &self.provider
    }

    pub fn registry(&self) -> &ResourceRegistry {
        &self.registry
    }

    pub fn stats(&self) -> SnapshotStats {
        let mut stats = self.registry.stats();
        let range_cache = self.range_cache.lock();
        stats.cached_range_count = range_cache.entries.len();
        stats.cached_range_bytes = range_cache.bytes;
        stats
    }

    pub async fn stat(&self, path: &ProviderPath) -> VfsResult<ResourceId> {
        let metadata = self.provider.stat(path, StatOptions::default()).await?;
        let (resource_id, _) = self.registry.intern_metadata(path.clone(), metadata)?;
        Ok(resource_id)
    }

    pub async fn read_range(
        &self,
        path: &ProviderPath,
        offset: u64,
        length: usize,
        context: OperationContext,
    ) -> VfsResult<Arc<[u8]>> {
        context
            .cancellation
            .check(VfsOperation::Read, &self.provider.descriptor().id)?;
        let length_u64 = u64::try_from(length).map_err(|error| {
            self.provider_error(VfsErrorCode::TooLarge, VfsOperation::Read, path)
                .with_source(error)
        })?;
        if length_u64 > self.provider.capabilities().limits.maximum_range_size.get() {
            return Err(self.provider_error(VfsErrorCode::TooLarge, VfsOperation::Read, path));
        }
        if length == 0 {
            return Ok(Arc::from([]));
        }

        let metadata = self
            .provider
            .stat(
                path,
                StatOptions {
                    context: context.clone(),
                    ..StatOptions::default()
                },
            )
            .await?;
        if metadata_is_directory(&metadata) {
            return Err(self.provider_error(VfsErrorCode::IsDirectory, VfsOperation::Read, path));
        }
        let source_version = metadata.content_version.clone();
        let (resource_id, _) = self.registry.intern_metadata(path.clone(), metadata)?;
        let cache_key = RangeCacheKey {
            resource_id,
            source_version: source_version.clone(),
            offset,
            length,
        };
        if let Some(bytes) = self.range_cache.lock().get(&cache_key) {
            return Ok(bytes);
        }

        let file = self
            .provider
            .open(
                path,
                OpenOptions {
                    access: FileAccess::Read,
                    create: CreateDisposition::OpenExisting,
                    expected_version: None,
                    context: context.clone(),
                },
            )
            .await?;
        let mut bytes = vec![0; length];
        let mut total_read = 0;
        while total_read < bytes.len() {
            let read_offset = offset
                .checked_add(u64::try_from(total_read).map_err(|error| {
                    self.provider_error(VfsErrorCode::TooLarge, VfsOperation::Read, path)
                        .with_source(error)
                })?)
                .ok_or_else(|| {
                    self.provider_error(VfsErrorCode::TooLarge, VfsOperation::Read, path)
                })?;
            let read = file
                .read_at(read_offset, &mut bytes[total_read..], context.clone())
                .await?;
            if read == 0 {
                break;
            }
            total_read = total_read.checked_add(read).ok_or_else(|| {
                self.provider_error(VfsErrorCode::TooLarge, VfsOperation::Read, path)
            })?;
            if total_read > bytes.len() {
                return Err(self
                    .provider_error(VfsErrorCode::CorruptData, VfsOperation::Read, path)
                    .with_detail("provider returned more bytes than requested"));
            }
        }
        bytes.truncate(total_read);

        let current_metadata = self
            .provider
            .stat(
                path,
                StatOptions {
                    context,
                    ..StatOptions::default()
                },
            )
            .await?;
        if current_metadata.content_version != source_version {
            return Err(self.provider_error(VfsErrorCode::StaleVersion, VfsOperation::Read, path));
        }
        self.registry
            .intern_metadata(path.clone(), current_metadata)?;
        let bytes: Arc<[u8]> = bytes.into();
        self.range_cache.lock().insert(cache_key, bytes.clone());
        Ok(bytes)
    }

    fn provider_error(
        &self,
        code: VfsErrorCode,
        operation: VfsOperation,
        path: &ProviderPath,
    ) -> VfsError {
        VfsError::new(code, operation, self.provider.descriptor().id.clone())
            .with_path(path.clone())
    }

    pub async fn load_directory(&self, path: &ProviderPath) -> VfsResult<SnapshotDelta> {
        let _event_reconciliation_guard = self.event_reconciliation_lock.read().await;
        let result = self.load_directory_for_reconciliation(path).await;
        self.registry.prune_tombstone_file_keys();
        result
    }

    async fn load_directory_for_reconciliation(
        &self,
        path: &ProviderPath,
    ) -> VfsResult<SnapshotDelta> {
        let directory_load_lock = self.directory_load_lock(path);
        let _directory_load_guard = directory_load_lock.lock().await;
        if self.registry.id_for_path(path).is_none() {
            self.stat(path).await?;
        }
        self.registry.mark_directory_loading(path)?;
        let directory_metadata = match self.provider.stat(path, StatOptions::default()).await {
            Ok(metadata) => metadata,
            Err(error) => {
                self.registry.mark_directory_stale(path);
                return Err(error);
            }
        };
        if !metadata_is_directory(&directory_metadata) {
            self.registry.mark_directory_stale(path);
            return Err(VfsError::new(
                VfsErrorCode::NotDirectory,
                VfsOperation::ReadDirectory,
                self.provider.descriptor().id.clone(),
            )
            .with_path(path.clone()));
        }

        let mut cursor = None;
        let mut entries = Vec::new();
        loop {
            let page = match self
                .provider
                .read_dir(
                    path,
                    DirPageRequest {
                        cursor,
                        limit: self.provider.capabilities().limits.maximum_page_size,
                        context: OperationContext::default(),
                    },
                )
                .await
            {
                Ok(page) => page,
                Err(error) => {
                    self.registry.mark_directory_stale(path);
                    return Err(error);
                }
            };
            entries.extend(page.entries);
            let Some(next_cursor) = page.next_cursor else {
                break;
            };
            cursor = Some(next_cursor);
        }
        match self
            .registry
            .reconcile_directory(path, directory_metadata, entries)
        {
            Ok(delta) => Ok(delta),
            Err(error) => {
                self.registry.mark_directory_stale(path);
                Err(error)
            }
        }
    }

    fn directory_load_lock(&self, path: &ProviderPath) -> Arc<AsyncMutex<()>> {
        let mut directory_load_locks = self.directory_load_locks.lock();
        directory_load_locks.retain(|_, load_lock| load_lock.strong_count() > 0);
        if let Some(load_lock) = directory_load_locks.get(path).and_then(Weak::upgrade) {
            return load_lock;
        }
        let load_lock = Arc::new(AsyncMutex::new(()));
        directory_load_locks.insert(path.clone(), Arc::downgrade(&load_lock));
        load_lock
    }

    pub async fn apply_event_batch(&self, batch: EventBatch) -> VfsResult<SnapshotDelta> {
        let _event_reconciliation_guard = self.event_reconciliation_lock.write().await;
        let last_sequence = self.registry.last_sequence();
        if batch.last_sequence <= last_sequence {
            return Ok(SnapshotDelta::default());
        }
        let rescan_roots = self.rescan_paths_for_batch(&batch, last_sequence);
        if !rescan_roots.is_empty() {
            self.registry.set_rescan_state(SnapshotRescanState::Running);
        }
        let mut delta = SnapshotDelta::default();
        for rescan_root in rescan_roots {
            if self.registry.id_for_path(&rescan_root).is_none() {
                continue;
            }
            let rescan_delta = match self.load_directory_for_reconciliation(&rescan_root).await {
                Ok(delta) => delta,
                Err(error) => {
                    self.registry.prune_tombstone_file_keys();
                    self.registry
                        .set_rescan_state(SnapshotRescanState::Required);
                    return Err(error);
                }
            };
            delta.changes.extend(rescan_delta.changes);
        }
        coalesce_delta_changes(&mut delta);
        self.registry.prune_tombstone_file_keys();
        self.registry.complete_rescan(batch.last_sequence);
        Ok(delta)
    }

    fn rescan_paths_for_batch(&self, batch: &EventBatch, last_sequence: u64) -> Vec<ProviderPath> {
        let mut rescan_roots = Vec::new();
        let mut seen_rescan_roots = BTreeSet::new();
        for event in &batch.events {
            match &event.kind {
                VfsEventKind::Overflow { rescan_root } => {
                    for path in self.registry.loaded_directory_paths_under(rescan_root) {
                        push_unique_path(&mut rescan_roots, &mut seen_rescan_roots, path);
                    }
                }
                VfsEventKind::Renamed { old_path } => {
                    if let Some(parent) = old_path.parent() {
                        push_unique_path(&mut rescan_roots, &mut seen_rescan_roots, parent);
                    }
                    if let Some(parent) = event.path.parent() {
                        push_unique_path(&mut rescan_roots, &mut seen_rescan_roots, parent);
                    }
                }
                VfsEventKind::Created | VfsEventKind::Modified | VfsEventKind::Removed => {
                    if let Some(parent) = event.path.parent() {
                        push_unique_path(&mut rescan_roots, &mut seen_rescan_roots, parent);
                    } else {
                        push_unique_path(
                            &mut rescan_roots,
                            &mut seen_rescan_roots,
                            event.path.clone(),
                        );
                    }
                }
            }
        }
        if batch.first_sequence > last_sequence.saturating_add(1) {
            rescan_roots.clear();
            seen_rescan_roots.clear();
            for path in self
                .registry
                .loaded_directory_paths_under(&self.registry.root_path())
            {
                push_unique_path(&mut rescan_roots, &mut seen_rescan_roots, path);
            }
        }
        rescan_roots
    }
}

fn coalesce_delta_changes(delta: &mut SnapshotDelta) {
    let renamed_resource_ids = delta
        .changes
        .iter()
        .filter_map(|change| {
            matches!(&change.kind, ResourceChangeKind::Renamed { .. }).then_some(change.resource_id)
        })
        .collect::<BTreeSet<_>>();
    delta.changes.retain(|change| {
        !matches!(&change.kind, ResourceChangeKind::Removed)
            || !renamed_resource_ids.contains(&change.resource_id)
    });
    delta.changes.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.resource_id.cmp(&right.resource_id))
    });
    delta.changes.dedup();
}

fn validate_directory_entries(
    state: &ResourceRegistryState,
    directory_path: &ProviderPath,
    entries: &[DirEntry],
) -> VfsResult<()> {
    let mut lookup_keys = BTreeSet::<LookupKey>::new();
    for entry in entries {
        let Some(name) = entry.metadata.name.as_ref() else {
            return Err(registry_error(
                state,
                VfsErrorCode::CorruptData,
                VfsOperation::ReadDirectory,
                Some(entry.path.clone()),
            )
            .with_detail("directory entry metadata is missing its name"));
        };
        if entry.path.parent().as_ref() != Some(directory_path)
            || entry.path.file_name() != Some(name.exact())
        {
            return Err(registry_error(
                state,
                VfsErrorCode::CorruptData,
                VfsOperation::ReadDirectory,
                Some(entry.path.clone()),
            )
            .with_detail("directory entry path does not match its metadata name"));
        }
        if !lookup_keys.insert(name.lookup_key().clone()) {
            return Err(registry_error(
                state,
                VfsErrorCode::Conflict,
                VfsOperation::ReadDirectory,
                Some(directory_path.clone()),
            )
            .with_detail("directory contains colliding provider lookup keys"));
        }
    }
    Ok(())
}

fn push_unique_path(
    paths: &mut Vec<ProviderPath>,
    seen_paths: &mut BTreeSet<ProviderPath>,
    path: ProviderPath,
) {
    if seen_paths.insert(path.clone()) {
        paths.push(path);
    }
}

fn intern_record(
    state: &mut ResourceRegistryState,
    path: ProviderPath,
    metadata: EntryMetadata,
    eligible_file_key_ids: Option<&BTreeSet<ResourceId>>,
) -> VfsResult<(ResourceId, Option<ResourceChange>)> {
    let file_key = metadata.provider_file_key.clone();
    let resource_id_by_path = state.ids_by_path.get(&path).copied().filter(|resource_id| {
        let existing_file_key = state
            .records
            .get(resource_id)
            .and_then(|record| record.provider_file_key.as_ref());
        file_key.is_none() || existing_file_key.is_none() || existing_file_key == file_key.as_ref()
    });
    let resource_id_by_file_key = file_key.as_ref().and_then(|file_key| {
        state
            .ids_by_file_key
            .get(file_key)?
            .iter()
            .copied()
            .find(|resource_id| {
                eligible_file_key_ids.is_some_and(|eligible| eligible.contains(resource_id))
                    || state.records.get(resource_id).is_some_and(|record| {
                        record.freshness == ResourceFreshness::Tombstone || record.path == path
                    })
            })
    });
    let resource_id = match resource_id_by_path.or(resource_id_by_file_key) {
        Some(resource_id) => resource_id,
        None => allocate_resource_id(state)?,
    };
    let access_clock = next_access_clock(state);

    if let Some(record) = state.records.get_mut(&resource_id) {
        let old_path = record.path.clone();
        let was_tombstone = record.freshness == ResourceFreshness::Tombstone;
        let metadata_changed = record
            .metadata
            .as_ref()
            .is_none_or(|old_metadata| metadata_changed(old_metadata, &metadata));
        if old_path != path {
            state.ids_by_path.remove(&old_path);
        }
        let old_file_key = (record.provider_file_key != file_key)
            .then(|| record.provider_file_key.clone())
            .flatten();
        record.path = path.clone();
        record.provider_file_key = file_key.clone();
        record.metadata = Some(metadata.clone());
        record.freshness = ResourceFreshness::Fresh;
        record.directory_state = if metadata_is_directory(&metadata) {
            record
                .directory_state
                .or(Some(DirectoryLoadState::Unloaded))
        } else {
            None
        };
        record.last_access = access_clock;
        if let Some(old_file_key) = old_file_key {
            remove_file_key_mapping(state, &old_file_key, resource_id);
        }
        state.ids_by_path.insert(path.clone(), resource_id);
        if let Some(file_key) = file_key {
            insert_file_key_mapping(state, file_key, resource_id);
        }
        let change = if old_path != path {
            Some(ResourceChange {
                resource_id,
                path,
                kind: ResourceChangeKind::Renamed { old_path },
            })
        } else if was_tombstone {
            Some(ResourceChange {
                resource_id,
                path,
                kind: ResourceChangeKind::Created,
            })
        } else if metadata_changed {
            Some(ResourceChange {
                resource_id,
                path,
                kind: ResourceChangeKind::Modified,
            })
        } else {
            None
        };
        return Ok((resource_id, change));
    }

    let directory_state = metadata_is_directory(&metadata).then_some(DirectoryLoadState::Unloaded);
    state.records.insert(
        resource_id,
        RegistryRecord {
            path: path.clone(),
            provider_file_key: file_key.clone(),
            metadata: Some(metadata),
            freshness: ResourceFreshness::Fresh,
            directory_state,
            children: None,
            last_access: access_clock,
        },
    );
    state.ids_by_path.insert(path.clone(), resource_id);
    if let Some(file_key) = file_key {
        insert_file_key_mapping(state, file_key, resource_id);
    }
    Ok((
        resource_id,
        Some(ResourceChange {
            resource_id,
            path,
            kind: ResourceChangeKind::Created,
        }),
    ))
}

fn insert_file_key_mapping(
    state: &mut ResourceRegistryState,
    file_key: ProviderFileKey,
    resource_id: ResourceId,
) {
    state
        .ids_by_file_key
        .entry(file_key)
        .or_default()
        .insert(resource_id);
}

fn remove_file_key_mapping(
    state: &mut ResourceRegistryState,
    file_key: &ProviderFileKey,
    resource_id: ResourceId,
) {
    let remove_key = state
        .ids_by_file_key
        .get_mut(file_key)
        .is_some_and(|resource_ids| {
            resource_ids.remove(&resource_id);
            resource_ids.is_empty()
        });
    if remove_key {
        state.ids_by_file_key.remove(file_key);
    }
}

fn allocate_resource_id(state: &mut ResourceRegistryState) -> VfsResult<ResourceId> {
    let resource_id = ResourceId::new(state.mount_id, state.next_node_id, 0);
    state.next_node_id = state.next_node_id.checked_add(1).ok_or_else(|| {
        registry_error(state, VfsErrorCode::Internal, VfsOperation::Stat, None)
            .with_detail("resource ID space exhausted")
    })?;
    Ok(resource_id)
}

fn next_access_clock(state: &mut ResourceRegistryState) -> u64 {
    state.access_clock = state.access_clock.saturating_add(1);
    state.access_clock
}

fn touch_record(state: &mut ResourceRegistryState, id: ResourceId) {
    let access_clock = next_access_clock(state);
    if let Some(record) = state.records.get_mut(&id) {
        record.last_access = access_clock;
    }
}

fn metadata_changed(left: &EntryMetadata, right: &EntryMetadata) -> bool {
    left.kind != right.kind
        || left.size != right.size
        || left.content_version != right.content_version
        || left.structure_version != right.structure_version
        || left.provider_file_key != right.provider_file_key
        || left.symbolic_link_target != right.symbolic_link_target
        || left.symbolic_link_target_kind != right.symbolic_link_target_kind
}

fn metadata_is_directory(metadata: &EntryMetadata) -> bool {
    metadata.kind == EntryKind::Directory
        || metadata.kind == EntryKind::SymbolicLink
            && metadata.symbolic_link_target_kind == Some(EntryKind::Directory)
}

fn enforce_budgets(state: &mut ResourceRegistryState) {
    let mut directory_bytes = state
        .records
        .values()
        .map(estimated_directory_bytes)
        .sum::<usize>();
    if directory_bytes > state.budgets.directory_bytes {
        let mut candidates = state
            .records
            .iter()
            .filter(|(_, record)| record.children.is_some())
            .map(|(resource_id, record)| (record.last_access, *resource_id))
            .collect::<Vec<_>>();
        candidates.sort_unstable();
        for (_, resource_id) in candidates {
            if directory_bytes <= state.budgets.directory_bytes {
                break;
            }
            if let Some(record) = state.records.get_mut(&resource_id) {
                directory_bytes = directory_bytes.saturating_sub(estimated_directory_bytes(record));
                record.children = None;
                record.directory_state =
                    record.directory_state.map(|_| DirectoryLoadState::Unloaded);
            }
        }
    }

    let mut metadata_bytes = state
        .records
        .values()
        .map(estimated_metadata_bytes)
        .sum::<usize>();
    if metadata_bytes > state.budgets.metadata_bytes {
        let mut candidates = state
            .records
            .iter()
            .filter(|(_, record)| record.metadata.is_some())
            .map(|(resource_id, record)| (record.last_access, *resource_id))
            .collect::<Vec<_>>();
        candidates.sort_unstable();
        for (_, resource_id) in candidates {
            if metadata_bytes <= state.budgets.metadata_bytes {
                break;
            }
            if let Some(record) = state.records.get_mut(&resource_id) {
                metadata_bytes = metadata_bytes.saturating_sub(estimated_metadata_bytes(record));
                record.metadata = None;
                if record.freshness == ResourceFreshness::Fresh {
                    record.freshness = ResourceFreshness::Stale;
                }
            }
        }
    }
}

fn snapshot_stats(state: &ResourceRegistryState) -> SnapshotStats {
    let mut stats = SnapshotStats {
        identity_count: state.records.len(),
        estimated_identity_bytes: state.records.values().map(estimated_identity_bytes).sum(),
        last_sequence: state.last_sequence,
        rescan_state: state.rescan_state,
        ..SnapshotStats::default()
    };
    for record in state.records.values() {
        if record.metadata.is_some() {
            stats.metadata_count += 1;
            stats.estimated_metadata_bytes += estimated_metadata_bytes(record);
        }
        if record.children.is_some() {
            stats.loaded_directory_count += 1;
            stats.estimated_directory_bytes += estimated_directory_bytes(record);
        }
    }
    stats
}

fn estimated_directory_bytes(record: &RegistryRecord) -> usize {
    record.children.as_ref().map_or(0, |children| {
        std::mem::size_of::<BTreeSet<ResourceId>>()
            + children.len() * std::mem::size_of::<ResourceId>() * 2
    })
}

fn estimated_metadata_bytes(record: &RegistryRecord) -> usize {
    let path_bytes = record
        .path
        .components()
        .map(|component| component.as_bytes().len())
        .sum::<usize>();
    256 + path_bytes
}

fn estimated_identity_bytes(record: &RegistryRecord) -> usize {
    let path_bytes = record
        .path
        .components()
        .map(|component| component.as_bytes().len())
        .sum::<usize>();
    128 + path_bytes
}

fn registry_error(
    state: &ResourceRegistryState,
    code: VfsErrorCode,
    operation: VfsOperation,
    path: Option<ProviderPath>,
) -> VfsError {
    let mut error = VfsError::new(
        code,
        operation,
        crate::ProviderId::new(format!("snapshot-mount-{}", state.mount_id.get())),
    );
    if let Some(path) = path {
        error = error.with_path(path);
    }
    error
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Atomicity, CaseSensitivity, CopyOptions, CreateDirOptions, CreateDisposition,
        DirectoryCapabilities, EntryPermissions, FileAccess, LinkCapabilities, MemoryProvider,
        MutationCapabilities, OpenOptions, ProviderCapabilities, ProviderDescriptor, ProviderId,
        ProviderLimits, ReadCapabilities, RemoveKind, RemoveOptions, RemoveOutcome, RenameOptions,
        SupportLevel, TrashCapabilities, VfsFile, VfsVersion, WatchCapabilities, WatchRequest,
        WriteAtOptions, WriteCapabilities,
    };
    use async_trait::async_trait;
    use futures::{executor::block_on, stream};
    use std::{
        num::{NonZeroU32, NonZeroU64},
        time::Instant,
    };

    struct SyntheticProvider {
        descriptor: ProviderDescriptor,
        directory_count: usize,
        files_per_directory: usize,
    }

    impl SyntheticProvider {
        fn new(directory_count: usize, files_per_directory: usize) -> Self {
            Self {
                descriptor: ProviderDescriptor {
                    id: ProviderId::new("synthetic-snapshot"),
                    display_name: Arc::from("Synthetic snapshot"),
                    path_encoding: crate::PathEncoding::PortableUtf8,
                    case_sensitivity: CaseSensitivity::Sensitive,
                },
                directory_count,
                files_per_directory,
            }
        }

        fn metadata(&self, path: &ProviderPath, kind: EntryKind, key: u64) -> EntryMetadata {
            EntryMetadata {
                name: path.file_name().cloned().map(crate::EntryName::from_exact),
                kind,
                size: if kind == EntryKind::File { 64 } else { 0 },
                modified_at: None,
                created_at: None,
                permissions: EntryPermissions {
                    writable: false,
                    executable: false,
                    private: false,
                    hidden: false,
                },
                provider_file_key: Some(ProviderFileKey::new(key.to_be_bytes().to_vec())),
                content_version: VfsVersion::new(1_u64.to_be_bytes().to_vec()),
                structure_version: VfsVersion::new(1_u64.to_be_bytes().to_vec()),
                symbolic_link_target: None,
                symbolic_link_target_kind: None,
                case_sensitivity: CaseSensitivity::Sensitive,
            }
        }

        fn unsupported<T>(&self, operation: VfsOperation) -> VfsResult<T> {
            Err(VfsError::new(
                VfsErrorCode::Unsupported,
                operation,
                self.descriptor.id.clone(),
            ))
        }
    }

    #[async_trait]
    impl VfsProvider for SyntheticProvider {
        fn descriptor(&self) -> &ProviderDescriptor {
            &self.descriptor
        }

        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                read: ReadCapabilities {
                    whole_file: SupportLevel::Unsupported,
                    stream: SupportLevel::Unsupported,
                    positioned: SupportLevel::Unsupported,
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
                    recursive_list: SupportLevel::Unsupported,
                    stat_many: SupportLevel::Unsupported,
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
                    maximum_stat_batch: NonZeroU32::MIN,
                    maximum_range_size: NonZeroU64::MIN,
                    maximum_request_size: NonZeroU64::new(8 * 1_024 * 1_024)
                        .unwrap_or(NonZeroU64::MIN),
                    maximum_open_handles: NonZeroU32::MIN,
                },
            }
        }

        async fn stat(
            &self,
            path: &ProviderPath,
            _options: StatOptions,
        ) -> VfsResult<EntryMetadata> {
            match path.component_count() {
                0 => Ok(self.metadata(path, EntryKind::Directory, 1)),
                1 => Ok(self.metadata(
                    path,
                    EntryKind::Directory,
                    directory_index(path).unwrap_or(0) as u64 + 2,
                )),
                2 => {
                    let directory = path.parent().and_then(|path| directory_index(&path));
                    let file = file_index(path);
                    match (directory, file) {
                        (Some(directory), Some(file)) => Ok(self.metadata(
                            path,
                            EntryKind::File,
                            2 + self.directory_count as u64
                                + (directory * self.files_per_directory + file) as u64,
                        )),
                        _ => self.unsupported(VfsOperation::Stat),
                    }
                }
                _ => self.unsupported(VfsOperation::Stat),
            }
        }

        async fn read_dir(
            &self,
            path: &ProviderPath,
            request: DirPageRequest,
        ) -> VfsResult<crate::DirPage> {
            let start = decode_test_cursor(request.cursor.as_ref());
            let limit = request.limit.get() as usize;
            let total = if path.is_root() {
                self.directory_count
            } else {
                self.files_per_directory
            };
            let end = start.saturating_add(limit).min(total);
            let mut entries = Vec::with_capacity(end.saturating_sub(start));
            for index in start..end {
                let name = if path.is_root() {
                    format!("dir-{index:04}")
                } else {
                    format!("file-{index:04}.rs")
                };
                let component = crate::ExactComponent::new(
                    crate::PathEncoding::PortableUtf8,
                    name.into_bytes(),
                )
                .map_err(|error| {
                    VfsError::new(
                        VfsErrorCode::Internal,
                        VfsOperation::ReadDirectory,
                        self.descriptor.id.clone(),
                    )
                    .with_detail(error.to_string())
                })?;
                let child_path = path.join_component(component).map_err(|error| {
                    VfsError::new(
                        VfsErrorCode::Internal,
                        VfsOperation::ReadDirectory,
                        self.descriptor.id.clone(),
                    )
                    .with_detail(error.to_string())
                })?;
                let key = if path.is_root() {
                    index as u64 + 2
                } else {
                    let directory = directory_index(path).unwrap_or(0);
                    2 + self.directory_count as u64
                        + (directory * self.files_per_directory + index) as u64
                };
                entries.push(DirEntry {
                    metadata: self.metadata(
                        &child_path,
                        if path.is_root() {
                            EntryKind::Directory
                        } else {
                            EntryKind::File
                        },
                        key,
                    ),
                    path: child_path,
                });
            }
            Ok(crate::DirPage {
                entries,
                next_cursor: (end < total).then(|| test_cursor(end)),
            })
        }

        async fn open(
            &self,
            _path: &ProviderPath,
            _options: OpenOptions,
        ) -> VfsResult<Arc<dyn VfsFile>> {
            self.unsupported(VfsOperation::Open)
        }

        async fn create_dir(
            &self,
            _path: &ProviderPath,
            _options: CreateDirOptions,
        ) -> VfsResult<()> {
            self.unsupported(VfsOperation::CreateDirectory)
        }

        async fn remove(
            &self,
            _path: &ProviderPath,
            _options: RemoveOptions,
        ) -> VfsResult<RemoveOutcome> {
            self.unsupported(VfsOperation::Remove)
        }

        async fn rename(
            &self,
            _source: &ProviderPath,
            _target: &ProviderPath,
            _options: RenameOptions,
        ) -> VfsResult<()> {
            self.unsupported(VfsOperation::Rename)
        }

        async fn copy(
            &self,
            _source: &ProviderPath,
            _target: &ProviderPath,
            _options: CopyOptions,
        ) -> VfsResult<()> {
            self.unsupported(VfsOperation::Copy)
        }

        async fn watch(
            &self,
            _request: WatchRequest,
        ) -> VfsResult<futures::stream::BoxStream<'static, VfsResult<EventBatch>>> {
            Ok(Box::pin(stream::pending()))
        }
    }

    #[test]
    fn million_entry_snapshot_stays_lazy_and_within_budget() {
        let provider = Arc::new(SyntheticProvider::new(1_000, 1_000));
        let snapshot = VfsSnapshot::new(MountId::new(1), provider, SnapshotBudgets::default());
        let root = ProviderPath::root(crate::PathEncoding::PortableUtf8);
        let first_paint_start = Instant::now();
        let root_result = block_on(snapshot.load_directory(&root));
        assert!(root_result.is_ok(), "root load failed: {root_result:?}");
        let first_paint = first_paint_start.elapsed();

        for directory_index in 0..10 {
            let directory_path = ProviderPath::from_byte_components(
                crate::PathEncoding::PortableUtf8,
                [format!("dir-{directory_index:04}").as_bytes()],
            );
            let Ok(directory_path) = directory_path else {
                panic!("synthetic directory path must be valid: {directory_path:?}");
            };
            let result = block_on(snapshot.load_directory(&directory_path));
            assert!(result.is_ok(), "directory load failed: {result:?}");
        }

        let stats = snapshot.stats();
        assert_eq!(stats.identity_count, 11_001);
        assert!(
            stats.estimated_identity_bytes + stats.estimated_metadata_bytes <= 128 * 1_024 * 1_024
        );
        assert!(first_paint < std::time::Duration::from_millis(250));

        let lookup_path = ProviderPath::from_byte_components(
            crate::PathEncoding::PortableUtf8,
            [b"dir-0005".as_slice(), b"file-0500.rs".as_slice()],
        );
        let Ok(lookup_path) = lookup_path else {
            panic!("lookup path must be valid: {lookup_path:?}");
        };
        let mut samples = Vec::with_capacity(200);
        for _ in 0..200 {
            let start = Instant::now();
            assert!(snapshot.registry().id_for_path(&lookup_path).is_some());
            samples.push(start.elapsed());
        }
        samples.sort_unstable();
        let percentile_index = samples.len() * 95 / 100;
        let Some(p95) = samples.get(percentile_index) else {
            panic!("lookup samples are missing");
        };
        assert!(*p95 < std::time::Duration::from_millis(2));
        println!(
            "snapshot first paint: {first_paint:?}; identities: {}; identity bytes: {}; metadata bytes: {}; lookup p95: {p95:?}",
            stats.identity_count, stats.estimated_identity_bytes, stats.estimated_metadata_bytes
        );
    }

    #[test]
    fn mounted_snapshots_allocate_distinct_mount_ids() {
        let first = VfsSnapshot::mount(
            Arc::new(MemoryProvider::new(
                "first-mounted-snapshot",
                crate::PathEncoding::PortableUtf8,
            )),
            SnapshotBudgets::default(),
        );
        let second = VfsSnapshot::mount(
            Arc::new(MemoryProvider::new(
                "second-mounted-snapshot",
                crate::PathEncoding::PortableUtf8,
            )),
            SnapshotBudgets::default(),
        );
        let (Ok(first), Ok(second)) = (first, second) else {
            panic!("mounted snapshots must allocate IDs");
        };
        assert_ne!(first.registry().mount_id(), second.registry().mount_id());
    }

    #[test]
    fn range_cache_is_versioned_and_lru_bounded() {
        let result = block_on(async {
            let provider = Arc::new(MemoryProvider::new(
                "snapshot-range-cache",
                crate::PathEncoding::PortableUtf8,
            ));
            let path = test_provider_path([b"data.bin".as_slice()]);
            let file = provider
                .open(
                    &path,
                    OpenOptions {
                        access: FileAccess::ReadWrite,
                        create: CreateDisposition::CreateNew,
                        expected_version: None,
                        context: OperationContext::default(),
                    },
                )
                .await?;
            file.write_at(0, b"abcdefgh", WriteAtOptions::default())
                .await?;
            let snapshot = VfsSnapshot::new(
                MountId::new(6),
                provider,
                SnapshotBudgets {
                    range_bytes: 4,
                    ..SnapshotBudgets::default()
                },
            );

            let first = snapshot
                .read_range(&path, 0, 4, OperationContext::default())
                .await?;
            let second = snapshot
                .read_range(&path, 4, 4, OperationContext::default())
                .await?;
            if first.as_ref() != b"abcd" || second.as_ref() != b"efgh" {
                return Err(VfsError::new(
                    VfsErrorCode::Internal,
                    VfsOperation::Read,
                    snapshot.provider().descriptor().id.clone(),
                )
                .with_detail("range cache returned incorrect bytes"));
            }
            let stats = snapshot.stats();
            if stats.cached_range_count != 1 || stats.cached_range_bytes != 4 {
                return Err(VfsError::new(
                    VfsErrorCode::Internal,
                    VfsOperation::Read,
                    snapshot.provider().descriptor().id.clone(),
                )
                .with_detail("range cache exceeded its configured budget"));
            }

            file.write_at(0, b"WXYZ", WriteAtOptions::default()).await?;
            let refreshed = snapshot
                .read_range(&path, 0, 4, OperationContext::default())
                .await?;
            if refreshed.as_ref() != b"WXYZ" {
                return Err(VfsError::new(
                    VfsErrorCode::Internal,
                    VfsOperation::Read,
                    snapshot.provider().descriptor().id.clone(),
                )
                .with_detail("range cache served bytes from a stale source version"));
            }
            Ok::<_, VfsError>(())
        });
        assert!(result.is_ok(), "range cache test failed: {result:?}");
    }

    #[test]
    fn reconciliation_preserves_renames_and_replaces_recreated_identity() {
        let result = block_on(async {
            let provider = Arc::new(MemoryProvider::new(
                "snapshot-identity",
                crate::PathEncoding::PortableUtf8,
            ));
            let directory = test_provider_path([b"workspace".as_slice()]);
            let original_path =
                test_provider_path([b"workspace".as_slice(), b"original.rs".as_slice()]);
            let renamed_path =
                test_provider_path([b"workspace".as_slice(), b"renamed.rs".as_slice()]);
            provider
                .create_dir(&directory, CreateDirOptions::default())
                .await?;
            let file = provider
                .open(
                    &original_path,
                    OpenOptions {
                        access: FileAccess::ReadWrite,
                        create: CreateDisposition::CreateNew,
                        expected_version: None,
                        context: OperationContext::default(),
                    },
                )
                .await?;
            file.write_at(0, b"content", WriteAtOptions::default())
                .await?;

            let snapshot = VfsSnapshot::new(
                MountId::new(7),
                provider.clone(),
                SnapshotBudgets::default(),
            );
            snapshot
                .load_directory(&ProviderPath::root(crate::PathEncoding::PortableUtf8))
                .await?;
            snapshot.load_directory(&directory).await?;
            let original_id = snapshot
                .registry()
                .id_for_path(&original_path)
                .ok_or_else(|| {
                    VfsError::new(
                        VfsErrorCode::Internal,
                        VfsOperation::Stat,
                        provider.descriptor().id.clone(),
                    )
                })?;

            provider
                .rename(&original_path, &renamed_path, RenameOptions::default())
                .await?;
            let rename_delta = snapshot.load_directory(&directory).await?;
            let renamed_id = snapshot
                .registry()
                .id_for_path(&renamed_path)
                .ok_or_else(|| {
                    VfsError::new(
                        VfsErrorCode::Internal,
                        VfsOperation::Stat,
                        provider.descriptor().id.clone(),
                    )
                })?;
            if original_id != renamed_id
                || !rename_delta.changes.iter().any(|change| {
                    change.resource_id == original_id
                        && matches!(
                            &change.kind,
                            ResourceChangeKind::Renamed { old_path }
                                if old_path == &original_path
                        )
                })
            {
                return Err(VfsError::new(
                    VfsErrorCode::Internal,
                    VfsOperation::Rename,
                    provider.descriptor().id.clone(),
                )
                .with_detail("rename did not preserve snapshot identity"));
            }

            provider
                .remove(
                    &renamed_path,
                    RemoveOptions {
                        kind: RemoveKind::File,
                        expected_version: None,
                        context: OperationContext::default(),
                    },
                )
                .await?;
            provider
                .open(
                    &renamed_path,
                    OpenOptions {
                        access: FileAccess::ReadWrite,
                        create: CreateDisposition::CreateNew,
                        expected_version: None,
                        context: OperationContext::default(),
                    },
                )
                .await?;
            snapshot.load_directory(&directory).await?;
            let recreated_id = snapshot
                .registry()
                .id_for_path(&renamed_path)
                .ok_or_else(|| {
                    VfsError::new(
                        VfsErrorCode::Internal,
                        VfsOperation::Stat,
                        provider.descriptor().id.clone(),
                    )
                })?;
            if recreated_id == renamed_id {
                return Err(VfsError::new(
                    VfsErrorCode::Internal,
                    VfsOperation::Stat,
                    provider.descriptor().id.clone(),
                )
                .with_detail("delete/recreate reused the previous identity"));
            }
            Ok::<_, VfsError>(())
        });
        assert!(result.is_ok(), "snapshot identity test failed: {result:?}");
    }

    #[test]
    fn reconciliation_keeps_aliases_with_the_same_file_key_distinct() {
        let provider = SyntheticProvider::new(0, 0);
        let root = ProviderPath::root(crate::PathEncoding::PortableUtf8);
        let original_path = test_provider_path([b"original.rs".as_slice()]);
        let alias_path = test_provider_path([b"alias.rs".as_slice()]);
        let renamed_path = test_provider_path([b"renamed.rs".as_slice()]);
        let registry =
            ResourceRegistry::new(MountId::new(17), root.clone(), SnapshotBudgets::default());

        let first = registry.reconcile_directory(
            &root,
            provider.metadata(&root, EntryKind::Directory, 1),
            vec![
                DirEntry {
                    path: original_path.clone(),
                    metadata: provider.metadata(&original_path, EntryKind::File, 42),
                },
                DirEntry {
                    path: alias_path.clone(),
                    metadata: provider.metadata(&alias_path, EntryKind::File, 42),
                },
            ],
        );
        assert!(
            first.is_ok(),
            "initial alias reconciliation failed: {first:?}"
        );
        let Some(original_id) = registry.id_for_path(&original_path) else {
            panic!("original identity is missing");
        };
        let Some(alias_id) = registry.id_for_path(&alias_path) else {
            panic!("alias identity is missing");
        };
        assert_ne!(original_id, alias_id);

        let second = registry.reconcile_directory(
            &root,
            provider.metadata(&root, EntryKind::Directory, 1),
            vec![
                DirEntry {
                    path: alias_path.clone(),
                    metadata: provider.metadata(&alias_path, EntryKind::File, 42),
                },
                DirEntry {
                    path: renamed_path.clone(),
                    metadata: provider.metadata(&renamed_path, EntryKind::File, 42),
                },
            ],
        );
        let Ok(second) = second else {
            panic!("alias rename reconciliation failed: {second:?}");
        };
        assert_eq!(registry.id_for_path(&alias_path), Some(alias_id));
        assert_eq!(registry.id_for_path(&renamed_path), Some(original_id));
        assert!(second.changes.iter().any(|change| {
            change.resource_id == original_id
                && matches!(
                    &change.kind,
                    ResourceChangeKind::Renamed { old_path } if old_path == &original_path
                )
        }));
    }

    #[test]
    fn reconciliation_rejects_colliding_provider_lookup_keys() {
        let provider = SyntheticProvider::new(0, 0);
        let root = ProviderPath::root(crate::PathEncoding::PortableUtf8);
        let upper_path = test_provider_path([b"Case.rs".as_slice()]);
        let lower_path = test_provider_path([b"case.rs".as_slice()]);
        let mut upper_metadata = provider.metadata(&upper_path, EntryKind::File, 1);
        let mut lower_metadata = provider.metadata(&lower_path, EntryKind::File, 2);
        let shared_lookup_key = LookupKey::new(b"case.rs".to_vec());
        let Some(upper_exact) = upper_path.file_name().cloned() else {
            panic!("upper path is missing its name");
        };
        let Some(lower_exact) = lower_path.file_name().cloned() else {
            panic!("lower path is missing its name");
        };
        upper_metadata.name = Some(crate::EntryName::new(
            upper_exact.clone(),
            upper_exact.display(),
            shared_lookup_key.clone(),
        ));
        lower_metadata.name = Some(crate::EntryName::new(
            lower_exact.clone(),
            lower_exact.display(),
            shared_lookup_key,
        ));
        let registry =
            ResourceRegistry::new(MountId::new(18), root.clone(), SnapshotBudgets::default());
        let result = registry.reconcile_directory(
            &root,
            provider.metadata(&root, EntryKind::Directory, 3),
            vec![
                DirEntry {
                    path: upper_path,
                    metadata: upper_metadata,
                },
                DirEntry {
                    path: lower_path,
                    metadata: lower_metadata,
                },
            ],
        );
        assert!(
            result
                .as_ref()
                .err()
                .is_some_and(|error| error.code() == VfsErrorCode::Conflict),
            "lookup-key collision was not rejected: {result:?}"
        );
    }

    #[test]
    fn overflow_batch_converges_through_deterministic_rescan() {
        let result = block_on(async {
            let provider = Arc::new(MemoryProvider::new(
                "snapshot-overflow",
                crate::PathEncoding::PortableUtf8,
            ));
            let root = ProviderPath::root(crate::PathEncoding::PortableUtf8);
            let file_path = test_provider_path([b"after-overflow.rs".as_slice()]);
            let snapshot = VfsSnapshot::new(
                MountId::new(9),
                provider.clone(),
                SnapshotBudgets::default(),
            );
            snapshot.load_directory(&root).await?;
            provider
                .open(
                    &file_path,
                    OpenOptions {
                        access: FileAccess::ReadWrite,
                        create: CreateDisposition::CreateNew,
                        expected_version: None,
                        context: OperationContext::default(),
                    },
                )
                .await?;
            let first = snapshot
                .apply_event_batch(EventBatch {
                    first_sequence: 10,
                    last_sequence: 10,
                    events: vec![crate::VfsEvent {
                        path: root.clone(),
                        kind: VfsEventKind::Overflow {
                            rescan_root: root.clone(),
                        },
                    }],
                })
                .await?;
            let second = snapshot
                .apply_event_batch(EventBatch {
                    first_sequence: 10,
                    last_sequence: 10,
                    events: Vec::new(),
                })
                .await?;
            if snapshot.registry().id_for_path(&file_path).is_none()
                || first.changes.is_empty()
                || !second.changes.is_empty()
            {
                return Err(VfsError::new(
                    VfsErrorCode::Internal,
                    VfsOperation::Watch,
                    provider.descriptor().id.clone(),
                )
                .with_detail("overflow rescan did not converge deterministically"));
            }
            Ok::<_, VfsError>(())
        });
        assert!(result.is_ok(), "overflow reconciliation failed: {result:?}");
    }

    #[test]
    fn event_reconciliation_preserves_cross_directory_rename_identity() {
        let result = block_on(async {
            let provider = Arc::new(MemoryProvider::new(
                "snapshot-cross-directory-rename",
                crate::PathEncoding::PortableUtf8,
            ));
            let root = ProviderPath::root(crate::PathEncoding::PortableUtf8);
            let source_directory = test_provider_path([b"source".as_slice()]);
            let target_directory = test_provider_path([b"target".as_slice()]);
            let source_path = test_provider_path([b"source".as_slice(), b"entry.rs".as_slice()]);
            let target_path = test_provider_path([b"target".as_slice(), b"entry.rs".as_slice()]);
            provider
                .create_dir(&source_directory, CreateDirOptions::default())
                .await?;
            provider
                .create_dir(&target_directory, CreateDirOptions::default())
                .await?;
            provider
                .open(
                    &source_path,
                    OpenOptions {
                        access: FileAccess::ReadWrite,
                        create: CreateDisposition::CreateNew,
                        expected_version: None,
                        context: OperationContext::default(),
                    },
                )
                .await?;
            let snapshot = VfsSnapshot::new(
                MountId::new(20),
                provider.clone(),
                SnapshotBudgets::default(),
            );
            snapshot.load_directory(&root).await?;
            snapshot.load_directory(&source_directory).await?;
            snapshot.load_directory(&target_directory).await?;
            let source_id = snapshot
                .registry()
                .id_for_path(&source_path)
                .ok_or_else(|| {
                    VfsError::new(
                        VfsErrorCode::Internal,
                        VfsOperation::Stat,
                        provider.descriptor().id.clone(),
                    )
                })?;

            provider
                .rename(&source_path, &target_path, RenameOptions::default())
                .await?;
            let delta = snapshot
                .apply_event_batch(EventBatch {
                    first_sequence: 1,
                    last_sequence: 1,
                    events: vec![crate::VfsEvent {
                        path: target_path.clone(),
                        kind: VfsEventKind::Renamed {
                            old_path: source_path.clone(),
                        },
                    }],
                })
                .await?;
            if snapshot.registry().id_for_path(&target_path) != Some(source_id)
                || !delta.changes.iter().any(|change| {
                    change.resource_id == source_id
                        && matches!(
                            &change.kind,
                            ResourceChangeKind::Renamed { old_path }
                                if old_path == &source_path
                        )
                })
                || delta.changes.iter().any(|change| {
                    change.resource_id == source_id
                        && matches!(&change.kind, ResourceChangeKind::Removed)
                })
            {
                return Err(VfsError::new(
                    VfsErrorCode::Internal,
                    VfsOperation::Rename,
                    provider.descriptor().id.clone(),
                )
                .with_detail("cross-directory rename did not preserve snapshot identity"));
            }
            Ok::<_, VfsError>(())
        });
        assert!(
            result.is_ok(),
            "cross-directory rename reconciliation failed: {result:?}"
        );
    }

    #[test]
    fn sequence_gap_rescans_the_mount_root() {
        let result = block_on(async {
            let provider = Arc::new(MemoryProvider::new(
                "snapshot-gap",
                crate::PathEncoding::PortableUtf8,
            ));
            let root = ProviderPath::root(crate::PathEncoding::PortableUtf8);
            let event_directory = test_provider_path([b"event-directory".as_slice()]);
            let missed_directory = test_provider_path([b"missed-directory".as_slice()]);
            let nested_file =
                test_provider_path([b"event-directory".as_slice(), b"nested.rs".as_slice()]);
            let missed_file =
                test_provider_path([b"missed-directory".as_slice(), b"missed.rs".as_slice()]);
            provider
                .create_dir(&event_directory, CreateDirOptions::default())
                .await?;
            provider
                .create_dir(&missed_directory, CreateDirOptions::default())
                .await?;
            provider
                .open(
                    &nested_file,
                    OpenOptions {
                        access: FileAccess::ReadWrite,
                        create: CreateDisposition::CreateNew,
                        expected_version: None,
                        context: OperationContext::default(),
                    },
                )
                .await?;
            let snapshot = VfsSnapshot::new(
                MountId::new(19),
                provider.clone(),
                SnapshotBudgets::default(),
            );
            snapshot.load_directory(&root).await?;
            snapshot.load_directory(&event_directory).await?;
            snapshot.load_directory(&missed_directory).await?;
            provider
                .open(
                    &missed_file,
                    OpenOptions {
                        access: FileAccess::ReadWrite,
                        create: CreateDisposition::CreateNew,
                        expected_version: None,
                        context: OperationContext::default(),
                    },
                )
                .await?;

            snapshot
                .apply_event_batch(EventBatch {
                    first_sequence: 3,
                    last_sequence: 3,
                    events: vec![crate::VfsEvent {
                        path: nested_file,
                        kind: VfsEventKind::Modified,
                    }],
                })
                .await?;
            if snapshot.registry().id_for_path(&missed_file).is_none()
                || snapshot.stats().rescan_state != SnapshotRescanState::Idle
            {
                return Err(VfsError::new(
                    VfsErrorCode::Internal,
                    VfsOperation::Watch,
                    provider.descriptor().id.clone(),
                )
                .with_detail("sequence gap did not converge through a root rescan"));
            }
            Ok::<_, VfsError>(())
        });
        assert!(
            result.is_ok(),
            "sequence-gap reconciliation failed: {result:?}"
        );
    }

    fn directory_index(path: &ProviderPath) -> Option<usize> {
        let name = path.file_name()?;
        let name = std::str::from_utf8(name.as_bytes()).ok()?;
        name.strip_prefix("dir-")?.parse().ok()
    }

    fn file_index(path: &ProviderPath) -> Option<usize> {
        let name = path.file_name()?;
        let name = std::str::from_utf8(name.as_bytes()).ok()?;
        name.strip_prefix("file-")?
            .strip_suffix(".rs")?
            .parse()
            .ok()
    }

    fn test_provider_path<const COMPONENT_COUNT: usize>(
        components: [&[u8]; COMPONENT_COUNT],
    ) -> ProviderPath {
        let result =
            ProviderPath::from_byte_components(crate::PathEncoding::PortableUtf8, components);
        let Ok(path) = result else {
            panic!("test provider path must be valid: {result:?}");
        };
        path
    }

    fn test_cursor(index: usize) -> crate::DirCursor {
        crate::DirCursor::new((index as u64).to_be_bytes().to_vec())
    }

    fn decode_test_cursor(cursor: Option<&crate::DirCursor>) -> usize {
        let Some(cursor) = cursor else {
            return 0;
        };
        let Ok(bytes) = <[u8; 8]>::try_from(cursor.as_bytes()) else {
            return 0;
        };
        usize::try_from(u64::from_be_bytes(bytes)).unwrap_or(0)
    }
}
