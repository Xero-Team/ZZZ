#![cfg(feature = "test-support")]

use anyhow::Context as _;
use gpui::{
    AnyWindowHandle, AppContext as _, AtlasContentKind, AtlasKey, AtlasPolicy, AtlasSnapshot,
    AtlasTextureKind, AtlasTile, AtlasUsage, Bounds, ContentMask, Context, Corners, DevicePixels,
    Edges, FontId, FontRun, FontStyle, FontWeight, GlyphId, GlyphRasterFormat, HeadlessAppContext,
    Hsla, ImageId, IntoElement, IsZero as _, MonochromeSprite, PaddedBool32, PathBuilder,
    PlatformAtlas, PlatformHeadlessRenderer, PlatformTextSystem, Point, PolychromeSprite, Quad,
    Render, RenderGlyphParams, RenderImageParams, RenderSvgParams, ScaledPixels, Scene, Shadow,
    Size, Styled as _, SubpixelSprite, TransformationMatrix, Underline, Window, canvas, font,
    point, px, radians, size,
};
use gpui_wgpu::{CosmicTextSystem, WgpuHeadlessRenderer};
use std::{borrow::Cow, collections::HashSet, path::Path, sync::Arc, time::Instant};

const RUNS: usize = 100;

struct GlyphAtOriginView {
    device_origin_x: f32,
    font_id: FontId,
    glyph_id: GlyphId,
}

impl Render for GlyphAtOriginView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let device_origin_x = self.device_origin_x;
        let font_id = self.font_id;
        let glyph_id = self.glyph_id;
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let origin_x = px(device_origin_x / window.scale_factor());
                window
                    .paint_glyph(
                        point(origin_x, px(20.0)),
                        font_id,
                        glyph_id,
                        px(20.0),
                        gpui::rgb(0xffffff).into(),
                        Default::default(),
                        Default::default(),
                    )
                    .expect("test glyph should paint");
            },
        )
        .size_full()
    }
}

fn target(scale: f32) -> Size<DevicePixels> {
    Size {
        width: DevicePixels((200.0 * scale) as i32),
        height: DevicePixels((100.0 * scale) as i32),
    }
}

fn bounds(x: f32, y: f32, width: f32, height: f32, scale: f32) -> Bounds<ScaledPixels> {
    Bounds {
        origin: Point {
            x: ScaledPixels(x * scale),
            y: ScaledPixels(y * scale),
        },
        size: Size {
            width: ScaledPixels(width * scale),
            height: ScaledPixels(height * scale),
        },
    }
}

fn full_mask(scale: f32) -> ContentMask<ScaledPixels> {
    ContentMask {
        bounds: bounds(0.0, 0.0, 200.0, 100.0, scale),
    }
}

fn tile(renderer: &WgpuHeadlessRenderer, key: AtlasKey, bytes: Vec<u8>) -> AtlasTile {
    renderer
        .sprite_atlas()
        .get_or_insert_with(key, &mut || {
            Ok(Some((
                Size {
                    width: DevicePixels(8),
                    height: DevicePixels(8),
                },
                Cow::Owned(bytes.clone()),
            )))
        })
        .expect("atlas insert should succeed")
        .expect("atlas insert should produce a tile")
}

fn image_tile(
    atlas: &Arc<dyn PlatformAtlas>,
    key: AtlasKey,
    size: Size<DevicePixels>,
    bgra: [u8; 4],
) -> anyhow::Result<AtlasTile> {
    let pixel_count = (size.width.0 as usize).saturating_mul(size.height.0 as usize);
    atlas
        .get_or_insert_with(key, &mut || {
            Ok(Some((
                size,
                Cow::Owned((0..pixel_count).flat_map(|_| bgra).collect()),
            )))
        })?
        .ok_or_else(|| anyhow::anyhow!("image builder returned no tile"))
}

fn raw_tile(
    atlas: &Arc<dyn PlatformAtlas>,
    key: AtlasKey,
    size: Size<DevicePixels>,
    bytes: Vec<u8>,
) -> anyhow::Result<AtlasTile> {
    atlas
        .get_or_insert_with(key, &mut || Ok(Some((size, Cow::Borrowed(&bytes)))))?
        .context("raw atlas fixture must produce a tile")
}

fn glyph_atlas_key(glyph_id: u32, format: GlyphRasterFormat) -> AtlasKey {
    AtlasKey::glyph(
        RenderGlyphParams {
            font_id: FontId(0),
            glyph_id: GlyphId(glyph_id),
            font_size: px(16.0),
            subpixel_variant: Point::default(),
            scale_factor: 1.0,
            synthetic_italic: Default::default(),
            synthetic_bold: Default::default(),
            is_emoji: format == GlyphRasterFormat::ColorBgra8,
            subpixel_rendering: format == GlyphRasterFormat::SubpixelBgra8,
            dilation: 0,
        },
        format,
    )
}

fn image_scene(tile: AtlasTile) -> Scene {
    let sprite_bounds = bounds(0.0, 0.0, 8.0, 8.0, 1.0);
    let mut scene = Scene::default();
    scene.insert_primitive(PolychromeSprite {
        order: 0,
        pad: 0,
        grayscale: PaddedBool32::from(false),
        opacity: 1.0,
        bounds: sprite_bounds,
        content_mask: ContentMask {
            bounds: sprite_bounds,
        },
        corner_radii: Corners::default(),
        tile,
    });
    scene.finish();
    scene
}

fn assert_pixel(image: &image::RgbaImage, expected: [u8; 4]) {
    assert_eq!(image.get_pixel(4, 4).0, expected);
}

fn assert_retired_tile_case(
    renderer: &mut WgpuHeadlessRenderer,
    case_name: &str,
    image_size: Size<DevicePixels>,
    first_image_id: usize,
    old_content_survives_page_retirement: bool,
) -> anyhow::Result<()> {
    let atlas = renderer.sprite_atlas();
    let first_key = AtlasKey::Image(RenderImageParams {
        image_id: ImageId(first_image_id),
        frame_index: 0,
    });
    let second_key = AtlasKey::Image(RenderImageParams {
        image_id: ImageId(first_image_id + 1),
        frame_index: 0,
    });

    let first_frame = atlas.begin_frame();
    let first_tile = image_tile(&atlas, first_key.clone(), image_size, [0, 0, 255, 255])?;
    let first_scene = image_scene(first_tile);
    atlas.finish_frame(first_frame, first_scene.atlas_usage());
    assert_pixel(
        &renderer.render_scene_to_image(
            &first_scene,
            Size {
                width: DevicePixels(8),
                height: DevicePixels(8),
            },
        )?,
        [255, 0, 0, 255],
    );

    atlas.remove(&first_key);
    let replacement_frame = atlas.begin_frame();
    let second_tile = image_tile(&atlas, second_key, image_size, [255, 0, 0, 255])?;
    assert_ne!(first_tile.tile_id, second_tile.tile_id);
    assert_ne!(
        (first_tile.texture_id, first_tile.bounds),
        (second_tile.texture_id, second_tile.bounds)
    );
    let second_scene = image_scene(second_tile);

    let old_before_finish = renderer.render_scene_to_image(
        &first_scene,
        Size {
            width: DevicePixels(8),
            height: DevicePixels(8),
        },
    )?;
    assert_pixel(&old_before_finish, [255, 0, 0, 255]);
    atlas.finish_frame(replacement_frame, second_scene.atlas_usage());

    let old_after_finish = renderer.render_scene_to_image(
        &first_scene,
        Size {
            width: DevicePixels(8),
            height: DevicePixels(8),
        },
    )?;
    assert_pixel(
        &old_after_finish,
        if old_content_survives_page_retirement {
            [255, 0, 0, 255]
        } else {
            [0, 0, 0, 0]
        },
    );
    let replacement = renderer.render_scene_to_image(
        &second_scene,
        Size {
            width: DevicePixels(8),
            height: DevicePixels(8),
        },
    )?;
    assert_pixel(&replacement, [0, 0, 255, 255]);

    save_named_output(
        &old_before_finish,
        &format!("text-003-{case_name}-old-before-finish"),
    );
    save_named_output(
        &old_after_finish,
        &format!("text-003-{case_name}-old-after-finish"),
    );
    save_named_output(&replacement, &format!("text-003-{case_name}-replacement"));
    Ok(())
}

