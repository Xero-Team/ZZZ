use gpui::{
    AnyElement, Context, EventEmitter, Focusable, InteractiveElement, IntoElement, ParentElement,
    Render, ScrollDelta, ScrollWheelEvent, SharedString, StatefulInteractiveElement, Styled,
    WeakEntity, Window, div, px,
};
use i18n::tr;
use ui::{Banner, ListItem, Severity, Toggleable as _, Tooltip, prelude::*};
use workspace::{ToolbarItemEvent, ToolbarItemLocation, ToolbarItemView, item::ItemHandle};

use crate::{
    FitPage, FitWidth, NextMatch, NextPage, PAGE_HORIZONTAL_MARGIN, PAGE_VERTICAL_MARGIN,
    PreviousMatch, PreviousPage, ResetZoom, ToggleSearch, ToggleSidebar, ZoomIn, ZoomOut,
    view::{LoadState, PdfView},
};

impl PdfView {
    fn render_content(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match &self.load_state {
            LoadState::Loading => centered(
                Label::new(tr(cx, "pdf_viewer.loading", "Loading PDF...")).color(Color::Muted),
            ),
            LoadState::PasswordRequired(state) => self.render_password_prompt(state, window, cx),
            LoadState::Error(message) => centered(
                h_flex()
                    .gap_2()
                    .child(Icon::new(IconName::Warning).color(Color::Error))
                    .child(Label::new(message.clone()).color(Color::Error)),
            ),
            LoadState::Loaded(_) => v_flex()
                .size_full()
                .when(self.encryption_warning(), |this| {
                    this.child(
                        div().p_2().child(
                            Banner::new()
                                .severity(Severity::Warning)
                                .child(Label::new(tr(
                                    cx,
                                    "pdf_viewer.encrypted_warning",
                                    "This PDF is encrypted. The current renderer may be unable to decrypt it, which can leave pages blank.",
                                ))),
                        ),
                    )
                })
                .child(self.render_pages(window, cx))
                .into_any_element(),
        }
    }

    fn render_password_prompt(
        &self,
        state: &crate::view::PasswordRequiredState,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        centered(
            v_flex()
                .w(px(420.0))
                .gap_3()
                .p_4()
                .bg(cx.theme().colors().elevated_surface_background)
                .border_1()
                .border_color(cx.theme().colors().border_variant)
                .rounded_lg()
                .shadow_lg()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(Icon::new(IconName::FileLock).color(Color::Warning))
                        .child(
                            Headline::new(tr(cx, "pdf_viewer.password.title", "Unlock PDF"))
                                .size(HeadlineSize::Small),
                        ),
                )
                .child(Label::new(tr(
                    cx,
                    "pdf_viewer.password.description",
                    "This PDF is password-protected. Enter the password to open and search it.",
                )))
                .child(state.input.clone())
                .when_some(state.error.clone(), |this, error| {
                    this.child(Label::new(error).color(Color::Error).size(LabelSize::Small))
                })
                .child(
                    h_flex().justify_end().gap_2().child(
                        Button::new(
                            "pdf-unlock",
                            if state.opening {
                                tr(cx, "pdf_viewer.password.unlocking", "Unlocking...")
                            } else {
                                tr(cx, "pdf_viewer.password.unlock", "Unlock")
                            },
                        )
                        .disabled(state.opening)
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.submit_password(&menu::Confirm, window, cx);
                        })),
                    ),
                ),
        )
    }

    fn render_pages(&self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = self.loaded() else {
            return div().into_any_element();
        };
        let border_color = cx.theme().colors().border;
        let current_page = self.current_page;
        let render_failed_text = tr(cx, "pdf_viewer.render_failed", "Failed to render page");
        let rendering_text = tr(cx, "pdf_viewer.rendering", "Rendering...");
        let pages = self
            .page_layouts
            .iter()
            .enumerate()
            .map(|(index, layout)| {
                let preview = state.page_cache.get(&index).cloned();
                let width = px(layout.width);
                let height = px(layout.height);
                let selected = index == current_page;
                div()
                    .id(("pdf-page-preview", index))
                    .flex()
                    .pt(if index == 0 { px(0.0) } else { px(20.0) })
                    .px(px(PAGE_HORIZONTAL_MARGIN))
                    .justify_center()
                    .w_full()
                    .child(
                        div()
                            .border_1()
                            .border_color(if selected {
                                cx.theme().colors().border_focused
                            } else {
                                border_color
                            })
                            .bg(gpui::white())
                            .shadow_md()
                            .w(width)
                            .h(height)
                            .child(match preview {
                                Some(preview) => gpui::img(preview.image)
                                    .w(width)
                                    .h(height)
                                    .with_fallback({
                                        let render_failed_text = render_failed_text.clone();
                                        move || {
                                            div()
                                                .size_full()
                                                .child(Label::new(render_failed_text.clone()))
                                                .into_any_element()
                                        }
                                    })
                                    .into_any_element(),
                                None => {
                                    centered(Label::new(rendering_text.clone()).color(Color::Muted))
                                }
                            }),
                    )
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        div()
            .id("pdf-page-scroll")
            .size_full()
            .overflow_scroll()
            .track_scroll(&self.scroll_handle)
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .on_mouse_move(cx.listener(|view, _, window, cx| {
                view.sync_current_page_with_scroll(cx);
                view.ensure_pages_rendered(window, cx);
            }))
            .child(div().w_full().h(px(PAGE_VERTICAL_MARGIN)))
            .children(pages)
            .child(div().w_full().h(px(PAGE_VERTICAL_MARGIN)))
            .into_any_element()
    }

    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Ctrl/Cmd + wheel zooms around the current reading position. A plain
        // wheel is left to the native scroll container for continuous scrolling;
        // afterwards we resync the focused page from the new scroll position.
        if event.modifiers.control || event.modifiers.platform {
            let delta = match event.delta {
                ScrollDelta::Pixels(pixels) => f32::from(pixels.y),
                ScrollDelta::Lines(lines) => lines.y * 20.0,
            };
            if delta > 0.0 {
                self.zoom_in(&ZoomIn, window, cx);
            } else if delta < 0.0 {
                self.zoom_out(&ZoomOut, window, cx);
            }
            return;
        }

        cx.on_next_frame(window, |view, window, cx| {
            view.sync_current_page_with_scroll(cx);
            view.ensure_pages_rendered(window, cx);
        });
    }
}

