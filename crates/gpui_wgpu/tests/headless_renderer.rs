#![cfg(feature = "test-support")]

use gpui::{
    AnyWindowHandle, AppContext as _, AtlasKey, AtlasSnapshot, AtlasTile, Bounds, ContentMask,
    Context, Corners, DevicePixels, Edges, FontId, FontRun, FontStyle, FontWeight, GlyphId,
    HeadlessAppContext, Hsla, ImageId, IntoElement, IsZero as _, MonochromeSprite, PaddedBool32,
    PathBuilder, PlatformAtlas, PlatformHeadlessRenderer, PlatformTextSystem, Point,
    PolychromeSprite, Quad, Render, RenderGlyphParams, RenderImageParams, RenderSvgParams,
    ScaledPixels, Scene, Shadow, Size, Styled as _, Underline, Window, canvas, font, point, px,
    size,
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
    })
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    let mut samples = samples.to_vec();
    samples.sort_unstable();
    let index = (samples.len() - 1) * percentile / 100;
    samples[index]
}

fn insert_shaped_text(
    text_system: &CosmicTextSystem,
    atlas: &Arc<dyn PlatformAtlas>,
    text: &str,
    font_size: f32,
    scale_factor: f32,
    unique_keys: &mut HashSet<AtlasKey>,
    keys_in_order: &mut Vec<AtlasKey>,
) -> anyhow::Result<()> {
    let font_id = text_system.font_id(&font("Lilex"))?;
    let runs = [FontRun {
        len: text.len(),
        font_id,
        font_style: FontStyle::Normal,
        font_weight: FontWeight::NORMAL,
    }];
    let layout = text_system.layout_line(text, px(font_size), &runs);

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
            let key = AtlasKey::from(params.clone());
            if !unique_keys.insert(key.clone()) {
                atlas
                    .get_or_insert_with(key, &mut || {
                        anyhow::bail!("a repeated glyph key must hit the atlas")
                    })?
                    .ok_or_else(|| anyhow::anyhow!("a repeated glyph key disappeared"))?;
                continue;
            }

            let raster_bounds = text_system.glyph_raster_bounds(&params)?;
            if raster_bounds.is_zero() {
                unique_keys.remove(&key);
                continue;
            }
            atlas
                .get_or_insert_with(key.clone(), &mut || {
                    let (size, bytes) = text_system.rasterize_glyph(&params, raster_bounds)?;
                    Ok(Some((size, Cow::Owned(bytes))))
                })?
                .ok_or_else(|| anyhow::anyhow!("glyph builder returned no atlas tile"))?;
            keys_in_order.push(key);
        }
    }
    Ok(())
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
