//! Annotation authoring and reading, link navigation, and form filling.
//!
//! All of these are overlays drawn over the page bitmap and, when they change
//! the document, `PdfEdit` requests the worker applies as an incremental update.

use gpui::{
    AnyElement, Context, Focusable as _, IntoElement, MouseButton, MouseDownEvent, MouseUpEvent,
    ParentElement, SharedString, StatefulInteractiveElement as _, Styled, Window, div, px,
};
use i18n::tr;
use ui::{IconButton, IconName, Label, ListItem, Toggleable as _, prelude::*};
use zpdf::{AnnotationSpec, Rect};

use crate::{
    document::{FormWidgetInfo, FormWidgetKind, PageAnnotation},
    edit::{MarkupSource, PdfEdit},
    view::{AnnotationDrag, FormState, NormalizedRect, PdfView},
};

/// Ink colour for the geometric annotation tools, as DeviceRGB in `0..=1`.
const SHAPE_COLOR: (f64, f64, f64) = (0.86, 0.16, 0.16);
/// Stroke width for the geometric annotation tools, in PDF points.
const SHAPE_WIDTH: f64 = 1.5;
/// Font size for authored free text, in PDF points.
const FREE_TEXT_SIZE: f64 = 12.0;

impl PdfView {
    /// The floating authoring palette, shown while annotation mode is on.
    pub(crate) fn render_annotation_palette(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.annotate.open {
            return None;
        }
        let active = self.annotate.tool;
        let tools = Self::annotation_tools(cx)
            .into_iter()
            .map(|tool| {
                let selected = tool == active;
                let button = Button::new(
                    SharedString::from(format!("pdf-annotate-{}", tool.label_key())),
                    tr(cx, tool.label_key(), tool.label_fallback()),
                )
                .label_size(LabelSize::Small)
                .toggle_state(selected);
                button
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.set_annotation_tool(tool, window, cx);
                    }))
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let has_selection = self
            .loaded()
            .is_some_and(|state| state.selected_annotation.is_some());
        let editing = self
            .loaded()
            .is_some_and(|state| state.pending_edit.is_some());

