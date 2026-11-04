use std::borrow::Cow;

use gpui::ElementId;
use ui::{
    TableSortDirection, prelude::*, table_sort_button, utils::replace_control_characters,
    with_copy_on_right_click,
};

use crate::{
    CsvPreviewView,
    table_data_engine::sorting_by_column::{AppliedSorting, SortDirection},
    types::AnyColumn,
};

impl CsvPreviewView {
    /// Create header for data, which is orderable with text on the left and sort button on the right
    pub(crate) fn create_header_element_with_sort_button(
        &self,
        header_text: SharedString,
        cx: &mut Context<'_, CsvPreviewView>,
        col_idx: AnyColumn,
    ) -> AnyElement {
        // CSV data columns: text + filter/sort buttons
        h_flex()
            .justify_between()
            .items_center()
            .w_full()
            .font_buffer(cx)
            .child({
                let header_text = match replace_control_characters(&header_text) {
                    Cow::Borrowed(_) => header_text.clone(),
                    Cow::Owned(replaced) => SharedString::from(replaced),
                };
                let header_text_cell = div()
                    .id(ElementId::NamedInteger(
                        "csv-col-header-text".into(),
                        col_idx.get() as u64,
                    ))
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap();
                with_copy_on_right_click(
                    header_text_cell,
                    header_text.clone(),
                    "csv_preview.copy_column_hint",
                    "Right click to copy column name",
                )
                .child(header_text)
            })
            .child(h_flex().gap_1().child(self.create_sort_button(cx, col_idx)))
            .into_any_element()
    }

    fn create_sort_button(
        &self,
        cx: &mut Context<'_, CsvPreviewView>,
        col_idx: AnyColumn,
    ) -> Button {
        let active = self
            .engine
            .applied_sorting
            .filter(|ordering| ordering.col_idx == col_idx)
            .map(|ordering| match ordering.direction {
                SortDirection::Asc => TableSortDirection::Ascending,
                SortDirection::Desc => TableSortDirection::Descending,
            });

        table_sort_button(
            ElementId::NamedInteger("sort-button".into(), col_idx.get() as u64),
            active,
            cx.listener(move |this, _event, _window, cx| {
                let new_sorting = match this.engine.applied_sorting {
                    Some(ordering) if ordering.col_idx == col_idx => {
                        // Same column clicked - cycle through states
                        match ordering.direction {
                            SortDirection::Asc => Some(AppliedSorting {
                                col_idx,
                                direction: SortDirection::Desc,
                            }),
                            SortDirection::Desc => None, // Clear sorting
                        }
                    }
                    _ => {
                        // Different column or no sorting - start with ascending
                        Some(AppliedSorting {
                            col_idx,
                            direction: SortDirection::Asc,
                        })
                    }
                };

                this.engine.applied_sorting = new_sorting;
                this.apply_sort();
                cx.notify();
            }),
            cx,
        )
    }
}