fn assert_working_set_over_budget(force_fallback_adapter: bool) -> anyhow::Result<()> {
    let mut policy = AtlasPolicy::default();
    policy.set_page_size(Size {
        width: DevicePixels(8),
        height: DevicePixels(8),
    });
    policy.set_retained_budget(AtlasContentKind::Image, 0);
    let mut renderer = WgpuHeadlessRenderer::new_with_atlas_policy(force_fallback_adapter, policy)?;
    let atlas = renderer.sprite_atlas();
    let frame = atlas.begin_frame();
    let colors = [
        [0, 0, 255, 255],
        [0, 255, 0, 255],
        [255, 0, 0, 255],
        [0, 255, 255, 255],
    ];
    let mut tiles = Vec::new();
    for (image_id, color) in colors.into_iter().enumerate() {
        tiles.push(image_tile(
            &atlas,
            AtlasKey::Image(RenderImageParams {
                image_id: ImageId(100 + image_id),
                frame_index: 0,
            }),
            Size {
                width: DevicePixels(8),
                height: DevicePixels(8),
            },
            color,
        )?);
    }

    let full_bounds = bounds(0.0, 0.0, 16.0, 16.0, 1.0);
    let mut scene = Scene::default();
    for (index, tile) in tiles.iter().copied().enumerate() {
        scene.insert_primitive(PolychromeSprite {
            order: 0,
            pad: 0,
            grayscale: PaddedBool32::from(false),
            opacity: 1.0,
            bounds: bounds(
                (index % 2) as f32 * 8.0,
                (index / 2) as f32 * 8.0,
                8.0,
                8.0,
                1.0,
            ),
            content_mask: ContentMask {
                bounds: full_bounds,
            },
            corner_radii: Corners::default(),
            tile,
        });
    }
    scene.finish();
    atlas.finish_frame(frame, scene.atlas_usage());

    let maintenance_frame = atlas.begin_frame();
    let snapshot = atlas.snapshot();
    let image = snapshot.content(AtlasContentKind::Image);
    assert_eq!(image.retained_budget_bytes, 0);
    assert_eq!(image.page_count, 4);
    assert_eq!(image.working_set_bytes, 4 * 10 * 10 * 4);
    assert_eq!(image.evictions, 0);
    assert_eq!(image.budget_pressure_frames, 1);
    assert_eq!(snapshot.entry_count, 4);
    atlas.finish_frame(maintenance_frame, scene.atlas_usage());

    let image = renderer.render_scene_to_image(
        &scene,
        Size {
            width: DevicePixels(16),
            height: DevicePixels(16),
        },
    )?;
    assert_eq!(image.get_pixel(4, 4).0, [255, 0, 0, 255]);
    assert_eq!(image.get_pixel(12, 4).0, [0, 255, 0, 255]);
    assert_eq!(image.get_pixel(4, 12).0, [0, 0, 255, 255]);
    assert_eq!(image.get_pixel(12, 12).0, [255, 255, 0, 255]);
    let adapter = if force_fallback_adapter {
        "fallback"
    } else {
        "hardware"
    };
    save_named_output(&image, &format!("text-005-{adapter}"));
    if let Ok(output_directory) = std::env::var("GPUI_HEADLESS_OUTPUT_DIR") {
        let artifact = serde_json::json!({
            "experiment": "TEXT-005",
            "adapter": adapter,
            "policy": {
                "page_size": [8, 8],
                "retained_image_budget_bytes": 0,
            },
            "visible_tiles": tiles.len(),
            "snapshot": snapshot_json(snapshot),
        });
        std::fs::write(
            std::path::Path::new(&output_directory).join(format!("text-005-{adapter}.json")),
            serde_json::to_vec_pretty(&artifact)?,
        )?;
    }
    Ok(())
}

fn primitive_scene(renderer: &WgpuHeadlessRenderer, scale: f32) -> Scene {
    let green: Hsla = gpui::rgb(0x00ff00).into();
    let red: Hsla = gpui::rgb(0xff0000).into();
    let white: Hsla = gpui::rgb(0xffffff).into();
    let blue: Hsla = gpui::rgb(0x0000ff).into();
    let mono = tile(
        renderer,
        AtlasKey::Svg(RenderSvgParams {
            path: "headless-mono".into(),
            size: Size {
                width: DevicePixels(8),
                height: DevicePixels(8),
            },
        }),
        vec![255; 64],
    );
    let poly = tile(
        renderer,
        AtlasKey::Image(RenderImageParams {
            image_id: ImageId(1),
            frame_index: 0,
        }),
        (0..64).flat_map(|_| [0u8, 0, 255, 255]).collect(),
    );

    let mut scene = Scene::default();
    scene.insert_primitive(Quad {
        bounds: bounds(5.0, 5.0, 30.0, 30.0, scale),
        content_mask: full_mask(scale),
        background: green.into(),
        ..Default::default()
    });
    scene.insert_primitive(Quad {
        bounds: bounds(45.0, 5.0, 30.0, 30.0, scale),
        content_mask: full_mask(scale),
        border_color: white,
        border_widths: Edges::all(ScaledPixels(4.0 * scale)),
        ..Default::default()
    });
    scene.insert_primitive(Shadow {
        order: 0,
        blur_radius: ScaledPixels(4.0 * scale),
        bounds: bounds(85.0, 5.0, 30.0, 30.0, scale),
        corner_radii: Corners::default(),
        content_mask: full_mask(scale),
        color: blue,
    });
    scene.insert_primitive(Underline {
        order: 0,
        pad: 0,
        bounds: bounds(125.0, 15.0, 30.0, 4.0, scale),
        content_mask: full_mask(scale),
        color: white,
        thickness: ScaledPixels(4.0 * scale),
        wavy: PaddedBool32::from(false),
    });
    scene.insert_primitive(Quad {
        bounds: bounds(160.0, 5.0, 30.0, 30.0, scale),
        content_mask: ContentMask {
            bounds: bounds(160.0, 5.0, 15.0, 30.0, scale),
        },
        background: red.into(),
        ..Default::default()
    });
    scene.insert_primitive(MonochromeSprite {
        order: 0,
        pad: 0,
        bounds: bounds(5.0, 55.0, 30.0, 30.0, scale),
        content_mask: full_mask(scale),
        color: green,
        tile: mono,
        transformation: Default::default(),
    });
    scene.insert_primitive(PolychromeSprite {
        order: 0,
        pad: 0,
        grayscale: PaddedBool32::from(false),
        opacity: 1.0,
        bounds: bounds(45.0, 55.0, 30.0, 30.0, scale),
        content_mask: full_mask(scale),
        corner_radii: Corners::default(),
        tile: poly,
    });

    let mut path_builder = PathBuilder::fill();
    path_builder.move_to(gpui::point(gpui::px(100.0 * scale), gpui::px(55.0 * scale)));
    path_builder.line_to(gpui::point(gpui::px(130.0 * scale), gpui::px(55.0 * scale)));
    path_builder.line_to(gpui::point(gpui::px(115.0 * scale), gpui::px(85.0 * scale)));
    path_builder.close();
    let mut path = path_builder
        .build()
        .expect("triangle path should build")
        .scale(1.0);
    path.content_mask = full_mask(scale);
    path.color = red.into();
    scene.insert_primitive(path);
    scene.finish();
    scene
}

fn assert_scene_pixels(image: &image::RgbaImage, scale: f32) {
    let close = |actual: u8, expected: u8| (actual as i16 - expected as i16).abs() <= 8;
    let assert_pixel = |x: u32, y: u32, expected: [u8; 4]| {
        let x = (x as f32 * scale) as u32;
        let y = (y as f32 * scale) as u32;
        let actual = image.get_pixel(x, y).0;
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(actual, expected)| close(*actual, expected)),
            "pixel ({x}, {y}) was {actual:?}, expected {expected:?}"
        );
    };
    assert_pixel(20, 20, [0, 255, 0, 255]);
    assert_pixel(47, 20, [255, 255, 255, 255]);
    assert_pixel(60, 20, [0, 0, 0, 0]);
    assert_pixel(100, 20, [0, 0, 255, 255]);
    assert_pixel(140, 17, [255, 255, 255, 255]);
    assert_pixel(165, 20, [255, 0, 0, 255]);
    assert_pixel(180, 20, [0, 0, 0, 0]);
    assert_pixel(20, 70, [0, 255, 0, 255]);
    assert_pixel(60, 70, [255, 0, 0, 255]);
    assert_pixel(115, 65, [255, 0, 0, 255]);
}

fn save_output(image: &image::RgbaImage, adapter: &str, scale: f32) {
    let Ok(directory) = std::env::var("GPUI_HEADLESS_OUTPUT_DIR") else {
        return;
    };
    std::fs::create_dir_all(&directory).expect("headless output directory should be created");
    image
        .save(std::path::Path::new(&directory).join(format!("{adapter}-{scale:.0}x.png")))
        .expect("headless debug image should save");
}

fn save_named_output(image: &image::RgbaImage, name: &str) {
    let Ok(directory) = std::env::var("GPUI_HEADLESS_OUTPUT_DIR") else {
        return;
    };
    std::fs::create_dir_all(&directory).expect("headless output directory should be created");
    image
        .save(std::path::Path::new(&directory).join(format!("{name}.png")))
        .expect("headless debug image should save");
}

