use crate::{
    Bounds, DevicePixels, Point, RenderGlyphParams, RenderImageParams, RenderSvgParams, Size,
};
use anyhow::Result;
use collections::{FxHashMap, FxHashSet};
use parking_lot::Mutex;
use std::borrow::Cow;

#[derive(PartialEq, Eq, Hash, Clone)]
#[expect(missing_docs)]
pub enum AtlasKey {
    Glyph(RenderGlyphParams),
    Svg(RenderSvgParams),
    Image(RenderImageParams),
}

impl AtlasKey {
    /// Returns the texture kind for this atlas key.
    pub fn texture_kind(&self) -> AtlasTextureKind {
        match self {
            AtlasKey::Glyph(params) => {
                if params.is_emoji {
                    AtlasTextureKind::Polychrome
                } else if params.subpixel_rendering {
                    AtlasTextureKind::Subpixel
                } else {
                    AtlasTextureKind::Monochrome
                }
            }
            AtlasKey::Svg(_) => AtlasTextureKind::Monochrome,
            AtlasKey::Image(_) => AtlasTextureKind::Polychrome,
        }
    }
}

impl From<RenderGlyphParams> for AtlasKey {
    fn from(params: RenderGlyphParams) -> Self {
        Self::Glyph(params)
    }
}

impl From<RenderSvgParams> for AtlasKey {
    fn from(params: RenderSvgParams) -> Self {
        Self::Svg(params)
    }
}

impl From<RenderImageParams> for AtlasKey {
    fn from(params: RenderImageParams) -> Self {
        Self::Image(params)
    }
}

#[expect(missing_docs)]
pub trait PlatformAtlas {
    /// The builder runs without the atlas lock. Concurrent misses may invoke it
    /// more than once, but insertion double-checks before creating residency.
    fn get_or_insert_with<'a>(
        &self,
        key: AtlasKey,
        build: &mut dyn FnMut() -> Result<Option<(Size<DevicePixels>, Cow<'a, [u8]>)>>,
    ) -> Result<Option<AtlasTile>>;
    fn remove(&self, key: &AtlasKey);
    fn begin_frame(&self) -> AtlasFrame;
    fn finish_frame(&self, frame: AtlasFrame, usage: &AtlasUsage) -> AtlasEpoch;
    fn current_epoch(&self) -> AtlasEpoch;

    /// Returns a point-in-time view of atlas residency and activity.
    fn snapshot(&self) -> AtlasSnapshot {
        AtlasSnapshot::default()
    }
}

/// A point-in-time view of sprite atlas residency and cumulative activity.
///
/// Page and byte fields are gauges at the instant of the snapshot. Hit, miss,
/// allocation, upload, removal, retirement, eviction, compaction, and pressure
/// fields are cumulative counts for the lifetime of the atlas instance.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AtlasSnapshot {
    /// Number of resident GPU texture pages.
    pub page_count: usize,
    /// Bytes reserved by resident GPU texture pages, including unused space.
    pub resident_bytes: usize,
    /// Number of keys currently addressable through the atlas lookup table.
    pub entry_count: usize,
    /// Number of successful key lookups.
    pub hits: u64,
    /// Number of key lookups that required invoking the miss builder.
    pub misses: u64,
    /// Number of successful resident entry allocations.
    pub allocations: u64,
    /// Number of uploads waiting for backend submission.
    pub pending_uploads: usize,
    /// Bytes held by uploads waiting for backend submission.
    pub pending_upload_bytes: usize,
    /// Number of backend upload calls issued.
    pub upload_calls: u64,
    /// Bytes submitted through backend upload calls.
    pub uploaded_bytes: u64,
    /// Number of entries removed from key lookup by remove or clear operations.
    pub removals: u64,
    /// Number of entries or pages moved into retirement.
    pub retirements: u64,
    /// Number of entries or pages evicted by cache policy.
    pub evictions: u64,
    /// Number of completed whole-page compactions.
    pub compactions: u64,
    /// Resident bytes required by the current completed-frame working set.
    pub current_frame_working_set_bytes: usize,
    /// Number of frames whose working set or misses exceeded retained budgets.
    pub budget_pressure_frames: u64,
}

/// A monotonically increasing atlas content generation.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AtlasEpoch(u64);

impl AtlasEpoch {
    /// Returns the numeric generation for diagnostics and tests.
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// A monotonically increasing identifier for an atlas frame build.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AtlasFrameId(u64);

impl AtlasFrameId {
    /// Returns the numeric identifier for diagnostics and tests.
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Identifies one in-progress atlas frame and the epoch it observes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AtlasFrame {
    id: AtlasFrameId,
    epoch: AtlasEpoch,
}

impl AtlasFrame {
    /// Returns this frame's monotonic identifier.
    pub fn id(self) -> AtlasFrameId {
        self.id
    }