fn centered(child: impl IntoElement) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size_full()
        .child(child)
        .into_any_element()
}

impl PdfView {
    fn render_sidebar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.show_sidebar {
            return None;
        }
        let state = self.loaded()?;
        let outline = state.summary.outline.clone();
        let current_page = self.current_page;

        let header = if outline.is_empty() {
            Label::new(tr(cx, "pdf_viewer.sidebar.pages", "Pages"))
                .size(LabelSize::Small)
                .color(Color::Muted)
        } else {
            Label::new(tr(cx, "pdf_viewer.sidebar.outline", "Outline"))
                .size(LabelSize::Small)
                .color(Color::Muted)
        };

        let entries = if outline.is_empty() {
            // Fall back to a flat page list when the document has no bookmarks.
            (0..state.summary.page_count)
                .map(|index| {
                    let selected = index == current_page;
                    ListItem::new(("pdf-page", index))
                        .toggle_state(selected)
                        .child(
                            Label::new(tr(cx, "pdf_viewer.sidebar.page", "Page {}").replacen(
                                "{}",
                                &(index + 1).to_string(),
                                1,
                            ))
                            .size(LabelSize::Small),
                        )
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.go_to_page(index, window, cx);
                        }))
                        .into_any_element()
                })
                .collect::<Vec<_>>()
        } else {
            outline
                .into_iter()
                .enumerate()
                .map(|(row, item)| {
                    let target = item.page_index;
                    let selected = target == Some(current_page);
                    ListItem::new(("pdf-outline", row))
                        .toggle_state(selected)
                        .child(
                            div()
                                .pl(px(item.depth.min(6) as f32 * 10.0))
                                .child(Label::new(item.title).size(LabelSize::Small)),
                        )
                        .when_some(target, |this, page| {
                            this.on_click(cx.listener(move |view, _, window, cx| {
                                view.go_to_page(page, window, cx);
                            }))
                        })
                        .into_any_element()
                })
                .collect::<Vec<_>>()
        };

        Some(
            v_flex()
                .w(px(220.0))
                .h_full()
                .border_r_1()
                .border_color(cx.theme().colors().border)
                .bg(cx.theme().colors().panel_background)
                .child(div().p_2().child(header))
                .child(
                    div()
                        .id("pdf-sidebar-list")
                        .flex_1()
                        .overflow_scroll()
                        .children(entries),
                )
                .into_any_element(),
        )
    }

    fn render_search_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.search_open {
            return None;
        }
        let state = self.loaded();
        let (match_count, active) = state
            .map(|state| (state.search.matches.len(), state.search.active_match))
            .unwrap_or((0, None));
        let status: SharedString = if match_count == 0 {
            tr(cx, "pdf_viewer.search.no_results", "No results").into()
        } else {
            format!(
                "{}/{}",
                active.map(|index| index + 1).unwrap_or(0),
                match_count
            )
            .into()
        };

        Some(
            h_flex()
                .gap_2()
                .p_2()
                .border_b_1()
                .border_color(cx.theme().colors().border)
                .bg(cx.theme().colors().toolbar_background)
                .child(div().min_w(px(200.0)).child(self.search_editor.clone()))
                .child(
                    Label::new(status)
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    IconButton::new("pdf-prev-match", IconName::ChevronUp)
                        .icon_size(IconSize::Small)
                        .tooltip(|_window, cx| {
                            Tooltip::for_action(
                                tr(cx, "pdf_viewer.search.previous_match", "Previous Match"),
                                &PreviousMatch,
                                cx,
                            )
                        })
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.previous_match(&PreviousMatch, window, cx);
                        })),
                )
                .child(
                    IconButton::new("pdf-next-match", IconName::ChevronDown)
                        .icon_size(IconSize::Small)
                        .tooltip(|_window, cx| {
                            Tooltip::for_action(
                                tr(cx, "pdf_viewer.search.next_match", "Next Match"),
                                &NextMatch,
                                cx,
                            )
                        })
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.next_match(&NextMatch, window, cx);
                        })),
                )
                .into_any_element(),
        )
    }
}

