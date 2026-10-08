mod font_fallbacks;
mod font_features;
mod line;
mod line_layout;
mod line_wrapper;

pub use font_fallbacks::*;
pub use font_features::*;
pub use line::*;
pub use line_layout::*;
pub use line_wrapper::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    Bounds, DevicePixels, Hsla, Pixels, PlatformTextSystem, Point, Result, SharedString, Size,
    StrikethroughStyle, TextRenderingMode, UnderlineStyle, px,
};
use anyhow::{Context as _, anyhow};
use collections::FxHashMap;
use core::fmt;
use derive_more::{Add, Deref, FromStr, Sub};
use itertools::Itertools;
use parking_lot::{Mutex, RwLock, RwLockUpgradableReadGuard};
use smallvec::{SmallVec, smallvec};
use std::{
    borrow::Cow,
    cmp,
    fmt::{Debug, Display, Formatter},
    hash::{Hash, Hasher},
    ops::{Deref, DerefMut, Range},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

/// An opaque identifier for a specific font.
#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
#[repr(C)]
pub struct FontId(pub usize);

/// An opaque identifier for a specific font family.
#[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub struct FontFamilyId(pub usize);

/// Number of subpixel glyph variants along the X axis.
pub const SUBPIXEL_VARIANTS_X: u8 = 4;

/// Number of subpixel glyph variants along the Y axis.
pub const SUBPIXEL_VARIANTS_Y: u8 = 1;

/// Pixel format produced by a platform glyph rasterizer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GlyphRasterFormat {
    /// One byte per pixel of tintable grayscale coverage.
    Alpha8,
    /// Four bytes per pixel of tintable subpixel coverage in BGRA channel order.
    SubpixelBgra8,
    /// Four bytes per pixel of untinted straight-alpha color in BGRA channel order.
    ColorBgra8,
}

/// Bounds and pixel format for one rasterized glyph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GlyphRasterInfo {
    /// Device-pixel bounds relative to the quantized glyph origin.
    pub bounds: Bounds<DevicePixels>,
    /// Pixel format the rasterizer will produce for this glyph.
    pub format: GlyphRasterFormat,
}

/// Pixel data and its authoritative raster metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RasterizedGlyph {
    /// Bounds and format corresponding to `pixels`.
    pub info: GlyphRasterInfo,
    /// Tightly packed rows in the format described by `info`.
    pub pixels: Vec<u8>,
}

const DEFAULT_RASTER_INFO_CACHE_STRIKES: usize = 256;
const DEFAULT_RASTER_INFO_CACHE_BYTES: usize = 8 * 1024 * 1024;
const ESTIMATED_HASH_ENTRY_OVERHEAD: usize = 32;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct GlyphStrikeKey {
    font_id: FontId,
    font_size_bits: u32,
    scale_factor_bits: u32,
    synthetic_italic: SyntheticItalic,
    synthetic_bold: SyntheticBold,
    is_emoji: bool,
    subpixel_rendering: bool,
    dilation: u8,
}

impl From<&RenderGlyphParams> for GlyphStrikeKey {
    fn from(params: &RenderGlyphParams) -> Self {
        Self {
            font_id: params.font_id,
            font_size_bits: params.font_size.0.to_bits(),
            scale_factor_bits: params.scale_factor.to_bits(),
            synthetic_italic: params.synthetic_italic,
            synthetic_bold: params.synthetic_bold,
            is_emoji: params.is_emoji,
            subpixel_rendering: params.subpixel_rendering,
            dilation: params.dilation,
        }
    }
}

