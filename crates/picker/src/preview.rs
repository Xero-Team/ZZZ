use std::ops::Range;
use std::path::PathBuf;

use editor::{Editor, HighlightKey, MultiBuffer, RowHighlightOptions};
use gpui::{App, AppContext, Context, Entity, Task, Window};
use gpui_util::ResultExt as _;
use i18n::tr;
use language::{Bias, Buffer, HighlightedText, ToPoint};
use project::{Project, Symbol};
use rope::Point;
use ui::{IntoElement, Pixels, prelude::*, px};

/// The preview window of a [`Picker`](crate::Picker).
pub struct Preview {
    content: Entity<EditorPreview>,
    pub(crate) layout: Layout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layout {
    Hidden,
    Below,
    Right,
}

impl Preview {
    pub fn new_editor(project: Entity<Project>, window: &mut Window, cx: &mut App) -> Self {
        Self {
            content: cx.new(|cx| EditorPreview::new(project, window, cx)),
            layout: Layout::Right,
        }
    }

    pub fn update(&mut self, update: Update, window: &mut Window, cx: &mut impl AppContext) {
        self.content.update(cx, |content, cx| {
            content.update(update, window, cx);
        });
    }

    pub fn render(&self, cx: &mut App) -> impl IntoElement {
        self.content
            .update(cx, |content, cx| content.render(cx).into_any_element())
    }

    pub(crate) fn clear(&self, cx: &mut App) {
        self.content.update(cx, |content, cx| content.clear(cx));
    }
}

pub enum PreviewSource {
    Path(PathBuf),
    Buffer(Entity<Buffer>),
    Symbol(Symbol),
    Message(HighlightedText),
}

pub struct MatchLocation {
    pub anchor_range: Range<language::Anchor>,
    pub range: Range<usize>,
}

pub struct Update {
    pub source: PreviewSource,
    pub match_location: Option<MatchLocation>,
}

impl Update {
    pub fn from_path(abs_path: PathBuf) -> Self {
        Self {
            source: PreviewSource::Path(abs_path),
            match_location: None,
        }
    }

    pub fn message(message: HighlightedText) -> Self {
        Self {
            source: PreviewSource::Message(message),
            match_location: None,
        }
    }

    pub fn from_buffer(buffer: Entity<Buffer>, highlight: MatchLocation) -> Self {
        Self {
            source: PreviewSource::Buffer(buffer),
            match_location: Some(highlight),
        }
    }

    pub fn from_symbol(symbol: Symbol) -> Self {
        Self {
            source: PreviewSource::Symbol(symbol),
            match_location: None,
        }
    }
}

struct SearchMatchLineHighlight;

struct EditorPreview {
    project: Entity<Project>,
    message: Option<HighlightedText>,
    preview_editor: Entity<Editor>,
    pending_update: Task<()>,
}

impl EditorPreview {
    fn new(project: Entity<Project>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let preview_editor = cx.new(|cx: &mut Context<Editor>| {
            let capability = language::Capability::ReadWrite;
            let multi_buffer = cx.new(|_| MultiBuffer::without_headers(capability));
            let mut editor = Editor::for_multibuffer(multi_buffer, None, window, cx);

            editor.set_read_only(true);
            editor.set_input_enabled(false);
            editor.set_forbid_vertical_scroll(true);
            editor.disable_scrollbars_and_minimap(window, cx);
            editor.disable_inline_diagnostics();
            editor.disable_diagnostics(cx);
            editor.disable_expand_excerpt_buttons(cx);
            editor.disable_mouse_wheel_zoom();
            editor.set_show_gutter(true, cx);
            editor.set_show_line_numbers(true, cx);
            editor.set_show_breakpoints(false, cx);
            editor.set_show_bookmarks(false, cx);
            editor.set_show_code_actions(false, cx);
            editor.set_show_runnables(false, cx);
            editor.set_show_git_diff_gutter(false, cx);
            editor.set_show_wrap_guides(false, cx);
            editor.set_show_indent_guides(false, cx);
            editor.set_show_cursor_when_unfocused(true, cx);
            editor.set_soft_wrap_mode(language::language_settings::SoftWrap::None, cx);
            editor
        });

        let mut this = Self {
            project,
            message: None,
            preview_editor,
            pending_update: Task::ready(()),
        };
        this.clear(cx);
        this
    }

    fn clear(&mut self, cx: &App) {
        self.message = Some(HighlightedText {
            text: tr(cx, "picker.preview.no_results", "No results to preview").into(),
            highlights: Vec::new(),
        });
    }

    fn update(&mut self, update: Update, window: &mut Window, cx: &mut Context<Self>) {
        match update.source {
            PreviewSource::Path(abs_path) => {
                self.update_from_path(abs_path, update.match_location, window, cx);
            }
            PreviewSource::Buffer(buffer) => {
                self.update_from_buffer(buffer, update.match_location, window, cx);
                cx.notify();
            }
            PreviewSource::Symbol(symbol) => self.update_from_symbol(symbol, window, cx),
            PreviewSource::Message(message) => {
                self.message = Some(message);
                cx.notify();
            }
        }
    }