fn render_glyph_at_device_x(
    force_fallback_adapter: bool,
    device_origin_x: f32,
) -> anyhow::Result<image::RgbaImage> {
    let platform_text_system = Arc::new(CosmicTextSystem::new_without_system_fonts("Lilex"));
    platform_text_system.add_fonts(vec![Cow::Borrowed(include_bytes!(
        "../../../assets/fonts/lilex/Lilex-Regular.ttf"
    ))])?;
    let font_id = platform_text_system.font_id(&font("Lilex"))?;
    let glyph_id = platform_text_system
        .glyph_for_char(font_id, 'M')
        .ok_or_else(|| anyhow::anyhow!("Lilex must contain M"))?;

    let mut context =
        HeadlessAppContext::with_platform(platform_text_system, Arc::new(()), move || {
            Some(Box::new(
                WgpuHeadlessRenderer::new_with_fallback(force_fallback_adapter)
                    .expect("WGPU headless renderer should initialize"),
            ))
        });
    let window = context.open_window(size(px(32.0), px(32.0)), move |_, cx| {
        cx.new(|_| GlyphAtOriginView {
            device_origin_x,
            font_id,
            glyph_id,
        })
    })?;
    let window_handle: AnyWindowHandle = window.into();
    context.update_window(window_handle, |_, window, cx| {
        window.draw(cx).clear();
    })?;
    context.capture_screenshot(window_handle)
}

fn assert_negative_subpixel_phase(force_fallback_adapter: bool) -> anyhow::Result<()> {
    let negative = render_glyph_at_device_x(force_fallback_adapter, -0.25)?;
    let positive = render_glyph_at_device_x(force_fallback_adapter, 0.75)?;
    assert_eq!(negative.dimensions(), positive.dimensions());

    let mut visible_pixels = 0usize;
    for y in 0..negative.height() {
        for x in 0..negative.width() - 1 {
            let negative_pixel = negative.get_pixel(x, y);
            let shifted_positive_pixel = positive.get_pixel(x + 1, y);
            assert_eq!(
                negative_pixel, shifted_positive_pixel,
                "glyph phases diverged at ({x}, {y})"
            );
            visible_pixels += usize::from(negative_pixel.0[3] > 0);
        }
    }
    assert!(visible_pixels > 0, "glyph must produce visible coverage");

    let adapter = if force_fallback_adapter {
        "fallback"
    } else {
        "hardware"
    };
    save_named_output(&negative, &format!("text-002-{adapter}-negative"));
    save_named_output(&positive, &format!("text-002-{adapter}-positive"));
    Ok(())
}

fn snapshot_json(snapshot: AtlasSnapshot) -> serde_json::Value {
    let content = |content_kind| {
        let content = snapshot.content(content_kind);
        serde_json::json!({
            "page_count": content.page_count,
            "resident_bytes": content.resident_bytes,
            "entry_count": content.entry_count,
            "retained_budget_bytes": content.retained_budget_bytes,
            "working_set_bytes": content.working_set_bytes,
            "evictions": content.evictions,
            "compactions": content.compactions,
            "budget_pressure_frames": content.budget_pressure_frames,
        })
    };
    serde_json::json!({
        "page_count": snapshot.page_count,
        "resident_bytes": snapshot.resident_bytes,
        "entry_count": snapshot.entry_count,
        "hits": snapshot.hits,
        "misses": snapshot.misses,
        "allocations": snapshot.allocations,
        "pending_uploads": snapshot.pending_uploads,
        "pending_upload_bytes": snapshot.pending_upload_bytes,
        "upload_calls": snapshot.upload_calls,
        "uploaded_bytes": snapshot.uploaded_bytes,
        "removals": snapshot.removals,
        "retirements": snapshot.retirements,
        "evictions": snapshot.evictions,
        "compactions": snapshot.compactions,
        "current_frame_working_set_bytes": snapshot.current_frame_working_set_bytes,
        "budget_pressure_frames": snapshot.budget_pressure_frames,
        "content": {
            "glyph_alpha": content(AtlasContentKind::GlyphAlpha),
            "glyph_subpixel": content(AtlasContentKind::GlyphSubpixel),
            "glyph_color": content(AtlasContentKind::GlyphColor),
            "svg_mask": content(AtlasContentKind::SvgMask),
            "image": content(AtlasContentKind::Image),
        },
    })
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    let mut samples = samples.to_vec();
    samples.sort_unstable();
    let index = (samples.len() - 1) * percentile / 100;
    samples[index]
}

struct InsertedText {
    usage: AtlasUsage,
    tiles: Vec<AtlasTile>,
}

fn insert_shaped_text(
    text_system: &CosmicTextSystem,
    atlas: &Arc<dyn PlatformAtlas>,
    text: &str,
    font_size: f32,
    scale_factor: f32,
    unique_keys: &mut HashSet<AtlasKey>,
    keys_in_order: &mut Vec<AtlasKey>,
) -> anyhow::Result<InsertedText> {
    let font_id = text_system.font_id(&font("Lilex"))?;
    let runs = [FontRun {
        len: text.len(),
        font_id,
        font_style: FontStyle::Normal,
        font_weight: FontWeight::NORMAL,
    }];
    let layout = text_system.layout_line(text, px(font_size), &runs);
    let mut usage = AtlasUsage::default();
    let mut tiles = Vec::new();

    for run in &layout.runs {
        for (glyph_index, glyph) in run.glyphs.iter().enumerate() {
            let params = RenderGlyphParams {
                font_id: run.font_id,
                glyph_id: glyph.id,
                font_size: layout.font_size,
                subpixel_variant: Point::new((glyph_index % 4) as u8, 0),
                scale_factor,
                synthetic_italic: run.synthetic_italic,
                synthetic_bold: run.synthetic_bold,
                is_emoji: glyph.is_emoji,
                subpixel_rendering: !glyph.is_emoji,
                dilation: 0,
            };
            let raster_info = text_system.glyph_raster_info(&params)?;
            if raster_info.bounds.is_zero() {
                continue;
            }
            let key = AtlasKey::glyph(params.clone(), raster_info.format);
            let Some(tile) = atlas.get_or_insert_with(key.clone(), &mut || {
                let rasterized = text_system.rasterize_glyph(&params, raster_info)?;
                Ok(Some((
                    rasterized.info.bounds.size,
                    Cow::Owned(rasterized.pixels),
                )))
            })?
            else {
                continue;
            };
            usage.insert(tile);
            tiles.push(tile);
            if unique_keys.insert(key.clone()) {
                keys_in_order.push(key);
            }
        }
    }
    Ok(InsertedText { usage, tiles })
}

fn glyph_grid_scene(tiles: &[AtlasTile]) -> (Scene, Size<DevicePixels>) {
    const COLUMNS: usize = 16;
    const CELL_SIZE: f32 = 8.0;
    let rows = tiles.len().div_ceil(COLUMNS).max(1);
    let target_size = Size {
        width: DevicePixels((COLUMNS as f32 * CELL_SIZE) as i32),
        height: DevicePixels((rows as f32 * CELL_SIZE) as i32),
    };
    let full_bounds = bounds(
        0.0,
        0.0,
        target_size.width.0 as f32,
        target_size.height.0 as f32,
        1.0,
    );
    let mut scene = Scene::default();
    for (index, tile) in tiles.iter().copied().enumerate() {
        let sprite_bounds = bounds(
            (index % COLUMNS) as f32 * CELL_SIZE,
            (index / COLUMNS) as f32 * CELL_SIZE,
            CELL_SIZE,
            CELL_SIZE,
            1.0,
        );
        let content_mask = ContentMask {
            bounds: full_bounds,
        };
        match tile.texture_id.kind {
            AtlasTextureKind::Monochrome => scene.insert_primitive(MonochromeSprite {
                order: 0,
                pad: 0,
                bounds: sprite_bounds,
                content_mask,
                color: gpui::rgb(0xffffff).into(),
                tile,
                transformation: TransformationMatrix::unit(),
            }),
            AtlasTextureKind::Subpixel => scene.insert_primitive(SubpixelSprite {
                order: 0,
                pad: 0,
                bounds: sprite_bounds,
                content_mask,
                color: gpui::rgb(0xffffff).into(),
                tile,
                transformation: TransformationMatrix::unit(),
            }),
            AtlasTextureKind::Polychrome => scene.insert_primitive(PolychromeSprite {
                order: 0,
                pad: 0,
                grayscale: PaddedBool32::from(false),
                opacity: 1.0,
                bounds: sprite_bounds,
                content_mask,
                corner_radii: Corners::default(),
                tile,
            }),
        }
    }
    scene.finish();
    (scene, target_size)
}

fn assert_glyph_grid_has_no_empty_cells(image: &image::RgbaImage, tile_count: usize) {
    const COLUMNS: usize = 16;
    const CELL_SIZE: u32 = 8;
    for index in 0..tile_count {
        let origin_x = (index % COLUMNS) as u32 * CELL_SIZE;
        let origin_y = (index / COLUMNS) as u32 * CELL_SIZE;
        let has_coverage = (origin_y..origin_y + CELL_SIZE).any(|y| {
            (origin_x..origin_x + CELL_SIZE)
                .any(|x| image.get_pixel(x, y).0.iter().any(|channel| *channel > 0))
        });
        assert!(has_coverage, "glyph cell {index} rendered no coverage");
    }
}

