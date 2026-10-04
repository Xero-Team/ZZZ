#![cfg(feature = "test-support")]

use gpui::{
    AtlasKey, AtlasTile, Bounds, ContentMask, Corners, DevicePixels, Edges, Hsla, ImageId,
    MonochromeSprite, PaddedBool32, PathBuilder, PlatformHeadlessRenderer, Point, PolychromeSprite,
    Quad, RenderImageParams, RenderSvgParams, ScaledPixels, Scene, Shadow, Size, Underline,
};
use gpui_wgpu::WgpuHeadlessRenderer;
use std::borrow::Cow;

const RUNS: usize = 100;

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
