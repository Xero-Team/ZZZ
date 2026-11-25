//! Boundary and containment tests for the shipped SVG renderer.
//!
//! The conformance harness (`svg_conformance.rs`) measures fidelity against the
//! reference suites. This file covers the surfaces that a reference rasterizer
//! never exercises but a Markdown viewer does: hostile or degenerate documents
//! pasted into a buffer, and the size contract the renderer promises its
//! callers.
//!
//! Every case here asserts one of three things:
//!
//! - a degenerate document produces an error or a bounded raster, never a panic,
//!   an unbounded allocation, or a hang;
//! - a document cannot reach the filesystem or the network through the renderer;
//! - the raster the renderer returns is the one its caller asked for, including
//!   colour order and alpha handling.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{DevicePixels, Size, SvgRenderer, SvgSize, size};

/// Borrowed from the renderer's own cap; asserted as a ceiling, not an equality,
/// so raising the cap does not break the test.
const MAX_RASTER_EDGE: u32 = 8192;

/// A pathological document must not take longer than this. The reference suites'
/// own harness uses the same order of magnitude for its timeout exclusions.
const HOSTILE_TIMEOUT: Duration = Duration::from_secs(10);

fn renderer() -> SvgRenderer {
    SvgRenderer::new(Arc::new(()))
}

fn render(svg: &str) -> anyhow::Result<std::sync::Arc<gpui::RenderImage>> {
    renderer()
        .render_single_frame(svg.as_bytes(), 1.0)
        .map_err(Into::into)
}

fn exact_size_edge(image: &gpui::RenderImage) -> (u32, u32) {
    let size = image.size(0);
    (size.width.0.max(0) as u32, size.height.0.max(0) as u32)
}

fn rgba(image: &gpui::RenderImage, x: u32, y: u32) -> [u8; 4] {
    let (width, _) = exact_size_edge(image);
    let bytes = image.as_bytes(0).expect("rendered image has a frame");
    let offset = ((y * width + x) * 4) as usize;
    // `SvgRenderer` returns BGRA; swap back to RGBA for readable assertions.
    let pixel = &bytes[offset..offset + 4];
    [pixel[2], pixel[1], pixel[0], pixel[3]]
}

#[test]
fn oversized_documents_stay_within_the_texture_limit() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="99999999" height="99999999">
            <rect width="99999999" height="99999999" fill="red"/>
        </svg>"##;

    let started = Instant::now();
    let image = render(svg).expect("an oversized document should still rasterize");
    assert!(
        started.elapsed() < HOSTILE_TIMEOUT,
        "oversized document took {:?}",
        started.elapsed()
    );

    let (width, height) = exact_size_edge(&image);
    assert!(
        width <= MAX_RASTER_EDGE && height <= MAX_RASTER_EDGE,
        "raster {width}x{height} exceeds the {MAX_RASTER_EDGE}px cap"
    );
    assert!(
        width > 0 && height > 0,
        "raster collapsed to {width}x{height}"
    );
}

#[test]
fn one_pixel_high_documents_survive_a_square_request() {
    // A 1000:1 viewport asked for at 1x1 computes a sub-pixel height. Truncating
    // that to zero pixels would drop the icon entirely, so the rasterizer has to
    // keep at least one row. This is the shape `render_alpha_mask` produces when
    // a wide icon is painted into a small square element.
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1">
            <rect width="1000" height="1" fill="red"/>
        </svg>"##;

    let renderer = renderer();
    let tree = renderer.parse_svg(svg).expect("parse");
    let image = renderer
        .render_parsed(
            &tree,
            SvgSize::Size(Size::new(DevicePixels(1), DevicePixels(1))),
        )
        .expect("a one-pixel-high document should not fail");
    let (width, height) = exact_size_edge(&image);
    assert!(
        width >= 1 && height >= 1,
        "raster collapsed to {width}x{height}"
    );
}

#[test]
fn degenerate_sizes_are_errors_not_panics() {
    let documents = [
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="0" height="0"></svg>"##,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="-5" height="-5"><rect width="5" height="5"/></svg>"##,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="0.4" height="0.4"><rect width="1" height="1"/></svg>"##,
        r##"<svg xmlns="http://www.w3.org/2000/svg"></svg>"##,
        r##"not an svg at all"##,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10"##,
    ];

    for document in documents {
        // Either outcome is fine; a panic, a hang, or an allocation failure is not.
        match render(document) {
            Ok(image) => {
                let (width, height) = exact_size_edge(&image);
                assert!(
                    width <= MAX_RASTER_EDGE && height <= MAX_RASTER_EDGE,
                    "{document:?} produced {width}x{height}"
                );
            }
            Err(_) => {}
        }
    }
}

