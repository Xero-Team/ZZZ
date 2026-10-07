use anyhow::{Context as _, Ok, Result};
use collections::HashMap;
use cosmic_text::{
    Attrs, AttrsList, Family, Font as CosmicTextFont, FontFeatures as CosmicFontFeatures,
    FontSystem, ShapeBuffer, ShapeLine,
};
use gpui::{
    Bounds, Font, FontFeatures, FontId, FontMetrics, FontRun, FontStyle, FontWeight, GlyphId,
    GlyphRasterFormat, GlyphRasterInfo, IsZero as _, LineLayout, Pixels, PlatformTextSystem,
    RasterizedGlyph, RenderGlyphParams, SUBPIXEL_VARIANTS_X, SUBPIXEL_VARIANTS_Y, ShapedGlyph,
    ShapedRun, SharedString, Size, SyntheticBold, SyntheticItalic, TextRenderingMode, point, size,
    synthetic_bold_for,
};

use itertools::Itertools;
use parking_lot::RwLock;
use smallvec::SmallVec;
use std::{borrow::Cow, ops::Range, sync::Arc};
use swash::{
    scale::{Render, ScaleContext, Source, StrikeWith},
    zeno::{Format, Transform, Vector},
};

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use skrifa::{
    FontRef as SkrifaFontRef, MetadataProvider,
    color::ColorGlyphFormat,
    instance::{LocationRef, Size as SkrifaSize},
    raw::TableProvider,
};
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
use vello_cpu::{
    Glyph as VelloGlyph, Pixmap as VelloPixmap, RenderContext, Resources,
    color::palette::css::BLACK,
    peniko::{Blob, FontData},
};

pub struct CosmicTextSystem(RwLock<CosmicTextSystemState>);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct FontKey {
    family: SharedString,
    features: FontFeatures,
}

impl FontKey {
    fn new(family: SharedString, features: FontFeatures) -> Self {
        Self { family, features }
    }
}

struct CosmicTextSystemState {
    font_system: FontSystem,
    scratch: ShapeBuffer,
    swash_scale_context: ScaleContext,
    /// Contains all already loaded fonts, including all faces. Indexed by `FontId`.
    loaded_fonts: Vec<LoadedFont>,
    /// Caches the `FontId`s associated with a specific family to avoid iterating the font database
    /// for every font face in a family.
    font_ids_by_family_cache: HashMap<FontKey, SmallVec<[FontId; 4]>>,
    system_font_fallback: String,
}

struct LoadedFont {
    font: Arc<CosmicTextFont>,
    features: CosmicFontFeatures,
    style: cosmic_text::Style,
    weight: FontWeight,
    prefers_color_sources: bool,
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    vector_color_font_data: Option<FontData>,
}

struct RenderedGlyphImage {
    info: GlyphRasterInfo,
    pixels: Vec<u8>,
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[derive(Clone, Copy)]
struct SvgViewport {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
struct ColorImageCrop {
    source_width: u16,
    minimum_x: usize,
    minimum_y: usize,
    maximum_x: usize,
    maximum_y: usize,
    source_left: i32,
    source_top: i32,
}

impl CosmicTextSystem {
    pub fn new(system_font_fallback: &str) -> Self {
        let font_system = FontSystem::new();

        Self(RwLock::new(CosmicTextSystemState {
            font_system,
            scratch: ShapeBuffer::default(),
            swash_scale_context: ScaleContext::new(),
            loaded_fonts: Vec::new(),
            font_ids_by_family_cache: HashMap::default(),
            system_font_fallback: system_font_fallback.to_owned(),
        }))
    }

    pub fn new_without_system_fonts(system_font_fallback: &str) -> Self {
        let font_system = FontSystem::new_with_locale_and_db(
            "en-US".to_owned(),
            cosmic_text::fontdb::Database::new(),
        );

        Self(RwLock::new(CosmicTextSystemState {
            font_system,
            scratch: ShapeBuffer::default(),
            swash_scale_context: ScaleContext::new(),
            loaded_fonts: Vec::new(),
            font_ids_by_family_cache: HashMap::default(),
            system_font_fallback: system_font_fallback.to_owned(),
        }))
    }

    /// Returns the first face registered for a fixture family without applying
    /// style matching. This is only available to renderer test infrastructure.
    #[cfg(feature = "test-support")]
    pub fn first_fixture_font_id(&self, family_name: &str) -> Result<FontId> {
        let family: SharedString = family_name.to_owned().into();
        let mut state = self.0.write();
        if let Some(font_id) = state
            .load_family(&family, &FontFeatures::default())?
            .first()
            .copied()
        {
            return Ok(font_id);
        }
        let cosmic_font_id = state
            .font_system
            .db()
            .faces()
            .find(|face| {
                face.families
                    .iter()
                    .any(|(candidate, _)| candidate.eq_ignore_ascii_case(family_name))
            })
            .with_context(|| format!("fixture family {family} has no faces"))?
            .id;
        state.font_id_for_cosmic_id(cosmic_font_id)
    }
}

impl PlatformTextSystem for CosmicTextSystem {
    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        self.0.write().add_fonts(fonts)
    }

    fn all_font_names(&self) -> Vec<String> {
        let mut result = self
            .0
            .read()
            .font_system
            .db()
            .faces()
            .filter_map(|face| face.families.first().map(|family| family.0.clone()))
            .collect_vec();
        result.sort_unstable();
        result.dedup();
        result
    }

    fn font_id(&self, font: &Font) -> Result<FontId> {
        let mut state = self.0.write();
        let key = FontKey::new(font.family.clone(), font.features.clone());
        let candidates = if let Some(font_ids) = state.font_ids_by_family_cache.get(&key) {
            font_ids.as_slice()
        } else {
            let font_ids = state.load_family(&font.family, &font.features)?;
            state.font_ids_by_family_cache.insert(key.clone(), font_ids);
            state.font_ids_by_family_cache[&key].as_ref()
        };

        let ix = find_best_match(font, candidates, &state)?;

        Ok(candidates[ix])
    }

    fn prewarm_fonts(&self, font_ids: &[FontId]) {
        self.0.write().prewarm_fonts(font_ids);
    }

    fn font_metrics(&self, font_id: FontId) -> FontMetrics {
        let metrics = self
            .0
            .read()
            .loaded_font(font_id)
            .font
            .as_swash()
            .metrics(&[]);

        FontMetrics {
            units_per_em: metrics.units_per_em as u32,
            ascent: metrics.ascent,
            descent: -metrics.descent,
            line_gap: metrics.leading,
            underline_position: metrics.underline_offset,
            underline_thickness: metrics.stroke_size,
            cap_height: metrics.cap_height,
            x_height: metrics.x_height,
            bounding_box: Bounds {
                origin: point(0.0, 0.0),
                size: size(metrics.max_width, metrics.ascent + metrics.descent),
            },
        }
    }

    fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Bounds<f32>> {
        let lock = self.0.read();
        let glyph_metrics = lock.loaded_font(font_id).font.as_swash().glyph_metrics(&[]);
        let glyph_id = glyph_id.0 as u16;
        Ok(Bounds {
            origin: point(0.0, 0.0),
            size: size(
                glyph_metrics.advance_width(glyph_id),
                glyph_metrics.advance_height(glyph_id),
            ),
        })
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
        self.0.read().advance(font_id, glyph_id)
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        self.0.read().glyph_for_char(font_id, ch)
    }