    /// Returns the atlas epoch observed before frame construction.
    pub fn epoch(self) -> AtlasEpoch {
        self.epoch
    }
}

/// Atlas resources referenced by one completed scene.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AtlasUsage {
    texture_ids: FxHashSet<AtlasTextureId>,
    tile_ids: FxHashSet<TileId>,
}

impl AtlasUsage {
    /// Records one visible tile and its texture page.
    pub fn insert(&mut self, tile: AtlasTile) {
        self.texture_ids.insert(tile.texture_id);
        self.tile_ids.insert(tile.tile_id);
    }

    pub(crate) fn clear(&mut self) {
        self.texture_ids.clear();
        self.tile_ids.clear();
    }

    /// Returns whether the scene references the given texture page.
    pub fn contains_texture(&self, texture_id: AtlasTextureId) -> bool {
        self.texture_ids.contains(&texture_id)
    }

    /// Returns whether the scene references the given tile.
    pub fn contains_tile(&self, tile_id: TileId) -> bool {
        self.tile_ids.contains(&tile_id)
    }

    /// Returns the number of distinct texture pages referenced by the scene.
    pub fn texture_count(&self) -> usize {
        self.texture_ids.len()
    }

    /// Returns the number of distinct tiles referenced by the scene.
    pub fn tile_count(&self) -> usize {
        self.tile_ids.len()
    }

    /// Returns whether the scene references no atlas resources.
    pub fn is_empty(&self) -> bool {
        self.tile_ids.is_empty()
    }
}

const DEFAULT_ATLAS_SIZE: Size<DevicePixels> = Size {
    width: DevicePixels(1024),
    height: DevicePixels(1024),
};

#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct AtlasTextureDescriptor {
    pub texture_id: AtlasTextureId,
    pub size: Size<DevicePixels>,
    pub kind: AtlasTextureKind,
}

#[doc(hidden)]
pub struct AtlasUpload<'a> {
    pub texture_id: AtlasTextureId,
    pub bounds: Bounds<DevicePixels>,
    pub bytes: &'a [u8],
}

#[doc(hidden)]
pub trait AtlasBackend {
    fn create_texture(&mut self, descriptor: AtlasTextureDescriptor) -> Result<()>;
    fn upload(&mut self, upload: AtlasUpload<'_>) -> Result<()>;
    fn destroy_texture(&mut self, texture_id: AtlasTextureId);
    fn clear_textures(&mut self);

    fn snapshot(&self) -> AtlasSnapshot {
        AtlasSnapshot::default()
    }

    fn stores_texture_pages(&self) -> bool {
        true
    }
}

#[derive(Clone, Copy)]
struct AtlasConfiguration {
    default_page_size: Size<DevicePixels>,
    max_texture_size: Size<DevicePixels>,
}

struct AtlasEntry {
    tile: AtlasTile,
    retired: bool,
}

struct AtlasPage {
    texture_id: AtlasTextureId,
    size: Size<DevicePixels>,
    allocator: etagere::BucketedAtlasAllocator,
    entry_ids: FxHashSet<TileId>,
    active_entry_count: usize,
}

impl AtlasPage {
    fn resident_bytes(&self) -> usize {
        let bytes_per_pixel = match self.texture_id.kind {
            AtlasTextureKind::Monochrome => 1,
            AtlasTextureKind::Polychrome | AtlasTextureKind::Subpixel => 4,
        };
        (self.size.width.0.max(0) as usize)
            .saturating_mul(self.size.height.0.max(0) as usize)
            .saturating_mul(bytes_per_pixel)
    }
}

struct AtlasState<Backend> {
    tile_ids_by_key: FxHashMap<AtlasKey, TileId>,
    entries_by_tile_id: FxHashMap<TileId, AtlasEntry>,
    pages: Vec<AtlasPage>,
    configuration: AtlasConfiguration,
    next_texture_id: u32,
    next_tile_id: u32,
    next_frame_id: u64,
    epoch: AtlasEpoch,
    active_frame: Option<AtlasFrame>,
    completed_usage: AtlasUsage,
    pending_removals: FxHashSet<AtlasKey>,
    hits: u64,
    misses: u64,
    allocations: u64,
    removals: u64,
    retirements: u64,
    backend: Backend,
}

impl<Backend: AtlasBackend> AtlasState<Backend> {
    fn new(
        backend: Backend,
        default_page_size: Size<DevicePixels>,
        max_texture_size: Size<DevicePixels>,
    ) -> Self {
        Self {
            tile_ids_by_key: FxHashMap::default(),
            entries_by_tile_id: FxHashMap::default(),
            pages: Vec::new(),
            configuration: AtlasConfiguration {
                default_page_size,
                max_texture_size,
            },
            next_texture_id: 0,
            next_tile_id: 0,
            next_frame_id: 0,
            epoch: AtlasEpoch::default(),
            active_frame: None,
            completed_usage: AtlasUsage::default(),
            pending_removals: FxHashSet::default(),
            hits: 0,
            misses: 0,
            allocations: 0,
            removals: 0,
            retirements: 0,
            backend,
        }
    }

    fn lookup(&mut self, key: &AtlasKey) -> Option<AtlasTile> {
        if let Some(entry) = self
            .tile_ids_by_key
            .get(key)
            .and_then(|tile_id| self.entries_by_tile_id.get(tile_id))
        {
            self.hits = self.hits.saturating_add(1);
            Some(entry.tile)
        } else {
            self.misses = self.misses.saturating_add(1);
            None
        }
    }