#[test]
fn recursive_use_and_deep_nesting_are_bounded() {
    let recursive_use = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
            <defs><g id="loop"><use href="#loop"/></g></defs>
            <use href="#loop"/>
        </svg>"##;

    // Deeper than usvg's default max_depth (1024) so the parse is rejected
    // instead of recursing without a bound.
    let mut deep =
        String::from(r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">"##);
    for _ in 0..2000 {
        deep.push_str("<g>");
    }
    deep.push_str("<rect width=\"1\" height=\"1\"/>");
    for _ in 0..2000 {
        deep.push_str("</g>");
    }
    deep.push_str("</svg>");

    for (label, document) in [("recursive use", recursive_use), ("deep nesting", &deep)] {
        let started = Instant::now();
        let _ = render(document);
        assert!(
            started.elapsed() < HOSTILE_TIMEOUT,
            "{label} took {:?}",
            started.elapsed()
        );
    }
}

#[test]
fn entity_expansion_does_not_blow_up() {
    // A billion-laughs variant. usvg parses XML without expanding DTD entities,
    // so this must fail (or render empty) quickly rather than allocate.
    let mut document = String::from("<!DOCTYPE svg [<!ENTITY lol0 \"lol\">");
    for index in 1..10 {
        document.push_str(&format!(
            "<!ENTITY lol{index} \"{}\">",
            format!("&lol{};", index - 1).repeat(10)
        ));
    }
    document.push_str(
        r##"]><svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><text>&lol9;</text></svg>"##,
    );

    let started = Instant::now();
    let _ = render(&document);
    assert!(
        started.elapsed() < HOSTILE_TIMEOUT,
        "entity expansion took {:?}",
        started.elapsed()
    );
}

#[test]
fn external_references_are_not_resolved() {
    // The renderer runs with `resources_dir: None`: a reference to a sibling
    // file resolves to nothing rather than reading the local filesystem, so the
    // shapes that would be covered by the image stay blank.
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20">
            <rect width="20" height="20" fill="#00ff00"/>
            <image href="file:///etc/hostname" width="20" height="20"/>
            <use href="/etc/passwd#icon"/>
        </svg>"##;

    let image = render(svg).expect("a document with external references still renders");
    assert_eq!(rgba(&image, 10, 10), [0, 255, 0, 255]);
}

#[test]
fn scripts_and_event_handlers_are_inert() {
    // usvg has no scripting engine and drops `foreignObject` content, so a
    // document that would be dangerous in a browser draws only its geometry.
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" onload="alert(1)">
            <script>alert('script')</script>
            <rect width="20" height="20" fill="#0000ff" onclick="alert(2)"/>
            <foreignObject width="20" height="20">
                <div xmlns="http://www.w3.org/1999/xhtml" style="background:red">html</div>
            </foreignObject>
            <a href="javascript:alert(3)"><rect x="5" y="5" width="5" height="5" fill="#ffff00"/></a>
        </svg>"##;

    let image = render(svg).expect("script-bearing documents still render their geometry");
    // The `foreignObject` HTML is not painted: the top-left corner is the blue
    // rect, not the red background.
    assert_eq!(rgba(&image, 1, 1), [0, 0, 255, 255]);
}

#[test]
fn solid_colors_survive_the_channel_swap() {
    // `SvgRenderer` returns BGRA. A channel swap bug shows up as red/blue
    // inversion, which no pixel tolerance in the conformance harness catches
    // when both sides are swapped the same way.
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4">
            <rect width="4" height="4" fill="#ff8000"/>
        </svg>"##;

    let image = render(svg).expect("solid rect should rasterize");
    let pixel = rgba(&image, 2, 2);
    assert_eq!(pixel[3], 255, "opaque rect should stay opaque: {pixel:?}");
    assert_eq!(&pixel[..3], &[255, 128, 0], "channel order is wrong");
}

#[test]
fn translucent_pixels_keep_their_alpha() {
    // The renderer demultiplies alpha before handing back BGRA. A double
    // demultiply, or none at all, halves (or squares) the colour channels here.
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4">
            <rect width="4" height="4" fill="#ff0000" fill-opacity="0.5"/>
        </svg>"##;

    let image = render(svg).expect("translucent rect should rasterize");
    let pixel = rgba(&image, 2, 2);
    assert!(
        (pixel[3] as i32 - 128).abs() <= 1,
        "alpha should be ~128, got {pixel:?}"
    );
    assert!(
        pixel[0] >= 254 && pixel[1] <= 1 && pixel[2] <= 1,
        "colour should be demultiplied red, got {pixel:?}"
    );
}

#[test]
fn missing_font_families_still_draw_glyphs() {
    // Documents generated by diagram tools and models routinely name fonts the
    // host does not have. usvg's default selector returns no face and the text
    // silently disappears; this renderer falls back to sans-serif, which is why
    // a substituted glyph is the expected outcome rather than an empty canvas.
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="40">
            <text x="5" y="30" font-family="No Such Font 42" font-size="30">Hi</text>
        </svg>"##;

    let image = render(svg).expect("text with a missing family should still render");
    let bytes = image.as_bytes(0).expect("rendered image has a frame");
    let painted = bytes.chunks_exact(4).filter(|pixel| pixel[3] > 0).count();
    assert!(
        painted > 100,
        "expected a substituted glyph, found {painted} painted pixels"
    );
}

#[test]
fn requested_sizes_are_honored() {
    let renderer = renderer();
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="24pt" height="12pt">
            <rect width="24" height="12" fill="red"/>
        </svg>"##;
    let tree = renderer.parse_svg(svg).expect("parse");

    // `ExactSize` is the only variant that promises both axes.
    let exact = renderer
        .render_parsed(
            &tree,
            SvgSize::ExactSize(size(DevicePixels(50), DevicePixels(50))),
        )
        .expect("exact size");
    assert_eq!(exact_size_edge(&exact), (50, 50));

    // `Size` is width-driven: the height follows from the aspect ratio, which is
    // what icon painting relies on.
    let width_driven = renderer
        .render_parsed(
            &tree,
            SvgSize::Size(Size::new(DevicePixels(24), DevicePixels(24))),
        )
        .expect("width-driven size");
    assert_eq!(exact_size_edge(&width_driven), (24, 12));

    // `ScaleFactor` multiplies by the smoothing factor, and the returned image
    // reports that factor so layout divides it back out. 24pt is 32 device
    // pixels at 96dpi, and the smoothing factor doubles the raster.
    let scaled = renderer
        .render_parsed(&tree, SvgSize::ScaleFactor(1.0))
        .expect("scale factor");
    let (width, height) = exact_size_edge(&scaled);
    assert_eq!((width, height), (64, 32));
}