fn insert_fixed_assets(
    atlas: &Arc<dyn PlatformAtlas>,
    unique_keys: &mut HashSet<AtlasKey>,
    keys_in_order: &mut Vec<AtlasKey>,
) -> anyhow::Result<()> {
    let size = Size {
        width: DevicePixels(64),
        height: DevicePixels(64),
    };
    for asset_index in 0..8 {
        let svg_key = AtlasKey::Svg(RenderSvgParams {
            path: format!("text-atlas-baseline-{asset_index}.svg").into(),
            size,
        });
        atlas
            .get_or_insert_with(svg_key.clone(), &mut || {
                Ok(Some((size, Cow::Owned(vec![asset_index as u8; 64 * 64]))))
            })?
            .ok_or_else(|| anyhow::anyhow!("SVG builder returned no atlas tile"))?;
        unique_keys.insert(svg_key.clone());
        keys_in_order.push(svg_key);

        let image_key = AtlasKey::Image(RenderImageParams {
            image_id: ImageId(asset_index + 1),
            frame_index: asset_index,
        });
        atlas
            .get_or_insert_with(image_key.clone(), &mut || {
                Ok(Some((
                    size,
                    Cow::Owned(vec![asset_index as u8; 64 * 64 * 4]),
                )))
            })?
            .ok_or_else(|| anyhow::anyhow!("image builder returned no atlas tile"))?;
        unique_keys.insert(image_key.clone());
        keys_in_order.push(image_key);
    }
    Ok(())
}

fn run_text_atlas_baseline(
    force_fallback_adapter: bool,
    output_directory: &Path,
) -> anyhow::Result<()> {
    const SEED: u64 = 0x5A5A_5445_5854_2026;
    const CJK_SCROLL_GLYPHS: u32 = 1024;
    const CJK_CHUNK_GLYPHS: usize = 64;

    let mut renderer = WgpuHeadlessRenderer::new_with_fallback(force_fallback_adapter)?;
    let gpu_specs = renderer.gpu_specs();
    anyhow::ensure!(
        gpu_specs.is_software_emulated == force_fallback_adapter,
        "requested fallback={force_fallback_adapter}, selected {}",
        gpu_specs.device_name
    );
    let atlas = renderer.sprite_atlas();
    let text_system = CosmicTextSystem::new("Noto Sans");
    text_system.add_fonts(vec![
        Cow::Borrowed(include_bytes!(
            "../../../assets/fonts/lilex/Lilex-Regular.ttf"
        )),
        Cow::Borrowed(include_bytes!(
            "../../../assets/fonts/openmoji/openmoji.ttf"
        )),
    ])?;

    let mixed_text = "office affine A\u{0301} 中文测试 مرحبا بالعالم 👩‍💻🌈";
    let cjk_characters = (0..CJK_SCROLL_GLYPHS)
        .filter_map(|offset| char::from_u32(0x4E00 + offset))
        .collect::<Vec<_>>();
    let mut unique_keys = HashSet::new();
    let mut keys_in_order = Vec::new();
    let mut cold_group_nanoseconds = Vec::new();

    for font_size in [14.0, 16.0, 20.0] {
        for scale_factor in [1.0, 1.25, 2.0] {
            let started = Instant::now();
            insert_shaped_text(
                &text_system,
                &atlas,
                mixed_text,
                font_size,
                scale_factor,
                &mut unique_keys,
                &mut keys_in_order,
            )?;
            cold_group_nanoseconds
                .push(u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX));

            for characters in cjk_characters.chunks(CJK_CHUNK_GLYPHS) {
                let text = characters.iter().collect::<String>();
                let started = Instant::now();
                insert_shaped_text(
                    &text_system,
                    &atlas,
                    &text,
                    font_size,
                    scale_factor,
                    &mut unique_keys,
                    &mut keys_in_order,
                )?;
                cold_group_nanoseconds
                    .push(u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX));
            }
        }
    }
    insert_fixed_assets(&atlas, &mut unique_keys, &mut keys_in_order)?;

    let before_flush = atlas.snapshot();
    let warm_started = Instant::now();
    for key in &keys_in_order {
        atlas
            .get_or_insert_with(key.clone(), &mut || {
                anyhow::bail!("warm atlas lookup must not invoke the builder")
            })?
            .ok_or_else(|| anyhow::anyhow!("warm atlas key disappeared"))?;
    }
    let warm_lookup_nanoseconds =
        u64::try_from(warm_started.elapsed().as_nanos()).unwrap_or(u64::MAX);

    let mut empty_scene = Scene::default();
    empty_scene.finish();
    let flush_started = Instant::now();
    renderer.render_scene_to_image(
        &empty_scene,
        Size {
            width: DevicePixels(1),
            height: DevicePixels(1),
        },
    )?;
    let upload_flush_nanoseconds =
        u64::try_from(flush_started.elapsed().as_nanos()).unwrap_or(u64::MAX);
    let after_flush = atlas.snapshot();

    anyhow::ensure!(
        before_flush.entry_count > 0,
        "workload produced no atlas entries"
    );
    anyhow::ensure!(
        before_flush.page_count >= 3,
        "workload did not cover every texture kind"
    );
    anyhow::ensure!(
        before_flush.pending_uploads == before_flush.allocations as usize,
        "every cold allocation should have exactly one pending upload"
    );
    anyhow::ensure!(after_flush.pending_uploads == 0, "uploads were not flushed");
    anyhow::ensure!(
        after_flush.upload_calls == before_flush.pending_uploads as u64,
        "the baseline backend should issue one upload call per pending entry"
    );

    let adapter = if force_fallback_adapter {
        "fallback"
    } else {
        "hardware"
    };
    let artifact = serde_json::json!({
        "experiment": "TEXT-001",
        "seed": SEED,
        "adapter": adapter,
        "gpu": {
            "device_name": gpu_specs.device_name,
            "driver_name": gpu_specs.driver_name,
            "driver_info": gpu_specs.driver_info,
            "is_software_emulated": gpu_specs.is_software_emulated,
        },
        "workload": {
            "mixed_text": mixed_text,
            "cjk_scroll_glyphs": CJK_SCROLL_GLYPHS,
            "cjk_chunk_glyphs": CJK_CHUNK_GLYPHS,
            "font_sizes_px": [14.0, 16.0, 20.0],
            "scale_factors": [1.0, 1.25, 2.0],
            "svg_entries": 8,
            "image_frames": 8,
            "unique_keys": keys_in_order.len(),
        },
        "timing_nanoseconds": {
            "cold_groups": cold_group_nanoseconds,
            "cold_group_p50": percentile(&cold_group_nanoseconds, 50),
            "cold_group_p95": percentile(&cold_group_nanoseconds, 95),
            "warm_all_keys": warm_lookup_nanoseconds,
            "upload_flush_and_empty_frame": upload_flush_nanoseconds,
        },
        "before_flush": snapshot_json(before_flush),
        "after_flush": snapshot_json(after_flush),
    });

    std::fs::create_dir_all(output_directory)?;
    let artifact_path = output_directory.join(format!("text-001-{adapter}.json"));
    std::fs::write(&artifact_path, serde_json::to_vec_pretty(&artifact)?)?;
    println!("{}", serde_json::to_string_pretty(&artifact)?);
    Ok(())
}

