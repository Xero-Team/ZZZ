use crate::types::TableCell;
use gpui::{AnyElement, Div, Entity, Stateful};
use std::ops::Range;
use ui::{ColumnWidthConfig, ResizableColumnsState, Table, UncheckedTableRow, div, prelude::*};

use crate::{
    CsvPreviewView,
    renderer::table_cell::create_table_cell,
    settings::RowRenderMechanism,
    types::{AnyColumn, DisplayCellId, DisplayRow},
};

fn render_data_cell(cell: Stateful<Div>, debug_information: Option<AnyElement>) -> AnyElement {
    match debug_information {
        Some(debug_information) => v_flex()
            .w_full()
            .min_w_0()
            .items_stretch()
            .child(debug_information)
            .child(cell.flex_grow())
            .into_any_element(),
        None => cell.into_any_element(),
    }
}

impl CsvPreviewView {
    /// Creates a new table.
    /// Column number is derived from the `ResizableColumnsState` entity.
    pub(crate) fn create_table(
        &self,
        current_widths: &Entity<ResizableColumnsState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.create_table_inner(self.engine.contents.rows.len(), current_widths, cx)
    }

    fn create_table_inner(
        &self,
        row_count: usize,
        current_widths: &Entity<ResizableColumnsState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cols = current_widths.read(cx).cols();
        // Create headers array with interactive elements
        let mut headers = Vec::with_capacity(cols);

        headers.push(self.create_row_identifier_header(cx));

        // Add the actual CSV headers with sort buttons
        for i in 0..(cols - 1) {
            let header_text = self
                .engine
                .contents
                .headers
                .get(AnyColumn(i))
                .map(|header| {
                    header.synthetic_column().map_or_else(
                        || header.display_value().cloned().unwrap_or_default(),
                        |column| {
                            i18n::tr(cx, "csv_preview.settings.synthetic_column", "Column {}")
                                .replacen("{}", &column.to_string(), 1)
                                .into()
                        },
                    )
                })
                .unwrap_or_default();

            headers.push(self.create_header_element_with_sort_button(
                header_text,
                cx,
                AnyColumn::from(i),
            ));
        }

        Table::new(cols)
            .interactable(&self.table_interaction_state)
            .striped()
            .width_config(ColumnWidthConfig::Resizable(current_widths.clone()))
            .header(headers)
            .disable_base_style()
            .pin_cols(1)
            .map(|table| {
                let row_identifier_text_color = cx.theme().colors().editor_line_number;
                match self.settings.rendering_with {
                    RowRenderMechanism::VariableList => {
                        table.variable_row_height_list(row_count, self.list_state.clone(), {
                            cx.processor(move |this, display_row: usize, _window, cx| {
                                this.performance_metrics.rendered_indices.push(display_row);

                                let display_row = DisplayRow(display_row);
                                Self::render_single_table_row(
                                    this,
                                    cols,
                                    display_row,
                                    row_identifier_text_color,
                                    cx,
                                )
                                .unwrap_or_else(|| panic!("Expected to render a table row"))
                            })
                        })
                    }
                    RowRenderMechanism::UniformList => {
                        table.uniform_list("csv-table", row_count, {
                            cx.processor(move |this, range: Range<usize>, _window, cx| {
                                // Record all display indices in the range for performance metrics
                                this.performance_metrics
                                    .rendered_indices
                                    .extend(range.clone());

                                range
                                    .filter_map(|display_index| {
                                        Self::render_single_table_row(
                                            this,
                                            cols,
                                            DisplayRow(display_index),
                                            row_identifier_text_color,
                                            cx,
                                        )
                                    })
                                    .collect()
                            })
                        })
                    }
                }
            })
            .into_any_element()
    }

    /// Render a single table row
    ///
    /// Used both by UniformList and VariableRowHeightList
    fn render_single_table_row(
        this: &CsvPreviewView,
        cols: usize,
        display_row: DisplayRow,
        row_identifier_text_color: gpui::Hsla,
        cx: &Context<CsvPreviewView>,
    ) -> Option<UncheckedTableRow<AnyElement>> {
        // Get the actual row index from our sorted indices
        let data_row = this.engine.d2d_mapping().get_data_row(display_row)?;
        let row = this.engine.contents.get_row(data_row)?;

        let mut elements = Vec::with_capacity(cols);
        elements.push(this.create_row_identifier_cell(display_row, data_row, cx)?);

        // Remaining columns: actual CSV data
        for col in (0..this.engine.contents.number_of_cols).map(AnyColumn) {
            let table_cell = row.expect_get(col);

            // TODO: Introduce `<null>` cell type
            let cell_content = table_cell.display_value().cloned().unwrap_or_default();

            let display_cell_id = DisplayCellId::new(display_row, col);

            let cell = create_table_cell(
                display_cell_id,
                cell_content,
                this.settings.multiline_cells_enabled,
                this.settings.vertical_alignment,
                cx,
            );

            let debug_information = this.settings.show_debug_info.then(|| {
                let description = match table_cell {
                    TableCell::Real { position, .. } => {
                        let start_line = position.start.timestamp().value;
                        let start_offset = position.start.offset;
                        let end_line = position.end.timestamp().value;
                        let end_offset = position.end.offset;
                        format!("Pos {start_offset}(L{start_line})-{end_offset}(L{end_line})")
                    }
                    TableCell::Synthetic { .. } => "Synthetic cell".into(),
                    TableCell::Virtual => "Virtual cell".into(),
                };

                div()
                    .text_color(row_identifier_text_color)
                    .text_ui(cx)
                    .child(description)
                    .into_any_element()
            });

            elements.push(render_data_cell(cell, debug_information));
        }

        Some(elements)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{ListAlignment, Render, TestAppContext, Window, px};
    use settings::SettingsStore;

    const ROW_COUNT: usize = 5;

    struct TestTable {
        list_state: gpui::ListState,
    }

    impl Render for TestTable {
        fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            Table::new(2)
                .width_config(ColumnWidthConfig::explicit(vec![px(80.), px(240.)]))
                .disable_base_style()
                .hide_row_borders()
                .hide_row_hover()
                .pin_cols(1)
                .variable_row_height_list(ROW_COUNT, self.list_state.clone(), {
                    cx.processor(|_, row_index, _, cx| test_row(row_index, cx))
                })
        }
    }