impl GlyphStrikeKey {
    #[inline(always)]
    fn matches(self, params: &RenderGlyphParams) -> bool {
        self.font_id == params.font_id
            && self.font_size_bits == params.font_size.0.to_bits()
            && self.scale_factor_bits == params.scale_factor.to_bits()
            && self.synthetic_italic == params.synthetic_italic
            && self.synthetic_bold == params.synthetic_bold
            && self.is_emoji == params.is_emoji
            && self.subpixel_rendering == params.subpixel_rendering
            && self.dilation == params.dilation
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct PackedGlyphKey(u64);

impl PackedGlyphKey {
    #[inline(always)]
    fn new(params: &RenderGlyphParams) -> Result<Self> {
        anyhow::ensure!(
            params.subpixel_variant.x < SUBPIXEL_VARIANTS_X,
            "glyph subpixel x variant {} exceeds {} variants",
            params.subpixel_variant.x,
            SUBPIXEL_VARIANTS_X,
        );
        anyhow::ensure!(
            params.subpixel_variant.y < SUBPIXEL_VARIANTS_Y,
            "glyph subpixel y variant {} exceeds {} variants",
            params.subpixel_variant.y,
            SUBPIXEL_VARIANTS_Y,
        );
        Ok(Self(
            u64::from(params.glyph_id.0)
                | (u64::from(params.subpixel_variant.x) << 32)
                | (u64::from(params.subpixel_variant.y) << 40),
        ))
    }
}

struct GlyphStrike {
    glyphs: FxHashMap<PackedGlyphKey, GlyphRasterInfo>,
    last_used: u64,
    estimated_bytes: usize,
}

struct CachedGlyphStrike {
    key: GlyphStrikeKey,
    strike: GlyphStrike,
}

impl GlyphStrike {
    fn new(last_used: u64) -> Self {
        Self {
            glyphs: FxHashMap::default(),
            last_used,
            estimated_bytes: Self::base_estimated_bytes(),
        }
    }

    fn base_estimated_bytes() -> usize {
        std::mem::size_of::<GlyphStrikeKey>()
            .saturating_add(std::mem::size_of::<Self>())
            .saturating_add(ESTIMATED_HASH_ENTRY_OVERHEAD)
    }

    fn glyph_estimated_bytes() -> usize {
        std::mem::size_of::<PackedGlyphKey>()
            .saturating_add(std::mem::size_of::<GlyphRasterInfo>())
            .saturating_add(ESTIMATED_HASH_ENTRY_OVERHEAD)
    }
}

struct GlyphRasterInfoCache {
    strikes: Vec<CachedGlyphStrike>,
    strike_indices: FxHashMap<GlyphStrikeKey, usize>,
    hot_strike_index: Option<usize>,
    max_strikes: usize,
    max_bytes: usize,
    estimated_bytes: usize,
    access_serial: u64,
    hits: AtomicU64,
    misses: u64,
    evictions: u64,
    record_hits: bool,
}

impl Default for GlyphRasterInfoCache {
    fn default() -> Self {
        Self {
            strikes: Vec::new(),
            strike_indices: FxHashMap::default(),
            hot_strike_index: None,
            max_strikes: DEFAULT_RASTER_INFO_CACHE_STRIKES,
            max_bytes: DEFAULT_RASTER_INFO_CACHE_BYTES,
            estimated_bytes: 0,
            access_serial: 0,
            hits: AtomicU64::new(0),
            misses: 0,
            evictions: 0,
            record_hits: false,
        }
    }
}

impl GlyphRasterInfoCache {
    fn strike_count(&self) -> usize {
        self.strikes.len()
    }

    fn next_access_serial(&mut self) -> u64 {
        self.access_serial = self.access_serial.saturating_add(1);
        self.access_serial
    }

    #[inline(always)]
    fn get_hot(
        &self,
        params: &RenderGlyphParams,
        glyph_key: PackedGlyphKey,
    ) -> Option<GlyphRasterInfo> {
        let entry = self.strikes.get(self.hot_strike_index?)?;
        if !entry.key.matches(params) {
            return None;
        }
        let info = entry.strike.glyphs.get(&glyph_key).copied()?;
        // The hot strike remains the most recent until a different strike is promoted.
        if self.record_hits {
            self.hits.fetch_add(1, Ordering::Relaxed);
        }
        Some(info)
    }

    fn get_or_promote(
        &mut self,
        strike_key: GlyphStrikeKey,
        glyph_key: PackedGlyphKey,
    ) -> Option<GlyphRasterInfo> {
        let access_serial = self.next_access_serial();
        let info = self
            .strike_indices
            .get(&strike_key)
            .copied()
            .and_then(|index| {
                self.hot_strike_index = Some(index);
                let entry = self.strikes.get_mut(index)?;
                entry.strike.last_used = access_serial;
                entry.strike.glyphs.get(&glyph_key).copied()
            });
        if info.is_some() {
            if self.record_hits {
                self.hits.fetch_add(1, Ordering::Relaxed);
            }
        } else {
            self.misses = self.misses.saturating_add(1);
        }
        info
    }

    fn insert_or_get(
        &mut self,
        strike_key: GlyphStrikeKey,
        glyph_key: PackedGlyphKey,
        info: GlyphRasterInfo,
    ) -> GlyphRasterInfo {
        let access_serial = self.next_access_serial();
        let strike_index = if let Some(index) = self.strike_indices.get(&strike_key).copied() {
            self.hot_strike_index = Some(index);
            if let Some(entry) = self.strikes.get_mut(index) {
                entry.strike.last_used = access_serial;
            }
            if let Some(cached) = self
                .strikes
                .get(index)
                .and_then(|entry| entry.strike.glyphs.get(&glyph_key))
            {
                return *cached;
            }
            index
        } else {
            let strike = GlyphStrike::new(access_serial);
            self.estimated_bytes = self.estimated_bytes.saturating_add(strike.estimated_bytes);
            let index = self.strikes.len();
            self.strikes.push(CachedGlyphStrike {
                key: strike_key,
                strike,
            });
            self.strike_indices.insert(strike_key, index);
            index
        };
        self.hot_strike_index = Some(strike_index);
        let Some(entry) = self.strikes.get_mut(strike_index) else {
            log::error!("glyph raster-info strike index disappeared during insertion");
            return info;
        };
        entry.strike.glyphs.insert(glyph_key, info);
        let glyph_bytes = GlyphStrike::glyph_estimated_bytes();
        entry.strike.estimated_bytes = entry.strike.estimated_bytes.saturating_add(glyph_bytes);
        self.estimated_bytes = self.estimated_bytes.saturating_add(glyph_bytes);
        self.enforce_limits();
        info
    }

    fn enforce_limits(&mut self) {
        while self.strike_count() > self.max_strikes || self.estimated_bytes > self.max_bytes {
            let Some(lru_index) = self
                .strikes
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.strike.last_used)
                .map(|(index, _)| index)
            else {
                self.estimated_bytes = 0;
                break;
            };
            let removed = self.strikes.swap_remove(lru_index);
            self.strike_indices.remove(&removed.key);
            if let Some(swapped) = self.strikes.get(lru_index) {
                self.strike_indices.insert(swapped.key, lru_index);
            }
            self.hot_strike_index = None;
            self.estimated_bytes = self
                .estimated_bytes
                .saturating_sub(removed.strike.estimated_bytes);
            self.evictions = self.evictions.saturating_add(1);
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    fn configure_for_test(&mut self, max_strikes: usize, max_bytes: usize) {
        self.strikes.clear();
        self.strike_indices.clear();
        self.hot_strike_index = None;
        self.max_strikes = max_strikes;
        self.max_bytes = max_bytes;
        self.estimated_bytes = 0;
        self.access_serial = 0;
        self.hits.store(0, Ordering::Relaxed);
        self.misses = 0;
        self.evictions = 0;
        self.record_hits = true;
    }

    #[cfg(any(test, feature = "test-support"))]
    fn set_record_hits_for_test(&mut self, record_hits: bool) {
        self.record_hits = record_hits;
    }

    #[cfg(any(test, feature = "test-support"))]
    fn snapshot(&self) -> GlyphRasterInfoCacheSnapshot {
        GlyphRasterInfoCacheSnapshot {
            strike_count: self.strike_count(),
            entry_count: self
                .strikes
                .iter()
                .map(|entry| entry.strike.glyphs.len())
                .sum(),
            estimated_bytes: self.estimated_bytes,
            max_strikes: self.max_strikes,
            max_bytes: self.max_bytes,
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses,
            evictions: self.evictions,
        }
    }
}

/// Test-only diagnostics for the bounded glyph raster-info cache.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GlyphRasterInfoCacheSnapshot {
    /// Number of resident strikes.
    pub strike_count: usize,
    /// Number of resident glyph metadata entries.
    pub entry_count: usize,
    /// Conservative estimated bytes retained by the cache.
    pub estimated_bytes: usize,
    /// Configured strike-count limit.
    pub max_strikes: usize,
    /// Configured estimated-byte limit.
    pub max_bytes: usize,
    /// Successful metadata lookups.
    pub hits: u64,
    /// Metadata lookups that queried the platform rasterizer.
    pub misses: u64,
    /// Whole strikes removed by the LRU.
    pub evictions: u64,
}

/// The GPUI text rendering sub system.
pub struct TextSystem {
    platform_text_system: Arc<dyn PlatformTextSystem>,
    font_ids_by_font: RwLock<FxHashMap<Font, Result<FontId>>>,
    font_metrics: RwLock<FxHashMap<FontId, FontMetrics>>,
    raster_info: RwLock<GlyphRasterInfoCache>,
    wrapper_pool: Mutex<FxHashMap<FontIdWithSize, Vec<LineWrapper>>>,
    font_runs_pool: Mutex<Vec<Vec<FontRun>>>,
    fallback_font_stack: SmallVec<[Font; 2]>,
}

impl TextSystem {
    /// Create a new TextSystem with the given platform text system.
    pub fn new(platform_text_system: Arc<dyn PlatformTextSystem>) -> Self {
        TextSystem {
            platform_text_system,
            font_metrics: RwLock::default(),
            raster_info: RwLock::default(),
            font_ids_by_font: RwLock::default(),
            wrapper_pool: Mutex::default(),
            font_runs_pool: Mutex::default(),
            fallback_font_stack: smallvec![
                // TODO: Remove this when Linux have implemented setting fallbacks.
                font(".ZZZMono"),
                font(".ZZZSans"),
                font("Helvetica"),
                font("Segoe UI"),     // Windows
                font("Ubuntu"),       // Gnome (Ubuntu)
                font("Adwaita Sans"), // Gnome 47
                font("Cantarell"),    // Gnome
                font("Noto Sans"),    // KDE
                font("DejaVu Sans"),
                font("Arial"), // macOS, Windows
            ],
        }
    }

    /// Reconfigures and clears the glyph raster-info cache for tests.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_raster_info_cache_limits_for_test(&self, max_strikes: usize, max_bytes: usize) {
        self.raster_info
            .write()
            .configure_for_test(max_strikes, max_bytes);
    }

    /// Returns test-only diagnostics for the glyph raster-info cache.
    #[cfg(any(test, feature = "test-support"))]
    pub fn raster_info_cache_snapshot_for_test(&self) -> GlyphRasterInfoCacheSnapshot {
        self.raster_info.read().snapshot()
    }

    /// Enables or disables test-only warm-hit accounting.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_raster_info_cache_hit_accounting_for_test(&self, enabled: bool) {
        self.raster_info.write().set_record_hits_for_test(enabled);
    }

    /// Queries glyph raster metadata through the bounded cache for tests.
    #[cfg(any(test, feature = "test-support"))]
    #[inline]
    pub fn glyph_raster_info_for_test(
        &self,
        params: &RenderGlyphParams,
    ) -> Result<GlyphRasterInfo> {
        self.raster_info(params)
    }

    /// Get sorted, unique font family names available to the platform text system.
    ///
    /// Includes fonts registered with [`Self::add_fonts`].
    pub fn all_font_names(&self) -> Vec<String> {
        let mut names = self.platform_text_system.all_font_names();
        names.sort_unstable();
        names.dedup();
        names
    }

