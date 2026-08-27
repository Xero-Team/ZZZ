use std::borrow::Cow;

use gpui::ElementId;
use i18n::tr;
use ui::{Tooltip, prelude::*, utils::replace_control_characters};

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
            .child(div().child(match replace_control_characters(&header_text) {
                Cow::Borrowed(_) => header_text.clone(),
                Cow::Owned(replaced) => SharedString::from(replaced),
            }))
            .child(h_flex().gap_1().child(self.create_sort_button(cx, col_idx)))
            .into_any_element()
    }

    fn create_sort_button(
        &self,
        cx: &mut Context<'_, CsvPreviewView>,
        col_idx: AnyColumn,
    ) -> Button {
        let sort_btn = Button::new(
            ElementId::NamedInteger("sort-button".into(), col_idx.get() as u64),
            match self.engine.applied_sorting {
                Some(ordering) if ordering.col_idx == col_idx => match ordering.direction {
                    SortDirection::Asc => "↓",
                    SortDirection::Desc => "↑",
                },
                _ => "↕", // Unsorted/available for sorting
            },
        )
        .size(ButtonSize::Compact)
        .style(
            if self
                .engine
                .applied_sorting
                .is_some_and(|o| o.col_idx == col_idx)
            {
                ButtonStyle::Filled
            } else {
                ButtonStyle::Subtle
            },
        )
        .tooltip(Tooltip::text(match self.engine.applied_sorting {
            Some(ordering) if ordering.col_idx == col_idx => match ordering.direction {
                SortDirection::Asc => tr(
                    cx,
                    "csv_preview.sort.sorted_asc_click_desc",
                    "Sorted A-Z. Click to sort Z-A",
                ),
                SortDirection::Desc => tr(
                    cx,
                    "csv_preview.sort.sorted_desc_click_disable",
                    "Sorted Z-A. Click to disable sorting",
                ),
            },
            _ => tr(
                cx,
                "csv_preview.sort.not_sorted_click_asc",
                "Not sorted. Click to sort A-Z",
            ),
        }))
        .on_click(cx.listener(move |this, _event, _window, cx| {
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
        }));
        sort_btn
    }
}
