//! ER diagram rendering for the database view.
//!
//! The diagram source comes from `database_core::er_diagram`; this module turns
//! it into SVG and rasterizes that SVG at the zoom the panel is showing.
//! Mermaid layout is the expensive part, so the SVG is kept in [`ErDiagram`]
//! and only the rasterization repeats when the zoom changes.
//!
//! The Mermaid theme and font come from the markdown crate so an ER diagram
//! looks like any other Mermaid diagram in the app.

use std::sync::Arc;

use gpui::{Pixels, RenderImage, Size, SvgRenderer, px, size};
use mermaid_render::MermaidTheme;

/// The Mermaid theme used for ER diagrams: the same theme as a markdown
/// preview, including the user's `mermaid_font_family` setting.
pub use markdown::mermaid_theme;

/// gpui refuses to allocate a raster larger than this on either axis and
/// silently downscales whatever asks for more, which is what made wide
/// diagrams blurry.
const MAX_RASTER_SIZE: f32 = 8192.0;
/// `SvgRenderer` rasterizes at twice the requested scale so the result stays
/// crisp on high-DPI displays, which halves the size a diagram may claim.
const RASTER_OVERSAMPLE: f32 = 2.0;
/// Zoom bounds relative to the diagram's natural size.
pub const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 4.0;
/// The factor one click of the zoom-in or zoom-out button applies.
pub const ZOOM_STEP: f32 = 1.25;

/// The state of the ER diagram panel.
pub struct ErDiagram {
    /// The Mermaid source, kept so it can be copied to the clipboard.
    pub source: String,
    /// The rendered SVG, kept so a zoom change does not repeat the layout.
    pub svg: Option<String>,
    /// The diagram's size at 100% zoom.
    pub size: Option<Size<Pixels>>,
    /// `None` while a raster is in flight; an error is shown to the user.
    pub image: Option<Result<Arc<RenderImage>, String>>,
    /// The zoom the panel is showing. `None` until the diagram has a size.
    pub zoom: Option<f32>,
}

impl ErDiagram {
    pub fn new(source: String) -> Self {
        Self {
            source,
            svg: None,
            size: None,
            image: None,
            zoom: None,
        }
    }
}

/// The largest zoom whose raster still fits the renderer's limit.
pub fn max_zoom(natural: Size<Pixels>) -> f32 {
    let longest_side = f32::from(natural.width.max(natural.height)).max(1.0);
    (MAX_RASTER_SIZE / (RASTER_OVERSAMPLE * longest_side)).min(MAX_ZOOM)
}

/// The smallest zoom the panel will show. A diagram long enough to need less
/// than [`MIN_ZOOM`] keeps its raster inside the limit instead: a sharp small
/// diagram beats a blurred one at the requested size.
pub fn min_zoom(natural: Size<Pixels>) -> f32 {
    MIN_ZOOM.min(max_zoom(natural))
}

pub fn clamp_zoom(zoom: f32, natural: Size<Pixels>) -> f32 {
    zoom.clamp(min_zoom(natural), max_zoom(natural))
}

/// The zoom that makes the diagram exactly as wide as the panel, never
/// magnifying a diagram that already fits.
pub fn fit_width_zoom(viewport_width: Pixels, natural: Size<Pixels>) -> f32 {
    let natural_width = f32::from(natural.width);
    if natural_width <= 0.0 {
        return 1.0;
    }
    (f32::from(viewport_width) / natural_width).clamp(MIN_ZOOM, 1.0)
}

/// Read the diagram's natural size from the root `viewBox`, falling back to
/// `width`/`height`. Mermaid emits `width="100%"` alongside a `viewBox`, so the
/// `viewBox` is the attribute that actually carries the size.
pub fn svg_size(svg: &str) -> Option<Size<Pixels>> {
    let root = svg.split_once("<svg")?.1.split_once('>')?.0;
    if let Some(view_box) = attribute_value(root, "viewBox") {
        let numbers: Vec<f32> = view_box
            .replace(',', " ")
            .split_whitespace()
            .filter_map(|number| number.parse().ok())
            .collect();
        if let [_, _, width, height] = numbers[..]
            && width > 0.0
            && height > 0.0
        {
            return Some(size(px(width), px(height)));
        }
    }
    let width = attribute_value(root, "width").and_then(length)?;
    let height = attribute_value(root, "height").and_then(length)?;
    Some(size(px(width), px(height)))
}

fn attribute_value<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let rest = tag.split_once(&format!("{name}=\""))?.1;
    rest.split_once('"').map(|(value, _)| value)
}

/// Parse a CSS length, accepting the bare numbers and `px` units Mermaid emits.
fn length(value: &str) -> Option<f32> {
    value.trim().trim_end_matches("px").parse().ok()
}

/// Render Mermaid source to SVG, together with the diagram's natural size.
pub async fn render_svg(
    source: String,
    theme: MermaidTheme,
) -> Result<(String, Size<Pixels>), String> {
    let svg =
        mermaid_render::render_to_svg(&source, &theme).map_err(|error| format!("{error:#}"))?;
    let natural = svg_size(&svg).ok_or_else(|| "the diagram has no size".to_owned())?;
    Ok((svg, natural))
}