    fn update_from_path(
        &mut self,
        abs_path: PathBuf,
        highlight: Option<MatchLocation>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open_task = self.project.update(cx, |project, cx| {
            match project.project_path_for_absolute_path(&abs_path, cx) {
                Some(project_path) => {
                    if let Some(buffer) = project.get_open_buffer(&project_path, cx) {
                        Task::ready(Ok(buffer))
                    } else {
                        project.open_buffer(project_path, cx)
                    }
                }
                None => project.open_local_buffer(&abs_path, cx),
            }
        });

        self.pending_update = cx.spawn_in(window, async move |this, cx| {
            let Some(buffer) = open_task.await.log_err() else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                this.update_from_buffer(buffer, highlight, window, cx);
                cx.notify();
            })
            .log_err();
        });
    }

    fn update_from_symbol(&mut self, symbol: Symbol, window: &mut Window, cx: &mut Context<Self>) {
        let open_task = self.project.update(cx, |project, cx| {
            project.open_buffer_for_symbol(&symbol, cx)
        });

        self.pending_update = cx.spawn_in(window, async move |this, cx| {
            let Some(buffer) = open_task.await.log_err() else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                let snapshot = buffer.read(cx).text_snapshot();
                let start = snapshot.clip_point_utf16(symbol.range.start, Bias::Left);
                let end = snapshot.clip_point_utf16(symbol.range.end, Bias::Left);
                let highlight = MatchLocation {
                    anchor_range: snapshot.anchor_before(start)..snapshot.anchor_after(end),
                    range: snapshot.point_utf16_to_offset(start)
                        ..snapshot.point_utf16_to_offset(end),
                };
                this.update_from_buffer(buffer, Some(highlight), window, cx);
                cx.notify();
            })
            .log_err();
        });
    }

    fn update_from_buffer(
        &mut self,
        buffer: Entity<Buffer>,
        highlight: Option<MatchLocation>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.message = None;

        const MIN_LINE_HEIGHT_PX: Pixels = px(6.0);
        const MARGIN: u32 = 2;
        let max_visible_rows =
            (window.viewport_size().height / MIN_LINE_HEIGHT_PX).ceil() as u32 + MARGIN;

        self.preview_editor.update(cx, |editor, cx| {
            let focus_row = highlight
                .as_ref()
                .map(|location| {
                    location
                        .anchor_range
                        .start
                        .to_point(&buffer.read(cx).text_snapshot())
                        .row
                })
                .unwrap_or_default();

            let multi_buffer = editor.buffer().clone();
            multi_buffer.update(cx, |multi_buffer, cx| {
                multi_buffer.clear(cx);
                multi_buffer.set_excerpts_for_buffer(
                    buffer,
                    [Point::new(focus_row, 0)..Point::new(focus_row, 0)],
                    max_visible_rows,
                    cx,
                );
            });

            editor.clear_row_highlights::<SearchMatchLineHighlight>();
            editor.clear_background_highlights(HighlightKey::SearchWithinRange, cx);

            let Some(highlight) = highlight else {
                return;
            };

            let multi_buffer_snapshot = multi_buffer.read(cx).snapshot(cx);
            let Some(range) = multi_buffer_snapshot
                .anchor_in_excerpt(highlight.anchor_range.start)
                .zip(multi_buffer_snapshot.anchor_in_excerpt(highlight.anchor_range.end))
                .map(|(start, end)| start..end)
            else {
                return;
            };

            editor.highlight_rows::<SearchMatchLineHighlight>(
                range.clone(),
                cx.theme().colors().editor_active_line_background,
                RowHighlightOptions::default(),
                cx,
            );

            editor.highlight_background(
                HighlightKey::SearchWithinRange,
                &[range],
                |_, theme| theme.colors().search_match_background,
                cx,
            );
        });

        self.scroll_to_focus_match(window, cx);
    }

    fn scroll_to_focus_match(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preview_editor.update(cx, |editor, cx| {
            let display_snapshot = editor.display_snapshot(cx);
            let buffer_snapshot = display_snapshot.buffer_snapshot();
            let search_range = buffer_snapshot.anchor_before(multi_buffer::MultiBufferOffset(0))
                ..buffer_snapshot.anchor_after(buffer_snapshot.len());

            let Some((range, _)) = editor
                .background_highlights_in_range(search_range, &display_snapshot, cx.theme())
                .into_iter()
                .next()
            else {
                return;
            };

            let target_row = range.start.row().0 as f64;
            let centered_y = editor
                .visible_line_count()
                .map_or(target_row, |visible_lines| {
                    (target_row - (visible_lines - 1.) / 2.).max(0.)
                });

            let start_column = range.start.column() as f64;
            let end_column = range.end.column() as f64;
            let centered_x = editor.visible_column_count().map_or(0., |visible_columns| {
                let min_x_for_end = (end_column - visible_columns + 1.).max(0.);
                min_x_for_end.min(start_column)
            });

            editor.set_forbid_vertical_scroll(false);
            editor.set_scroll_position(gpui::Point::new(centered_x, centered_y), window, cx);
            editor.set_forbid_vertical_scroll(true);
        });
    }

    fn render(&self, cx: &App) -> impl IntoElement {
        if let Some(message) = &self.message {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .font_ui(cx)
                .text_ui(cx)
                .text_color(Color::Muted.color(cx))
                .child(
                    gpui::StyledText::new(message.text.clone())
                        .with_highlights(message.highlights.iter().cloned()),
                )
                .into_any_element();
        }

        div()
            .flex_1()
            .overflow_hidden()
            .child(self.preview_editor.clone())
            .into_any_element()
    }
}
