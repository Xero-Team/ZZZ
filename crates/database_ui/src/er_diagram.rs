//! ER diagram rendering for the database view.
//!
//! The diagram source comes from `database_core::er_diagram`; this module only
//! maps the current ZZZ theme onto a `mermaid_render` theme and rasterizes the
//! SVG. Rendering happens off the main thread because layout is CPU-bound.

use std::sync::Arc;

use gpui::{App, RenderImage, SvgRenderer};
use mermaid_render::{AccentColor, MermaidTheme};
use theme::ActiveTheme as _;

/// A font stack that is present on every supported platform. Mermaid measures
/// text while laying out, so the family has to be resolvable by the rasterizer.
const MERMAID_FONT_FAMILY: &str = "Inter, ui-sans-serif, system-ui, -apple-system, \"Segoe UI\", \"DejaVu Sans\", \"Liberation Sans\", sans-serif";

/// The state of the ER diagram panel: the Mermaid source and its rasterized
/// image. The image is `None` while rendering is in flight.
pub struct ErDiagram {
    pub source: String,
    pub image: Option<Result<Arc<RenderImage>, String>>,
}

impl ErDiagram {
    pub fn new(source: String) -> Self {
        Self {
            source,
            image: None,
        }
    }
}

/// Build a Mermaid theme from the active ZZZ theme.
pub fn mermaid_theme(cx: &App) -> MermaidTheme {
    let colors = cx.theme().colors();
    let is_dark = !cx.theme().appearance.is_light();
    let players = cx.theme().players();
    let git_branch_colors = std::array::from_fn(|index| {
        players
            .0
            .get(index % players.0.len().max(1))
            .map_or(colors.text, |player| player.cursor)
    });
    let git_branch_label_colors = git_branch_colors.map(mermaid_render::text_color_for_background);

    MermaidTheme {
        dark_mode: is_dark,
        font_family: MERMAID_FONT_FAMILY.to_owned(),
        background: colors.editor_background,
        primary_color: colors.surface_background,
        primary_text_color: colors.text,
        primary_border_color: colors.border,
        secondary_color: colors.element_background,
        tertiary_color: colors.ghost_element_hover,
        line_color: colors.border,
        text_color: colors.text,
        edge_label_background: colors.editor_background,
        cluster_background: colors.panel_background,
        cluster_border: colors.border_variant,
        note_background: colors.surface_background,
        note_border: colors.border_variant,
        actor_background: colors.element_background,
        actor_border: colors.border,
        activation_background: colors.ghost_element_hover,
        activation_border: colors.border,
        git_branch_colors,
        git_branch_label_colors,
        er_attr_bg_odd: colors.surface_background,
        er_attr_bg_even: colors.element_background,
        error_color: cx.theme().status().error,
        warning_color: cx.theme().status().warning,
        accent_colors: players
            .0
            .iter()
            .map(|player| AccentColor {
                foreground: player.cursor,
                background: player.background,
            })
            .collect(),
    }
}

/// Render Mermaid source to a GPUI image. Errors are strings because they are
/// shown to the user; the underlying error types do not cross the UI boundary.
pub async fn render_diagram(
    source: String,
    theme: MermaidTheme,
    renderer: SvgRenderer,
) -> Result<Arc<RenderImage>, String> {
    let svg =
        mermaid_render::render_to_svg(&source, &theme).map_err(|error| format!("{error:#}"))?;
    renderer
        .render_single_frame(svg.as_bytes(), 1.0)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
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
}
