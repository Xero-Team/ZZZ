//! Table Cell Rendering

use gpui::{AnyElement, ElementId};
use ui::{SharedString, Tooltip, div, prelude::*, v_flex};

use crate::{CsvPreviewView, settings::VerticalAlignment, types::DisplayCellId};

impl CsvPreviewView {
    /// Create selectable table cell with mouse event handlers.
    pub fn create_selectable_cell(
        display_cell_id: DisplayCellId,
        cell_content: SharedString,
        multiline_cells_enabled: bool,
        vertical_alignment: VerticalAlignment,
        cx: &Context<CsvPreviewView>,
    ) -> AnyElement {
        create_table_cell(
            display_cell_id,
            cell_content,
            multiline_cells_enabled,
            vertical_alignment,
            cx,
        )
            // Mouse events handlers will be here
            .into_any_element()
    }
}

/// Create styled table cell div element.
fn create_table_cell(
    display_cell_id: DisplayCellId,
    cell_content: SharedString,
    multiline_cells_enabled: bool,
    vertical_alignment: VerticalAlignment,
    cx: &Context<'_, CsvPreviewView>,
) -> gpui::Stateful<Div> {
    let cell_body = render_cell_content(cell_content.clone(), multiline_cells_enabled);

    div()
        .id(ElementId::Name(
            format!(
                "csv-display-cell-{}-{}",
                *display_cell_id.row, *display_cell_id.col
            )
            .into(),
        ))
        .cursor_pointer()
        .flex()
        .w_full()
        .min_w_0()
        .px_1()
        .bg(cx.theme().colors().editor_background)
        .border_b_1()
        .border_color(cx.theme().colors().border_variant)
        .map(|div| match vertical_alignment {
            VerticalAlignment::Top => div.items_start(),
            VerticalAlignment::Center => div.items_center(),
        })
        .map(|div| match vertical_alignment {
            VerticalAlignment::Top => div.content_start(),
            VerticalAlignment::Center => div.content_center(),
        })
        .font_buffer(cx)
        .tooltip(Tooltip::text(cell_content.clone()))
        .child(cell_body)
}

fn render_cell_content(cell_content: SharedString, multiline_cells_enabled: bool) -> AnyElement {
    if multiline_cells_enabled {
        let lines = cell_content
            .as_ref()
            .split('\n')
            .map(|line| {
                div()
                    .w_full()
                    .min_w_0()
                    .whitespace_normal()
                    .child(SharedString::from(line.to_owned()))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        return div()
            .w_full()
            .min_w_0()
            .child(v_flex().w_full().min_w_0().children(lines))
            .into_any_element();
    }

    div()
        .w_full()
        .min_w_0()
        .overflow_x_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .child(cell_content)
        .into_any_element()
}
