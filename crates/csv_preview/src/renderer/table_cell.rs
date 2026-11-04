//! Table Cell Rendering

use gpui::{ElementId, Stateful};
use ui::{CopyableTableCell, TableCellAlignment, prelude::*};

use crate::{settings::VerticalAlignment, types::DisplayCellId};

/// Create a styled, copyable table cell. The shared `ui` cell keeps the CSV
/// preview and the database result grid visually and behaviorally identical.
pub(super) fn create_table_cell(
    display_cell_id: DisplayCellId,
    cell_content: SharedString,
    multiline_cells_enabled: bool,
    vertical_alignment: VerticalAlignment,
    cx: &App,
) -> Stateful<Div> {
    CopyableTableCell::new(
        ElementId::Name(
            format!(
                "csv-display-cell-{}-{}",
                *display_cell_id.row, *display_cell_id.col
            )
            .into(),
        ),
        cell_content,
    )
    .multiline(multiline_cells_enabled)
    .alignment(match vertical_alignment {
        VerticalAlignment::Top => TableCellAlignment::Top,
        VerticalAlignment::Center => TableCellAlignment::Center,
    })
    .copy_hint("csv_preview.copy_cell_hint", "Right click to copy content")
    .render(cx)
}