impl Render for PdfView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = self.render_content(window, cx);
        let sidebar = self.render_sidebar(cx);
        let search_bar = self.render_search_bar(cx);

        div()
            .id("PdfView")
            .key_context("PdfViewer")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::previous_page))
            .on_action(cx.listener(Self::next_page))
            .on_action(cx.listener(Self::zoom_in))
            .on_action(cx.listener(Self::zoom_out))
            .on_action(cx.listener(Self::reset_zoom))
            .on_action(cx.listener(Self::fit_width))
            .on_action(cx.listener(Self::fit_page))
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::toggle_search))
            .on_action(cx.listener(Self::next_match))
            .on_action(cx.listener(Self::previous_match))
            .on_action(cx.listener(Self::submit_password))
            .size_full()
            .flex()
            .bg(cx.theme().colors().editor_background)
            .when_some(sidebar, |this, sidebar| this.child(sidebar))
            .child(
                v_flex()
                    .flex_1()
                    .h_full()
                    .when_some(search_bar, |this, bar| this.child(bar))
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .child(
                                // Capture the viewport size so fit-width and
                                // fit-page can size the page to the container.
                                gpui::canvas(
                                    {
                                        let view = cx.entity().downgrade();
                                        move |bounds, window, cx| {
                                            view.update(cx, |view, cx| {
                                                view.set_container_bounds(bounds, window, cx);
                                            })
                                            .ok();
                                        }
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .child(content),
                    ),
            )
    }
}

pub struct PdfToolbarControls {
    pdf_view: Option<WeakEntity<PdfView>>,
    _subscription: Option<gpui::Subscription>,
}

impl PdfToolbarControls {
    pub fn new() -> Self {
        Self {
            pdf_view: None,
            _subscription: None,
        }
    }
}

impl Default for PdfToolbarControls {
    fn default() -> Self {
        Self::new()
    }
}

fn toolbar_action_button<A: gpui::Action>(
    id: &'static str,
    icon: IconName,
    label: SharedString,
    action: A,
    view: &WeakEntity<PdfView>,
    handler: impl Fn(&mut PdfView, &mut Window, &mut Context<PdfView>) + 'static,
) -> IconButton {
    let action = action.boxed_clone();
    let view = view.clone();
    IconButton::new(id, icon)
        .icon_size(IconSize::Small)
        .tooltip(move |_window, cx| Tooltip::for_action(label.clone(), &*action, cx))
        .on_click(move |_, window, cx| {
            if let Some(view) = view.upgrade() {
                view.update(cx, |view, cx| handler(view, window, cx));
            }
        })
}