    fn insert_or_get(
        &mut self,
        key: AtlasKey,
        size: Size<DevicePixels>,
        bytes: &[u8],
    ) -> Result<AtlasTile> {
        if let Some(entry) = self
            .tile_ids_by_key
            .get(&key)
            .and_then(|tile_id| self.entries_by_tile_id.get(tile_id))
        {
            return Ok(entry.tile);
        }

        let kind = key.texture_kind();
        let (page_index, allocation) = self.allocate_region(kind, size)?;
        let tile_id = match self.allocate_tile_id() {
            Ok(tile_id) => tile_id,
            Err(error) => {
                self.rollback_allocation(page_index, allocation.id);
                return Err(error);
            }
        };
        let tile = AtlasTile {
            texture_id: self.pages[page_index].texture_id,
            tile_id,
            padding: 0,
            bounds: Bounds {
                origin: etagere_point_to_device(allocation.rectangle.min),
                size,
            },
        };

        if let Err(error) = self.backend.upload(AtlasUpload {
            texture_id: tile.texture_id,
            bounds: tile.bounds,
            bytes,
        }) {
            self.rollback_allocation(page_index, allocation.id);
            return Err(error);
        }

        self.pages[page_index].entry_ids.insert(tile.tile_id);
        self.tile_ids_by_key.insert(key, tile.tile_id);
        self.entries_by_tile_id.insert(
            tile.tile_id,
            AtlasEntry {
                tile,
                retired: false,
            },
        );
        self.allocations = self.allocations.saturating_add(1);
        Ok(tile)
    }

    fn allocate_region(
        &mut self,
        kind: AtlasTextureKind,
        size: Size<DevicePixels>,
    ) -> Result<(usize, etagere::Allocation)> {
        anyhow::ensure!(
            size.width.0 > 0 && size.height.0 > 0,
            "atlas entry size must be positive"
        );
        anyhow::ensure!(
            size.width.0 <= self.configuration.max_texture_size.width.0
                && size.height.0 <= self.configuration.max_texture_size.height.0,
            "atlas entry {}x{} exceeds maximum texture size {}x{}",
            size.width.0,
            size.height.0,
            self.configuration.max_texture_size.width.0,
            self.configuration.max_texture_size.height.0,
        );

        for page_index in (0..self.pages.len()).rev() {
            let page = &mut self.pages[page_index];
            if page.texture_id.kind != kind {
                continue;
            }
            if let Some(allocation) = page.allocator.allocate(device_size_to_etagere(size)) {
                page.active_entry_count += 1;
                return Ok((page_index, allocation));
            }
        }

        self.create_page(kind, size)
    }

    fn create_page(
        &mut self,
        kind: AtlasTextureKind,
        minimum_size: Size<DevicePixels>,
    ) -> Result<(usize, etagere::Allocation)> {
        let default_page_size = self
            .configuration
            .default_page_size
            .min(&self.configuration.max_texture_size);
        let page_size = minimum_size.max(&default_page_size);
        let texture_id = self.allocate_texture_id(kind)?;
        self.backend.create_texture(AtlasTextureDescriptor {
            texture_id,
            size: page_size,
            kind,
        })?;

        let mut page = AtlasPage {
            texture_id,
            size: page_size,
            allocator: etagere::BucketedAtlasAllocator::new(device_size_to_etagere(page_size)),
            entry_ids: FxHashSet::default(),
            active_entry_count: 0,
        };
        let Some(allocation) = page
            .allocator
            .allocate(device_size_to_etagere(minimum_size))
        else {
            self.backend.destroy_texture(texture_id);
            anyhow::bail!("new atlas page could not allocate its requested entry");
        };
        page.active_entry_count = 1;
        self.pages.push(page);
        Ok((self.pages.len() - 1, allocation))
    }

    fn allocate_texture_id(&mut self, kind: AtlasTextureKind) -> Result<AtlasTextureId> {
        let index = self.next_texture_id;
        self.next_texture_id = self
            .next_texture_id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("atlas texture ID space exhausted"))?;
        Ok(AtlasTextureId { index, kind })
    }