    /// Add a font's data to the text system.
    pub fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        self.platform_text_system.add_fonts(fonts)
    }

    /// Get the FontId for the configure font family and style.
    fn font_id(&self, font: &Font) -> Result<FontId> {
        fn clone_font_id_result(font_id: &Result<FontId>) -> Result<FontId> {
            match font_id {
                Ok(font_id) => Ok(*font_id),
                Err(err) => Err(anyhow!("{err}")),
            }
        }

        let font_id = self
            .font_ids_by_font
            .read()
            .get(font)
            .map(clone_font_id_result);
        if let Some(font_id) = font_id {
            font_id
        } else {
            let font_id = self.platform_text_system.font_id(font);
            self.font_ids_by_font
                .write()
                .insert(font.clone(), clone_font_id_result(&font_id));
            font_id
        }
    }

    /// Get the Font for the Font Id.
    pub fn get_font_for_id(&self, id: FontId) -> Option<Font> {
        let lock = self.font_ids_by_font.read();
        lock.iter().find_map(|(font, result)| match result {
            Ok(font_id) if *font_id == id => Some(font.clone()),
            _ => None,
        })
    }

    /// Resolves the specified font, falling back to the default font stack if
    /// the font fails to load.
    ///
    /// # Panics
    ///
    /// Panics if the font and none of the fallbacks can be resolved.
    pub fn resolve_font(&self, font: &Font) -> FontId {
        if let Ok(font_id) = self.font_id(font) {
            return font_id;
        }
        for fallback in &self.fallback_font_stack {
            if let Ok(font_id) = self.font_id(fallback) {
                return font_id;
            }
        }

        panic!(
            "failed to resolve font '{}' or any of the fallbacks: {}",
            font.family,
            self.fallback_font_stack
                .iter()
                .map(|fallback| &fallback.family)
                .join(", ")
        );
    }

    /// Prewarm any system font caches needed to shape text.
    ///
    /// This may be expensive, so callers should generally invoke it on a
    /// background executor. Missing entries are still populated on demand by
    /// the normal shaping path.
    pub fn prewarm_fonts(&self, fonts: &[Font]) {
        let mut font_ids = SmallVec::<[FontId; 8]>::new();
        for font in fonts {
            let font_id = self.resolve_font(font);
            if !font_ids.contains(&font_id) {
                font_ids.push(font_id);
            }
        }
        self.platform_text_system.prewarm_fonts(&font_ids);
    }

    /// Get the bounding box for the given font and font size.
    /// A font's bounding box is the smallest rectangle that could enclose all glyphs
    /// in the font. superimposed over one another.
    pub fn bounding_box(&self, font_id: FontId, font_size: Pixels) -> Bounds<Pixels> {
        self.read_metrics(font_id, |metrics| metrics.bounding_box(font_size))
    }

    /// Get the typographic bounds for the given character, in the given font and size.
    pub fn typographic_bounds(
        &self,
        font_id: FontId,
        font_size: Pixels,
        character: char,
    ) -> Result<Bounds<Pixels>> {
        let glyph_id = self
            .platform_text_system
            .glyph_for_char(font_id, character)
            .with_context(|| format!("glyph not found for character '{character}'"))?;
        let bounds = self
            .platform_text_system
            .typographic_bounds(font_id, glyph_id)?;
        Ok(self.read_metrics(font_id, |metrics| {
            (bounds / metrics.units_per_em as f32 * font_size.0).map(px)
        }))
    }

    /// Get the advance width for the given character, in the given font and size.
    pub fn advance(&self, font_id: FontId, font_size: Pixels, ch: char) -> Result<Size<Pixels>> {
        let glyph_id = self
            .platform_text_system
            .glyph_for_char(font_id, ch)
            .with_context(|| format!("glyph not found for character '{ch}'"))?;
        let result = self.platform_text_system.advance(font_id, glyph_id)?
            / self.units_per_em(font_id) as f32;

        Ok(result * font_size)
    }

    // Consider removing this?
    /// Returns the shaped layout width of for the given character, in the given font and size.
    pub fn layout_width(&self, font_id: FontId, font_size: Pixels, ch: char) -> Pixels {
        let mut buffer = [0; 4];
        let buffer = ch.encode_utf8(&mut buffer);
        self.platform_text_system
            .layout_line(
                buffer,
                font_size,
                &[FontRun {
                    len: buffer.len(),
                    font_id,
                    font_style: FontStyle::Normal,
                    font_weight: FontWeight::NORMAL,
                }],
            )
            .width
    }

    /// Returns the width of an `em`.
    ///
    /// Uses the width of the `m` character in the given font and size.
    pub fn em_width(&self, font_id: FontId, font_size: Pixels) -> Result<Pixels> {
        Ok(self.typographic_bounds(font_id, font_size, 'm')?.size.width)
    }

    /// Returns the advance width of an `em`.
    ///
    /// Uses the advance width of the `m` character in the given font and size.
    pub fn em_advance(&self, font_id: FontId, font_size: Pixels) -> Result<Pixels> {
        Ok(self.advance(font_id, font_size, 'm')?.width)
    }

    // Consider removing this?
    /// Returns the shaped layout width of an `em`.
    pub fn em_layout_width(&self, font_id: FontId, font_size: Pixels) -> Pixels {
        self.layout_width(font_id, font_size, 'm')
    }

    /// Returns the width of an `ch`.
    ///
    /// Uses the width of the `0` character in the given font and size.
    pub fn ch_width(&self, font_id: FontId, font_size: Pixels) -> Result<Pixels> {
        Ok(self.typographic_bounds(font_id, font_size, '0')?.size.width)
    }

    /// Returns the advance width of an `ch`.
    ///
    /// Uses the advance width of the `0` character in the given font and size.
    pub fn ch_advance(&self, font_id: FontId, font_size: Pixels) -> Result<Pixels> {
        Ok(self.advance(font_id, font_size, '0')?.width)
    }

    /// Get the number of font size units per 'em square',
    /// Per MDN: "an abstract square whose height is the intended distance between
    /// lines of type in the same type size"
    pub fn units_per_em(&self, font_id: FontId) -> u32 {
        self.read_metrics(font_id, |metrics| metrics.units_per_em)
    }

    /// Get the height of a capital letter in the given font and size.
    pub fn cap_height(&self, font_id: FontId, font_size: Pixels) -> Pixels {
        self.read_metrics(font_id, |metrics| metrics.cap_height(font_size))
    }

    /// Get the height of the x character in the given font and size.
    pub fn x_height(&self, font_id: FontId, font_size: Pixels) -> Pixels {
        self.read_metrics(font_id, |metrics| metrics.x_height(font_size))
    }

    /// Get the recommended distance from the baseline for the given font
    pub fn ascent(&self, font_id: FontId, font_size: Pixels) -> Pixels {
        self.read_metrics(font_id, |metrics| metrics.ascent(font_size))
    }

    /// Get the recommended distance below the baseline for the given font,
    /// in single spaced text.
    pub fn descent(&self, font_id: FontId, font_size: Pixels) -> Pixels {
        self.read_metrics(font_id, |metrics| metrics.descent(font_size))
    }

    /// Get the recommended baseline offset for the given font and line height.
    pub fn baseline_offset(
        &self,
        font_id: FontId,
        font_size: Pixels,
        line_height: Pixels,
    ) -> Pixels {
        let ascent = self.ascent(font_id, font_size);
        let descent = self.descent(font_id, font_size);
        let padding_top = (line_height - ascent - descent) / 2.;
        padding_top + ascent
    }

    fn read_metrics<T>(&self, font_id: FontId, read: impl FnOnce(&FontMetrics) -> T) -> T {
        let lock = self.font_metrics.upgradable_read();

        if let Some(metrics) = lock.get(&font_id) {
            read(metrics)
        } else {
            let mut lock = RwLockUpgradableReadGuard::upgrade(lock);
            let metrics = lock
                .entry(font_id)
                .or_insert_with(|| self.platform_text_system.font_metrics(font_id));
            read(metrics)
        }
    }

    /// Returns a handle to a line wrapper, for the given font and font size.
    pub fn line_wrapper(self: &Arc<Self>, font: Font, font_size: Pixels) -> LineWrapperHandle {
        let lock = &mut self.wrapper_pool.lock();
        let font_id = self.resolve_font(&font);
        let wrappers = lock
            .entry(FontIdWithSize { font_id, font_size })
            .or_default();
        let wrapper = wrappers
            .pop()
            .unwrap_or_else(|| LineWrapper::new(font_id, font_size, self.clone()));

        LineWrapperHandle {
            wrapper: Some(wrapper),
            text_system: self.clone(),
        }
    }

    /// Get the rasterized size and location of a specific, rendered glyph.
    #[inline]
    pub(crate) fn raster_info(&self, params: &RenderGlyphParams) -> Result<GlyphRasterInfo> {
        let glyph_key = PackedGlyphKey::new(params)?;
        if let Some(info) = self.raster_info.read().get_hot(params, glyph_key) {
            return Ok(info);
        }
        let strike_key = GlyphStrikeKey::from(params);
        if let Some(info) = self
            .raster_info
            .write()
            .get_or_promote(strike_key, glyph_key)
        {
            return Ok(info);
        }
        let info = self.platform_text_system.glyph_raster_info(params)?;
        Ok(self
            .raster_info
            .write()
            .insert_or_get(strike_key, glyph_key, info))
    }

    pub(crate) fn rasterize_glyph(&self, params: &RenderGlyphParams) -> Result<RasterizedGlyph> {
        let raster_info = self.raster_info(params)?;
        let rasterized = self
            .platform_text_system
            .rasterize_glyph(params, raster_info)?;
        anyhow::ensure!(
            rasterized.info == raster_info,
            "glyph raster format or bounds changed between info and pixel queries"
        );
        Ok(rasterized)
    }

    /// Returns the dilation level to use for a glyph painted in the given color.
    pub(crate) fn glyph_dilation_for_color(&self, color: Hsla) -> u8 {
        self.platform_text_system.glyph_dilation_for_color(color)
    }

    /// Returns the text rendering mode recommended by the platform for the given font and size.
    /// The return value will never be [`TextRenderingMode::PlatformDefault`].
    pub(crate) fn recommended_rendering_mode(
        &self,
        font_id: FontId,
        font_size: Pixels,
    ) -> TextRenderingMode {
        self.platform_text_system
            .recommended_rendering_mode(font_id, font_size)
    }
}