    fn test_cell(
        row_index: usize,
        selector: &'static str,
        content: &'static str,
        cx: &App,
    ) -> Stateful<Div> {
        let vertical_alignment = if row_index == 2 {
            crate::settings::VerticalAlignment::Center
        } else {
            crate::settings::VerticalAlignment::Top
        };
        create_table_cell(
            DisplayCellId::new(DisplayRow(row_index), AnyColumn(0)),
            content.into(),
            true,
            vertical_alignment,
            cx,
        )
        .debug_selector(move || selector.to_owned())
    }

    fn test_row(row_index: usize, cx: &App) -> UncheckedTableRow<AnyElement> {
        match row_index {
            0 => vec![
                div()
                    .debug_selector(|| "tall-identifier".into())
                    .h(px(64.))
                    .into_any_element(),
                render_data_cell(test_cell(row_index, "single-line-cell", "value", cx), None),
            ],
            1 => vec![
                div()
                    .debug_selector(|| "short-identifier".into())
                    .child("3")
                    .into_any_element(),
                render_data_cell(
                    test_cell(row_index, "multiline-cell", "first\nsecond\nthird", cx),
                    None,
                ),
            ],
            2 => vec![
                div()
                    .debug_selector(|| "debug-identifier".into())
                    .h(px(80.))
                    .into_any_element(),
                render_data_cell(
                    test_cell(row_index, "debug-cell", "value", cx),
                    Some(div().h(px(16.)).into_any_element()),
                ),
            ],
            3 => vec![
                div()
                    .debug_selector(|| "empty-identifier".into())
                    .h(px(64.))
                    .into_any_element(),
                render_data_cell(test_cell(row_index, "empty-cell", "", cx), None),
            ],
            4 => vec![
                div()
                    .debug_selector(|| "wrapped-identifier".into())
                    .child("6")
                    .into_any_element(),
                render_data_cell(
                    test_cell(
                        row_index,
                        "wrapped-cell",
                        "a deliberately long value that must wrap across several visual lines in a narrow column",
                        cx,
                    ),
                    None,
                ),
            ],
            _ => unreachable!("the table renders exactly five rows"),
        }
    }

    #[gpui::test]
    fn pinned_data_cells_fill_variable_height_rows(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
            theme_settings::init(theme::LoadThemes::JustBase, cx);
        });

        let list_state = gpui::ListState::new(ROW_COUNT, ListAlignment::Top, px(0.)).measure_all();
        let (_, cx) = cx.add_window_view(|_, _| TestTable { list_state });

        let tall_identifier = cx
            .debug_bounds("tall-identifier")
            .expect("tall identifier should be rendered");
        let single_line_cell = cx
            .debug_bounds("single-line-cell")
            .expect("single-line cell should be rendered");
        assert_eq!(single_line_cell.top(), tall_identifier.top());
        assert_eq!(single_line_cell.size.height, tall_identifier.size.height);
        assert_eq!(single_line_cell.bottom(), tall_identifier.bottom());

        let short_identifier = cx
            .debug_bounds("short-identifier")
            .expect("short identifier should be rendered");
        let multiline_cell = cx
            .debug_bounds("multiline-cell")
            .expect("multiline cell should be rendered");
        assert_eq!(short_identifier.top(), tall_identifier.bottom());
        assert_eq!(multiline_cell.top(), short_identifier.top());
        assert_eq!(short_identifier.size.height, multiline_cell.size.height);
        assert_eq!(short_identifier.bottom(), multiline_cell.bottom());
        assert!(multiline_cell.size.height > px(40.));

        let debug_identifier = cx
            .debug_bounds("debug-identifier")
            .expect("debug identifier should be rendered");
        let debug_cell = cx
            .debug_bounds("debug-cell")
            .expect("debug cell should be rendered");
        assert_eq!(debug_identifier.top(), short_identifier.bottom());
        assert!(debug_cell.top() > debug_identifier.top());
        assert_eq!(debug_cell.bottom(), debug_identifier.bottom());

        let empty_identifier = cx
            .debug_bounds("empty-identifier")
            .expect("empty identifier should be rendered");
        let empty_cell = cx
            .debug_bounds("empty-cell")
            .expect("empty cell should be rendered");
        assert_eq!(empty_identifier.top(), debug_identifier.bottom());
        assert_eq!(empty_cell.top(), empty_identifier.top());
        assert_eq!(empty_cell.size.height, empty_identifier.size.height);
        assert_eq!(empty_cell.bottom(), empty_identifier.bottom());

        let wrapped_identifier = cx
            .debug_bounds("wrapped-identifier")
            .expect("wrapped identifier should be rendered");
        let wrapped_cell = cx
            .debug_bounds("wrapped-cell")
            .expect("wrapped cell should be rendered");
        assert_eq!(wrapped_identifier.top(), empty_identifier.bottom());
        assert_eq!(wrapped_cell.top(), wrapped_identifier.top());
        assert_eq!(wrapped_identifier.size.height, wrapped_cell.size.height);
        assert_eq!(wrapped_identifier.bottom(), wrapped_cell.bottom());
        assert!(wrapped_cell.size.height > px(40.));
    }
}