    fn glyph_raster_info(&self, params: &RenderGlyphParams) -> Result<GlyphRasterInfo> {
        self.0.write().raster_info(params)
    }

    fn rasterize_glyph(
        &self,
        params: &RenderGlyphParams,
        raster_info: GlyphRasterInfo,
    ) -> Result<RasterizedGlyph> {
        self.0.write().rasterize_glyph(params, raster_info)
    }

    fn layout_line(&self, text: &str, font_size: Pixels, runs: &[FontRun]) -> LineLayout {
        self.0.write().layout_line(text, font_size, runs)
    }

    fn recommended_rendering_mode(
        &self,
        _font_id: FontId,
        _font_size: Pixels,
    ) -> TextRenderingMode {
        TextRenderingMode::Subpixel
    }
}

impl CosmicTextSystemState {
    fn loaded_font(&self, font_id: FontId) -> &LoadedFont {
        &self.loaded_fonts[font_id.0]
    }

    fn prewarm_fonts(&mut self, font_ids: &[FontId]) {
        for &font_id in font_ids {
            let (family, stretch, style, weight, features) = {
                let loaded_font = self.loaded_font(font_id);
                let Some(face) = self.font_system.db().face(loaded_font.font.id()) else {
                    continue;
                };
                let Some(family) = face.families.first() else {
                    continue;
                };
                (
                    family.0.clone(),
                    face.stretch,
                    face.style,
                    face.weight,
                    loaded_font.features.clone(),
                )
            };
            let attributes = Attrs::new()
                .metadata(font_id.0)
                .family(Family::Name(&family))
                .stretch(stretch)
                .style(style)
                .weight(weight)
                .font_features(features);
            self.font_system.get_font_matches(&attributes);
        }
    }

    #[profiling::function]
    fn add_fonts(&mut self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        let db = self.font_system.db_mut();
        for bytes in fonts {
            db.load_font_source(cosmic_text::fontdb::Source::Binary(Arc::new(bytes)));
        }
        Ok(())
    }

    #[profiling::function]
    fn load_family(
        &mut self,
        name: &str,
        features: &FontFeatures,
    ) -> Result<SmallVec<[FontId; 4]>> {
        let name = gpui::font_name_with_fallbacks(name, &self.system_font_fallback);

        let families = self
            .font_system
            .db()
            .faces()
            .filter(|face| face.families.iter().any(|family| *name == family.0))
            .map(|face| (face.id, face.post_script_name.clone()))
            .collect::<SmallVec<[_; 4]>>();

        let mut loaded_font_ids = SmallVec::new();
        for (font_id, postscript_name) in families {
            let (style, weight, face_index) = {
                let face = self
                    .font_system
                    .db()
                    .face(font_id)
                    .context("font face not found in database")?;
                (face.style, FontWeight(face.weight.0.into()), face.index)
            };
            let font = self
                .font_system
                .get_font(font_id, cosmic_text::Weight::NORMAL)
                .context("Could not load font")?;
            let prefers_color_sources = is_color_capable_font(font.as_swash())
                || check_is_known_emoji_font(&postscript_name);
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            let vector_color_font_data = vector_color_font_data(&font, face_index);

            // HACK: To let the storybook run and render Windows caption icons. We should actually do better font fallback.
            let allowed_bad_font_names = [
                "SegoeFluentIcons", // NOTE: Segoe fluent icons postscript name is inconsistent
                "Segoe Fluent Icons",
            ];

            if font.as_swash().charmap().map('m') == 0
                && !allowed_bad_font_names.contains(&postscript_name.as_str())
                && !prefers_color_sources
            {
                self.font_system.db_mut().remove_face(font.id());
                continue;
            };

            let font_id = FontId(self.loaded_fonts.len());
            loaded_font_ids.push(font_id);
            self.loaded_fonts.push(LoadedFont {
                font,
                features: cosmic_font_features(features)?,
                style,
                weight,
                prefers_color_sources,
                #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                vector_color_font_data,
            });
        }

        Ok(loaded_font_ids)
    }

    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> {
        let glyph_metrics = self.loaded_font(font_id).font.as_swash().glyph_metrics(&[]);
        Ok(Size {
            width: glyph_metrics.advance_width(glyph_id.0 as u16),
            height: glyph_metrics.advance_height(glyph_id.0 as u16),
        })
    }

    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> {
        let glyph_id = self.loaded_font(font_id).font.as_swash().charmap().map(ch);
        if glyph_id == 0 {
            None
        } else {
            Some(GlyphId(glyph_id.into()))
        }
    }

    fn raster_info(&mut self, params: &RenderGlyphParams) -> Result<GlyphRasterInfo> {
        Ok(self.render_glyph_image(params)?.info)
    }

    #[profiling::function]
    fn rasterize_glyph(
        &mut self,
        params: &RenderGlyphParams,
        raster_info: GlyphRasterInfo,
    ) -> Result<RasterizedGlyph> {
        let glyph_bounds = raster_info.bounds;
        if glyph_bounds.size.width.0 == 0 || glyph_bounds.size.height.0 == 0 {
            anyhow::bail!("glyph bounds are empty");
        }

        let image = self.render_glyph_image(params)?;
        anyhow::ensure!(
            image.info == raster_info,
            "glyph raster format or bounds changed between info and pixel queries"
        );
        Ok(RasterizedGlyph {
            info: raster_info,
            pixels: image.pixels,
        })
    }

    fn render_glyph_image(&mut self, params: &RenderGlyphParams) -> Result<RenderedGlyphImage> {
        let loaded_font = &self.loaded_fonts[params.font_id.0];
        let font_ref = loaded_font.font.as_swash();
        let pixel_size = f32::from(params.font_size);
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let vector_color_font_data = loaded_font.vector_color_font_data.clone();

        let subpixel_offset = Vector::new(
            params.subpixel_variant.x as f32 / SUBPIXEL_VARIANTS_X as f32 / params.scale_factor,
            params.subpixel_variant.y as f32 / SUBPIXEL_VARIANTS_Y as f32 / params.scale_factor,
        );

        let mut scaler = self
            .swash_scale_context
            .builder(font_ref)
            .size(pixel_size * params.scale_factor)
            .hint(true)
            .build();

        let sources: &[Source] =
            if params.synthetic_italic.is_enabled() || params.synthetic_bold.is_enabled() {
                &[Source::Outline]
            } else if params.is_emoji {
                &[
                    Source::ColorOutline(0),
                    Source::ColorBitmap(StrikeWith::BestFit),
                    Source::Outline,
                ]
            } else {
                &[Source::Bitmap(StrikeWith::ExactSize), Source::Outline]
            };

        let mut renderer = Render::new(sources);
        if params.subpixel_rendering {
            // There seems to be a bug in Swash where the B and R values are swapped.
            renderer
                .format(Format::subpixel_bgra())
                .offset(subpixel_offset);
        } else {
            renderer.format(Format::Alpha).offset(subpixel_offset);
        }

        if params.synthetic_italic.is_enabled() {
            let skew = params.synthetic_italic.to_skew();
            renderer.transform(Some(Transform::new(1.0, 0.0, skew, 1.0, 0.0, 0.0)));
        }
        if params.synthetic_bold.is_enabled() {
            renderer.embolden(
                params
                    .synthetic_bold
                    .device_pixel_amount(params.font_size, params.scale_factor),
            );
        }

        let glyph_id: u16 = params.glyph_id.0.try_into()?;
        let swash_image = renderer
            .render(&mut scaler, glyph_id)
            .map(|image| normalize_swash_image(image, params.subpixel_rendering))
            .transpose()?;
        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        if let Some(font_data) = vector_color_font_data {
            let raster_pixel_size = pixel_size * params.scale_factor;
            if let Some(image) = render_svg_glyph(&font_data, params.glyph_id, raster_pixel_size)? {
                return Ok(image);
            }
            if swash_image
                .as_ref()
                .is_some_and(|image| !image.info.bounds.is_zero())
            {
                return swash_image.context("non-empty Swash image disappeared");
            }
            if let Some(image) =
                render_colrv1_glyph(&font_data, params.glyph_id, raster_pixel_size)?
            {
                return Ok(image);
            }
        }

        if swash_image
            .as_ref()
            .is_some_and(|image| !image.info.bounds.is_zero())
        {
            return swash_image.context("non-empty Swash image disappeared");
        }

        swash_image.with_context(|| format!("unable to render glyph via Swash for {params:?}"))
    }