    fn allocate_tile_id(&mut self) -> Result<TileId> {
        let id = self.next_tile_id;
        self.next_tile_id = self
            .next_tile_id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("atlas tile ID space exhausted"))?;
        Ok(TileId(id))
    }

    fn rollback_allocation(&mut self, page_index: usize, allocation_id: etagere::AllocId) {
        let page = &mut self.pages[page_index];
        page.allocator.deallocate(allocation_id);
        page.active_entry_count -= 1;
        if page.active_entry_count == 0 && page.entry_ids.is_empty() {
            let page = self.pages.remove(page_index);
            self.backend.destroy_texture(page.texture_id);
        }
    }

    fn request_remove(&mut self, key: &AtlasKey) {
        if self.tile_ids_by_key.contains_key(key) {
            self.pending_removals.insert(key.clone());
        }
    }

    fn begin_frame(&mut self) -> AtlasFrame {
        debug_assert!(
            self.active_frame.is_none(),
            "an atlas frame must finish before another begins"
        );
        self.apply_pending_removals();
        let id = AtlasFrameId(self.next_frame_id);
        self.next_frame_id = self
            .next_frame_id
            .checked_add(1)
            .expect("invariant: atlas frame ID space is exhausted");
        let frame = AtlasFrame {
            id,
            epoch: self.epoch,
        };
        self.active_frame = Some(frame);
        frame
    }

    fn finish_frame(&mut self, frame: AtlasFrame, usage: &AtlasUsage) -> AtlasEpoch {
        debug_assert_eq!(
            self.active_frame,
            Some(frame),
            "atlas frame token must match the active build"
        );
        debug_assert_eq!(
            frame.epoch, self.epoch,
            "atlas epoch cannot change while a frame is being built"
        );
        self.active_frame = None;
        self.completed_usage = usage.clone();
        self.destroy_unreferenced_retired_pages();
        self.epoch
    }

    fn apply_pending_removals(&mut self) {
        let pending_removals = std::mem::take(&mut self.pending_removals);
        let mut changed = false;
        for key in pending_removals {
            let Some(tile_id) = self.tile_ids_by_key.remove(&key) else {
                continue;
            };
            let Some(entry) = self.entries_by_tile_id.get_mut(&tile_id) else {
                log::error!("atlas key referred to missing tile {tile_id:?}");
                continue;
            };
            if entry.retired {
                continue;
            }
            entry.retired = true;
            let texture_id = entry.tile.texture_id;
            let Some(page) = self
                .pages
                .iter_mut()
                .find(|page| page.texture_id == texture_id)
            else {
                log::error!("atlas tile {tile_id:?} referred to a missing page");
                continue;
            };
            page.active_entry_count -= 1;
            self.removals = self.removals.saturating_add(1);
            self.retirements = self.retirements.saturating_add(1);
            changed = true;
        }
        if changed {
            self.advance_epoch();
        }
    }

    fn destroy_unreferenced_retired_pages(&mut self) {
        for page_index in (0..self.pages.len()).rev() {
            let page = &self.pages[page_index];
            if page.active_entry_count > 0 || self.completed_usage.contains_texture(page.texture_id)
            {
                continue;
            }
            let page = self.pages.remove(page_index);
            for tile_id in page.entry_ids {
                self.entries_by_tile_id.remove(&tile_id);
            }
            self.backend.destroy_texture(page.texture_id);
        }
    }

    fn clear(&mut self) {
        debug_assert!(
            self.active_frame.is_none(),
            "atlas resources cannot be cleared during frame construction"
        );
        let removed_entries = self.tile_ids_by_key.len();
        self.removals = self
            .removals
            .saturating_add(u64::try_from(removed_entries).unwrap_or(u64::MAX));
        self.retirements = self
            .retirements
            .saturating_add(u64::try_from(removed_entries).unwrap_or(u64::MAX));
        self.tile_ids_by_key.clear();
        self.entries_by_tile_id.clear();
        self.pages.clear();
        self.pending_removals.clear();
        self.completed_usage.clear();
        self.backend.clear_textures();
        self.advance_epoch();
    }

    fn advance_epoch(&mut self) {
        self.epoch.0 = self
            .epoch
            .0
            .checked_add(1)
            .expect("invariant: atlas epoch space is exhausted");
    }

    fn snapshot(&self) -> AtlasSnapshot {
        let mut snapshot = self.backend.snapshot();
        snapshot.entry_count = self.tile_ids_by_key.len();
        snapshot.hits = self.hits;
        snapshot.misses = self.misses;
        snapshot.allocations = self.allocations;
        snapshot.removals = self.removals;
        snapshot.retirements = self.retirements;
        if self.backend.stores_texture_pages() {
            snapshot.page_count = self.pages.len();
            snapshot.resident_bytes = self.pages.iter().fold(0usize, |total, page| {
                total.saturating_add(page.resident_bytes())
            });
        } else {
            snapshot.page_count = 0;
            snapshot.resident_bytes = 0;
        }
        snapshot
    }
}

#[doc(hidden)]
pub struct Atlas<Backend> {
    state: Mutex<AtlasState<Backend>>,
}

impl<Backend: AtlasBackend> Atlas<Backend> {
    pub fn new(backend: Backend, max_texture_size: Size<DevicePixels>) -> Self {
        Self::with_page_size(backend, DEFAULT_ATLAS_SIZE, max_texture_size)
    }

    fn with_page_size(
        backend: Backend,
        default_page_size: Size<DevicePixels>,
        max_texture_size: Size<DevicePixels>,
    ) -> Self {
        Self {
            state: Mutex::new(AtlasState::new(
                backend,
                default_page_size,
                max_texture_size,
            )),
        }
    }

