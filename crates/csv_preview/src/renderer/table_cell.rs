//! Table Cell Rendering

use gpui::{
    AnyElement, App, ClipboardItem, ElementId, MouseButton, Stateful, StatefulInteractiveElement,
};
use i18n::tr;
use ui::{Color, Divider, Label, LabelSize, SharedString, Tooltip, div, prelude::*, v_flex};

use crate::{settings::VerticalAlignment, types::DisplayCellId};

/// Adds a right-click-to-copy handler and a tooltip showing `text` plus a localized hint.
pub(super) fn with_copy_on_right_click<E: StatefulInteractiveElement>(
    element: E,
    text: SharedString,
    hint_key: &'static str,
    hint_fallback: &'static str,
) -> E {
    element
        .on_mouse_down(MouseButton::Right, {
            let text_to_copy = text.clone();
            move |_event, _window, cx| {
                cx.stop_propagation();
                cx.write_to_clipboard(ClipboardItem::new_string(text_to_copy.to_string()));
            }
        })
        .tooltip(Tooltip::element(move |_window, cx| {
            v_flex()
                .gap_1()
                .child(div().font_buffer(cx).child(text.clone()))
                .child(Divider::horizontal())
                .child(
                    Label::new(tr(cx, hint_key, hint_fallback))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element()
        }))
}

/// Create styled table cell div element.
pub(super) fn create_table_cell(
    display_cell_id: DisplayCellId,
    cell_content: SharedString,
    multiline_cells_enabled: bool,
    vertical_alignment: VerticalAlignment,
    cx: &App,
) -> Stateful<Div> {
    let cell_body = render_cell_content(cell_content.clone(), multiline_cells_enabled);

    let cell = div()
        .id(ElementId::Name(
            format!(
                "csv-display-cell-{}-{}",
                *display_cell_id.row, *display_cell_id.col
            )
            .into(),
        ))
        .flex()
        .w_full()
        .min_w_0()
        .px_1()
        .border_b_1()
        .border_color(cx.theme().colors().border_variant)
        .map(|div| match vertical_alignment {
            VerticalAlignment::Top => div.items_start().content_start(),
            VerticalAlignment::Center => div.items_center().content_center(),
        })
        .text_ui(cx)
        .font_buffer(cx);
    with_copy_on_right_click(
        cell,
        cell_content,
        "csv_preview.copy_cell_hint",
        "Right click to copy content",
    )
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