fn run_text_atlas_budget(
    force_fallback_adapter: bool,
    output_directory: &Path,
) -> anyhow::Result<()> {
    const CJK_GLYPHS: u32 = 10_000;
    const GLYPHS_PER_FRAME: usize = 128;
    const PAGE_EDGE: i32 = 256;
    const RETAINED_PAGES: usize = 4;

    let page_size = Size {
        width: DevicePixels(PAGE_EDGE),
        height: DevicePixels(PAGE_EDGE),
    };
    let page_bytes = PAGE_EDGE as usize * PAGE_EDGE as usize * 4;
    let retained_budget = RETAINED_PAGES * page_bytes;
    let mut policy = AtlasPolicy::default();
    policy.set_page_size(page_size);
    policy.set_retained_budget(AtlasContentKind::GlyphSubpixel, retained_budget);
    let mut renderer = WgpuHeadlessRenderer::new_with_atlas_policy(force_fallback_adapter, policy)?;
    let gpu_specs = renderer.gpu_specs();
    let atlas = renderer.sprite_atlas();
    let text_system = CosmicTextSystem::new("Noto Sans");
    text_system.add_fonts(vec![Cow::Borrowed(include_bytes!(
        "../../../assets/fonts/lilex/Lilex-Regular.ttf"
    ))])?;
    let cjk_characters = (0..CJK_GLYPHS)
        .filter_map(|offset| char::from_u32(0x4E00 + offset))
        .collect::<Vec<_>>();
    anyhow::ensure!(
        cjk_characters.len() == CJK_GLYPHS as usize,
        "CJK workload range must contain 10000 scalar values"
    );

    let mut unique_keys = HashSet::new();
    let mut keys_in_order = Vec::new();
    let mut frame_nanoseconds = Vec::new();
    let mut resident_samples = Vec::new();
    let mut maximum_resident_bytes = 0usize;
    let mut maximum_working_set_bytes = 0usize;
    let mut frame_count = 0usize;
    for font_size in [14.0, 16.0, 20.0] {
        for scale_factor in [1.0, 1.25, 2.0] {
            for characters in cjk_characters.chunks(GLYPHS_PER_FRAME) {
                let text = characters.iter().collect::<String>();
                let frame_started = Instant::now();
                let frame = atlas.begin_frame();
                let inserted = insert_shaped_text(
                    &text_system,
                    &atlas,
                    &text,
                    font_size,
                    scale_factor,
                    &mut unique_keys,
                    &mut keys_in_order,
                )?;
                let (scene, target_size) = glyph_grid_scene(&inserted.tiles);
                atlas.finish_frame(frame, &inserted.usage);
                let image = renderer.render_scene_to_image(&scene, target_size)?;
                assert_glyph_grid_has_no_empty_cells(&image, inserted.tiles.len());

                let snapshot = atlas.snapshot();
                let glyphs = snapshot.content(AtlasContentKind::GlyphSubpixel);
                let allowed_resident_bytes = retained_budget
                    .max(glyphs.working_set_bytes)
                    .saturating_add(page_bytes);
                anyhow::ensure!(
                    glyphs.resident_bytes <= allowed_resident_bytes,
                    "resident glyph bytes {} exceeded allowed {}",
                    glyphs.resident_bytes,
                    allowed_resident_bytes
                );
                maximum_resident_bytes = maximum_resident_bytes.max(glyphs.resident_bytes);
                maximum_working_set_bytes = maximum_working_set_bytes.max(glyphs.working_set_bytes);
                resident_samples.push(glyphs.resident_bytes);
                frame_nanoseconds
                    .push(u64::try_from(frame_started.elapsed().as_nanos()).unwrap_or(u64::MAX));
                frame_count += 1;
            }
        }
    }

    let final_frame = atlas.begin_frame();
    atlas.finish_frame(final_frame, &AtlasUsage::default());
    let final_snapshot = atlas.snapshot();
    let final_glyphs = final_snapshot.content(AtlasContentKind::GlyphSubpixel);
    anyhow::ensure!(
        final_glyphs.evictions > 0,
        "budget workload must evict cold glyphs"
    );
    anyhow::ensure!(
        final_glyphs.resident_bytes <= retained_budget.saturating_add(page_bytes),
        "final retained glyph bytes did not converge"
    );

    let adapter = if force_fallback_adapter {
        "fallback"
    } else {
        "hardware"
    };
    let artifact = serde_json::json!({
        "experiment": "TEXT-004",
        "adapter": adapter,
        "gpu": {
            "device_name": gpu_specs.device_name,
            "driver_name": gpu_specs.driver_name,
            "driver_info": gpu_specs.driver_info,
            "is_software_emulated": gpu_specs.is_software_emulated,
        },
        "workload": {
            "unique_cjk_scalars": CJK_GLYPHS,
            "glyphs_per_frame": GLYPHS_PER_FRAME,
            "font_sizes_px": [14.0, 16.0, 20.0],
            "scale_factors": [1.0, 1.25, 2.0],
            "frames": frame_count,
            "unique_atlas_keys": unique_keys.len(),
        },
        "policy": {
            "page_size": [PAGE_EDGE, PAGE_EDGE],
            "page_bytes": page_bytes,
            "retained_budget_bytes": retained_budget,
        },
        "result": {
            "maximum_resident_bytes": maximum_resident_bytes,
            "maximum_working_set_bytes": maximum_working_set_bytes,
            "resident_plateau_tail": resident_samples.iter().rev().take(32).copied().collect::<Vec<_>>(),
            "frame_p50_nanoseconds": percentile(&frame_nanoseconds, 50),
            "frame_p95_nanoseconds": percentile(&frame_nanoseconds, 95),
            "frame_p99_nanoseconds": percentile(&frame_nanoseconds, 99),
            "final_snapshot": snapshot_json(final_snapshot),
        },
    });
    std::fs::create_dir_all(output_directory)?;
    let artifact_path = output_directory.join(format!("text-004-{adapter}.json"));
    std::fs::write(&artifact_path, serde_json::to_vec_pretty(&artifact)?)?;
    println!("{}", serde_json::to_string_pretty(&artifact)?);
    Ok(())
}

fn run_text_atlas_content_isolation(
    force_fallback_adapter: bool,
    output_directory: &Path,
) -> anyhow::Result<()> {
    const FRAMES: usize = 128;
    let page_size = Size {
        width: DevicePixels(64),
        height: DevicePixels(64),
    };
    let monochrome_page_bytes = 64 * 64;
    let color_page_bytes = 64 * 64 * 4;
    let mut policy = AtlasPolicy::default();
    policy.set_page_size(page_size);
    policy.set_retained_budget(AtlasContentKind::GlyphSubpixel, 4 * color_page_bytes);
    policy.set_retained_budget(AtlasContentKind::SvgMask, monochrome_page_bytes);
    policy.set_retained_budget(AtlasContentKind::Image, color_page_bytes);
    let mut renderer = WgpuHeadlessRenderer::new_with_atlas_policy(force_fallback_adapter, policy)?;
    let gpu_specs = renderer.gpu_specs();
    let atlas = renderer.sprite_atlas();
    let text_system = CosmicTextSystem::new("Noto Sans");
    text_system.add_fonts(vec![Cow::Borrowed(include_bytes!(
        "../../../assets/fonts/lilex/Lilex-Regular.ttf"
    ))])?;
    let mut unique_keys = HashSet::new();
    let mut keys_in_order = Vec::new();
    let mut image_eviction_samples = Vec::new();
    let mut svg_eviction_samples = Vec::new();
    let mut final_image = None;

    for frame_index in 0..FRAMES {
        let frame = atlas.begin_frame();
        let inserted = insert_shaped_text(
            &text_system,
            &atlas,
            "缓存隔离 Cache",
            16.0,
            1.0,
            &mut unique_keys,
            &mut keys_in_order,
        )?;
        let glyph_tile = *inserted
            .tiles
            .first()
            .context("mixed workload must produce a glyph tile")?;
        let svg_size = Size {
            width: DevicePixels(32),
            height: DevicePixels(32),
        };
        let svg_tile = atlas
            .get_or_insert_with(
                AtlasKey::Svg(RenderSvgParams {
                    path: format!("content-isolation-{frame_index}.svg").into(),
                    size: svg_size,
                }),
                &mut || Ok(Some((svg_size, Cow::Owned(vec![255; 32 * 32])))),
            )?
            .context("SVG tile should exist")?;
        let image_frame_tile = image_tile(
            &atlas,
            AtlasKey::Image(RenderImageParams {
                image_id: ImageId(300),
                frame_index,
            }),
            svg_size,
            [0, 0, 255, 255],
        )?;
        let large_image = image_tile(
            &atlas,
            AtlasKey::Image(RenderImageParams {
                image_id: ImageId(1000 + frame_index),
                frame_index: 0,
            }),
            Size {
                width: DevicePixels(128),
                height: DevicePixels(128),
            },
            [255, 0, 0, 255],
        )?;
        assert_ne!(large_image.texture_id, image_frame_tile.texture_id);

        let mut usage = inserted.usage;
        usage.insert(svg_tile);
        usage.insert(image_frame_tile);
        atlas.finish_frame(frame, &usage);

        let full_bounds = bounds(0.0, 0.0, 24.0, 8.0, 1.0);
        let mut scene = Scene::default();
        scene.insert_primitive(SubpixelSprite {
            order: 0,
            pad: 0,
            bounds: bounds(0.0, 0.0, 8.0, 8.0, 1.0),
            content_mask: ContentMask {
                bounds: full_bounds,
            },
            color: gpui::rgb(0xffffff).into(),
            tile: glyph_tile,
            transformation: TransformationMatrix::unit(),
        });
        scene.insert_primitive(MonochromeSprite {
            order: 0,
            pad: 0,
            bounds: bounds(8.0, 0.0, 8.0, 8.0, 1.0),
            content_mask: ContentMask {
                bounds: full_bounds,
            },
            color: gpui::rgb(0xffffff).into(),
            tile: svg_tile,
            transformation: TransformationMatrix::unit(),
        });
        scene.insert_primitive(PolychromeSprite {
            order: 0,
            pad: 0,
            grayscale: PaddedBool32::from(false),
            opacity: 1.0,
            bounds: bounds(16.0, 0.0, 8.0, 8.0, 1.0),
            content_mask: ContentMask {
                bounds: full_bounds,
            },
            corner_radii: Corners::default(),
            tile: image_frame_tile,
        });
        scene.finish();
        let image = renderer.render_scene_to_image(
            &scene,
            Size {
                width: DevicePixels(24),
                height: DevicePixels(8),
            },
        )?;
        assert_glyph_grid_has_no_empty_cells(&image, 1);
        assert_eq!(image.get_pixel(12, 4).0, [255, 255, 255, 255]);
        assert_eq!(image.get_pixel(20, 4).0, [255, 0, 0, 255]);
        final_image = Some(image);

        let snapshot = atlas.snapshot();
        let glyphs = snapshot.content(AtlasContentKind::GlyphSubpixel);
        let images = snapshot.content(AtlasContentKind::Image);
        let svg_masks = snapshot.content(AtlasContentKind::SvgMask);
        anyhow::ensure!(glyphs.evictions == 0, "image churn evicted glyph entries");
        anyhow::ensure!(
            glyphs.resident_bytes <= glyphs.retained_budget_bytes,
            "glyph class exceeded its independent budget"
        );
        anyhow::ensure!(
            images.resident_bytes
                <= images
                    .retained_budget_bytes
                    .max(images.working_set_bytes)
                    .saturating_add(color_page_bytes),
            "image class did not converge independently"
        );
        image_eviction_samples.push(images.evictions);
        svg_eviction_samples.push(svg_masks.evictions);
    }

    let final_frame = atlas.begin_frame();
    atlas.finish_frame(final_frame, &AtlasUsage::default());
    let final_snapshot = atlas.snapshot();
    anyhow::ensure!(
        final_snapshot.content(AtlasContentKind::Image).evictions > 0,
        "image churn must trigger image-class eviction"
    );
    anyhow::ensure!(
        final_snapshot.content(AtlasContentKind::SvgMask).evictions > 0,
        "SVG churn must trigger SVG-class eviction"
    );
    let adapter = if force_fallback_adapter {
        "fallback"
    } else {
        "hardware"
    };
    let artifact = serde_json::json!({
        "experiment": "TEXT-006",
        "adapter": adapter,
        "gpu": {
            "device_name": gpu_specs.device_name,
            "driver_name": gpu_specs.driver_name,
            "driver_info": gpu_specs.driver_info,
            "is_software_emulated": gpu_specs.is_software_emulated,
        },
        "workload": {
            "frames": FRAMES,
            "animated_image_frames": FRAMES,
            "large_image_size": [128, 128],
            "svg_entries": FRAMES,
            "glyph_text": "缓存隔离 Cache",
        },
        "result": {
            "image_eviction_tail": image_eviction_samples.iter().rev().take(16).copied().collect::<Vec<_>>(),
            "svg_eviction_tail": svg_eviction_samples.iter().rev().take(16).copied().collect::<Vec<_>>(),
            "final_snapshot": snapshot_json(final_snapshot),
        },
    });
    std::fs::create_dir_all(output_directory)?;
    std::fs::write(
        output_directory.join(format!("text-006-{adapter}.json")),
        serde_json::to_vec_pretty(&artifact)?,
    )?;
    let final_image = final_image.context("mixed workload must render an image")?;
    final_image.save(output_directory.join(format!("text-006-{adapter}.png")))?;
    println!("{}", serde_json::to_string_pretty(&artifact)?);
    Ok(())
}