    /// This is used when cosmic_text has chosen a fallback font instead of using the requested
    /// font, typically to handle some unicode characters. When this happens, `loaded_fonts` may not
    /// yet have an entry for this fallback font, and so one is added.
    ///
    /// Note that callers shouldn't use this `FontId` somewhere that will retrieve the corresponding
    /// `LoadedFont.features`, as it will have an arbitrarily chosen or empty value. The only
    /// current use of this field is for the *input* of `layout_line`, and so it's fine to use
    /// `font_id_for_cosmic_id` when computing the *output* of `layout_line`.
    fn font_id_for_cosmic_id(&mut self, id: cosmic_text::fontdb::ID) -> Result<FontId> {
        if let Some(ix) = self
            .loaded_fonts
            .iter()
            .position(|loaded_font| loaded_font.font.id() == id)
        {
            Ok(FontId(ix))
        } else {
            let font = self
                .font_system
                .get_font(id, cosmic_text::Weight::NORMAL)
                .context("failed to get fallback font from cosmic-text font system")?;
            let face = self
                .font_system
                .db()
                .face(id)
                .context("fallback font face not found in cosmic-text database")?;
            let prefers_color_sources = is_color_capable_font(font.as_swash())
                || check_is_known_emoji_font(&face.post_script_name);
            #[cfg(any(target_os = "linux", target_os = "freebsd"))]
            let vector_color_font_data = vector_color_font_data(&font, face.index);

            let font_id = FontId(self.loaded_fonts.len());
            self.loaded_fonts.push(LoadedFont {
                font,
                features: CosmicFontFeatures::new(),
                style: face.style,
                weight: FontWeight(face.weight.0.into()),
                prefers_color_sources,
                #[cfg(any(target_os = "linux", target_os = "freebsd"))]
                vector_color_font_data,
            });

            Ok(font_id)
        }
    }

    #[profiling::function]
    fn layout_line(&mut self, text: &str, font_size: Pixels, font_runs: &[FontRun]) -> LineLayout {
        if contains_paragraph_separator(text) {
            self.layout_line_with_separators(text, font_size, font_runs)
        } else {
            self.layout_line_no_separators(text, font_size, font_runs)
        }
    }

    fn layout_line_with_separators(
        &mut self,
        text: &str,
        font_size: Pixels,
        font_runs: &[FontRun],
    ) -> LineLayout {
        let mut layout = LineLayout {
            font_size,
            len: text.len(),
            ..Default::default()
        };
        let mut paragraph_start = 0;

        for (separator_start, separator) in text
            .char_indices()
            .filter(|(_, character)| is_paragraph_separator(*character))
        {
            let separator_end = separator_start + separator.len_utf8();
            self.shape_segment(
                text,
                paragraph_start..separator_start,
                font_size,
                font_runs,
                &mut layout,
            );
            self.shape_segment(
                text,
                separator_start..separator_end,
                font_size,
                font_runs,
                &mut layout,
            );
            paragraph_start = separator_end;
        }

        self.shape_segment(
            text,
            paragraph_start..text.len(),
            font_size,
            font_runs,
            &mut layout,
        );

        layout
    }

    fn shape_segment(
        &mut self,
        text: &str,
        range: Range<usize>,
        font_size: Pixels,
        font_runs: &[FontRun],
        layout: &mut LineLayout,
    ) {
        if range.is_empty() {
            return;
        }

        let segment_font_runs = clip_font_runs(font_runs, range.clone());
        let segment =
            self.layout_line_no_separators(&text[range.clone()], font_size, &segment_font_runs);

        let mut segment_runs = segment.runs;
        for run in &mut segment_runs {
            for glyph in &mut run.glyphs {
                glyph.index += range.start;
                glyph.position.x += layout.width;
            }
        }

        for mut run in segment_runs {
            if let Some(same_run) = layout
                .runs
                .last_mut()
                .filter(|last| last.font_id == run.font_id)
            {
                same_run.glyphs.append(&mut run.glyphs);
            } else {
                layout.runs.push(run);
            }
        }

        layout.width += segment.width;
        layout.ascent = layout.ascent.max(segment.ascent);
        layout.descent = layout.descent.max(segment.descent);
    }

