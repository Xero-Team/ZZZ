//! Shared table header controls.

use gpui::ClickEvent;
use i18n::tr;

use crate::Tooltip;
use crate::prelude::*;

/// The active sort state of one column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableSortDirection {
    Ascending,
    Descending,
}

/// A compact header button that shows the column's sort state and delegates the
/// state transition to the caller (each table owns its own sorting model).
pub fn table_sort_button(
    id: impl Into<ElementId>,
    active: Option<TableSortDirection>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Button {
    let (label, tooltip) = match active {
        Some(TableSortDirection::Ascending) => (
            "↓",
            tr(
                cx,
                "table.sort.sorted_asc_click_desc",
                "Sorted A-Z. Click to sort Z-A",
            ),
        ),
        Some(TableSortDirection::Descending) => (
            "↑",
            tr(
                cx,
                "table.sort.sorted_desc_click_disable",
                "Sorted Z-A. Click to disable sorting",
            ),
        ),
        None => (
            "↕",
            tr(
                cx,
                "table.sort.not_sorted_click_asc",
                "Not sorted. Click to sort A-Z",
            ),
        ),
    };

    Button::new(id, SharedString::from(label))
        .size(ButtonSize::Compact)
        .style(match active {
            Some(_) => ButtonStyle::Filled,
            None => ButtonStyle::Subtle,
        })
        .tooltip(Tooltip::text(tooltip))
        .on_click(on_click)
}
