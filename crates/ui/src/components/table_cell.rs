//! Shared table-cell presentation.
//!
//! A table cell is more than text: it needs consistent padding, wrapping, and a
//! copy affordance. The CSV preview and the database result grid both render
//! cells through this module so their interaction cannot drift apart.

use gpui::{ClipboardItem, MouseButton, Stateful, StatefulInteractiveElement};
use i18n::tr;

use crate::prelude::*;
use crate::{Divider, Tooltip};

/// Vertical placement of cell content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TableCellAlignment {
    #[default]
    Top,
    Center,
}

/// Horizontal placement of cell content. Numbers are right-aligned so a column
/// can be compared at a glance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TableCellTextAlignment {
    #[default]
    Left,
    Right,
}

/// Builder for a table cell that can be copied with a right click.
pub struct CopyableTableCell {
    id: ElementId,
    content: SharedString,
    copy_text: Option<SharedString>,
    copy_hint: Option<(&'static str, &'static str)>,
    multiline: bool,
    alignment: TableCellAlignment,
    text_alignment: TableCellTextAlignment,
    text_color: Color,
}

impl CopyableTableCell {
    pub fn new(id: impl Into<ElementId>, content: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            copy_text: None,
            copy_hint: None,
            multiline: false,
            alignment: TableCellAlignment::Top,
            text_alignment: TableCellTextAlignment::Left,
            text_color: Color::Default,
        }
    }

    /// Text written to the clipboard instead of the rendered content. The
    /// database grid uses it to copy NULL as an empty field.
    pub fn copy_text(mut self, text: impl Into<SharedString>) -> Self {
        self.copy_text = Some(text.into());
        self
    }

    /// Enables right-click copy and shows `fallback` in the tooltip.
    pub fn copy_hint(mut self, key: &'static str, fallback: &'static str) -> Self {
        self.copy_hint = Some((key, fallback));
        self
    }

    pub fn multiline(mut self, multiline: bool) -> Self {
        self.multiline = multiline;
        self
    }

    pub fn alignment(mut self, alignment: TableCellAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    pub fn text_alignment(mut self, alignment: TableCellTextAlignment) -> Self {
        self.text_alignment = alignment;
        self
    }

    pub fn text_color(mut self, color: Color) -> Self {
        self.text_color = color;
        self
    }

    pub fn render(self, cx: &App) -> Stateful<Div> {
        let Self {
            id,
            content,
            copy_text,
            copy_hint,
            multiline,
            alignment,
            text_alignment,
            text_color,
        } = self;
        let copy_text = copy_text.unwrap_or_else(|| content.clone());
        let cell_body = render_cell_content(content, multiline, text_alignment);

        let cell = div()
            .id(id)
            .flex()
            .w_full()
            .min_w_0()
            .px_1()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .map(|div| match alignment {
                TableCellAlignment::Top => div.items_start().content_start(),
                TableCellAlignment::Center => div.items_center().content_center(),
            })
            .text_ui(cx)
            .font_buffer(cx)
            .text_color(text_color.color(cx))
            .child(cell_body);

        match copy_hint {
            Some((key, fallback)) => with_copy_on_right_click(cell, copy_text, key, fallback),
            None => cell,
        }
    }
}

/// Adds a right-click-to-copy handler and a tooltip showing `text` plus a
/// localized hint.
pub fn with_copy_on_right_click<E: StatefulInteractiveElement>(
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

fn render_cell_content(
    cell_content: SharedString,
    multiline: bool,
    text_alignment: TableCellTextAlignment,
) -> AnyElement {
    if multiline {
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
        .map(|div| match text_alignment {
            TableCellTextAlignment::Left => div,
            TableCellTextAlignment::Right => div.text_right(),
        })
        .child(cell_content)
        .into_any_element()
}