        Some(
            v_flex()
                .gap_1()
                .p_2()
                .border_b_1()
                .border_color(cx.theme().colors().border)
                .bg(cx.theme().colors().toolbar_background)
                .child(h_flex().gap_1().flex_wrap().children(tools))
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(div().min_w(px(180.0)).child(self.annotate.contents.clone()))
                        .child(
                            IconButton::new("pdf-annotate-delete", IconName::Trash)
                                .icon_size(IconSize::Small)
                                .disabled(!has_selection || editing)
                                .on_click(cx.listener(|view, _, window, cx| {
                                    view.delete_selected_annotation(window, cx);
                                })),
                        ),
                )
                .into_any_element(),
        )
    }

    fn annotation_tools(cx: &mut Context<Self>) -> Vec<crate::view::AnnotationTool> {
        let _ = cx;
        crate::view::AnnotationTool::ALL.to_vec()
    }

    /// Overlays for one page: existing annotations, links, form widgets, and the
    /// in-progress drag rectangle.
    pub(crate) fn render_page_overlays(
        &self,
        page: usize,
        layout: &crate::view::PageLayout,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(state) = self.loaded() else {
            return Vec::new();
        };
        let mut overlays = Vec::new();
        let selected = state.selected_annotation;
        let annotate_open = self.annotate.open;

        for annotation in state.annotations.get(&page).into_iter().flatten() {
            if !annotation.is_visible() {
                continue;
            }
            let Some(rect) = self.user_rect_to_fractions(page, annotation.rect) else {
                continue;
            };
            if annotation.is_form_widget() {
                continue;
            }
            if let Some(uri) = annotation.uri.clone() {
                overlays.push(
                    overlay_box(
                        rect,
                        layout,
                        ("pdf-link-uri", combine_id(page, row_id(annotation.id))),
                    )
                    .cursor_pointer()
                    .on_click(cx.listener(move |_view, _, _window, cx| {
                        cx.open_url(&uri);
                    }))
                    .into_any_element(),
                );
                continue;
            }
            if let Some(target) = annotation.dest_page {
                overlays.push(
                    overlay_box(
                        rect,
                        layout,
                        ("pdf-link-dest", combine_id(page, row_id(annotation.id))),
                    )
                    .cursor_pointer()
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.go_to_page(target, window, cx);
                    }))
                    .into_any_element(),
                );
                continue;
            }
            if !annotate_open && annotation.subtype != "Widget" {
                continue;
            }
            let is_selected = selected == Some(annotation.id);
            let id = annotation.id;
            let annotation = annotation.clone();
            let overlay_id = row_id(annotation.id);
            let mut box_element = overlay_box(
                rect,
                layout,
                ("pdf-annotation", combine_id(page, overlay_id)),
            )
            .border_1()
            .border_color(if is_selected {
                cx.theme().colors().border_focused
            } else {
                cx.theme().colors().border_variant
            });
            if is_selected {
                box_element = box_element.bg(gpui::Hsla {
                    a: 0.15,
                    ..cx.theme().colors().search_match_background
                });
            }
            overlays.push(
                box_element
                    .cursor_pointer()
                    .on_click(cx.listener(move |view, _, _window, cx| {
                        view.select_annotation(&annotation, cx);
                    }))
                    .into_any_element(),
            );
            let _ = id;
        }

        for (widget_index, widget) in state
            .summary
            .form_widgets
            .iter()
            .enumerate()
            .filter(|(_, widget)| widget.page == page)
        {
            let Some(rect) = self.user_rect_to_fractions(page, widget.rect) else {
                continue;
            };
            let widget = widget.clone();
            overlays.push(
                overlay_box(
                    rect,
                    layout,
                    ("pdf-widget", combine_id(page, widget_index as u64)),
                )
                .border_1()
                .border_color(cx.theme().colors().border_variant)
                .bg(gpui::Hsla {
                    a: 0.08,
                    ..cx.theme().colors().search_match_background
                })
                .cursor_pointer()
                .on_click(cx.listener(move |view, _, window, cx| {
                    view.on_widget_click(&widget, window, cx);
                }))
                .into_any_element(),
            );
        }

        if let Some(drag) = self.annotate.drag
            && drag.page == page
            && let Some(rect) = self.display_rect_to_fractions(page, drag.rect())
        {
            overlays.push(
                overlay_box(rect, layout, ("pdf-drag", combine_id(page, 0)))
                    .border_1()
                    .border_color(cx.theme().colors().border_focused)
                    .bg(gpui::Hsla {
                        a: 0.2,
                        ..cx.theme().colors().search_match_background
                    })
                    .into_any_element(),
            );
        }

        overlays.extend(self.render_form_editor(page, layout, cx));
        overlays
    }

    /// The inline editor for the form field being filled, positioned over its
    /// widget so it reads like the field itself.
    fn render_form_editor(
        &self,
        page: usize,
        layout: &crate::view::PageLayout,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let FormState {
            active: Some(active),
            input,
        } = &self.form
        else {
            return Vec::new();
        };
        if active.page != page {
            return Vec::new();
        }
        let Some(widget) = self.loaded().and_then(|state| {
            state
                .summary
                .form_widgets
                .iter()
                .find(|widget| widget.page == page && widget.name == active.name)
                .cloned()
        }) else {
            return Vec::new();
        };
        let Some(rect) = self.user_rect_to_fractions(page, widget.rect) else {
            return Vec::new();
        };
        let left = rect.x * layout.width;
        let top = rect.y * layout.height;
        let width = (rect.width * layout.width).max(120.0);
        let height = (rect.height * layout.height).max(28.0);

        vec![
            div()
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(width))
                .h(px(height))
                .flex()
                .gap_1()
                .bg(cx.theme().colors().elevated_surface_background)
                .border_1()
                .border_color(cx.theme().colors().border_focused)
                .child(div().flex_1().child(input.clone()))
                .child(
                    Button::new("pdf-form-apply", tr(cx, "pdf_viewer.form.apply", "Apply"))
                        .label_size(LabelSize::XSmall)
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.commit_form_field(window, cx);
                        })),
                )
                .child(
                    IconButton::new("pdf-form-cancel", IconName::Close)
                        .icon_size(IconSize::Small)
                        .on_click(cx.listener(|view, _, _window, cx| {
                            view.cancel_form_field(cx);
                        })),
                )
                .into_any_element(),
        ]
    }

    /// The sidebar list of annotations, also the place existing ones are deleted
    /// from.
    pub(crate) fn render_annotations_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.loaded()?;
        if state.annotations.is_empty() {
            return None;
        }
        let selected = state.selected_annotation;
        let mut entries = Vec::new();
        let mut pages: Vec<usize> = state.annotations.keys().copied().collect();
        pages.sort_unstable();
        for page in pages {
            for annotation in state.annotations.get(&page).into_iter().flatten() {
                if annotation.is_form_widget() && annotation.contents.is_none() {
                    continue;
                }
                let id = annotation.id;
                let page_for_click = annotation.page;
                let is_selected = selected == Some(id);
                let summary = annotation
                    .contents
                    .clone()
                    .filter(|contents| !contents.is_empty())
                    .unwrap_or_else(|| annotation.subtype.clone());
                let mut row = ListItem::new(("pdf-annotation", row_id(id)))
                    .toggle_state(is_selected)
                    .child(
                        v_flex()
                            .child(Label::new(summary).size(LabelSize::Small))
                            .child(
                                Label::new(match annotation.author.clone() {
                                    Some(author) => format!(
                                        "{} {} · {}",
                                        self.page_label(page_for_click),
                                        annotation.subtype,
                                        author
                                    ),
                                    None => format!(
                                        "{} {}",
                                        self.page_label(page_for_click),
                                        annotation.subtype
                                    ),
                                })
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                            ),
                    );
                let annotation_for_click = annotation.clone();
                row = row.on_click(cx.listener(move |view, _, window, cx| {
                    view.select_annotation(&annotation_for_click, cx);
                    view.go_to_page(page_for_click, window, cx);
                }));
                entries.push(row.into_any_element());
            }
        }
        if entries.is_empty() {
            return None;
        }
        Some(
            v_flex()
                .gap_1()
                .child(
                    Label::new(tr(cx, "pdf_viewer.sidebar.annotations", "Annotations"))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .children(entries)
                .into_any_element(),
        )
    }

    /// Metadata, signatures, and attachments. Read-only: the file itself is the
    /// source of truth and every value here comes straight from it.
    pub(crate) fn render_properties_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.loaded()?;
        let details = state.details.as_ref()?;
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut row = |label: String, value: Option<String>| {
            if let Some(value) = value.filter(|value| !value.is_empty()) {
                rows.push(
                    h_flex()
                        .gap_1()
                        .child(
                            Label::new(label)
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        )
                        .child(Label::new(value).size(LabelSize::XSmall))
                        .into_any_element(),
                );
            }
        };
        row(
            tr(cx, "pdf_viewer.properties.title", "Title"),
            details.title.clone(),
        );
        row(
            tr(cx, "pdf_viewer.properties.author", "Author"),
            details.author.clone(),
        );
        row(
            tr(cx, "pdf_viewer.properties.subject", "Subject"),
            details.subject.clone(),
        );
        row(
            tr(cx, "pdf_viewer.properties.producer", "Producer"),
            details.producer.clone(),
        );
        row(
            tr(cx, "pdf_viewer.properties.creator", "Creator"),
            details.creator.clone(),
        );
        row(
            tr(cx, "pdf_viewer.properties.created", "Created"),
            details.creation_date.clone(),
        );
        row(
            tr(cx, "pdf_viewer.properties.modified", "Modified"),
            details.mod_date.clone(),
        );
        row(
            tr(cx, "pdf_viewer.properties.keywords", "Keywords"),
            details.keywords.clone(),
        );
        if let Some(xmp) = &details.xmp {
            row(
                tr(cx, "pdf_viewer.properties.xmp_title", "XMP title"),
                xmp.title.clone(),
            );
            if !xmp.creators.is_empty() {
                row(
                    tr(cx, "pdf_viewer.properties.xmp_creators", "XMP creators"),
                    Some(xmp.creators.join(", ")),
                );
            }
            row(
                tr(
                    cx,
                    "pdf_viewer.properties.xmp_description",
                    "XMP description",
                ),
                xmp.description.clone(),
            );
            if !xmp.subjects.is_empty() {
                row(
                    tr(cx, "pdf_viewer.properties.xmp_subjects", "XMP subjects"),
                    Some(xmp.subjects.join(", ")),
                );
            }
            row(
                tr(cx, "pdf_viewer.properties.xmp_producer", "XMP producer"),
                xmp.producer.clone(),
            );
            row(
                tr(
                    cx,
                    "pdf_viewer.properties.xmp_creator_tool",
                    "XMP creator tool",
                ),
                xmp.creator_tool.clone(),
            );
            row(
                tr(cx, "pdf_viewer.properties.xmp_modify_date", "XMP modified"),
                xmp.modify_date.clone(),
            );
        }
        if !details.output_intents.is_empty() {
            row(
                tr(cx, "pdf_viewer.properties.output_intents", "Output intents"),
                Some(details.output_intents.join(", ")),
            );
        }
        if details.is_tagged {
            row(
                tr(cx, "pdf_viewer.properties.tagged", "Tagged"),
                Some(tr(cx, "pdf_viewer.properties.yes", "yes")),
            );
        }
        if details.has_optional_content {
            row(
                tr(cx, "pdf_viewer.properties.layers", "Optional content"),
                Some(tr(cx, "pdf_viewer.properties.yes", "yes")),
            );
        }
        for signature in &details.signatures {
            let mut line = format!(
                "{}: {} / {} / {}",
                tr(cx, "pdf_viewer.properties.signature", "Signature"),
                signature.field_name,
                signature.digest,
                signature.crypto
            );
            if let Some(time) = &signature.signing_time {
                line.push_str(" · ");
                line.push_str(time);
            }
            if let Some(reason) = &signature.reason {
                line.push_str(" · ");
                line.push_str(reason);
            }
            if let Some(location) = &signature.location {
                line.push_str(" · ");
                line.push_str(location);
            }
            line.push_str(" · ");
            line.push_str(&signature.coverage);
            rows.push(
                Label::new(line)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .into_any_element(),
            );
        }
        for attachment in &details.attachments {
            let mut line = format!(
                "{}: {}",
                tr(cx, "pdf_viewer.properties.attachment", "Attachment"),
                attachment.name
            );
            if let Some(size) = attachment.size {
                line.push_str(&format!(" ({size} bytes)"));
            }
            if let Some(mime) = &attachment.mime_type {
                line.push_str(" · ");
                line.push_str(mime);
            }
            if let Some(description) = &attachment.description {
                line.push_str(" · ");
                line.push_str(description);
            }
            rows.push(
                Label::new(line)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .into_any_element(),
            );
        }
        if rows.is_empty() {
            return None;
        }
        Some(
            v_flex()
                .gap_1()
                .child(
                    Label::new(tr(cx, "pdf_viewer.sidebar.document", "Document"))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .children(rows)
                .into_any_element(),
        )
    }

    pub(crate) fn set_annotation_tool(
        &mut self,
        tool: crate::view::AnnotationTool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.annotate.tool = tool;
        self.annotate.drag = None;
        self.form.active = None;
        if tool != crate::view::AnnotationTool::Select {
            let handle = self.annotate.contents.focus_handle(cx);
            let _ = window;
            let _ = handle;
        }
        cx.notify();
    }

    pub(crate) fn start_annotation_drag(
        &mut self,
        page: usize,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.annotate.open || event.button != MouseButton::Left {
            return;
        }
        let Some(fractions) = self.pointer_page_fractions(page, event.position) else {
            return;
        };
        let Some(display) = self.fractions_to_display_point_on(page, fractions) else {
            return;
        };

        if self.annotate.tool == crate::view::AnnotationTool::Note {
            self.author_note(page, display, window, cx);
            return;
        }
        if !self.annotate.tool.is_drag_tool() {
            return;
        }
        self.annotate.drag = Some(AnnotationDrag {
            page,
            start: display,
            current: display,
        });
        cx.notify();
    }

    pub(crate) fn update_annotation_drag(
        &mut self,
        page: usize,
        event: &gpui::MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.annotate.drag else {
            return;
        };
        if drag.page != page || event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let Some(fractions) = self.pointer_page_fractions(page, event.position) else {
            return;
        };
        let Some(display) = self.fractions_to_display_point_on(page, fractions) else {
            return;
        };
        if let Some(drag) = self.annotate.drag.as_mut() {
            drag.current = display;
        }
        cx.notify();
    }

    pub(crate) fn finish_annotation_drag(
        &mut self,
        page: usize,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        let Some(drag) = self.annotate.drag else {
            return;
        };
        if drag.page != page {
            return;
        }
        self.annotate.drag = None;
        if !drag.is_significant() {
            cx.notify();
            return;
        }
        self.author_drag(page, drag.rect(), window, cx);
    }

    fn fractions_to_display_point_on(
        &self,
        page: usize,
        fractions: (f64, f64),
    ) -> Option<(f64, f64)> {
        let summary = self.page_summary(page)?;
        Some(crate::view::fractions_to_display_point(
            fractions,
            summary.width as f32,
            summary.height as f32,
        ))
    }

    fn annotation_contents(&self, cx: &App) -> Option<String> {
        let contents = self.annotate.contents.read(cx).text(cx);
        (!contents.trim().is_empty()).then(|| contents.trim().to_owned())
    }

    fn author_note(
        &mut self,
        page: usize,
        point: (f64, f64),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(summary) = self.page_summary(page) else {
            return;
        };
        let (x, y) = summary.from_display_point_abs(point.0, point.1);
        let contents = self.annotation_contents(cx).unwrap_or_default();
        let spec = AnnotationSpec::Note {
            x,
            y,
            contents,
            color: None,
            icon: None,
        };
        self.commit_edits(vec![PdfEdit::Annotation { page, spec }], window, cx);
    }

    fn author_drag(
        &mut self,
        page: usize,
        display: Rect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(summary) = self.page_summary(page) else {
            return;
        };
        let user = summary.from_display_rect(display);
        let contents = self.annotation_contents(cx);
        let tool = self.annotate.tool;

        let edit = if let Some(kind) = tool.markup_kind() {
            PdfEdit::Markup {
                page,
                kind,
                source: MarkupSource::Region(user),
                contents,
            }
        } else {
            let spec = match tool {
                crate::view::AnnotationTool::FreeText => AnnotationSpec::FreeText {
                    rect: user,
                    contents: contents.unwrap_or_default(),
                    size: Some(FREE_TEXT_SIZE),
                    color: None,
                },
                crate::view::AnnotationTool::Rectangle => AnnotationSpec::Square {
                    rect: user,
                    color: SHAPE_COLOR,
                    interior: None,
                    width: SHAPE_WIDTH,
                },
                crate::view::AnnotationTool::Ellipse => AnnotationSpec::Circle {
                    rect: user,
                    color: SHAPE_COLOR,
                    interior: None,
                    width: SHAPE_WIDTH,
                },
                crate::view::AnnotationTool::Line => {
                    let (x1, y1) = summary.from_display_point_abs(display.x0, display.y0);
                    let (x2, y2) = summary.from_display_point_abs(display.x1, display.y1);
                    AnnotationSpec::Line {
                        x1,
                        y1,
                        x2,
                        y2,
                        color: SHAPE_COLOR,
                        width: SHAPE_WIDTH,
                    }
                }
                _ => return,
            };
            PdfEdit::Annotation { page, spec }
        };
        self.commit_edits(vec![edit], window, cx);
    }

    pub(crate) fn select_annotation(
        &mut self,
        annotation: &PageAnnotation,
        cx: &mut Context<Self>,
    ) {
        if let Some(state) = self.loaded_mut() {
            state.selected_annotation = Some(annotation.id);
        }
        cx.notify();
    }

    pub(crate) fn delete_selected_annotation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.loaded() else {
            return;
        };
        let Some(selected) = state.selected_annotation else {
            return;
        };
        let page = state
            .annotations
            .iter()
            .find_map(|(page, annotations)| {
                annotations
                    .iter()
                    .any(|annotation| annotation.id == selected)
                    .then_some(*page)
            })
            .unwrap_or(self.current_page);
        self.commit_edits(
            vec![PdfEdit::DeleteAnnotation {
                page,
                annot: selected,
            }],
            window,
            cx,
        );
    }

    pub(crate) fn on_widget_click(
        &mut self,
        widget: &FormWidgetInfo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if widget.readonly || self.loaded().is_none() {
            return;
        }
        match widget.kind {
            FormWidgetKind::Checkbox | FormWidgetKind::Radio => {
                let Some(on_state) = widget.on_state.clone() else {
                    return;
                };
                let checked = widget.value.as_deref() == Some(on_state.as_str());
                let value = if checked { "Off".to_owned() } else { on_state };
                self.set_form_value(widget.page, &widget.name, value, window, cx);
            }
            FormWidgetKind::Text | FormWidgetKind::Choice => {
                // A stored password value must not be shown back to the user.
                let existing = if widget.password {
                    String::new()
                } else {
                    widget.value.clone().unwrap_or_default()
                };
                self.form.input.update(cx, |input, cx| {
                    input.set_text(&existing, window, cx);
                    input.set_masked(widget.password, window, cx);
                });
                self.form.active = Some(crate::view::ActiveField {
                    page: widget.page,
                    name: widget.name.clone(),
                    multiline: widget.multiline,
                });
                let handle = self.form.input.focus_handle(cx);
                window.focus(&handle, cx);
                cx.notify();
            }
            FormWidgetKind::Signature | FormWidgetKind::Other => {}
        }
    }

    pub(crate) fn commit_form_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active) = self.form.active.clone() else {
            return;
        };
        let raw = self.form.input.read(cx).text(cx);
        let Some(state) = self.loaded() else {
            return;
        };
        // Choice fields store an export value, so a typed display label has to be
        // mapped back to it or every viewer shows the wrong option.
        let value = state
            .summary
            .form_widgets
            .iter()
            .find(|widget| widget.page == active.page && widget.name == active.name)
            .and_then(|widget| {
                widget
                    .options
                    .iter()
                    .position(|option| *option == raw)
                    .map(|position| position.to_string())
            })
            .map_or_else(|| raw.clone(), |_position| raw.clone());
        self.form.active = None;
        self.set_form_value(active.page, &active.name, value, window, cx);
    }

    pub(crate) fn cancel_form_field(&mut self, cx: &mut Context<Self>) {
        self.form.active = None;
        cx.notify();
    }

    fn set_form_value(
        &mut self,
        page: usize,
        name: &str,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = page;
        self.commit_edits(
            vec![PdfEdit::SetField {
                name: name.to_owned(),
                value,
            }],
            window,
            cx,
        );
    }
}

/// An absolutely positioned overlay covering a page fraction rectangle. It is
/// interactive (and so needs an id) because every overlay is a click target.
fn overlay_box(
    rect: NormalizedRect,
    layout: &crate::view::PageLayout,
    id: (&'static str, u64),
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .absolute()
        .left(px(rect.x * layout.width))
        .top(px(rect.y * layout.height))
        .w(px(rect.width * layout.width))
        .h(px(rect.height * layout.height))
}

/// Widget ids are `(object number, generation)`; both are needed for a stable
/// element id.
fn row_id(id: zpdf::ObjectId) -> u64 {
    (u64::from(id.0) << 16) | u64::from(id.1)
}

/// Fold a page index and a per-page row into one id, since element ids take a
/// single integer alongside their tag.
fn combine_id(page: usize, row: u64) -> u64 {
    ((page as u64) << 40) | (row & 0xFF_FFFF_FFFF)
}