struct GlyphFixtureResult {
    family: &'static str,
    character: char,
    format: GlyphRasterFormat,
    image: image::RgbaImage,
}

fn rasterize_glyph_fixture(
    renderer: &mut WgpuHeadlessRenderer,
    font_path: &Path,
    family: &'static str,
    weight: FontWeight,
    character: char,
    color_source_hint: bool,
) -> anyhow::Result<GlyphFixtureResult> {
    let text_system = CosmicTextSystem::new_without_system_fonts(family);
    text_system.add_fonts(vec![Cow::Owned(std::fs::read(font_path)?)])?;
    let mut descriptor = font(family);
    descriptor.weight = weight;
    let font_id = text_system
        .font_id(&descriptor)
        .or_else(|_| text_system.first_fixture_font_id(family))
        .with_context(|| format!("failed to select {family} from {}", font_path.display()))?;
    let glyph_id = text_system
        .glyph_for_char(font_id, character)
        .with_context(|| format!("{family} does not contain {character}"))?;
    let params = RenderGlyphParams {
        font_id,
        glyph_id,
        font_size: px(64.0),
        subpixel_variant: Point::default(),
        scale_factor: 1.0,
        synthetic_italic: Default::default(),
        synthetic_bold: Default::default(),
        is_emoji: color_source_hint,
        subpixel_rendering: false,
        dilation: 0,
    };
    let raster_info = text_system.glyph_raster_info(&params)?;
    anyhow::ensure!(
        !raster_info.bounds.is_zero(),
        "{family} raster bounds are empty"
    );
    let rasterized = text_system.rasterize_glyph(&params, raster_info)?;
    let atlas = renderer.sprite_atlas();
    let frame = atlas.begin_frame();
    let tile = atlas
        .get_or_insert_with(AtlasKey::glyph(params, rasterized.info.format), &mut || {
            Ok(Some((
                rasterized.info.bounds.size,
                Cow::Borrowed(rasterized.pixels.as_slice()),
            )))
        })?
        .context("fixture glyph must produce an atlas tile")?;
    let target_size = Size {
        width: DevicePixels(96),
        height: DevicePixels(96),
    };
    let target_bounds = bounds(0.0, 0.0, 96.0, 96.0, 1.0);
    let sprite_bounds = bounds(16.0, 16.0, 64.0, 64.0, 1.0);
    let mut scene = Scene::default();
    match rasterized.info.format {
        GlyphRasterFormat::Alpha8 => scene.insert_primitive(MonochromeSprite {
            order: 0,
            pad: 0,
            bounds: sprite_bounds,
            content_mask: ContentMask {
                bounds: target_bounds,
            },
            color: gpui::rgb(0x00ff00).into(),
            tile,
            transformation: TransformationMatrix::unit(),
        }),
        GlyphRasterFormat::SubpixelBgra8 => scene.insert_primitive(SubpixelSprite {
            order: 0,
            pad: 0,
            bounds: sprite_bounds,
            content_mask: ContentMask {
                bounds: target_bounds,
            },
            color: gpui::rgb(0x00ff00).into(),
            tile,
            transformation: TransformationMatrix::unit(),
        }),
        GlyphRasterFormat::ColorBgra8 => scene.insert_primitive(PolychromeSprite {
            order: 0,
            pad: 0,
            grayscale: PaddedBool32::from(false),
            opacity: 1.0,
            bounds: sprite_bounds,
            content_mask: ContentMask {
                bounds: target_bounds,
            },
            corner_radii: Corners::default(),
            tile,
        }),
    }
    scene.finish();
    atlas.finish_frame(frame, scene.atlas_usage());
    let image = renderer.render_scene_to_image(&scene, target_size)?;
    Ok(GlyphFixtureResult {
        family,
        character,
        format: rasterized.info.format,
        image,
    })
}

fn has_chromatic_pixel(image: &image::RgbaImage) -> bool {
    image.pixels().any(|pixel| {
        let [red, green, blue, alpha] = pixel.0;
        alpha > 0 && (red.abs_diff(green) > 8 || green.abs_diff(blue) > 8)
    })
}

fn has_green_tinted_pixel(image: &image::RgbaImage) -> bool {
    image.pixels().any(|pixel| {
        let [red, green, blue, alpha] = pixel.0;
        alpha > 0 && green > red.saturating_add(32) && green > blue.saturating_add(32)
    })
}