/// The GPUI text layout subsystem.
#[derive(Deref)]
pub struct WindowTextSystem {
    line_layout_cache: LineLayoutCache,
    #[deref]
    text_system: Arc<TextSystem>,
}

impl WindowTextSystem {
    /// Create a new WindowTextSystem with the given TextSystem.
    pub fn new(text_system: Arc<TextSystem>) -> Self {
        Self {
            line_layout_cache: LineLayoutCache::new(text_system.platform_text_system.clone()),
            text_system,
        }
    }

    pub(crate) fn layout_index(&self) -> LineLayoutIndex {
        self.line_layout_cache.layout_index()
    }

    pub(crate) fn reuse_layouts(&self, index: Range<LineLayoutIndex>) {
        self.line_layout_cache.reuse_layouts(index)
    }

    pub(crate) fn truncate_layouts(&self, index: LineLayoutIndex) {
        self.line_layout_cache.truncate_layouts(index)
    }

    /// Shape the given line, at the given font_size, for painting to the screen.
    /// Subsets of the line can be styled independently with the `runs` parameter.
    ///
    /// Note that this method can only shape a single line of text. It will panic
    /// if the text contains newlines. If you need to shape multiple lines of text,
    /// use [`Self::shape_text`] instead.
    pub fn shape_line(
        &self,
        text: SharedString,
        font_size: Pixels,
        runs: &[TextRun],
        force_width: Option<Pixels>,
    ) -> ShapedLine {
        debug_assert!(
            text.find('\n').is_none(),
            "text argument should not contain newlines"
        );

        let mut decoration_runs = SmallVec::<[DecorationRun; 32]>::new();
        for run in runs {
            if let Some(last_run) = decoration_runs.last_mut()
                && last_run.color == run.color
                && last_run.underline == run.underline
                && last_run.strikethrough == run.strikethrough
                && last_run.background_color == run.background_color
            {
                last_run.len += run.len as u32;
                continue;
            }
            decoration_runs.push(DecorationRun {
                len: run.len as u32,
                color: run.color,
                background_color: run.background_color,
                underline: run.underline,
                strikethrough: run.strikethrough,
            });
        }

        let layout = self.layout_line(&text, font_size, runs, force_width);

        ShapedLine {
            layout,
            text,
            decoration_runs,
        }
    }

    /// Shape the given line using a caller-provided content hash as the cache key.
    ///
    /// This enables cache hits without materializing a contiguous `SharedString` for the text.
    /// If the cache misses, `materialize_text` is invoked to produce the `SharedString` for shaping.
    ///
    /// Contract (caller enforced):
    /// - Same `text_hash` implies identical text content (collision risk accepted by caller).
    /// - `text_len` should be the UTF-8 byte length of the text (helps reduce accidental collisions).
    ///
    /// Like [`Self::shape_line`], this must be used only for single-line text (no `\n`).
    pub fn shape_line_by_hash(
        &self,
        text_hash: u64,
        text_len: usize,
        font_size: Pixels,
        runs: &[TextRun],
        force_width: Option<Pixels>,
        materialize_text: impl FnOnce() -> SharedString,
    ) -> ShapedLine {
        let mut decoration_runs = SmallVec::<[DecorationRun; 32]>::new();
        for run in runs {
            if let Some(last_run) = decoration_runs.last_mut()
                && last_run.color == run.color
                && last_run.underline == run.underline
                && last_run.strikethrough == run.strikethrough
                && last_run.background_color == run.background_color
            {
                last_run.len += run.len as u32;
                continue;
            }
            decoration_runs.push(DecorationRun {
                len: run.len as u32,
                color: run.color,
                background_color: run.background_color,
                underline: run.underline,
                strikethrough: run.strikethrough,
            });
        }

        let mut used_force_width = force_width;
        let layout = self.layout_line_by_hash(
            text_hash,
            text_len,
            font_size,
            runs,
            used_force_width,
            || {
                let text = materialize_text();
                debug_assert!(
                    text.find('\n').is_none(),
                    "text argument should not contain newlines"
                );
                text
            },
        );

        // We only materialize actual text on cache miss; on hit we avoid allocations.
        // Since `ShapedLine` carries a `SharedString`, use an empty placeholder for hits.
        // NOTE: Callers must not rely on `ShapedLine.text` for content when using this API.
        let text: SharedString = SharedString::new_static("");

        ShapedLine {
            layout,
            text,
            decoration_runs,
        }
    }

