use gpui::App;
use i18n::tr;
use ui::{
    ActiveTheme as _, AnyElement, ButtonSize, Context, ContextMenu, DropdownMenu, ElementId,
    FluentBuilder as _, IntoElement as _, ParentElement as _, Styled as _, Tooltip, Window, div,
    h_flex,
};

use crate::{
    CsvPreviewView,
    settings::{DelimiterSelection, HeaderMode, VerticalAlignment},
};

///// Settings related /////
impl CsvPreviewView {
    /// Render settings panel above the table
    pub(crate) fn render_settings_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current_alignment_text = match self.settings.vertical_alignment {
            VerticalAlignment::Top => tr(cx, "csv_preview.settings.alignment.top", "Top"),
            VerticalAlignment::Center => tr(cx, "csv_preview.settings.alignment.center", "Center"),
        };
        let top_label = tr(cx, "csv_preview.settings.alignment.top", "Top");
        let center_label = tr(cx, "csv_preview.settings.alignment.center", "Center");
        let delimiter_label =
            delimiter_selection_label(self.settings.delimiter, self.detected_delimiter, cx);
        let header_label = match self.settings.header_mode {
            HeaderMode::FirstRow => tr(cx, "csv_preview.settings.header.first_row", "First row"),
            HeaderMode::NoHeader => tr(cx, "csv_preview.settings.header.none", "No header"),
        };

        let view = cx.entity();
        let alignment_dropdown_menu = ContextMenu::build(window, cx, move |menu, _window, _cx| {
            menu.entry(top_label.clone(), None, {
                let view = view.clone();
                move |_window, cx| {
                    view.update(cx, |this, cx| {
                        this.settings.vertical_alignment = VerticalAlignment::Top;
                        cx.notify();
                    });
                }
            })
            .entry(center_label.clone(), None, {
                let view = view;
                move |_window, cx| {
                    view.update(cx, |this, cx| {
                        this.settings.vertical_alignment = VerticalAlignment::Center;
                        cx.notify();
                    });
                }
            })
        });

        let view = cx.entity();
        let delimiter_dropdown_menu = ContextMenu::build(window, cx, move |menu, _window, cx| {
            let entries = [
                (
                    tr(cx, "csv_preview.settings.delimiter.auto", "Auto"),
                    DelimiterSelection::Auto,
                ),
                (
                    tr(cx, "csv_preview.settings.delimiter.comma", "Comma (,)"),
                    DelimiterSelection::Character(','),
                ),
                (
                    tr(cx, "csv_preview.settings.delimiter.tab", "Tab"),
                    DelimiterSelection::Character('\t'),
                ),
                (
                    tr(cx, "csv_preview.settings.delimiter.pipe", "Pipe (|)"),
                    DelimiterSelection::Character('|'),
                ),
                (
                    tr(
                        cx,
                        "csv_preview.settings.delimiter.semicolon",
                        "Semicolon (;)",
                    ),
                    DelimiterSelection::Character(';'),
                ),
            ];
            entries.into_iter().fold(menu, |menu, (label, selection)| {
                let view = view.clone();
                menu.entry(label, None, move |_window, cx| {
                    view.update(cx, |this, cx| {
                        this.settings.delimiter = selection;
                        this.parse_delimited_from_active_editor(false, cx);
                        cx.notify();
                    });
                })
            })
        });

        let view = cx.entity();
        let header_dropdown_menu = ContextMenu::build(window, cx, move |menu, _window, cx| {
            let first_row_label = tr(cx, "csv_preview.settings.header.first_row", "First row");
            let no_header_label = tr(cx, "csv_preview.settings.header.none", "No header");
            menu.entry(first_row_label, None, {
                let view = view.clone();
                move |_window, cx| {
                    view.update(cx, |this, cx| {
                        this.settings.header_mode = HeaderMode::FirstRow;
                        this.parse_delimited_from_active_editor(false, cx);
                        cx.notify();
                    });
                }
            })
            .entry(no_header_label, None, move |_window, cx| {
                view.update(cx, |this, cx| {
                    this.settings.header_mode = HeaderMode::NoHeader;
                    this.parse_delimited_from_active_editor(false, cx);
                    cx.notify();
                });
            })
        });