fn run_text_glyph_format(
    force_fallback_adapter: bool,
    output_directory: &Path,
) -> anyhow::Result<()> {
    let fixture = |name: &str, file_name: &str| {
        std::env::var_os(name).map_or_else(
            || {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../assets/fonts/text-rendering-fixtures")
                    .join(file_name)
            },
            std::path::PathBuf::from,
        )
    };
    let colrv1_path = fixture("GPUI_COLRV1_FONT", "noto-colrv1-grinning-face.ttf");
    let bitmap_path = fixture(
        "GPUI_BITMAP_COLOR_FONT",
        "noto-color-emoji-bitmap-grinning-face.ttf",
    );
    let svg_path = fixture("GPUI_SVG_COLOR_FONT", "twitter-color-emoji-svg-rocket.ttf");
    let monochrome_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts/openmoji/openmoji.ttf");
    let mut renderer = WgpuHeadlessRenderer::new_with_fallback(force_fallback_adapter)?;
    let results = [
        rasterize_glyph_fixture(
            &mut renderer,
            &bitmap_path,
            "Noto Color Emoji",
            FontWeight::NORMAL,
            '😀',
            true,
        )?,
        rasterize_glyph_fixture(
            &mut renderer,
            &svg_path,
            "Twitter Color Emoji",
            FontWeight::NORMAL,
            '🚀',
            true,
        )?,
        rasterize_glyph_fixture(
            &mut renderer,
            &colrv1_path,
            "Noto Color Emoji",
            FontWeight::NORMAL,
            '😀',
            true,
        )?,
        rasterize_glyph_fixture(
            &mut renderer,
            &monochrome_path,
            "OpenMoji",
            FontWeight::BLACK,
            '😀',
            true,
        )?,
    ];
    for result in &results[..3] {
        anyhow::ensure!(
            result.format == GlyphRasterFormat::ColorBgra8,
            "{} {} produced {:?}",
            result.family,
            result.character,
            result.format
        );
        anyhow::ensure!(
            has_chromatic_pixel(&result.image),
            "{} {} produced no chromatic pixels",
            result.family,
            result.character
        );
    }
    let monochrome = &results[3];
    anyhow::ensure!(
        monochrome.format == GlyphRasterFormat::Alpha8,
        "OpenMoji Black source hint incorrectly selected a color atlas"
    );
    anyhow::ensure!(
        has_green_tinted_pixel(&monochrome.image),
        "monochrome fixture did not remain tintable"
    );

    let adapter = if force_fallback_adapter {
        "fallback"
    } else {
        "hardware"
    };
    std::fs::create_dir_all(output_directory)?;
    let labels = ["bitmap", "svg", "colrv1", "monochrome"];
    for (label, result) in labels.into_iter().zip(&results) {
        result
            .image
            .save(output_directory.join(format!("text-007-{adapter}-{label}.png")))?;
    }
    let artifact = serde_json::json!({
        "experiment": "TEXT-007",
        "adapter": adapter,
        "fixtures": {
            "colrv1": colrv1_path,
            "bitmap": bitmap_path,
            "svg": svg_path,
            "monochrome": monochrome_path,
        },
        "results": labels.into_iter().zip(&results).map(|(label, result)| {
            (label.to_owned(), serde_json::json!({
                "family": result.family,
                "character": result.character.to_string(),
                "format": format!("{:?}", result.format),
                "chromatic": has_chromatic_pixel(&result.image),
                "green_tinted": has_green_tinted_pixel(&result.image),
            }))
        }).collect::<serde_json::Map<_, _>>(),
    });
    std::fs::write(
        output_directory.join(format!("text-007-{adapter}.json")),
        serde_json::to_vec_pretty(&artifact)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&artifact)?);
    Ok(())
}

fn assert_solid_region(
    image: &image::RgbaImage,
    x_range: std::ops::Range<u32>,
    y_range: std::ops::Range<u32>,
    expected: [u8; 4],
) {
    for y in y_range {
        for x in x_range.clone() {
            assert_eq!(
                image.get_pixel(x, y).0,
                expected,
                "unexpected sampling result at ({x}, {y})"
            );
        }
    }
}

fn run_text_atlas_sampling_gutter(
    force_fallback_adapter: bool,
    output_directory: &Path,
) -> anyhow::Result<()> {
    let mut renderer = WgpuHeadlessRenderer::new_with_fallback(force_fallback_adapter)?;
    let gpu_specs = renderer.gpu_specs();
    anyhow::ensure!(
        gpu_specs.is_software_emulated == force_fallback_adapter,
        "requested fallback={force_fallback_adapter}, selected {}",
        gpu_specs.device_name
    );
    let atlas = renderer.sprite_atlas();
    let fixture_size = Size {
        width: DevicePixels(2),
        height: DevicePixels(2),
    };
    let frame = atlas.begin_frame();

    let red_image = image_tile(
        &atlas,
        AtlasKey::Image(RenderImageParams {
            image_id: ImageId(8_000),
            frame_index: 0,
        }),
        fixture_size,
        [0, 0, 255, 255],
    )?;
    let blue_image = image_tile(
        &atlas,
        AtlasKey::Image(RenderImageParams {
            image_id: ImageId(8_001),
            frame_index: 0,
        }),
        fixture_size,
        [255, 0, 0, 255],
    )?;
    anyhow::ensure!(
        red_image.texture_id == blue_image.texture_id,
        "high-contrast image fixtures must share an atlas page"
    );

    let alpha_opaque = raw_tile(
        &atlas,
        glyph_atlas_key(8_100, GlyphRasterFormat::Alpha8),
        fixture_size,
        vec![255; 4],
    )?;
    let alpha_transparent = raw_tile(
        &atlas,
        glyph_atlas_key(8_101, GlyphRasterFormat::Alpha8),
        fixture_size,
        vec![0; 4],
    )?;
    let subpixel_opaque = raw_tile(
        &atlas,
        glyph_atlas_key(8_200, GlyphRasterFormat::SubpixelBgra8),
        fixture_size,
        vec![255; 16],
    )?;
    let subpixel_transparent = raw_tile(
        &atlas,
        glyph_atlas_key(8_201, GlyphRasterFormat::SubpixelBgra8),
        fixture_size,
        vec![0; 16],
    )?;
    let color_opaque = raw_tile(
        &atlas,
        glyph_atlas_key(8_300, GlyphRasterFormat::ColorBgra8),
        fixture_size,
        [255, 0, 255, 255].repeat(4),
    )?;
    let color_transparent = raw_tile(
        &atlas,
        glyph_atlas_key(8_301, GlyphRasterFormat::ColorBgra8),
        fixture_size,
        vec![0; 16],
    )?;
    let svg_opaque = raw_tile(
        &atlas,
        AtlasKey::Svg(RenderSvgParams {
            path: "text-008-opaque.svg".into(),
            size: fixture_size,
        }),
        fixture_size,
        vec![255; 4],
    )?;
    let svg_transparent = raw_tile(
        &atlas,
        AtlasKey::Svg(RenderSvgParams {
            path: "text-008-transparent.svg".into(),
            size: fixture_size,
        }),
        fixture_size,
        vec![0; 4],
    )?;

    for (opaque, transparent) in [
        (alpha_opaque, alpha_transparent),
        (subpixel_opaque, subpixel_transparent),
        (color_opaque, color_transparent),
        (svg_opaque, svg_transparent),
    ] {
        anyhow::ensure!(
            opaque.texture_id == transparent.texture_id,
            "high-contrast fixtures must share an atlas page"
        );
        anyhow::ensure!(
            opaque.padding == 1 && transparent.padding == 1,
            "TEXT-008 requires one-pixel gutters"
        );
    }

    let image_target = Size {
        width: DevicePixels(152),
        height: DevicePixels(36),
    };
    let image_mask = ContentMask {
        bounds: bounds(0.0, 0.0, 152.0, 36.0, 1.0),
    };
    let mut image_scene = Scene::default();
    for (tile, sprite_bounds) in [
        (red_image, bounds(4.0, 4.0, 24.0, 24.0, 1.0)),
        (red_image, bounds(36.25, 4.5, 24.0, 24.0, 1.0)),
        (red_image, bounds(68.5, 6.25, 40.0, 18.0, 1.0)),
        (blue_image, bounds(120.25, 4.5, 24.0, 24.0, 1.0)),
    ] {
        image_scene.insert_primitive(PolychromeSprite {
            order: 0,
            pad: 0,
            grayscale: PaddedBool32::from(false),
            opacity: 1.0,
            bounds: sprite_bounds,
            content_mask: image_mask,
            corner_radii: Corners::default(),
            tile,
        });
    }
    image_scene.finish();

    let transparent_target = Size {
        width: DevicePixels(128),
        height: DevicePixels(96),
    };
    let transparent_mask = ContentMask {
        bounds: bounds(0.0, 0.0, 128.0, 96.0, 1.0),
    };
    let alpha_transform = TransformationMatrix::unit()
        .translate(point(ScaledPixels(54.5), ScaledPixels(24.25)))
        .rotate(radians(0.43))
        .scale(size(1.4, 0.75));
    let subpixel_transform = TransformationMatrix::unit()
        .translate(point(ScaledPixels(82.25), ScaledPixels(66.5)))
        .rotate(radians(-0.37))
        .scale(size(1.2, 1.4));
    let mut transparent_scene = Scene::default();
    transparent_scene.insert_primitive(MonochromeSprite {
        order: 0,
        pad: 0,
        bounds: bounds(8.25, 8.5, 20.0, 20.0, 1.0),
        content_mask: transparent_mask,
        color: gpui::rgb(0xffffff).into(),
        tile: alpha_transparent,
        transformation: TransformationMatrix::unit(),
    });
    transparent_scene.insert_primitive(MonochromeSprite {
        order: 0,
        pad: 0,
        bounds: bounds(-10.0, -10.0, 20.0, 20.0, 1.0),
        content_mask: transparent_mask,
        color: gpui::rgb(0xffffff).into(),
        tile: alpha_transparent,
        transformation: alpha_transform,
    });
    transparent_scene.insert_primitive(MonochromeSprite {
        order: 0,
        pad: 0,
        bounds: bounds(92.5, 8.25, 20.0, 20.0, 1.0),
        content_mask: transparent_mask,
        color: gpui::rgb(0xffffff).into(),
        tile: svg_transparent,
        transformation: TransformationMatrix::unit(),
    });
    transparent_scene.insert_primitive(SubpixelSprite {
        order: 0,
        pad: 0,
        bounds: bounds(-8.0, -8.0, 16.0, 16.0, 1.0),
        content_mask: transparent_mask,
        color: gpui::rgb(0xffffff).into(),
        tile: subpixel_transparent,
        transformation: subpixel_transform,
    });
    transparent_scene.insert_primitive(PolychromeSprite {
        order: 0,
        pad: 0,
        grayscale: PaddedBool32::from(false),
        opacity: 1.0,
        bounds: bounds(12.5, 52.25, 28.0, 20.0, 1.0),
        content_mask: transparent_mask,
        corner_radii: Corners::default(),
        tile: color_transparent,
    });
    transparent_scene.finish();

    let mut usage = AtlasUsage::default();
    for tile in [
        red_image,
        blue_image,
        alpha_opaque,
        alpha_transparent,
        subpixel_opaque,
        subpixel_transparent,
        color_opaque,
        color_transparent,
        svg_opaque,
        svg_transparent,
    ] {
        usage.insert(tile);
    }
    atlas.finish_frame(frame, &usage);

    let image_output = renderer.render_scene_to_image(&image_scene, image_target)?;
    assert_solid_region(&image_output, 4..28, 4..28, [255, 0, 0, 255]);
    assert_solid_region(&image_output, 40..56, 8..24, [255, 0, 0, 255]);
    assert_solid_region(&image_output, 74..103, 10..20, [255, 0, 0, 255]);
    assert_solid_region(&image_output, 124..140, 8..24, [0, 0, 255, 255]);

    let transparent_output =
        renderer.render_scene_to_image(&transparent_scene, transparent_target)?;
    anyhow::ensure!(
        transparent_output
            .pixels()
            .all(|pixel| pixel.0 == [0, 0, 0, 0]),
        "transparent gutter fixtures sampled neighboring atlas content"
    );

    let adapter = if force_fallback_adapter {
        "fallback"
    } else {
        "hardware"
    };
    std::fs::create_dir_all(output_directory)?;
    image_output.save(output_directory.join(format!("text-008-{adapter}-images.png")))?;
    transparent_output
        .save(output_directory.join(format!("text-008-{adapter}-transparent.png")))?;
    let artifact = serde_json::json!({
        "experiment": "TEXT-008",
        "adapter": adapter,
        "gpu": {
            "device_name": gpu_specs.device_name,
            "driver_name": gpu_specs.driver_name,
            "driver_info": gpu_specs.driver_info,
            "is_software_emulated": gpu_specs.is_software_emulated,
        },
        "gutter_pixels": red_image.padding,
        "image_cases": ["integer", "fractional", "non_uniform_scale", "adjacent_high_contrast"],
        "transparent_cases": ["glyph_alpha", "glyph_subpixel", "glyph_color", "svg_mask", "rotation"],
        "result": {
            "ordinary_image_has_no_transparent_seam": true,
            "transparent_content_has_no_cross_tile_bleed": true,
        },
    });
    std::fs::write(
        output_directory.join(format!("text-008-{adapter}.json")),
        serde_json::to_vec_pretty(&artifact)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&artifact)?);
    Ok(())
}