    pub fn get_or_insert_with<'a>(
        &self,
        key: AtlasKey,
        build: &mut dyn FnMut() -> Result<Option<(Size<DevicePixels>, Cow<'a, [u8]>)>>,
    ) -> Result<Option<AtlasTile>> {
        if let Some(tile) = self.state.lock().lookup(&key) {
            return Ok(Some(tile));
        }

        profiling::scope!("new tile");
        let Some((size, bytes)) = build()? else {
            return Ok(None);
        };
        self.state.lock().insert_or_get(key, size, &bytes).map(Some)
    }

    pub fn remove(&self, key: &AtlasKey) {
        self.state.lock().request_remove(key);
    }

    pub fn begin_frame(&self) -> AtlasFrame {
        self.state.lock().begin_frame()
    }

    pub fn finish_frame(&self, frame: AtlasFrame, usage: &AtlasUsage) -> AtlasEpoch {
        self.state.lock().finish_frame(frame, usage)
    }

    pub fn current_epoch(&self) -> AtlasEpoch {
        self.state.lock().epoch
    }

    pub fn snapshot(&self) -> AtlasSnapshot {
        self.state.lock().snapshot()
    }

    pub fn with_backend<R>(&self, read: impl FnOnce(&Backend) -> R) -> R {
        read(&self.state.lock().backend)
    }

    pub fn update_backend<R>(&self, update: impl FnOnce(&mut Backend) -> R) -> R {
        update(&mut self.state.lock().backend)
    }

    pub fn clear(&self) {
        self.state.lock().clear();
    }

    pub fn reset(
        &self,
        max_texture_size: Size<DevicePixels>,
        reset_backend: impl FnOnce(&mut Backend),
    ) {
        let mut state = self.state.lock();
        state.clear();
        state.configuration.max_texture_size = max_texture_size;
        reset_backend(&mut state.backend);
    }

    #[cfg(test)]
    fn contains(&self, key: &AtlasKey) -> bool {
        self.state.lock().tile_ids_by_key.contains_key(key)
    }
}

impl<Backend: AtlasBackend> PlatformAtlas for Atlas<Backend> {
    fn get_or_insert_with<'a>(
        &self,
        key: AtlasKey,
        build: &mut dyn FnMut() -> Result<Option<(Size<DevicePixels>, Cow<'a, [u8]>)>>,
    ) -> Result<Option<AtlasTile>> {
        self.get_or_insert_with(key, build)
    }

    fn remove(&self, key: &AtlasKey) {
        self.remove(key);
    }

    fn begin_frame(&self) -> AtlasFrame {
        self.begin_frame()
    }

    fn finish_frame(&self, frame: AtlasFrame, usage: &AtlasUsage) -> AtlasEpoch {
        self.finish_frame(frame, usage)
    }

    fn current_epoch(&self) -> AtlasEpoch {
        self.current_epoch()
    }

    fn snapshot(&self) -> AtlasSnapshot {
        self.snapshot()
    }
}

/// A sprite atlas for windows without a GPU. It hands out uniquely identified
/// tiles without uploading any pixels, so glyph, SVG, and image painting can
/// run to completion in tests and headless platforms.
pub struct HeadlessAtlas(Atlas<HeadlessAtlasBackend>);

impl Default for HeadlessAtlas {
    fn default() -> Self {
        const MAX_HEADLESS_TEXTURE_SIZE: Size<DevicePixels> = Size {
            width: DevicePixels(16384),
            height: DevicePixels(16384),
        };
        Self(Atlas::new(HeadlessAtlasBackend, MAX_HEADLESS_TEXTURE_SIZE))
    }
}

#[doc(hidden)]
pub struct HeadlessAtlasBackend;

impl AtlasBackend for HeadlessAtlasBackend {
    fn create_texture(&mut self, _descriptor: AtlasTextureDescriptor) -> Result<()> {
        Ok(())
    }

    fn upload(&mut self, _upload: AtlasUpload<'_>) -> Result<()> {
        Ok(())
    }

    fn destroy_texture(&mut self, _texture_id: AtlasTextureId) {}

    fn clear_textures(&mut self) {}

    fn stores_texture_pages(&self) -> bool {
        false
    }
}

impl PlatformAtlas for HeadlessAtlas {
    fn get_or_insert_with<'a>(
        &self,
        key: AtlasKey,
        build: &mut dyn FnMut() -> Result<Option<(Size<DevicePixels>, Cow<'a, [u8]>)>>,
    ) -> Result<Option<AtlasTile>> {
        self.0.get_or_insert_with(key, build)
    }

    fn remove(&self, key: &AtlasKey) {
        self.0.remove(key);
    }

    fn begin_frame(&self) -> AtlasFrame {
        self.0.begin_frame()
    }

    fn finish_frame(&self, frame: AtlasFrame, usage: &AtlasUsage) -> AtlasEpoch {
        self.0.finish_frame(frame, usage)
    }

    fn current_epoch(&self) -> AtlasEpoch {
        self.0.current_epoch()
    }