/// Rasterize a diagram SVG at `zoom` relative to its natural size.
pub async fn rasterize(
    svg: String,
    zoom: f32,
    renderer: SvgRenderer,
) -> Result<Arc<RenderImage>, String> {
    renderer
        .render_single_frame(svg.as_bytes(), zoom)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use database_core::{Column, ColumnType, ForeignKey, ObjectKind, TableRef, TableSchema};

    fn cjk_schema() -> TableSchema {
        let mut id = Column::new("编号", ColumnType::Integer);
        id.primary_key = true;
        TableSchema {
            reference: TableRef::new("顧客"),
            kind: ObjectKind::Table,
            columns: vec![id, Column::new("名前", ColumnType::Text)],
            primary_key: None,
            indexes: Vec::new(),
            foreign_keys: vec![ForeignKey {
                name: None,
                columns: vec!["名前".to_owned()],
                referenced_table: "顧客".to_owned(),
                referenced_columns: vec!["编号".to_owned()],
                on_delete: None,
                on_update: None,
            }],
            definition: None,
        }
    }

    /// The generated source must be accepted by the real Mermaid parser with
    /// non-ASCII table and column names, not just look plausible.
    #[test]
    fn cjk_er_diagram_is_valid_mermaid() {
        let source = database_core::er_diagram(&[cjk_schema()]);
        assert!(source.contains("顧客"));
        let theme = mermaid_render::MermaidTheme::default();
        let svg = mermaid_render::render_to_svg(&source, &theme)
            .expect("the generated diagram should render");
        assert!(svg.contains("<svg"));
    }

    #[test]
    fn diagram_without_foreign_keys_is_still_valid() {
        let mut schema = cjk_schema();
        schema.foreign_keys.clear();
        let source = database_core::er_diagram(&[schema]);
        let theme = mermaid_render::MermaidTheme::default();
        mermaid_render::render_to_svg(&source, &theme).expect("render");
    }

    /// The size of a real diagram comes from the `viewBox` that Mermaid emits
    /// next to `width="100%"`.
    #[test]
    fn reads_the_size_of_a_rendered_diagram() {
        let source = database_core::er_diagram(&[cjk_schema()]);
        let theme = mermaid_render::MermaidTheme::default();
        let svg = mermaid_render::render_to_svg(&source, &theme).expect("render");
        let natural = svg_size(&svg).expect("the diagram reports a size");
        assert!(f32::from(natural.width) > 0.0);
        assert!(f32::from(natural.height) > 0.0);
    }

    #[test]
    fn reads_a_view_box_with_comma_separators() {
        let svg = r#"<svg viewBox="0,0,120.5,40"><g/></svg>"#;
        let natural = svg_size(svg).expect("size");
        assert_eq!(f32::from(natural.width), 120.5);
        assert_eq!(f32::from(natural.height), 40.0);
    }

    #[test]
    fn falls_back_to_width_and_height_attributes() {
        let svg = r#"<svg width="300px" height="150px"><g/></svg>"#;
        let natural = svg_size(svg).expect("size");
        assert_eq!(f32::from(natural.width), 300.0);
        assert_eq!(f32::from(natural.height), 150.0);
    }

    /// A diagram wider than the renderer's limit must be capped, or gpui
    /// silently downscales it and the panel loses the sharpness it asked for.
    #[test]
    fn zoom_is_capped_below_the_raster_limit() {
        let wide = size(px(4760.78), px(2627.0));
        let capped = clamp_zoom(1.0, wide);
        assert!(capped < 1.0);
        assert!(f32::from(wide.width) * capped * RASTER_OVERSAMPLE <= MAX_RASTER_SIZE + 0.01);

        let narrow = size(px(200.0), px(120.0));
        assert_eq!(clamp_zoom(1.0, narrow), 1.0);
        assert_eq!(clamp_zoom(0.01, narrow), MIN_ZOOM);
    }

    /// A diagram too long to rasterize even at [`MIN_ZOOM`] keeps the raster
    /// limit rather than the display minimum, or gpui would downscale it and
    /// the panel would render it blurry.
    #[test]
    fn a_very_long_diagram_keeps_its_raster_inside_the_limit() {
        let long = size(px(200.0), px(100000.0));
        let zoom = clamp_zoom(1.0, long);
        assert!(zoom < MIN_ZOOM);
        assert!(f32::from(long.height) * zoom * RASTER_OVERSAMPLE <= MAX_RASTER_SIZE + 0.01);
        assert_eq!(zoom, min_zoom(long));
    }

    #[test]
    fn fitting_never_magnifies_the_diagram() {
        let natural = size(px(400.0), px(200.0));
        assert_eq!(fit_width_zoom(px(900.0), natural), 1.0);
        assert!((fit_width_zoom(px(200.0), natural) - 0.5).abs() < f32::EPSILON);
    }

    /// The zoom the panel chooses for a diagram it cannot fit must still fit
    /// the renderer, or the first frame would blur.
    #[test]
    fn the_fitting_zoom_of_a_wide_diagram_stays_within_the_raster_limit() {
        let wide = size(px(9600.0), px(1000.0));
        let fit = fit_width_zoom(px(900.0), wide);
        let capped = clamp_zoom(fit, wide);
        assert!(f32::from(wide.width) * capped * RASTER_OVERSAMPLE <= MAX_RASTER_SIZE + 0.01);
    }
}