    /// Shape a multi line string of text, at the given font_size, for painting to the screen.
    /// Subsets of the text can be styled independently with the `runs` parameter.
    /// If `wrap_width` is provided, the line breaks will be adjusted to fit within the given width.
    pub fn shape_text(
        &self,
        text: SharedString,
        font_size: Pixels,
        runs: &[TextRun],
        wrap_width: Option<Pixels>,
        line_clamp: Option<usize>,
    ) -> Result<SmallVec<[WrappedLine; 1]>> {
        let mut runs = runs.iter().filter(|run| run.len > 0).cloned().peekable();
        let mut font_runs = self.font_runs_pool.lock().pop().unwrap_or_default();

        let mut lines = SmallVec::new();
        let mut max_wrap_lines = line_clamp;
        let mut wrapped_lines = 0;

        let mut process_line = |line_text: SharedString, line_start, line_end| {
            font_runs.clear();

            let mut decoration_runs = <Vec<DecorationRun>>::with_capacity(32);
            let mut run_start = line_start;
            while run_start < line_end {
                let Some(run) = runs.peek_mut() else {
                    log::warn!("`TextRun`s do not cover the entire to be shaped text");
                    break;
                };

                let run_len_within_line = cmp::min(line_end - run_start, run.len);

                let decoration_changed = if let Some(last_run) = decoration_runs.last_mut()
                    && last_run.color == run.color
                    && last_run.underline == run.underline
                    && last_run.strikethrough == run.strikethrough
                    && last_run.background_color == run.background_color
                {
                    last_run.len += run_len_within_line as u32;
                    false
                } else {
                    decoration_runs.push(DecorationRun {
                        len: run_len_within_line as u32,
                        color: run.color,
                        background_color: run.background_color,
                        underline: run.underline,
                        strikethrough: run.strikethrough,
                    });
                    true
                };

                let font_id = self.resolve_font(&run.font);
                if let Some(font_run) = font_runs.last_mut()
                    && font_id == font_run.font_id
                    && run.font.style == font_run.font_style
                    && run.font.weight == font_run.font_weight
                    && !decoration_changed
                {
                    font_run.len += run_len_within_line;
                } else {
                    font_runs.push(FontRun {
                        len: run_len_within_line,
                        font_id,
                        font_style: run.font.style,
                        font_weight: run.font.weight,
                    });
                }

                // Preserve the remainder of the run for the next line
                run.len -= run_len_within_line;
                if run.len == 0 {
                    runs.next();
                }
                run_start += run_len_within_line;
            }

            let layout = self.line_layout_cache.layout_wrapped_line(
                &line_text,
                font_size,
                &font_runs,
                wrap_width,
                max_wrap_lines.map(|max| max.saturating_sub(wrapped_lines)),
            );
            wrapped_lines += layout.wrap_boundaries.len();

            lines.push(WrappedLine {
                layout,
                decoration_runs,
                text: line_text,
            });

            // Skip `\n` character.
            if let Some(run) = runs.peek_mut() {
                run.len -= 1;
                if run.len == 0 {
                    runs.next();
                }
            }
        };

        let mut split_lines = text.split('\n');

        // Special case single lines to prevent allocating a sharedstring
        if let Some(first_line) = split_lines.next()
            && let Some(second_line) = split_lines.next()
        {
            let mut line_start = 0;
            process_line(
                SharedString::new(first_line),
                line_start,
                line_start + first_line.len(),
            );
            line_start += first_line.len() + '\n'.len_utf8();
            process_line(
                SharedString::new(second_line),
                line_start,
                line_start + second_line.len(),
            );
            for line_text in split_lines {
                line_start += line_text.len() + '\n'.len_utf8();
                process_line(
                    SharedString::new(line_text),
                    line_start,
                    line_start + line_text.len(),
                );
            }
        } else {
            let end = text.len();
            process_line(text, 0, end);
        }

        self.font_runs_pool.lock().push(font_runs);

        Ok(lines)
    }

    pub(crate) fn finish_frame(&self) {
        self.line_layout_cache.finish_frame()
    }

    /// Layout the given line of text, at the given font_size.
    /// Subsets of the line can be styled independently with the `runs` parameter.
    /// Generally, you should prefer to use [`Self::shape_line`] instead, which
    /// can be painted directly.
    pub fn layout_line(
        &self,
        text: &str,
        font_size: Pixels,
        runs: &[TextRun],
        force_width: Option<Pixels>,
    ) -> Arc<LineLayout> {
        let mut last_run = None::<&TextRun>;
        let mut font_runs = self.font_runs_pool.lock().pop().unwrap_or_default();
        font_runs.clear();

        for run in runs {
            let decoration_changed = if let Some(last_run) = last_run
                && last_run.color == run.color
                && last_run.underline == run.underline
                && last_run.strikethrough == run.strikethrough
            // we do not consider differing background color relevant, as it does not affect glyphs
            // && last_run.background_color == run.background_color
            {
                false
            } else {
                last_run = Some(run);
                true
            };

            let font_id = self.resolve_font(&run.font);
            if let Some(font_run) = font_runs.last_mut()
                && font_id == font_run.font_id
                && run.font.style == font_run.font_style
                && run.font.weight == font_run.font_weight
                && !decoration_changed
            {
                font_run.len += run.len;
            } else {
                font_runs.push(FontRun {
                    len: run.len,
                    font_id,
                    font_style: run.font.style,
                    font_weight: run.font.weight,
                });
            }
        }

        let layout = self.line_layout_cache.layout_line(
            &SharedString::new(text),
            font_size,
            &font_runs,
            force_width,
        );

        self.font_runs_pool.lock().push(font_runs);

        layout
    }

    /// Probe the line layout cache using a caller-provided content hash, without allocating.
    ///
    /// Returns `Some(layout)` if the layout is already cached in either the current frame
    /// or the previous frame. Returns `None` if it is not cached.
    ///
    /// Contract (caller enforced):
    /// - Same `text_hash` implies identical text content (collision risk accepted by caller).
    /// - `text_len` should be the UTF-8 byte length of the text (helps reduce accidental collisions).
    pub fn try_layout_line_by_hash(
        &self,
        text_hash: u64,
        text_len: usize,
        font_size: Pixels,
        runs: &[TextRun],
        force_width: Option<Pixels>,
    ) -> Option<Arc<LineLayout>> {
        let mut last_run = None::<&TextRun>;
        let mut font_runs = self.font_runs_pool.lock().pop().unwrap_or_default();
        font_runs.clear();

        for run in runs {
            let decoration_changed = if let Some(last_run) = last_run
                && last_run.color == run.color
                && last_run.underline == run.underline
                && last_run.strikethrough == run.strikethrough
            // we do not consider differing background color relevant, as it does not affect glyphs
            // && last_run.background_color == run.background_color
            {
                false
            } else {
                last_run = Some(run);
                true
            };

            let font_id = self.resolve_font(&run.font);
            if let Some(font_run) = font_runs.last_mut()
                && font_id == font_run.font_id
                && run.font.style == font_run.font_style
                && run.font.weight == font_run.font_weight
                && !decoration_changed
            {
                font_run.len += run.len;
            } else {
                font_runs.push(FontRun {
                    len: run.len,
                    font_id,
                    font_style: run.font.style,
                    font_weight: run.font.weight,
                });
            }
        }

        let layout = self.line_layout_cache.try_layout_line_by_hash(
            text_hash,
            text_len,
            font_size,
            &font_runs,
            force_width,
        );

        self.font_runs_pool.lock().push(font_runs);

        layout
    }

    /// Layout the given line of text using a caller-provided content hash as the cache key.
    ///
    /// This enables cache hits without materializing a contiguous `SharedString` for the text.
    /// If the cache misses, `materialize_text` is invoked to produce the `SharedString` for shaping.
    ///
    /// Contract (caller enforced):
    /// - Same `text_hash` implies identical text content (collision risk accepted by caller).
    /// - `text_len` should be the UTF-8 byte length of the text (helps reduce accidental collisions).
    pub fn layout_line_by_hash(
        &self,
        text_hash: u64,
        text_len: usize,
        font_size: Pixels,
        runs: &[TextRun],
        force_width: Option<Pixels>,
        materialize_text: impl FnOnce() -> SharedString,
    ) -> Arc<LineLayout> {
        let mut last_run = None::<&TextRun>;
        let mut font_runs = self.font_runs_pool.lock().pop().unwrap_or_default();
        font_runs.clear();

        for run in runs {
            let decoration_changed = if let Some(last_run) = last_run
                && last_run.color == run.color
                && last_run.underline == run.underline
                && last_run.strikethrough == run.strikethrough
            // we do not consider differing background color relevant, as it does not affect glyphs
            // && last_run.background_color == run.background_color
            {
                false
            } else {
                last_run = Some(run);
                true
            };

            let font_id = self.resolve_font(&run.font);
            if let Some(font_run) = font_runs.last_mut()
                && font_id == font_run.font_id
                && run.font.style == font_run.font_style
                && run.font.weight == font_run.font_weight
                && !decoration_changed
            {
                font_run.len += run.len;
            } else {
                font_runs.push(FontRun {
                    len: run.len,
                    font_id,
                    font_style: run.font.style,
                    font_weight: run.font.weight,
                });
            }
        }

        let layout = self.line_layout_cache.layout_line_by_hash(
            text_hash,
            text_len,
            font_size,
            &font_runs,
            force_width,
            materialize_text,
        );

        self.font_runs_pool.lock().push(font_runs);

        layout
    }
}