    fn layout_line_no_separators(
        &mut self,
        text: &str,
        font_size: Pixels,
        font_runs: &[FontRun],
    ) -> LineLayout {
        let mut attrs_list = AttrsList::new(&Attrs::new());
        let mut offs = 0;
        for (run_index, run) in font_runs.iter().enumerate() {
            let loaded_font = self.loaded_font(run.font_id);
            let Some(face) = self.font_system.db().face(loaded_font.font.id()) else {
                log::warn!(
                    "font face not found in database for font_id {:?}",
                    run.font_id
                );
                offs += run.len;
                continue;
            };
            let Some(first_family) = face.families.first() else {
                log::warn!(
                    "font face has no family names for font_id {:?}",
                    run.font_id
                );
                offs += run.len;
                continue;
            };

            attrs_list.add_span(
                offs..(offs + run.len),
                &Attrs::new()
                    .metadata(run_index)
                    .family(Family::Name(&first_family.0))
                    .stretch(face.stretch)
                    .style(face.style)
                    .weight(face.weight)
                    .font_features(loaded_font.features.clone()),
            );
            offs += run.len;
        }

        let line = ShapeLine::new(
            &mut self.font_system,
            text,
            &attrs_list,
            cosmic_text::Shaping::Advanced,
            4,
        );
        let mut layout_lines = Vec::with_capacity(1);
        line.layout_to_buffer(
            &mut self.scratch,
            f32::from(font_size),
            None, // We do our own wrapping
            cosmic_text::Wrap::None,
            cosmic_text::Ellipsize::None,
            None,
            &mut layout_lines,
            None,
            cosmic_text::Hinting::Disabled,
        );

        let Some(layout) = layout_lines.first() else {
            return LineLayout {
                font_size,
                width: Pixels::ZERO,
                ascent: Pixels::ZERO,
                descent: Pixels::ZERO,
                runs: Vec::new(),
                len: text.len(),
            };
        };

        let mut runs: Vec<ShapedRun> = Vec::new();
        for glyph in &layout.glyphs {
            let Some(font_run) = font_runs.get(glyph.metadata) else {
                log::warn!(
                    "glyph metadata points to missing font run {:?}",
                    glyph.metadata
                );
                continue;
            };
            let mut font_id = font_run.font_id;
            let mut loaded_font = self.loaded_font(font_id);
            if loaded_font.font.id() != glyph.font_id {
                match self.font_id_for_cosmic_id(glyph.font_id) {
                    std::result::Result::Ok(resolved_id) => {
                        font_id = resolved_id;
                        loaded_font = self.loaded_font(font_id);
                    }
                    Err(error) => {
                        log::warn!(
                            "failed to resolve cosmic font id {:?}: {error:#}",
                            glyph.font_id
                        );
                        continue;
                    }
                }
            }
            let is_emoji = loaded_font.prefers_color_sources;
            let synthetic_italic = if is_emoji {
                SyntheticItalic::disabled()
            } else {
                synthetic_italic_for(font_run.font_style, loaded_font.style)
            };
            let synthetic_bold = if is_emoji {
                SyntheticBold::disabled()
            } else {
                synthetic_bold_for(font_run.font_weight, loaded_font.weight)
            };

            // HACK: Prevent crash caused by variation selectors.
            if glyph.glyph_id == 3 && is_emoji {
                continue;
            }

            let shaped_glyph = ShapedGlyph {
                id: GlyphId(glyph.glyph_id as u32),
                position: point(glyph.x.into(), glyph.y.into()),
                index: glyph.start,
                is_emoji,
            };

            if let Some(last_run) = runs.last_mut().filter(|last_run| {
                last_run.font_id == font_id
                    && last_run.synthetic_italic == synthetic_italic
                    && last_run.synthetic_bold == synthetic_bold
            }) {
                last_run.glyphs.push(shaped_glyph);
            } else {
                runs.push(ShapedRun {
                    font_id,
                    synthetic_italic,
                    synthetic_bold,
                    glyphs: vec![shaped_glyph],
                });
            }
        }

        LineLayout {
            font_size,
            width: layout.w.into(),
            ascent: layout.max_ascent.into(),
            descent: layout.max_descent.into(),
            runs,
            len: text.len(),
        }
    }
}

#[inline(always)]
fn is_paragraph_separator(character: char) -> bool {
    unicode_bidi::bidi_class(character) == unicode_bidi::BidiClass::B
}

fn contains_paragraph_separator(text: &str) -> bool {
    if text
        .bytes()
        .any(|byte| matches!(byte, b'\n' | b'\r' | 0x1c | 0x1d | 0x1e))
    {
        return true;
    }

    !text.is_ascii() && text.chars().any(is_paragraph_separator)
}

fn clip_font_runs(font_runs: &[FontRun], range: Range<usize>) -> SmallVec<[FontRun; 4]> {
    let mut clipped = SmallVec::new();
    let mut offs = 0;
    for run in font_runs {
        let run_start = offs;
        offs += run.len;
        if offs <= range.start {
            continue;
        }
        if run_start >= range.end {
            break;
        }
        let start = run_start.max(range.start);
        let end = offs.min(range.end);
        if start < end {
            clipped.push(FontRun {
                len: end - start,
                font_id: run.font_id,
                font_style: run.font_style,
                font_weight: run.font_weight,
            });
        }
    }
    clipped
}

#[cfg(feature = "font-kit")]
fn find_best_match(
    font: &Font,
    candidates: &[FontId],
    state: &CosmicTextSystemState,
) -> Result<usize> {
    let candidate_properties = candidates
        .iter()
        .map(|font_id| {
            let database_id = state.loaded_font(*font_id).font.id();
            let face_info = state
                .font_system
                .db()
                .face(database_id)
                .context("font face not found in database")?;
            Ok(face_info_into_properties(face_info))
        })
        .collect::<Result<SmallVec<[_; 4]>>>()?;

    let ix =
        font_kit::matching::find_best_match(&candidate_properties, &font_into_properties(font))
            .context("requested font family contains no font matching the other parameters")?;

    Ok(ix)
}

#[cfg(not(feature = "font-kit"))]
fn find_best_match(
    font: &Font,
    candidates: &[FontId],
    state: &CosmicTextSystemState,
) -> Result<usize> {
    if candidates.is_empty() {
        anyhow::bail!("requested font family contains no font matching the other parameters");
    }
    if candidates.len() == 1 {
        return Ok(0);
    }

    let target_weight = font.weight.0;
    let target_italic = matches!(
        font.style,
        gpui::FontStyle::Italic | gpui::FontStyle::Oblique
    );

    let mut best_index = 0;
    let mut best_score = u32::MAX;

    for (index, font_id) in candidates.iter().enumerate() {
        let database_id = state.loaded_font(*font_id).font.id();
        let face_info = state
            .font_system
            .db()
            .face(database_id)
            .context("font face not found in database")?;

        let is_italic = matches!(
            face_info.style,
            cosmic_text::Style::Italic | cosmic_text::Style::Oblique
        );
        let style_penalty: u32 = if is_italic == target_italic { 0 } else { 1000 };
        let weight_diff = (face_info.weight.0 as i32 - target_weight as i32).unsigned_abs();
        let score = style_penalty + weight_diff;

        if score < best_score {
            best_score = score;
            best_index = index;
        }
    }

    Ok(best_index)
}

fn synthetic_italic_for(
    requested_style: FontStyle,
    actual_style: cosmic_text::Style,
) -> SyntheticItalic {
    if requested_style == FontStyle::Italic
        && !matches!(
            actual_style,
            cosmic_text::Style::Italic | cosmic_text::Style::Oblique
        )
    {
        SyntheticItalic::enabled()
    } else {
        SyntheticItalic::disabled()
    }
}

fn cosmic_font_features(features: &FontFeatures) -> Result<CosmicFontFeatures> {
    let mut result = CosmicFontFeatures::new();
    for feature in features.0.iter() {
        let name_bytes: [u8; 4] = feature
            .0
            .as_bytes()
            .try_into()
            .context("Incorrect feature flag format")?;

        let tag = cosmic_text::FeatureTag::new(&name_bytes);

        result.set(tag, feature.1);
    }
    Ok(result)
}

#[cfg(feature = "font-kit")]
fn font_into_properties(font: &gpui::Font) -> font_kit::properties::Properties {
    font_kit::properties::Properties {
        style: match font.style {
            gpui::FontStyle::Normal => font_kit::properties::Style::Normal,
            gpui::FontStyle::Italic => font_kit::properties::Style::Italic,
            gpui::FontStyle::Oblique => font_kit::properties::Style::Oblique,
        },
        weight: font_kit::properties::Weight(font.weight.0),
        stretch: Default::default(),
    }
}

#[cfg(feature = "font-kit")]
fn face_info_into_properties(
    face_info: &cosmic_text::fontdb::FaceInfo,
) -> font_kit::properties::Properties {
    font_kit::properties::Properties {
        style: match face_info.style {
            cosmic_text::Style::Normal => font_kit::properties::Style::Normal,
            cosmic_text::Style::Italic => font_kit::properties::Style::Italic,
            cosmic_text::Style::Oblique => font_kit::properties::Style::Oblique,
        },
        weight: font_kit::properties::Weight(face_info.weight.0.into()),
        stretch: match face_info.stretch {
            cosmic_text::Stretch::Condensed => font_kit::properties::Stretch::CONDENSED,
            cosmic_text::Stretch::Expanded => font_kit::properties::Stretch::EXPANDED,
            cosmic_text::Stretch::ExtraCondensed => font_kit::properties::Stretch::EXTRA_CONDENSED,
            cosmic_text::Stretch::ExtraExpanded => font_kit::properties::Stretch::EXTRA_EXPANDED,
            cosmic_text::Stretch::Normal => font_kit::properties::Stretch::NORMAL,
            cosmic_text::Stretch::SemiCondensed => font_kit::properties::Stretch::SEMI_CONDENSED,
            cosmic_text::Stretch::SemiExpanded => font_kit::properties::Stretch::SEMI_EXPANDED,
            cosmic_text::Stretch::UltraCondensed => font_kit::properties::Stretch::ULTRA_CONDENSED,
            cosmic_text::Stretch::UltraExpanded => font_kit::properties::Stretch::ULTRA_EXPANDED,
        },
    }
}

fn normalize_swash_image(
    mut image: swash::scale::image::Image,
    subpixel_rendering: bool,
) -> Result<RenderedGlyphImage> {
    let format = glyph_raster_format(image.content, subpixel_rendering);
    let bounds = Bounds {
        origin: point(image.placement.left.into(), (-image.placement.top).into()),
        size: size(image.placement.width.into(), image.placement.height.into()),
    };
    let pixel_count = usize::try_from(image.placement.width)?
        .checked_mul(usize::try_from(image.placement.height)?)
        .context("glyph pixel count overflowed")?;
    let pixels = match image.content {
        swash::scale::image::Content::Color | swash::scale::image::Content::SubpixelMask => {
            anyhow::ensure!(
                image.data.len() == pixel_count.saturating_mul(4),
                "Swash returned an invalid four-channel glyph buffer"
            );
            for pixel in image.data.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            image.data
        }
        swash::scale::image::Content::Mask if subpixel_rendering => {
            anyhow::ensure!(
                image.data.len() == pixel_count,
                "Swash returned an invalid alpha glyph buffer"
            );
            image.data.iter().flat_map(|&alpha| [alpha; 4]).collect()
        }
        swash::scale::image::Content::Mask => {
            anyhow::ensure!(
                image.data.len() == pixel_count,
                "Swash returned an invalid alpha glyph buffer"
            );
            image.data
        }
    };
    Ok(RenderedGlyphImage {
        info: GlyphRasterInfo { bounds, format },
        pixels,
    })
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn vector_color_font_data(font: &CosmicTextFont, face_index: u32) -> Option<FontData> {
    let swash_font = font.as_swash();
    let has_colrv1 = swash_font
        .table(swash::tag_from_bytes(b"COLR"))
        .is_some_and(|table| table.get(..2) == Some([0, 1].as_slice()));
    let has_svg = swash_font.table(swash::tag_from_bytes(b"SVG ")).is_some();
    if !has_colrv1 && !has_svg {
        return None;
    }
    Some(FontData::new(
        Blob::new(Arc::new(font.data().to_vec())),
        face_index,
    ))
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn render_svg_glyph(
    font_data: &FontData,
    glyph_id: GlyphId,
    pixel_size: f32,
) -> Result<Option<RenderedGlyphImage>> {
    anyhow::ensure!(
        pixel_size.is_finite() && pixel_size > 0.0,
        "SVG glyph pixel size must be finite and positive"
    );
    let font_ref = SkrifaFontRef::from_index(font_data.data.data(), font_data.index)
        .context("failed to parse OpenType-SVG font data")?;
    let Some(document) = font_ref
        .svg()
        .ok()
        .and_then(|table| table.glyph_data(skrifa::GlyphId::new(glyph_id.0)))
    else {
        return Ok(None);
    };

    let metrics = font_ref.metrics(SkrifaSize::unscaled(), LocationRef::default());
    anyhow::ensure!(metrics.units_per_em > 0, "SVG font units per em are zero");
    let ascent = metrics.ascent.max(0.0);
    let line_height = (metrics.ascent + metrics.descent).max(f32::from(metrics.units_per_em));
    let advance_width = font_ref
        .glyph_metrics(SkrifaSize::unscaled(), LocationRef::default())
        .advance_width(skrifa::GlyphId::new(glyph_id.0))
        .unwrap_or(f32::from(metrics.units_per_em))
        .max(f32::from(metrics.units_per_em));
    anyhow::ensure!(
        [ascent, line_height, advance_width]
            .into_iter()
            .all(f32::is_finite),
        "SVG font metrics must be finite"
    );

    let document = svg_document_with_viewport(
        document,
        SvgViewport {
            x: 0.0,
            y: -ascent,
            width: advance_width,
            height: line_height,
        },
    )?;
    let tree = usvg::Tree::from_data(&document, &usvg::Options::default())
        .context("failed to parse OpenType-SVG glyph document")?;
    let scale = pixel_size / f32::from(metrics.units_per_em);
    let width = (advance_width * scale).ceil() as u32;
    let height = (line_height * scale).ceil() as u32;
    anyhow::ensure!(width > 0 && height > 0, "SVG glyph bounds are empty");
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(width, height).context("SVG glyph pixmap is too large")?;
    let tree_size = tree.size();
    let transform = resvg::tiny_skia::Transform::from_scale(
        width as f32 / tree_size.width(),
        height as f32 / tree_size.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let width = u16::try_from(width).context("SVG glyph width exceeds the raster contract")?;
    let height = u16::try_from(height).context("SVG glyph height exceeds the raster contract")?;
    let (minimum_x, minimum_y, maximum_x, maximum_y) =
        visible_pixel_bounds(pixmap.data(), width, height)
            .context("SVG glyph renderer produced no visible pixels")?;
    let baseline_y = (ascent * scale).ceil() as i32;
    cropped_color_image(
        pixmap.data(),
        ColorImageCrop {
            source_width: width,
            minimum_x,
            minimum_y,
            maximum_x,
            maximum_y,
            source_left: 0,
            source_top: baseline_y,
        },
    )
    .map(Some)
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn svg_document_with_viewport(document: &[u8], viewport: SvgViewport) -> Result<Vec<u8>> {
    let document = if document.starts_with(&[0x1f, 0x8b]) {
        usvg::decompress_svgz(document).context("failed to decompress OpenType-SVG document")?
    } else {
        document.to_vec()
    };
    let mut document = String::from_utf8(document).context("OpenType-SVG document is not UTF-8")?;
    let svg_start = document
        .find("<svg")
        .context("OpenType-SVG root is missing")?;
    let root_end = document[svg_start..]
        .find('>')
        .map(|offset| svg_start + offset)
        .context("OpenType-SVG root tag is unterminated")?;
    let root = &document[svg_start..root_end];
    let mut attributes = String::new();
    // OpenType supplies an implicit font-space viewport that standalone SVG parsers do not know.
    if !svg_root_has_attribute(root, "viewBox") {
        attributes.push_str(&format!(
            " viewBox=\"{} {} {} {}\"",
            viewport.x, viewport.y, viewport.width, viewport.height
        ));
    }
    if !svg_root_has_attribute(root, "width") {
        attributes.push_str(&format!(" width=\"{}\"", viewport.width));
    }
    if !svg_root_has_attribute(root, "height") {
        attributes.push_str(&format!(" height=\"{}\"", viewport.height));
    }
    document.insert_str(root_end, &attributes);
    Ok(document.into_bytes())
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn svg_root_has_attribute(root: &str, attribute: &str) -> bool {
    root.match_indices(attribute).any(|(index, _)| {
        let has_boundary = index == 0
            || root[..index]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        has_boundary
            && root[index + attribute.len()..]
                .trim_start()
                .starts_with('=')
    })
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn render_colrv1_glyph(
    font_data: &FontData,
    glyph_id: GlyphId,
    pixel_size: f32,
) -> Result<Option<RenderedGlyphImage>> {
    anyhow::ensure!(
        pixel_size.is_finite() && pixel_size > 0.0,
        "COLRv1 glyph pixel size must be finite and positive"
    );
    let font_ref = SkrifaFontRef::from_index(font_data.data.data(), font_data.index)
        .context("failed to parse COLRv1 font data")?;
    let Some(color_glyph) = font_ref
        .color_glyphs()
        .get_with_format(skrifa::GlyphId::new(glyph_id.0), ColorGlyphFormat::ColrV1)
    else {
        return Ok(None);
    };

    let bounding_box =
        color_glyph.bounding_box(LocationRef::default(), SkrifaSize::new(pixel_size));
    let has_explicit_bounds = bounding_box.is_some();
    let (left, bottom, right, top) = if let Some(bounding_box) = bounding_box {
        anyhow::ensure!(
            [
                bounding_box.x_min,
                bounding_box.y_min,
                bounding_box.x_max,
                bounding_box.y_max,
            ]
            .into_iter()
            .all(f32::is_finite),
            "COLRv1 glyph bounds must be finite"
        );
        (
            bounding_box.x_min.floor() as i32,
            bounding_box.y_min.floor() as i32,
            bounding_box.x_max.ceil() as i32,
            bounding_box.y_max.ceil() as i32,
        )
    } else {
        let extent = (pixel_size * 2.0).ceil().max(1.0) as i32;
        (-extent, -extent, extent, extent)
    };
    let width = u16::try_from(right.checked_sub(left).context("COLRv1 width overflowed")?)
        .context("COLRv1 glyph is too wide to rasterize")?;
    let height = u16::try_from(
        top.checked_sub(bottom)
            .context("COLRv1 height overflowed")?,
    )
    .context("COLRv1 glyph is too tall to rasterize")?;
    anyhow::ensure!(width > 0 && height > 0, "COLRv1 glyph bounds are empty");

    let mut resources = Resources::new();
    let mut render_context = RenderContext::new(width, height);
    render_context.set_paint(BLACK);
    render_context
        .glyph_run(&mut resources, font_data)
        .font_size(pixel_size)
        .hint(false)
        .fill_glyphs(
            [VelloGlyph {
                id: glyph_id.0,
                x: -left as f32,
                y: top as f32,
            }]
            .into_iter(),
        );
    render_context.flush();
    let mut pixmap = VelloPixmap::new(width, height);
    render_context.render_to_pixmap(&mut resources, &mut pixmap);

    let (minimum_x, minimum_y, maximum_x, maximum_y) =
        visible_pixel_bounds(pixmap.data_as_u8_slice(), width, height)
            .context("COLRv1 renderer produced no visible pixels")?;
    if !has_explicit_bounds {
        anyhow::ensure!(
            minimum_x > 0
                && minimum_y > 0
                && maximum_x + 1 < usize::from(width)
                && maximum_y + 1 < usize::from(height),
            "COLRv1 glyph exceeded the conservative fallback canvas"
        );
    }

    cropped_color_image(
        pixmap.data_as_u8_slice(),
        ColorImageCrop {
            source_width: width,
            minimum_x,
            minimum_y,
            maximum_x,
            maximum_y,
            source_left: left,
            source_top: top,
        },
    )
    .map(Some)
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn cropped_color_image(source_pixels: &[u8], crop: ColorImageCrop) -> Result<RenderedGlyphImage> {
    let cropped_width = crop.maximum_x - crop.minimum_x + 1;
    let cropped_height = crop.maximum_y - crop.minimum_y + 1;
    let pixel_capacity = cropped_width
        .checked_mul(cropped_height)
        .and_then(|count| count.checked_mul(4))
        .context("cropped color glyph pixel count overflowed")?;
    let mut pixels = Vec::with_capacity(pixel_capacity);
    for y in crop.minimum_y..=crop.maximum_y {
        for x in crop.minimum_x..=crop.maximum_x {
            let offset = (y * usize::from(crop.source_width) + x) * 4;
            let pixel: [u8; 4] = source_pixels[offset..offset + 4]
                .try_into()
                .context("color glyph pixel buffer is truncated")?;
            pixels.extend_from_slice(&premultiplied_rgba_to_straight_bgra(pixel));
        }
    }

    let cropped_left = crop
        .source_left
        .checked_add(i32::try_from(crop.minimum_x)?)
        .context("color glyph left bearing overflowed")?;
    let cropped_top = crop
        .source_top
        .checked_sub(i32::try_from(crop.minimum_y)?)
        .context("color glyph top bearing overflowed")?;
    Ok(RenderedGlyphImage {
        info: GlyphRasterInfo {
            bounds: Bounds {
                origin: point(cropped_left.into(), (-cropped_top).into()),
                size: size(
                    i32::try_from(cropped_width)?.into(),
                    i32::try_from(cropped_height)?.into(),
                ),
            },
            format: GlyphRasterFormat::ColorBgra8,
        },
        pixels,
    })
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn visible_pixel_bounds(
    pixels: &[u8],
    width: u16,
    height: u16,
) -> Option<(usize, usize, usize, usize)> {
    let expected_length = usize::from(width)
        .checked_mul(usize::from(height))?
        .checked_mul(4)?;
    if pixels.len() != expected_length {
        return None;
    }
    let mut bounds: Option<(usize, usize, usize, usize)> = None;
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        if pixel[3] == 0 {
            continue;
        }
        let x = index % usize::from(width);
        let y = index / usize::from(width);
        bounds = Some(match bounds {
            Some((minimum_x, minimum_y, maximum_x, maximum_y)) => (
                minimum_x.min(x),
                minimum_y.min(y),
                maximum_x.max(x),
                maximum_y.max(y),
            ),
            None => (x, y, x, y),
        });
    }
    bounds
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn premultiplied_rgba_to_straight_bgra(pixel: [u8; 4]) -> [u8; 4] {
    let [red, green, blue, alpha] = pixel;
    if alpha == 0 {
        return [0; 4];
    }
    let unpremultiply = |component: u8| {
        let numerator = u32::from(component) * 255 + u32::from(alpha) / 2;
        u8::try_from((numerator / u32::from(alpha)).min(255)).unwrap_or(255)
    };
    [
        unpremultiply(blue),
        unpremultiply(green),
        unpremultiply(red),
        alpha,
    ]
}

fn glyph_raster_format(
    content: swash::scale::image::Content,
    subpixel_rendering: bool,
) -> GlyphRasterFormat {
    match content {
        swash::scale::image::Content::Color => GlyphRasterFormat::ColorBgra8,
        swash::scale::image::Content::SubpixelMask => GlyphRasterFormat::SubpixelBgra8,
        swash::scale::image::Content::Mask if subpixel_rendering => {
            GlyphRasterFormat::SubpixelBgra8
        }
        swash::scale::image::Content::Mask => GlyphRasterFormat::Alpha8,
    }
}

fn is_color_capable_font(font: swash::FontRef<'_>) -> bool {
    [b"COLR", b"CBDT", b"sbix", b"SVG "]
        .into_iter()
        .any(|tag| font.table(swash::tag_from_bytes(tag)).is_some())
}

fn check_is_known_emoji_font(postscript_name: &str) -> bool {
    // This only selects color-capable Swash sources. The rendered Image::content
    // remains authoritative for the atlas format.
    matches!(postscript_name, "NotoColorEmoji" | "OpenMojiBlack")
}
#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    #[test]
    fn vello_color_pixels_are_normalized_once() {
        assert_eq!(
            premultiplied_rgba_to_straight_bgra([32, 64, 16, 128]),
            [32, 128, 64, 128]
        );
        assert_eq!(
            premultiplied_rgba_to_straight_bgra([200, 100, 50, 0]),
            [0, 0, 0, 0]
        );
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    #[test]
    fn opentype_svg_gets_an_explicit_font_viewport() -> Result<()> {
        let document = svg_document_with_viewport(
            br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"><path/></svg>"#,
            SvgViewport {
                x: 0.0,
                y: -800.0,
                width: 1000.0,
                height: 1000.0,
            },
        )?;
        let document = String::from_utf8(document)?;
        assert!(document.contains("viewBox=\"0 -800 1000 1000\""));
        assert!(document.contains("width=\"1000\""));
        assert!(document.contains("height=\"1000\""));
        Ok(())
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    #[test]
    fn opentype_svg_preserves_explicit_viewport_attributes() -> Result<()> {
        let document = svg_document_with_viewport(
            br#"<svg viewBox = '1 2 3 4' width = '5' height = '6'></svg>"#,
            SvgViewport {
                x: 0.0,
                y: 0.0,
                width: 1000.0,
                height: 1000.0,
            },
        )?;
        let document = String::from_utf8(document)?;
        assert_eq!(document.matches("viewBox").count(), 1);
        assert_eq!(document.matches("width").count(), 1);
        assert_eq!(document.matches("height").count(), 1);
        Ok(())
    }

    #[test]
    fn swash_content_is_authoritative_for_glyph_format() -> Result<()> {
        assert_eq!(
            glyph_raster_format(swash::scale::image::Content::Mask, false),
            GlyphRasterFormat::Alpha8
        );
        assert_eq!(
            glyph_raster_format(swash::scale::image::Content::Mask, true),
            GlyphRasterFormat::SubpixelBgra8
        );
        assert_eq!(
            glyph_raster_format(swash::scale::image::Content::SubpixelMask, false),
            GlyphRasterFormat::SubpixelBgra8
        );
        assert_eq!(
            glyph_raster_format(swash::scale::image::Content::Color, false),
            GlyphRasterFormat::ColorBgra8
        );

        let text_system = CosmicTextSystem::new_without_system_fonts("Lilex");
        text_system.add_fonts(vec![Cow::Borrowed(include_bytes!(
            "../../../assets/fonts/openmoji/openmoji.ttf"
        ))])?;
        let mut descriptor = gpui::font("OpenMoji");
        descriptor.weight = FontWeight::BLACK;
        let font_id = text_system.font_id(&descriptor)?;
        let glyph_id = text_system
            .glyph_for_char(font_id, '😀')
            .context("OpenMoji fixture must contain the grinning face")?;
        let params = RenderGlyphParams {
            font_id,
            glyph_id,
            font_size: gpui::px(32.0),
            subpixel_variant: gpui::Point::default(),
            scale_factor: 1.0,
            synthetic_italic: SyntheticItalic::disabled(),
            synthetic_bold: SyntheticBold::disabled(),
            is_emoji: true,
            subpixel_rendering: false,
            dilation: 0,
        };
        assert_eq!(
            text_system.glyph_raster_info(&params)?.format,
            GlyphRasterFormat::Alpha8,
            "the emoji source hint must not turn monochrome OpenMoji pixels into color"
        );
        Ok(())
    }

    fn fid(i: usize) -> FontId {
        FontId(i)
    }

    #[test]
    fn all_font_names_tracks_available_families() -> Result<()> {
        let text_system = gpui::TextSystem::new(Arc::new(
            CosmicTextSystem::new_without_system_fonts("IBM Plex Sans"),
        ));
        assert!(text_system.all_font_names().is_empty());

        text_system.add_fonts(vec![Cow::Borrowed(include_bytes!(
            "../../../assets/fonts/lilex/Lilex-Regular.ttf"
        ))])?;
        assert_eq!(text_system.all_font_names(), ["Lilex"]);

        text_system.add_fonts(vec![
            Cow::Borrowed(IBM_PLEX),
            Cow::Borrowed(include_bytes!("../../../assets/fonts/lilex/Lilex-Bold.ttf")),
        ])?;
        assert_eq!(text_system.all_font_names(), ["IBM Plex Sans", "Lilex"]);
        Ok(())
    }

    fn font_run(len: usize, font_id: FontId) -> FontRun {
        FontRun {
            len,
            font_id,
            font_style: FontStyle::Normal,
            font_weight: FontWeight::NORMAL,
        }
    }

    const IBM_PLEX: &[u8] =
        include_bytes!("../../../assets/fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf");

    /// Every code point of `Bidi_Class=B`, each of which starts a new bidi
    /// paragraph and so can split one line into mixed-direction paragraphs.
    const SEPARATORS: &[char] = &[
        '\u{000a}', '\u{000d}', '\u{001c}', '\u{001d}', '\u{001e}', '\u{0085}', '\u{2029}',
    ];

    fn text_system() -> Result<CosmicTextSystem> {
        let text_system = CosmicTextSystem::new_without_system_fonts("IBM Plex Sans");
        text_system.add_fonts(vec![Cow::Borrowed(IBM_PLEX)])?;
        Ok(text_system)
    }

    fn layout_text(text_system: &CosmicTextSystem, text: &str) -> Result<LineLayout> {
        let font_id = text_system.font_id(&gpui::font("IBM Plex Sans"))?;
        let runs = [font_run(text.len(), font_id)];
        Ok(text_system.layout_line(text, gpui::px(14.0), &runs))
    }

    /// Mirrors the original crash: mixed-direction text reaching the shaper
    /// through `shape_text`, which only splits lines on `\n`.
    #[test]
    fn shape_text_with_mixed_direction_paragraphs() -> Result<()> {
        let platform_text_system = Arc::new(text_system()?);
        let text_system = Arc::new(gpui::TextSystem::new(platform_text_system));
        let window_text_system = gpui::WindowTextSystem::new(text_system);

        let text: SharedString = "first line\n\u{05d0}\u{001c}A".into();
        let runs = [gpui::TextRun {
            len: text.len(),
            font: gpui::font("IBM Plex Sans"),
            ..Default::default()
        }];

        let lines = window_text_system.shape_text(text, gpui::px(14.0), &runs, None, None)?;

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].len(), "\u{05d0}\u{001c}A".len());
        assert!(lines[1].width() > Pixels::ZERO);
        Ok(())
    }

    #[test]
    fn layout_line_with_mixed_direction_paragraphs() -> Result<()> {
        let text_system = text_system()?;

        for separator in SEPARATORS {
            for text in [
                format!("\u{05d0}{separator}A"),
                format!("A{separator}\u{05d0}"),
            ] {
                let layout = layout_text(&text_system, &text)?;

                assert_eq!(layout.len, text.len(), "{text:?}");
                assert!(layout.width > Pixels::ZERO, "{text:?}");
                assert!(
                    layout.runs.iter().any(|run| !run.glyphs.is_empty()),
                    "{text:?}"
                );
            }
        }

        Ok(())
    }

    #[test]
    fn layout_line_with_separators_at_line_edges() -> Result<()> {
        let text_system = text_system()?;

        for text in [
            "\u{001c}",
            "\u{001c}\u{001c}",
            "\u{001c}\u{05d0}",
            "\u{05d0}\u{001c}",
            "\u{05d0}\u{001c}\u{001c}A",
            "\u{001c}\u{05d0}\u{001c}A\u{001c}",
        ] {
            let layout = layout_text(&text_system, text)?;
            assert_eq!(layout.len, text.len(), "{text:?}");
        }

        Ok(())
    }

    /// Glyph indices must stay absolute and positions ordered across segment
    /// boundaries, otherwise cursor placement and hit testing desync. Uses
    /// single-direction text so visual order matches logical order.
    #[test]
    fn layout_line_keeps_indices_and_positions_ordered_across_paragraphs() -> Result<()> {
        let text_system = text_system()?;
        let text = "ab\u{001c}cd\u{2029}ef";
        let layout = layout_text(&text_system, text)?;

        let glyphs: Vec<_> = layout.runs.iter().flat_map(|run| &run.glyphs).collect();
        assert!(!glyphs.is_empty());

        for glyph in &glyphs {
            assert!(glyph.index < text.len(), "{:?}", glyph.index);
            assert!(text.is_char_boundary(glyph.index), "{:?}", glyph.index);
        }
        for pair in glyphs.windows(2) {
            assert!(pair[0].index < pair[1].index);
            assert!(pair[0].position.x <= pair[1].position.x);
        }

        // Every segment contributes width, so the whole line is wider than its
        // leading paragraph alone.
        assert!(layout.width > layout_text(&text_system, "ab")?.width);
        Ok(())
    }

    /// A font run boundary that does not line up with a paragraph boundary must
    /// still be clipped to the right segments.
    #[test]
    fn layout_line_with_font_run_straddling_a_separator() -> Result<()> {
        let text_system = text_system()?;
        let font_id = text_system.font_id(&gpui::font("IBM Plex Sans"))?;
        let text = "ab\u{001c}\u{05d0}\u{05d1}";

        // The run boundary falls inside the trailing RTL paragraph.
        let runs = [
            font_run("ab\u{001c}\u{05d0}".len(), font_id),
            font_run("\u{05d1}".len(), font_id),
        ];
        let layout = text_system.layout_line(text, gpui::px(14.0), &runs);

        assert_eq!(layout.len, text.len());
        assert!(layout.width > Pixels::ZERO);
        Ok(())
    }

    /// Lines with no separator take the fast path and must be shaped exactly as
    /// they were before paragraph splitting existed.
    #[test]
    fn layout_line_without_separators_takes_fast_path() -> Result<()> {
        let text_system = text_system()?;

        for text in [
            "hello world",
            "\u{05d0}\u{05d1}\u{05d2}",
            "mixed \u{05d0}\u{05d1}",
        ] {
            assert!(!contains_paragraph_separator(text), "{text:?}");
            let layout = layout_text(&text_system, text)?;
            assert_eq!(layout.len, text.len(), "{text:?}");
            assert!(layout.width > Pixels::ZERO, "{text:?}");
        }

        Ok(())
    }

    /// cosmic-text sums word widths to get a line's width but accumulates glyph
    /// advances to position glyphs, so a trailing zero-advance glyph (here a
    /// zero-width space) can land a few ulps past the width. When that glyph is a
    /// wrap boundary, the row before it extends past the line's width, and hit
    /// testing in that sliver used to panic (ZED-BW8, ZED-75K, ZED-81Z).
    #[test]
    #[allow(
        clippy::while_float,
        reason = "the loop advances by one ULP per iteration and is bounded by the line width"
    )]
    fn index_for_position_past_line_width() -> Result<()> {
        let text_system = Arc::new(gpui::TextSystem::new(Arc::new(text_system()?)));
        let window_text_system = gpui::WindowTextSystem::new(text_system);
        let text: SharedString = "Warning: this will delete files\u{200b}".into();
        let runs = [gpui::TextRun {
            len: text.len(),
            font: gpui::font("IBM Plex Sans"),
            ..Default::default()
        }];
        let lines =
            window_text_system.shape_text(text, gpui::px(14.), &runs, Some(gpui::px(4.)), None)?;
        let line = &lines[0];
        let width = line.unwrapped_layout.width;
        let boundary_glyph = |row: usize| {
            let boundary = line.wrap_boundaries()[row];
            &line.runs()[boundary.run_ix].glyphs[boundary.glyph_ix]
        };
        let row = (1..line.wrap_boundaries().len())
            .find(|row| boundary_glyph(*row).position.x > width)
            .expect("trailing zero-width space should be a wrap boundary past the line width");
        let row_start_x = f32::from(boundary_glyph(row - 1).position.x);
        let row_end = boundary_glyph(row);

        let mut x = f32::from(width) - row_start_x;
        while x + row_start_x < f32::from(width) {
            x = x.next_up();
        }
        assert!(gpui::px(x + row_start_x) < row_end.position.x);

        let line_height = gpui::px(20.);
        let position = gpui::point(gpui::px(x), line_height * row as f32 + gpui::px(1.));
        assert_eq!(
            line.index_for_position(position, line_height),
            Err(row_end.index)
        );
        Ok(())
    }

    #[test]
    fn paragraph_separator_detection() {
        for separator in SEPARATORS {
            assert!(is_paragraph_separator(*separator), "{separator:?}");
            assert!(contains_paragraph_separator(&format!("a{separator}b")));
        }

        for text in [
            "",
            "plain ascii",
            "\u{05d0}",
            "tab\there",
            "emoji \u{1f600}",
        ] {
            assert!(!contains_paragraph_separator(text), "{text:?}");
        }
    }

    #[test]
    fn font_runs_are_clipped_to_segment() {
        let runs = [font_run(3, fid(1)), font_run(4, fid(2))];

        assert_eq!(clip_font_runs(&runs, 0..7).as_slice(), &runs);
        assert_eq!(
            clip_font_runs(&runs, 2..5).as_slice(),
            &[font_run(1, fid(1)), font_run(2, fid(2)),]
        );
        assert_eq!(
            clip_font_runs(&runs, 3..7).as_slice(),
            &[font_run(4, fid(2))]
        );
        assert!(clip_font_runs(&runs, 5..5).is_empty());
    }
}