        let panel = h_flex()
            .gap_4()
            .p_2()
            .bg(cx.theme().colors().surface_background)
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .flex_wrap()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().colors().text_muted)
                            .child(tr(
                                cx,
                                "csv_preview.settings.text_alignment",
                                "Text Alignment:",
                            )),
                    )
                    .child(
                        DropdownMenu::new(
                            ElementId::Name("vertical-alignment-dropdown".into()),
                            current_alignment_text,
                            alignment_dropdown_menu,
                        )
                        .trigger_size(ButtonSize::Compact)
                        .trigger_tooltip(Tooltip::text(tr(
                            cx,
                            "csv_preview.settings.choose_vertical_text_alignment",
                            "Choose vertical text alignment within cells",
                        ))),
                    ),
            );

        let panel = panel.child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().colors().text_muted)
                        .child(tr(cx, "csv_preview.settings.delimiter", "Delimiter:")),
                )
                .child(
                    DropdownMenu::new(
                        ElementId::Name("delimiter-dropdown".into()),
                        delimiter_label,
                        delimiter_dropdown_menu,
                    )
                    .trigger_size(ButtonSize::Compact)
                    .trigger_tooltip(Tooltip::text(tr(
                        cx,
                        "csv_preview.settings.choose_delimiter",
                        "Choose how columns are separated",
                    ))),
                )
                .child(self.custom_delimiter_editor.clone())
                .when(self.custom_delimiter_error, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().colors().text_muted)
                            .child(tr(
                                cx,
                                "csv_preview.settings.invalid_delimiter",
                                "Enter exactly one character other than quote or newline",
                            )),
                    )
                }),
        );

        let panel = panel.child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().colors().text_muted)
                        .child(tr(cx, "csv_preview.settings.header", "Header:")),
                )
                .child(
                    DropdownMenu::new(
                        ElementId::Name("header-dropdown".into()),
                        header_label,
                        header_dropdown_menu,
                    )
                    .trigger_size(ButtonSize::Compact),
                ),
        );

        #[cfg(feature = "dev-tools")]
        let panel = panel.child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().colors().text_muted)
                        .child(tr(cx, "csv_preview.settings.dev_only", "Dev-only:")),
                )
                .child(create_dev_only_popover_menu(cx)),
        );

        panel.into_any_element()
    }
}

fn delimiter_selection_label(
    selection: DelimiterSelection,
    detected_delimiter: Option<char>,
    cx: &App,
) -> ui::SharedString {
    match (selection, detected_delimiter) {
        (DelimiterSelection::Auto, Some(delimiter)) => format!(
            "{} ({})",
            tr(cx, "csv_preview.settings.delimiter.auto", "Auto"),
            delimiter_display(delimiter, cx)
        )
        .into(),
        (DelimiterSelection::Auto, None) => {
            tr(cx, "csv_preview.settings.delimiter.auto", "Auto").into()
        }
        (DelimiterSelection::Character(','), _) => {
            tr(cx, "csv_preview.settings.delimiter.comma", "Comma (,)").into()
        }
        (DelimiterSelection::Character('\t'), _) => {
            tr(cx, "csv_preview.settings.delimiter.tab", "Tab").into()
        }
        (DelimiterSelection::Character('|'), _) => {
            tr(cx, "csv_preview.settings.delimiter.pipe", "Pipe (|)").into()
        }
        (DelimiterSelection::Character(';'), _) => tr(
            cx,
            "csv_preview.settings.delimiter.semicolon",
            "Semicolon (;)",
        )
        .into(),
        (DelimiterSelection::Character(delimiter), _) => format!(
            "{} ({})",
            tr(cx, "csv_preview.settings.delimiter.custom", "Custom"),
            delimiter_display(delimiter, cx)
        )
        .into(),
    }
}