#[derive(Hash, Eq, PartialEq)]
struct FontIdWithSize {
    font_id: FontId,
    font_size: Pixels,
}

/// A handle into the text system, which can be used to compute the wrapped layout of text
pub struct LineWrapperHandle {
    wrapper: Option<LineWrapper>,
    text_system: Arc<TextSystem>,
}

impl Drop for LineWrapperHandle {
    fn drop(&mut self) {
        let mut state = self.text_system.wrapper_pool.lock();
        let wrapper = self.wrapper.take().expect("entry should be present");
        state
            .get_mut(&FontIdWithSize {
                font_id: wrapper.font_id,
                font_size: wrapper.font_size,
            })
            .expect("entry should be present")
            .push(wrapper);
    }
}

impl Deref for LineWrapperHandle {
    type Target = LineWrapper;

    fn deref(&self) -> &Self::Target {
        self.wrapper
            .as_ref()
            .expect("value should have the expected type")
    }
}

impl DerefMut for LineWrapperHandle {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.wrapper
            .as_mut()
            .expect("value should have the expected type")
    }
}

/// The degree of blackness or stroke thickness of a font. This value ranges from 100.0 to 900.0,
/// with 400.0 as normal.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize, Add, Sub, FromStr)]
#[serde(transparent)]
pub struct FontWeight(pub f32);

impl Display for FontWeight {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<f32> for FontWeight {
    fn from(weight: f32) -> Self {
        FontWeight(weight)
    }
}

impl Default for FontWeight {
    #[inline]
    fn default() -> FontWeight {
        FontWeight::NORMAL
    }
}

impl Hash for FontWeight {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u32(u32::from_be_bytes(self.0.to_be_bytes()));
    }
}

impl Eq for FontWeight {}

impl FontWeight {
    /// Thin weight (100), the thinnest value.
    pub const THIN: FontWeight = FontWeight(100.0);
    /// Extra light weight (200).
    pub const EXTRA_LIGHT: FontWeight = FontWeight(200.0);
    /// Light weight (300).
    pub const LIGHT: FontWeight = FontWeight(300.0);
    /// Normal (400).
    pub const NORMAL: FontWeight = FontWeight(400.0);
    /// Medium weight (500, higher than normal).
    pub const MEDIUM: FontWeight = FontWeight(500.0);
    /// Semibold weight (600).
    pub const SEMIBOLD: FontWeight = FontWeight(600.0);
    /// Bold weight (700).
    pub const BOLD: FontWeight = FontWeight(700.0);
    /// Extra-bold weight (800).
    pub const EXTRA_BOLD: FontWeight = FontWeight(800.0);
    /// Black weight (900), the thickest value.
    pub const BLACK: FontWeight = FontWeight(900.0);

    /// All of the font weights, in order from thinnest to thickest.
    pub const ALL: [FontWeight; 9] = [
        Self::THIN,
        Self::EXTRA_LIGHT,
        Self::LIGHT,
        Self::NORMAL,
        Self::MEDIUM,
        Self::SEMIBOLD,
        Self::BOLD,
        Self::EXTRA_BOLD,
        Self::BLACK,
    ];
}

impl schemars::JsonSchema for FontWeight {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "FontWeight".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        use schemars::json_schema;
        json_schema!({
            "type": "number",
            "minimum": Self::THIN,
            "maximum": Self::BLACK,
            "default": Self::default(),
            "description": "Font weight value between 100 (thin) and 900 (black)"
        })
    }
}

/// Allows italic or oblique faces to be selected.
#[derive(Clone, Copy, Eq, PartialEq, Debug, Hash, Default, Serialize, Deserialize, JsonSchema)]
pub enum FontStyle {
    /// A face that is neither italic not obliqued.
    #[default]
    Normal,
    /// A form that is generally cursive in nature.
    Italic,
    /// A typically-sloped version of the regular face.
    Oblique,
}

impl Display for FontStyle {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Debug::fmt(self, f)
    }
}

/// A styled run of text, for use in [`crate::TextLayout`].
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TextRun {
    /// A number of utf8 bytes
    pub len: usize,
    /// The font to use for this run.
    pub font: Font,
    /// The color
    pub color: Hsla,
    /// The background color (if any)
    pub background_color: Option<Hsla>,
    /// The underline style (if any)
    pub underline: Option<UnderlineStyle>,
    /// The strikethrough style (if any)
    pub strikethrough: Option<StrikethroughStyle>,
}

#[cfg(all(target_os = "macos", test))]
impl TextRun {
    fn with_len(&self, len: usize) -> Self {
        let mut this = self.clone();
        this.len = len;
        this
    }
}

/// An identifier for a specific glyph, as returned by [`WindowTextSystem::layout_line`].
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[repr(C)]
pub struct GlyphId(pub u32);

/// Parameters for rendering a glyph, used as cache keys for raster bounds.
///
/// This struct identifies a specific glyph rendering configuration including
/// font, size, subpixel positioning, and scale factor. It's used to look up
/// cached raster bounds and sprite atlas entries.
#[derive(Clone, Debug, PartialEq)]
#[expect(missing_docs)]
pub struct RenderGlyphParams {
    pub font_id: FontId,
    pub glyph_id: GlyphId,
    pub font_size: Pixels,
    pub subpixel_variant: Point<u8>,
    pub scale_factor: f32,
    pub synthetic_italic: SyntheticItalic,
    pub synthetic_bold: SyntheticBold,
    pub is_emoji: bool,
    pub subpixel_rendering: bool,
    pub dilation: u8,
}

impl Eq for RenderGlyphParams {}

impl Hash for RenderGlyphParams {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.font_id.0.hash(state);
        self.glyph_id.0.hash(state);
        self.font_size.0.to_bits().hash(state);
        self.subpixel_variant.hash(state);
        self.scale_factor.to_bits().hash(state);
        self.synthetic_italic.hash(state);
        self.synthetic_bold.hash(state);
        self.is_emoji.hash(state);
        self.subpixel_rendering.hash(state);
        self.dilation.hash(state);
    }
}

/// A synthetic bold transform applied when a requested bold face falls back to a lighter face.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SyntheticBold {
    amount_per_em_times_1024: u16,
}

impl SyntheticBold {
    const AMOUNT_SCALE: f32 = 1024.0;
    const WEBRENDER_DEFAULT_AMOUNT: f32 = 1.0 / 48.0;

    /// Returns a disabled synthetic bold transform.
    pub fn disabled() -> Self {
        Self {
            amount_per_em_times_1024: 0,
        }
    }

    /// Returns an enabled synthetic bold transform with the given embolden amount in ems.
    pub fn enabled(amount_per_em: f32) -> Self {
        Self {
            amount_per_em_times_1024: (amount_per_em.max(0.0) * Self::AMOUNT_SCALE) as u16,
        }
    }

    /// Returns the default synthetic bold transform used by WebRender.
    pub fn default_enabled() -> Self {
        Self::enabled(Self::WEBRENDER_DEFAULT_AMOUNT)
    }

    /// Returns whether this synthetic bold transform is enabled.
    pub fn is_enabled(self) -> bool {
        self.amount_per_em_times_1024 != 0
    }

    /// Returns the embolden amount in ems.
    pub fn amount(self) -> f32 {
        self.amount_per_em_times_1024 as f32 / Self::AMOUNT_SCALE
    }