#[test]
fn hardware_adapter_renders_primitive_corpus() {
    let mut renderer = WgpuHeadlessRenderer::new().expect("hardware headless renderer");
    let specs = renderer.gpu_specs();
    assert!(
        !specs.is_software_emulated,
        "hardware corpus selected software adapter: {}",
        specs.device_name
    );
    for scale in [1.0, 2.0] {
        let scene = primitive_scene(&renderer, scale);
        for run in 0..RUNS {
            let image = renderer
                .render_scene_to_image(&scene, target(scale))
                .expect("hardware render should complete");
            assert_scene_pixels(&image, scale);
            if run + 1 == RUNS {
                save_output(&image, "hardware", scale);
            }
        }
    }
}

#[test]
fn fallback_adapter_renders_primitive_corpus() {
    let mut renderer =
        WgpuHeadlessRenderer::new_with_fallback(true).expect("fallback headless renderer");
    let specs = renderer.gpu_specs();
    assert!(
        specs.is_software_emulated,
        "fallback corpus selected non-software adapter: {}",
        specs.device_name
    );
    for scale in [1.0, 2.0] {
        let scene = primitive_scene(&renderer, scale);
        for run in 0..RUNS {
            let image = renderer
                .render_scene_to_image(&scene, target(scale))
                .expect("fallback render should complete");
            assert_scene_pixels(&image, scale);
            if run + 1 == RUNS {
                save_output(&image, "fallback", scale);
            }
        }
    }
}

#[test]
fn negative_subpixel_glyph_origin_matches_positive_phase_after_one_pixel_shift()
-> anyhow::Result<()> {
    assert_negative_subpixel_phase(false)?;
    assert_negative_subpixel_phase(true)
}

#[test]
fn retired_tiles_never_sample_replacement_content() -> anyhow::Result<()> {
    for force_fallback_adapter in [false, true] {
        let adapter = if force_fallback_adapter {
            "fallback"
        } else {
            "hardware"
        };
        let mut renderer = WgpuHeadlessRenderer::new_with_fallback(force_fallback_adapter)?;
        assert_retired_tile_case(
            &mut renderer,
            &format!("{adapter}-suballocation"),
            Size {
                width: DevicePixels(8),
                height: DevicePixels(8),
            },
            10,
            true,
        )?;
        assert_retired_tile_case(
            &mut renderer,
            &format!("{adapter}-page"),
            Size {
                width: DevicePixels(1024),
                height: DevicePixels(1024),
            },
            20,
            false,
        )?;
    }
    Ok(())
}

#[test]
fn working_set_over_budget_renders_without_missing_tiles() -> anyhow::Result<()> {
    assert_working_set_over_budget(false)?;
    assert_working_set_over_budget(true)
}

#[test]
#[ignore = "phase-0 baseline runner writes explicit diagnostics artifacts"]
fn text_atlas_baseline_runner() -> anyhow::Result<()> {
    let output_directory = std::env::var_os("GPUI_TEXT_ATLAS_OUTPUT_DIR")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("GPUI_TEXT_ATLAS_OUTPUT_DIR must be set"))?;
    anyhow::ensure!(
        output_directory.is_absolute(),
        "GPUI_TEXT_ATLAS_OUTPUT_DIR must be an absolute path"
    );
    run_text_atlas_baseline(false, &output_directory)?;
    run_text_atlas_baseline(true, &output_directory)
}

#[test]
#[ignore = "phase-4 CJK budget runner writes explicit diagnostics artifacts"]
fn text_atlas_budget_runner() -> anyhow::Result<()> {
    let output_directory = std::env::var_os("GPUI_TEXT_ATLAS_OUTPUT_DIR")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("GPUI_TEXT_ATLAS_OUTPUT_DIR must be set"))?;
    anyhow::ensure!(
        output_directory.is_absolute(),
        "GPUI_TEXT_ATLAS_OUTPUT_DIR must be an absolute path"
    );
    run_text_atlas_budget(false, &output_directory)?;
    run_text_atlas_budget(true, &output_directory)
}

#[test]
#[ignore = "phase-4 content isolation runner writes explicit diagnostics artifacts"]
fn text_atlas_content_isolation_runner() -> anyhow::Result<()> {
    let output_directory = std::env::var_os("GPUI_TEXT_ATLAS_OUTPUT_DIR")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("GPUI_TEXT_ATLAS_OUTPUT_DIR must be set"))?;
    anyhow::ensure!(
        output_directory.is_absolute(),
        "GPUI_TEXT_ATLAS_OUTPUT_DIR must be an absolute path"
    );
    run_text_atlas_content_isolation(false, &output_directory)?;
    run_text_atlas_content_isolation(true, &output_directory)
}

#[test]
#[ignore = "phase-5 color-font runner writes explicit diagnostics artifacts"]
fn text_glyph_format_runner() -> anyhow::Result<()> {
    let output_directory = std::env::var_os("GPUI_TEXT_ATLAS_OUTPUT_DIR")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("GPUI_TEXT_ATLAS_OUTPUT_DIR must be set"))?;
    anyhow::ensure!(
        output_directory.is_absolute(),
        "GPUI_TEXT_ATLAS_OUTPUT_DIR must be an absolute path"
    );
    run_text_glyph_format(false, &output_directory)?;
    run_text_glyph_format(true, &output_directory)
}

#[test]
#[ignore = "phase-6 sampling gutter runner writes explicit pixel artifacts"]
fn text_atlas_sampling_gutter_runner() -> anyhow::Result<()> {
    let output_directory = std::env::var_os("GPUI_TEXT_ATLAS_OUTPUT_DIR")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("GPUI_TEXT_ATLAS_OUTPUT_DIR must be set"))?;
    anyhow::ensure!(
        output_directory.is_absolute(),
        "GPUI_TEXT_ATLAS_OUTPUT_DIR must be an absolute path"
    );
    run_text_atlas_sampling_gutter(false, &output_directory)?;
    run_text_atlas_sampling_gutter(true, &output_directory)
}