impl Render for PdfToolbarControls {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(pdf_view) = self.pdf_view.as_ref().and_then(|view| view.upgrade()) else {
            return div().into_any_element();
        };
        let view = pdf_view.read(cx);
        let current_page = view.current_page();
        let page_count = view.page_count();
        let zoom = view.zoom();
        let weak = pdf_view.downgrade();

        let zoom_text = format!("{}%", (zoom * 100.0).round() as i32);
        let page_text: SharedString = if page_count == 0 {
            "—".into()
        } else {
            format!("{} / {}", current_page + 1, page_count).into()
        };

        h_flex()
            .gap_1()
            .child(toolbar_action_button(
                "pdf-sidebar",
                IconName::ListTree,
                tr(cx, "pdf_viewer.toolbar.toggle_sidebar", "Toggle Sidebar").into(),
                ToggleSidebar,
                &weak,
                |view, window, cx| view.toggle_sidebar(&ToggleSidebar, window, cx),
            ))
            .child(toolbar_action_button(
                "pdf-search",
                IconName::MagnifyingGlass,
                tr(cx, "pdf_viewer.toolbar.search", "Search").into(),
                ToggleSearch,
                &weak,
                |view, window, cx| view.toggle_search(&ToggleSearch, window, cx),
            ))
            .child(div().w(px(8.0)))
            .child(toolbar_action_button(
                "pdf-prev",
                IconName::ArrowLeft,
                tr(cx, "pdf_viewer.toolbar.previous_page", "Previous Page").into(),
                PreviousPage,
                &weak,
                |view, window, cx| view.previous_page(&PreviousPage, window, cx),
            ))
            .child(Label::new(page_text).size(LabelSize::Small))
            .child(toolbar_action_button(
                "pdf-next",
                IconName::ArrowRight,
                tr(cx, "pdf_viewer.toolbar.next_page", "Next Page").into(),
                NextPage,
                &weak,
                |view, window, cx| view.next_page(&NextPage, window, cx),
            ))
            .child(div().w(px(8.0)))
            .child(toolbar_action_button(
                "pdf-zoom-out",
                IconName::Dash,
                tr(cx, "pdf_viewer.toolbar.zoom_out", "Zoom Out").into(),
                ZoomOut,
                &weak,
                |view, window, cx| view.zoom_out(&ZoomOut, window, cx),
            ))
            .child(
                Button::new("pdf-zoom-level", zoom_text)
                    .label_size(LabelSize::Small)
                    .tooltip(|_window, cx| {
                        Tooltip::for_action(
                            tr(cx, "pdf_viewer.toolbar.reset_zoom", "Reset Zoom"),
                            &ResetZoom,
                            cx,
                        )
                    })
                    .on_click({
                        let weak = weak.clone();
                        move |_, window, cx| {
                            if let Some(view) = weak.upgrade() {
                                view.update(cx, |view, cx| view.reset_zoom(&ResetZoom, window, cx));
                            }
                        }
                    }),
            )
            .child(toolbar_action_button(
                "pdf-zoom-in",
                IconName::Plus,
                tr(cx, "pdf_viewer.toolbar.zoom_in", "Zoom In").into(),
                ZoomIn,
                &weak,
                |view, window, cx| view.zoom_in(&ZoomIn, window, cx),
            ))
            .child(div().w(px(8.0)))
            .child(toolbar_action_button(
                "pdf-fit-width",
                IconName::ArrowRightLeft,
                tr(cx, "pdf_viewer.toolbar.fit_width", "Fit Width").into(),
                FitWidth,
                &weak,
                |view, window, cx| view.fit_width(&FitWidth, window, cx),
            ))
            .child(toolbar_action_button(
                "pdf-fit-page",
                IconName::Maximize,
                tr(cx, "pdf_viewer.toolbar.fit_page", "Fit Page").into(),
                FitPage,
                &weak,
                |view, window, cx| view.fit_page(&FitPage, window, cx),
            ))
            .into_any_element()
    }
}

impl EventEmitter<ToolbarItemEvent> for PdfToolbarControls {}

impl ToolbarItemView for PdfToolbarControls {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn ItemHandle>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ToolbarItemLocation {
        self.pdf_view = None;
        self._subscription = None;

        if let Some(item) = active_pane_item.and_then(|item| item.downcast::<PdfView>()) {
            self._subscription = Some(cx.observe(&item, |_, _, cx| cx.notify()));
            self.pdf_view = Some(item.downgrade());
            cx.notify();
            return ToolbarItemLocation::PrimaryRight;
        }

        ToolbarItemLocation::Hidden
    }
}