fn delimiter_display(delimiter: char, cx: &App) -> String {
    match delimiter {
        '\t' => tr(cx, "csv_preview.settings.delimiter.tab", "Tab"),
        ' ' => tr(cx, "csv_preview.settings.delimiter.space", "Space"),
        delimiter => delimiter.to_string(),
    }
}

#[cfg(feature = "dev-tools")]
fn create_dev_only_popover_menu(
    cx: &mut Context<'_, CsvPreviewView>,
) -> ui::PopoverMenu<ContextMenu> {
    use crate::settings::RowRenderMechanism;
    use ui::{IconButton, IconName, IconPosition, IconSize, PopoverMenu};

    PopoverMenu::new("debug-options-menu")
        .trigger_with_tooltip(
            IconButton::new("debug-options-trigger", IconName::Settings).icon_size(IconSize::Small),
            Tooltip::text(tr(
                cx,
                "csv_preview.settings.dev_tools_tooltip",
                "Dev-only section used for debugging purposes.\nWill be removed on public release of delimited text preview",
            )),
        )
        .menu({
            let view_entity = cx.entity();
            move |window, cx| {
                let view = view_entity.read(cx);
                let settings = view.settings.clone();
                Some(ContextMenu::build(window, cx, |menu, _, cx| {
                    menu.header(tr(
                        cx,
                        "csv_preview.settings.rendering_mode",
                        "Rendering Mode",
                    ))
                        .toggleable_entry(
                            tr(
                                cx,
                                "csv_preview.settings.variable_height",
                                "Variable Height",
                            ),
                            settings.rendering_with == RowRenderMechanism::VariableList,
                            IconPosition::Start,
                            None,
                            {
                                let view_entity = view_entity.clone();
                                move |_w, cx| {
                                    view_entity.update(cx, |view, cx| {
                                        view.settings.rendering_with =
                                            RowRenderMechanism::VariableList;
                                        view.settings.multiline_cells_enabled = true;
                                        cx.notify();
                                    })
                                }
                            },
                        )
                        .toggleable_entry(
                            tr(
                                cx,
                                "csv_preview.settings.uniform_height",
                                "Uniform Height",
                            ),
                            settings.rendering_with == RowRenderMechanism::UniformList,
                            IconPosition::Start,
                            None,
                            {
                                let view_entity = view_entity.clone();
                                move |_w, cx| {
                                    view_entity.update(cx, |view, cx| {
                                        view.settings.rendering_with =
                                            RowRenderMechanism::UniformList;
                                        view.settings.multiline_cells_enabled = false;
                                        cx.notify();
                                    })
                                }
                            },
                        )
                        .separator()
                        .toggleable_entry(
                            tr(
                                cx,
                                "csv_preview.settings.show_perf_metrics",
                                "Show perf metrics",
                            ),
                            settings.show_perf_metrics_overlay,
                            IconPosition::Start,
                            None,
                            {
                                let view_entity = view_entity.clone();
                                move |_w, cx| {
                                    view_entity.update(cx, |view, cx| {
                                        view.settings.show_perf_metrics_overlay =
                                            !view.settings.show_perf_metrics_overlay;
                                        cx.notify();
                                    })
                                }
                            },
                        )
                        .toggleable_entry(
                            tr(
                                cx,
                                "csv_preview.settings.show_cell_positions",
                                "Show cell positions",
                            ),
                            settings.show_debug_info,
                            IconPosition::Start,
                            None,
                            {
                                let view_entity = view_entity.clone();
                                move |_, cx| {
                                    view_entity.update(cx, |view, cx| {
                                        view.settings.show_debug_info =
                                            !view.settings.show_debug_info;
                                        cx.notify();
                                    })
                                }
                            },
                        )
                }))
            }
        })
}