    /// Returns the embolden amount in device pixels, matching WebRender's default scaling.
    pub fn device_pixel_amount(self, font_size: Pixels, scale_factor: f32) -> f32 {
        if !self.is_enabled() {
            return 0.0;
        }

        let mut amount = font_size.0 * scale_factor * self.amount();
        if amount < 1.0 {
            amount = 0.25 + 0.75 * amount;
        }
        amount.max(1.0)
    }
}

impl Default for SyntheticBold {
    fn default() -> Self {
        Self::disabled()
    }
}

/// A synthetic italic transform applied when a requested italic face falls back to an upright face.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SyntheticItalic {
    angle_degrees_times_256: i16,
}

impl SyntheticItalic {
    const ANGLE_SCALE: f32 = 256.0;

    /// Returns the default synthetic italic transform.
    pub fn enabled() -> Self {
        Self::from_degrees(14.0)
    }

    /// Returns a disabled synthetic italic transform.
    pub fn disabled() -> Self {
        Self {
            angle_degrees_times_256: 0,
        }
    }

    /// Creates a synthetic italic transform from an angle in degrees.
    pub fn from_degrees(degrees: f32) -> Self {
        Self {
            angle_degrees_times_256: (degrees.clamp(-89.0, 89.0) * Self::ANGLE_SCALE) as i16,
        }
    }

    /// Returns whether this synthetic italic transform is enabled.
    pub fn is_enabled(self) -> bool {
        self.angle_degrees_times_256 != 0
    }

    /// Returns the synthetic italic angle in degrees.
    pub fn to_degrees(self) -> f32 {
        self.angle_degrees_times_256 as f32 / Self::ANGLE_SCALE
    }

    /// Returns the synthetic italic angle in radians.
    pub fn to_radians(self) -> f32 {
        self.to_degrees().to_radians()
    }

    /// Returns the shear factor for this synthetic italic angle.
    pub fn to_skew(self) -> f32 {
        self.to_radians().tan()
    }
}

impl Default for SyntheticItalic {
    fn default() -> Self {
        Self::disabled()
    }
}

/// Returns the synthetic bold transform to use for a requested and actual font weight.
pub fn synthetic_bold_for(
    requested_weight: FontWeight,
    actual_weight: FontWeight,
) -> SyntheticBold {
    const SYNTHETIC_BOLD_THRESHOLD: f32 = 150.0;

    if requested_weight >= FontWeight::SEMIBOLD
        && actual_weight < requested_weight
        && requested_weight.0 - actual_weight.0 >= SYNTHETIC_BOLD_THRESHOLD
    {
        SyntheticBold::default_enabled()
    } else {
        SyntheticBold::disabled()
    }
}

/// The configuration details for identifying a specific font.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Font {
    /// The font family name.
    ///
    /// The special name ".SystemUIFont" is used to identify the system UI font, which varies based on platform.
    pub family: SharedString,

    /// The font features to use.
    pub features: FontFeatures,

    /// The fallbacks fonts to use.
    pub fallbacks: Option<FontFallbacks>,

    /// The font weight.
    pub weight: FontWeight,

    /// The font style.
    pub style: FontStyle,
}

impl Default for Font {
    fn default() -> Self {
        font(".SystemUIFont")
    }
}

/// Get a [`Font`] for a given name.
pub fn font(family: impl Into<SharedString>) -> Font {
    Font {
        family: family.into(),
        features: FontFeatures::default(),
        weight: FontWeight::default(),
        style: FontStyle::default(),
        fallbacks: None,
    }
}

impl Font {
    /// Set this Font to be bold
    pub fn bold(mut self) -> Self {
        self.weight = FontWeight::BOLD;
        self
    }

    /// Set this Font to be italic
    pub fn italic(mut self) -> Self {
        self.style = FontStyle::Italic;
        self
    }
}

/// A struct for storing font metrics.
/// It is used to define the measurements of a typeface.
#[derive(Clone, Copy, Debug)]
pub struct FontMetrics {
    /// The number of font units that make up the "em square",
    /// a scalable grid for determining the size of a typeface.
    pub units_per_em: u32,

    /// The vertical distance from the baseline of the font to the top of the glyph covers.
    pub ascent: f32,

    /// The vertical distance from the baseline of the font to the bottom of the glyph covers.
    pub descent: f32,

    /// The recommended additional space to add between lines of type.
    pub line_gap: f32,

    /// The suggested position of the underline.
    pub underline_position: f32,

    /// The suggested thickness of the underline.
    pub underline_thickness: f32,

    /// The height of a capital letter measured from the baseline of the font.
    pub cap_height: f32,

    /// The height of a lowercase x.
    pub x_height: f32,

    /// The outer limits of the area that the font covers.
    /// Corresponds to the xMin / xMax / yMin / yMax values in the OpenType `head` table
    pub bounding_box: Bounds<f32>,
}

impl FontMetrics {
    /// Returns the vertical distance from the baseline of the font to the top of the glyph covers in pixels.
    pub fn ascent(&self, font_size: Pixels) -> Pixels {
        Pixels((self.ascent / self.units_per_em as f32) * font_size.0)
    }

    /// Returns the vertical distance from the baseline of the font to the bottom of the glyph covers in pixels.
    pub fn descent(&self, font_size: Pixels) -> Pixels {
        Pixels((self.descent / self.units_per_em as f32) * font_size.0)
    }

    /// Returns the recommended additional space to add between lines of type in pixels.
    pub fn line_gap(&self, font_size: Pixels) -> Pixels {
        Pixels((self.line_gap / self.units_per_em as f32) * font_size.0)
    }

    /// Returns the suggested position of the underline in pixels.
    pub fn underline_position(&self, font_size: Pixels) -> Pixels {
        Pixels((self.underline_position / self.units_per_em as f32) * font_size.0)
    }

    /// Returns the suggested thickness of the underline in pixels.
    pub fn underline_thickness(&self, font_size: Pixels) -> Pixels {
        Pixels((self.underline_thickness / self.units_per_em as f32) * font_size.0)
    }

    /// Returns the height of a capital letter measured from the baseline of the font in pixels.
    pub fn cap_height(&self, font_size: Pixels) -> Pixels {
        Pixels((self.cap_height / self.units_per_em as f32) * font_size.0)
    }

    /// Returns the height of a lowercase x in pixels.
    pub fn x_height(&self, font_size: Pixels) -> Pixels {
        Pixels((self.x_height / self.units_per_em as f32) * font_size.0)
    }

    /// Returns the outer limits of the area that the font covers in pixels.
    pub fn bounding_box(&self, font_size: Pixels) -> Bounds<Pixels> {
        (self.bounding_box / self.units_per_em as f32 * font_size.0).map(px)
    }
}

/// Maps well-known virtual font names to their concrete equivalents.
pub fn font_name_with_fallbacks<'a>(name: &'a str, system: &'a str) -> &'a str {
    // Note: the "ZZZ Plex" fonts were deprecated as we are not allowed to use "Plex"
    // in a derived font name. They are essentially indistinguishable from IBM Plex/Lilex,
    // and so retained here for backward compatibility.
    match name {
        ".SystemUIFont" => system,
        ".ZZZSans" | "ZZZ Plex Sans" => "IBM Plex Sans",
        ".ZZZMono" | "ZZZ Plex Mono" => "Lilex",
        _ => name,
    }
}

/// Like [`font_name_with_fallbacks`] but accepts and returns [`SharedString`] references.
pub fn font_name_with_fallbacks_shared<'a>(
    name: &'a SharedString,
    system: &'a SharedString,
) -> &'a SharedString {
    // Note: the "ZZZ Plex" fonts were deprecated as we are not allowed to use "Plex"
    // in a derived font name. They are essentially indistinguishable from IBM Plex/Lilex,
    // and so retained here for backward compatibility.
    match name.as_str() {
        ".SystemUIFont" => system,
        ".ZZZSans" | "ZZZ Plex Sans" => const { &SharedString::new_static("IBM Plex Sans") },
        ".ZZZMono" | "ZZZ Plex Mono" => const { &SharedString::new_static("Lilex") },
        _ => name,
    }
}