    fn snapshot(&self) -> AtlasSnapshot {
        self.0.snapshot()
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(C)]
#[expect(missing_docs)]
pub struct AtlasTile {
    /// The texture this tile belongs to.
    pub texture_id: AtlasTextureId,
    /// The stable identity of this tile within the atlas instance.
    pub tile_id: TileId,
    /// Padding around the tile content in pixels.
    pub padding: u32,
    /// The bounds of this tile within the texture.
    pub bounds: Bounds<DevicePixels>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
#[expect(missing_docs)]
pub struct AtlasTextureId {
    // We use u32 instead of usize for Metal Shader Language compatibility.
    /// A monotonic texture identity that is not reused by this atlas instance.
    pub index: u32,
    /// The kind of content stored in this texture.
    pub kind: AtlasTextureKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
#[cfg_attr(
    all(
        any(target_os = "linux", target_os = "freebsd"),
        not(any(feature = "x11", feature = "wayland"))
    ),
    allow(dead_code)
)]
#[expect(missing_docs)]
pub enum AtlasTextureKind {
    Monochrome = 0,
    Polychrome = 1,
    Subpixel = 2,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[repr(C)]
#[expect(missing_docs)]
pub struct TileId(pub u32);

fn device_size_to_etagere(size: Size<DevicePixels>) -> etagere::Size {
    etagere::size2(size.width.0, size.height.0)
}

fn etagere_point_to_device(point: etagere::Point) -> Point<DevicePixels> {
    Point {
        x: DevicePixels(point.x),
        y: DevicePixels(point.y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ImageId;
    use anyhow::Context as _;
    use std::{
        collections::HashSet,
        sync::{
            Arc, Barrier,
            atomic::{AtomicUsize, Ordering},
        },
    };

    const TILE_SIZE: Size<DevicePixels> = Size {
        width: DevicePixels(1),
        height: DevicePixels(1),
    };
    const PAGE_SIZE: Size<DevicePixels> = Size {
        width: DevicePixels(2),
        height: DevicePixels(2),
    };

    #[derive(Default)]
    struct RecordingAtlasBackend {
        textures: HashSet<AtlasTextureId>,
        create_calls: usize,
        upload_calls: usize,
        destroyed_textures: Vec<AtlasTextureId>,
        clear_calls: usize,
        fail_next_create: bool,
        fail_next_upload: bool,
    }

    impl AtlasBackend for RecordingAtlasBackend {
        fn create_texture(&mut self, descriptor: AtlasTextureDescriptor) -> Result<()> {
            self.create_calls += 1;
            if std::mem::take(&mut self.fail_next_create) {
                anyhow::bail!("backend create failed");
            }
            anyhow::ensure!(
                self.textures.insert(descriptor.texture_id),
                "texture identity was reused"
            );
            Ok(())
        }

        fn upload(&mut self, upload: AtlasUpload<'_>) -> Result<()> {
            if std::mem::take(&mut self.fail_next_upload) {
                anyhow::bail!("backend upload failed");
            }
            anyhow::ensure!(
                self.textures.contains(&upload.texture_id),
                "upload referred to a missing texture"
            );
            self.upload_calls += 1;
            Ok(())
        }

        fn destroy_texture(&mut self, texture_id: AtlasTextureId) {
            self.textures.remove(&texture_id);
            self.destroyed_textures.push(texture_id);
        }

        fn clear_textures(&mut self) {
            self.textures.clear();
            self.clear_calls += 1;
        }
    }

    fn atlas() -> Atlas<RecordingAtlasBackend> {
        Atlas::with_page_size(RecordingAtlasBackend::default(), PAGE_SIZE, PAGE_SIZE)
    }

    fn image_key(image_id: usize) -> AtlasKey {
        AtlasKey::Image(RenderImageParams {
            image_id: ImageId(image_id),
            frame_index: 0,
        })
    }

    fn build_tile() -> Result<Option<(Size<DevicePixels>, Cow<'static, [u8]>)>> {
        Ok(Some((TILE_SIZE, Cow::Borrowed(&[0, 0, 0, 255]))))
    }

    fn usage(tiles: impl IntoIterator<Item = AtlasTile>) -> AtlasUsage {
        let mut usage = AtlasUsage::default();
        for tile in tiles {
            usage.insert(tile);
        }
        usage
    }

    fn complete_frame(
        atlas: &Atlas<RecordingAtlasBackend>,
        tiles: impl IntoIterator<Item = AtlasTile>,
    ) -> AtlasEpoch {
        let frame = atlas.begin_frame();
        atlas.finish_frame(frame, &usage(tiles))
    }

    #[test]
    fn only_successful_inserts_are_cached() -> Result<()> {
        let atlas = atlas();
        let key = image_key(1);

        assert_eq!(
            atlas.get_or_insert_with(key.clone(), &mut || Ok(None))?,
            None
        );
        atlas
            .get_or_insert_with(key.clone(), &mut || anyhow::bail!("builder failed"))
            .expect_err("builder error should propagate");
        assert!(!atlas.contains(&key));

        atlas.update_backend(|backend| backend.fail_next_create = true);
        atlas
            .get_or_insert_with(key.clone(), &mut build_tile)
            .expect_err("backend create error should propagate");
        assert!(!atlas.contains(&key));

        atlas.update_backend(|backend| backend.fail_next_upload = true);
        atlas
            .get_or_insert_with(key.clone(), &mut build_tile)
            .expect_err("backend upload error should propagate");
        assert!(!atlas.contains(&key));
        assert_eq!(atlas.snapshot().page_count, 0);

        let tile = atlas
            .get_or_insert_with(key.clone(), &mut build_tile)?
            .context("builder should produce a tile")?;
        assert_eq!(tile.texture_id.kind, key.texture_kind());
        assert_eq!(
            atlas.get_or_insert_with(key.clone(), &mut || {
                anyhow::bail!("cache hit must not call the builder")
            })?,
            Some(tile)
        );
        assert!(atlas.contains(&key));
        assert_eq!(
            atlas.snapshot(),
            AtlasSnapshot {
                page_count: 1,
                resident_bytes: 16,
                entry_count: 1,
                hits: 1,
                misses: 5,
                allocations: 1,
                ..AtlasSnapshot::default()
            }
        );
        Ok(())
    }

    #[test]
    fn builder_runs_outside_the_atlas_lock() -> Result<()> {
        let atlas = atlas();
        let key = image_key(1);
        atlas
            .get_or_insert_with(key, &mut || {
                assert_eq!(atlas.snapshot().entry_count, 0);
                build_tile()
            })?
            .context("builder should produce a tile")?;
        Ok(())
    }

    #[test]
    fn concurrent_misses_double_check_before_inserting() {
        let atlas = Arc::new(atlas());
        let barrier = Arc::new(Barrier::new(2));
        let builder_calls = Arc::new(AtomicUsize::new(0));
        let key = image_key(1);
        let mut threads = Vec::new();

        for _ in 0..2 {
            let atlas = atlas.clone();
            let barrier = barrier.clone();
            let builder_calls = builder_calls.clone();
            let key = key.clone();
            threads.push(std::thread::spawn(move || {
                atlas
                    .get_or_insert_with(key, &mut || {
                        builder_calls.fetch_add(1, Ordering::SeqCst);
                        barrier.wait();
                        build_tile()
                    })
                    .expect("concurrent insert should succeed")
                    .expect("builder should produce a tile")
            }));
        }

        let tiles = threads
            .into_iter()
            .map(|thread| thread.join().expect("atlas thread should not panic"))
            .collect::<Vec<_>>();
        assert_eq!(tiles[0], tiles[1]);
        assert_eq!(builder_calls.load(Ordering::SeqCst), 2);
        assert_eq!(atlas.snapshot().entry_count, 1);
        assert_eq!(atlas.snapshot().allocations, 1);
        assert_eq!(atlas.with_backend(|backend| backend.upload_calls), 1);
    }

    #[test]
    fn remove_clear_and_recreate_do_not_reuse_identities() -> Result<()> {
        let one_pixel_page = Size {
            width: DevicePixels(1),
            height: DevicePixels(1),
        };
        let atlas = Atlas::with_page_size(
            RecordingAtlasBackend::default(),
            one_pixel_page,
            one_pixel_page,
        );
        let first_key = image_key(1);
        let second_key = image_key(2);
        let third_key = image_key(3);

        let first = atlas
            .get_or_insert_with(first_key.clone(), &mut build_tile)?
            .context("first tile should exist")?;
        assert_eq!(complete_frame(&atlas, [first]), AtlasEpoch::default());
        atlas.remove(&first_key);
        assert!(atlas.contains(&first_key));
        let replacement_frame = atlas.begin_frame();
        assert_ne!(replacement_frame.epoch(), AtlasEpoch::default());
        assert!(!atlas.contains(&first_key));
        assert!(atlas.with_backend(|backend| backend.textures.contains(&first.texture_id)));
        let second = atlas
            .get_or_insert_with(second_key, &mut build_tile)?
            .context("second tile should exist")?;
        assert_ne!(first.texture_id, second.texture_id);
        assert_ne!(first.tile_id, second.tile_id);
        atlas.finish_frame(replacement_frame, &usage([second]));
        assert!(!atlas.with_backend(|backend| backend.textures.contains(&first.texture_id)));

        atlas.clear();
        let third = atlas
            .get_or_insert_with(third_key, &mut build_tile)?
            .context("third tile should exist")?;
        assert_ne!(second.texture_id, third.texture_id);
        assert_ne!(second.tile_id, third.tile_id);
        assert!(!atlas.with_backend(|backend| backend.textures.contains(&first.texture_id)));
        assert!(!atlas.with_backend(|backend| backend.textures.contains(&second.texture_id)));
        assert!(atlas.with_backend(|backend| backend.textures.contains(&third.texture_id)));
        Ok(())
    }

    #[test]
    fn remove_and_clear_invalidate_keys() -> Result<()> {
        let atlas = atlas();
        let key = image_key(1);
        let other_key = image_key(2);
        let tile = atlas
            .get_or_insert_with(key.clone(), &mut build_tile)?
            .context("builder should produce a tile")?;
        let other_tile = atlas
            .get_or_insert_with(other_key.clone(), &mut build_tile)?
            .context("builder should produce another tile")?;
        complete_frame(&atlas, [tile, other_tile]);

        atlas.remove(&key);
        atlas.remove(&key);
        assert!(atlas.contains(&key));
        let frame = atlas.begin_frame();
        assert!(!atlas.contains(&key));
        assert!(atlas.contains(&other_key));
        assert_eq!(atlas.snapshot().page_count, 1);
        assert_eq!(atlas.snapshot().retirements, 1);
        atlas.finish_frame(frame, &usage([other_tile]));

        atlas.clear();
        assert!(!atlas.contains(&other_key));
        assert_eq!(atlas.snapshot().page_count, 0);
        assert_eq!(atlas.snapshot().removals, 2);
        assert_eq!(atlas.with_backend(|backend| backend.clear_calls), 1);
        Ok(())
    }

    #[test]
    fn removal_during_frame_build_is_deferred_to_the_next_boundary() -> Result<()> {
        let atlas = atlas();
        let key = image_key(1);
        let tile = atlas
            .get_or_insert_with(key.clone(), &mut build_tile)?
            .context("tile should exist")?;
        complete_frame(&atlas, [tile]);

        let frame = atlas.begin_frame();
        let frame_epoch = frame.epoch();
        atlas.remove(&key);
        assert!(atlas.contains(&key));
        assert_eq!(atlas.current_epoch(), frame_epoch);
        atlas.finish_frame(frame, &usage([tile]));

        let next_frame = atlas.begin_frame();
        assert_ne!(next_frame.epoch(), frame_epoch);
        assert!(!atlas.contains(&key));
        atlas.finish_frame(next_frame, &AtlasUsage::default());
        Ok(())
    }

    #[test]
    fn retired_suballocations_remain_tombstones() -> Result<()> {
        let atlas = atlas();
        let retired_key = image_key(1);
        let keeper_key = image_key(2);
        let replacement_key = image_key(3);
        let retired = atlas
            .get_or_insert_with(retired_key.clone(), &mut build_tile)?
            .context("retired tile should exist")?;
        let keeper = atlas
            .get_or_insert_with(keeper_key, &mut build_tile)?
            .context("keeper tile should exist")?;
        complete_frame(&atlas, [retired, keeper]);

        atlas.remove(&retired_key);
        let frame = atlas.begin_frame();
        let replacement = atlas
            .get_or_insert_with(replacement_key, &mut build_tile)?
            .context("replacement tile should exist")?;
        assert_ne!(replacement.tile_id, retired.tile_id);
        assert_ne!(
            (replacement.texture_id, replacement.bounds),
            (retired.texture_id, retired.bounds)
        );
        atlas.finish_frame(frame, &usage([keeper, replacement]));
        Ok(())
    }

    #[test]
    fn content_formats_report_actual_resident_page_bytes() -> Result<()> {
        let atlas = atlas();
        atlas
            .get_or_insert_with(image_key(1), &mut build_tile)?
            .context("image tile should exist")?;
        let svg_key = AtlasKey::Svg(RenderSvgParams {
            path: "test.svg".into(),
            size: TILE_SIZE,
        });
        atlas
            .get_or_insert_with(svg_key, &mut || {
                Ok(Some((TILE_SIZE, Cow::Borrowed(&[255]))))
            })?
            .context("SVG tile should exist")?;

        let snapshot = atlas.snapshot();
        assert_eq!(snapshot.page_count, 2);
        assert_eq!(snapshot.resident_bytes, 20);
        Ok(())
    }

    #[test]
    fn oversized_entries_fail_without_creating_a_texture() {
        let atlas = atlas();
        let oversized = Size {
            width: DevicePixels(3),
            height: DevicePixels(3),
        };
        atlas
            .get_or_insert_with(image_key(1), &mut || {
                Ok(Some((oversized, Cow::Owned(vec![0; 3 * 3 * 4]))))
            })
            .expect_err("oversized entry should fail");
        assert_eq!(atlas.snapshot().page_count, 0);
        assert_eq!(atlas.with_backend(|backend| backend.create_calls), 0);
    }

    #[test]
    fn identity_exhaustion_fails_without_leaking_pages() {
        let texture_exhausted = atlas();
        texture_exhausted.state.lock().next_texture_id = u32::MAX;
        texture_exhausted
            .get_or_insert_with(image_key(1), &mut build_tile)
            .expect_err("texture identity exhaustion should fail");
        assert_eq!(texture_exhausted.snapshot().page_count, 0);
        assert_eq!(
            texture_exhausted.with_backend(|backend| backend.textures.len()),
            0
        );

        let tile_exhausted = atlas();
        tile_exhausted.state.lock().next_tile_id = u32::MAX;
        tile_exhausted
            .get_or_insert_with(image_key(1), &mut build_tile)
            .expect_err("tile identity exhaustion should fail");
        assert_eq!(tile_exhausted.snapshot().page_count, 0);
        assert_eq!(
            tile_exhausted.with_backend(|backend| backend.textures.len()),
            0
        );
    }
}