#[cfg(test)]
mod raster_info_cache_tests {
    use super::*;
    use crate::NoopTextSystem;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingTextSystem {
        inner: NoopTextSystem,
        raster_queries: AtomicUsize,
    }

    impl CountingTextSystem {
        fn new() -> Self {
            Self {
                inner: NoopTextSystem::new(),
                raster_queries: AtomicUsize::new(0),
            }
        }

        fn raster_queries(&self) -> usize {
            self.raster_queries.load(Ordering::SeqCst)
        }
    }

    impl PlatformTextSystem for CountingTextSystem {
        fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
            self.inner.add_fonts(fonts)
        }

        fn all_font_names(&self) -> Vec<String> {
            self.inner.all_font_names()
        }

        fn font_id(&self, descriptor: &Font) -> Result<FontId> {
            self.inner.font_id(descriptor)
        }

        fn prewarm_fonts(&self, font_ids: &[FontId]) {
            self.inner.prewarm_fonts(font_ids);
        }

        fn font_metrics(&self, font_id: FontId) -> FontMetrics {
            self.inner.font_metrics(font_id)
        }

        fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Bounds<f32>> {
            self.inner.typographic_bounds(font_id, glyph_id)
        }

        fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
            self.inner.advance(font_id, glyph_id)
        }

        fn glyph_for_char(&self, font_id: FontId, character: char) -> Option<GlyphId> {
            self.inner.glyph_for_char(font_id, character)
        }

        fn glyph_raster_info(&self, params: &RenderGlyphParams) -> Result<GlyphRasterInfo> {
            self.raster_queries.fetch_add(1, Ordering::SeqCst);
            Ok(GlyphRasterInfo {
                bounds: Bounds {
                    origin: Point::new(
                        DevicePixels(i32::try_from(params.glyph_id.0)?),
                        DevicePixels(params.dilation.into()),
                    ),
                    size: Size {
                        width: DevicePixels(params.font_size.0.round() as i32),
                        height: DevicePixels(1),
                    },
                },
                format: if params.is_emoji {
                    GlyphRasterFormat::ColorBgra8
                } else if params.subpixel_rendering {
                    GlyphRasterFormat::SubpixelBgra8
                } else {
                    GlyphRasterFormat::Alpha8
                },
            })
        }

        fn rasterize_glyph(
            &self,
            _params: &RenderGlyphParams,
            raster_info: GlyphRasterInfo,
        ) -> Result<RasterizedGlyph> {
            Ok(RasterizedGlyph {
                info: raster_info,
                pixels: Vec::new(),
            })
        }

        fn layout_line(&self, text: &str, font_size: Pixels, runs: &[FontRun]) -> LineLayout {
            self.inner.layout_line(text, font_size, runs)
        }

        fn recommended_rendering_mode(
            &self,
            font_id: FontId,
            font_size: Pixels,
        ) -> TextRenderingMode {
            self.inner.recommended_rendering_mode(font_id, font_size)
        }
    }

    fn params(font_size: f32, glyph_id: u32) -> RenderGlyphParams {
        RenderGlyphParams {
            font_id: FontId(0),
            glyph_id: GlyphId(glyph_id),
            font_size: px(font_size),
            subpixel_variant: Point::default(),
            scale_factor: 1.0,
            synthetic_italic: SyntheticItalic::disabled(),
            synthetic_bold: SyntheticBold::disabled(),
            is_emoji: false,
            subpixel_rendering: false,
            dilation: 0,
        }
    }

    #[test]
    fn packed_glyph_key_validates_subpixel_ranges() {
        let mut invalid_x = params(16.0, 1);
        invalid_x.subpixel_variant.x = SUBPIXEL_VARIANTS_X;
        PackedGlyphKey::new(&invalid_x).expect_err("x variant must be range checked");

        let mut invalid_y = params(16.0, 1);
        invalid_y.subpixel_variant.y = SUBPIXEL_VARIANTS_Y;
        PackedGlyphKey::new(&invalid_y).expect_err("y variant must be range checked");
    }

    #[test]
    fn warm_strike_lookups_do_not_repeat_platform_queries() -> Result<()> {
        let platform = Arc::new(CountingTextSystem::new());
        let text_system = TextSystem::new(platform.clone());
        text_system.set_raster_info_cache_limits_for_test(8, usize::MAX);
        let first = params(16.0, 1);
        let second = params(16.0, 2);

        let first_info = text_system.glyph_raster_info_for_test(&first)?;
        let second_info = text_system.glyph_raster_info_for_test(&second)?;
        assert_eq!(text_system.glyph_raster_info_for_test(&first)?, first_info);
        assert_eq!(
            text_system.glyph_raster_info_for_test(&second)?,
            second_info
        );
        assert_eq!(platform.raster_queries(), 2);

        let snapshot = text_system.raster_info_cache_snapshot_for_test();
        assert_eq!(snapshot.strike_count, 1);
        assert_eq!(snapshot.entry_count, 2);
        assert_eq!(snapshot.hits, 2);
        assert_eq!(snapshot.misses, 2);
        assert_eq!(snapshot.evictions, 0);
        Ok(())
    }

    #[test]
    fn lru_eviction_returns_cold_equivalent_metadata() -> Result<()> {
        let platform = Arc::new(CountingTextSystem::new());
        let text_system = TextSystem::new(platform.clone());
        text_system.set_raster_info_cache_limits_for_test(2, usize::MAX);
        let first = params(14.0, 1);
        let second = params(15.0, 1);
        let third = params(16.0, 1);

        text_system.glyph_raster_info_for_test(&first)?;
        let second_cold = text_system.glyph_raster_info_for_test(&second)?;
        text_system.glyph_raster_info_for_test(&first)?;
        text_system.glyph_raster_info_for_test(&third)?;
        let after_eviction = text_system.raster_info_cache_snapshot_for_test();
        assert_eq!(after_eviction.strike_count, 2);
        assert_eq!(after_eviction.evictions, 1);

        assert_eq!(
            text_system.glyph_raster_info_for_test(&second)?,
            second_cold
        );
        assert_eq!(platform.raster_queries(), 4);
        Ok(())
    }

    #[test]
    fn byte_and_strike_limits_are_hard_caps() -> Result<()> {
        let platform = Arc::new(CountingTextSystem::new());
        let text_system = TextSystem::new(platform);
        text_system.set_raster_info_cache_limits_for_test(8, usize::MAX);
        text_system.glyph_raster_info_for_test(&params(10.0, 1))?;
        let one_strike_bytes = text_system
            .raster_info_cache_snapshot_for_test()
            .estimated_bytes;

        text_system.set_raster_info_cache_limits_for_test(2, one_strike_bytes);
        for font_size in 10..20 {
            text_system.glyph_raster_info_for_test(&params(font_size as f32, 1))?;
            let snapshot = text_system.raster_info_cache_snapshot_for_test();
            assert!(snapshot.strike_count <= snapshot.max_strikes);
            assert!(snapshot.estimated_bytes <= snapshot.max_bytes);
        }
        let snapshot = text_system.raster_info_cache_snapshot_for_test();
        assert_eq!(snapshot.strike_count, 1);
        assert!(snapshot.evictions > 0);
        Ok(())
    }

    #[test]
    fn zero_limits_disable_residency_without_replacement_growth() -> Result<()> {
        let platform = Arc::new(CountingTextSystem::new());
        let text_system = TextSystem::new(platform.clone());
        text_system.set_raster_info_cache_limits_for_test(0, 0);
        let params = params(16.0, 1);
        text_system.glyph_raster_info_for_test(&params)?;
        text_system.glyph_raster_info_for_test(&params)?;

        let snapshot = text_system.raster_info_cache_snapshot_for_test();
        assert_eq!(snapshot.strike_count, 0);
        assert_eq!(snapshot.entry_count, 0);
        assert_eq!(snapshot.estimated_bytes, 0);
        assert_eq!(platform.raster_queries(), 2);
        Ok(())
    }
}
