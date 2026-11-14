//! One-off developer utilities. The JSON prettier is the first tool: it formats,
//! minifies or validates JSON in a dedicated editor panel.
//!
//! Ported from asylum (https://github.com/viniengelage/asylum) at
//! eaab3fb8226637dbd65f8184c1890c5b6556b902, licensed GPL-3.0-or-later.
//! Original copyright notice retained in NOTICE.

use std::fmt;

use editor::Editor;
use gpui::{
    Action as _, App, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, Focusable, Pixels,
    Render, SharedString, Window, actions, px,
};
use i18n::tr;
use serde_json::Value;
use ui::{
    Button, ButtonSize, ButtonStyle, Color, Icon, IconName, IconSize, Label, LabelSize, Tooltip,
    prelude::*,
};
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

actions!(
    toolbox,
    [
        /// Opens the JSON Prettier tool.
        OpenJsonPrettier,
        /// Toggles focus on the JSON Prettier panel.
        ToggleFocus,
        /// Applies the selected mode (format, minify or validate) to the JSON in the panel.
        FormatJson,
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(|workspace, _: &OpenJsonPrettier, window, cx| {
            open(workspace, window, cx);
        });
        workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
            if !workspace.toggle_panel_focus::<JsonPrettierPanel>(window, cx) {
                open(workspace, window, cx);
            }
        });
    })
    .detach();
}

fn open(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    if workspace.panel::<JsonPrettierPanel>(cx).is_none() {
        let panel = cx.new(|cx| JsonPrettierPanel::new(window, cx));
        workspace.add_panel(panel, window, cx);
    }
    workspace.focus_panel::<JsonPrettierPanel>(window, cx);
}

/// Where and why a document failed to parse. The position is what makes the
/// error worth surfacing verbatim, so it leads the message.
#[derive(Debug)]
struct ParseError {
    line: usize,
    column: usize,
    message: String,
}

impl ParseError {
    fn localized(&self, cx: &App) -> String {
        if self.line == 0 {
            return self.message.clone();
        }
        tr(cx, "toolbox.json.error_position", "line {}, column {}: {}")
            .replacen("{}", &self.line.to_string(), 1)
            .replacen("{}", &self.column.to_string(), 1)
            .replacen("{}", &self.message, 1)
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            return write!(formatter, "{}", self.message);
        }
        write!(
            formatter,
            "line {}, column {}: {}",
            self.line, self.column, self.message
        )
    }
}

impl From<serde_json::Error> for ParseError {
    fn from(error: serde_json::Error) -> Self {
        let line = error.line();
        let column = error.column();
        // `serde_json` appends the position to its message; it is shown in front instead.
        let full_message = error.to_string();
        let position_suffix = format!(" at line {line} column {column}");
        let message = full_message
            .strip_suffix(&position_suffix)
            .unwrap_or(&full_message)
            .to_string();
        Self {
            line,
            column,
            message,
        }
    }
}

/// `serde_json`'s `preserve_order` feature is what keeps object keys in the
/// order they were written: formatting a document must not reshuffle it.
fn parse_json(text: &str) -> Result<Value, ParseError> {
    Ok(serde_json::from_str::<Value>(text)?)
}

fn format_json(text: &str) -> Result<String, ParseError> {
    Ok(serde_json::to_string_pretty(&parse_json(text)?)?)
}

fn minify_json(text: &str) -> Result<String, ParseError> {
    Ok(serde_json::to_string(&parse_json(text)?)?)
}

fn describe_size(text: &str, cx: &App) -> String {
    let line_count = text.lines().count().max(1);
    let lines = if line_count == 1 {
        tr(cx, "toolbox.json.one_line", "1 line")
    } else {
        tr(cx, "toolbox.json.line_count", "{} lines").replacen("{}", &line_count.to_string(), 1)
    };
    let byte_count = text.len();
    let size = if byte_count < 1024 {
        format!("{byte_count} B")
    } else if byte_count < 1024 * 1024 {
        format!("{:.1} KB", byte_count as f64 / 1024.)
    } else {
        format!("{:.1} MB", byte_count as f64 / (1024. * 1024.))
    };
    format!("{lines} · {size}")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Format,
    Minify,
    Validate,
}

impl Mode {
    const ALL: [Mode; 3] = [Mode::Format, Mode::Minify, Mode::Validate];

    fn label(self, cx: &App) -> SharedString {
        match self {
            Mode::Format => tr(cx, "toolbox.json.mode_format", "Format"),
            Mode::Minify => tr(cx, "toolbox.json.mode_minify", "Minify"),
            Mode::Validate => tr(cx, "toolbox.json.mode_validate", "Validate"),
        }
        .into()
    }
}

/// What the last attempt made of the editor's contents.
enum Status {
    /// Nothing to say yet: no attempt, or an empty editor.
    Idle,
    Valid {
        summary: SharedString,
    },
    Invalid(SharedString),
}

pub struct JsonPrettierPanel {
    focus_handle: FocusHandle,
    editor: Entity<Editor>,
    mode: Mode,
    status: Status,
    _subscriptions: Vec<gpui::Subscription>,
}

impl JsonPrettierPanel {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            let mut editor = Editor::multi_line(window, cx);
            editor.set_placeholder_text(
                &tr(
                    cx,
                    "toolbox.json.placeholder",
                    "Paste JSON here, or open a file's contents and copy them in.",
                ),
                window,
                cx,
            );
            editor
        });

        Self {
            focus_handle: cx.focus_handle(),
            editor,
            mode: Mode::Format,
            status: Status::Idle,
            _subscriptions: Vec::new(),
        }
    }

    fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.mode = mode;
            cx.notify();
        }
    }

    fn apply_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).text(cx);
        let text = text.trim();

        if text.is_empty() {
            self.status = Status::Idle;
            cx.notify();
            return;
        }

        let rewritten = match self.mode {
            Mode::Format => format_json(text).map(Some),
            Mode::Minify => minify_json(text).map(Some),
            Mode::Validate => parse_json(text).map(|_| None),
        };

        self.status = match rewritten {
            Ok(Some(output)) => {
                let summary = describe_size(&output, cx).into();
                self.editor.update(cx, |editor, cx| {
                    editor.set_text(output, window, cx);
                });
                Status::Valid { summary }
            }
            Ok(None) => Status::Valid {
                summary: describe_size(text, cx).into(),
            },
            Err(error) => Status::Invalid(error.localized(cx).into()),
        };
        cx.notify();
    }

    fn paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.editor.update(cx, |editor, cx| {
            editor.set_text(text, window, cx);
        });
        self.status = Status::Idle;
        cx.notify();
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).text(cx);
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.set_text("", window, cx);
        });
        self.status = Status::Idle;
        cx.notify();
    }

    fn render_mode_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_1()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .children(Mode::ALL.into_iter().enumerate().map(|(index, mode)| {
                Button::new(
                    SharedString::from(format!("json-prettier-mode-{index}")),
                    mode.label(cx),
                )
                .style(ButtonStyle::Subtle)
                .size(ButtonSize::Compact)
                .label_size(LabelSize::Small)
                .selected_style(ButtonStyle::Tinted(ui::TintColor::Accent))
                .toggle_state(self.mode == mode)
                .on_click(cx.listener(move |this, _, _, cx| this.set_mode(mode, cx)))
            }))
    }

    fn render_status(&self, _cx: &App) -> Option<impl IntoElement> {
        let (icon, color, detail) = match &self.status {
            Status::Idle => return None,
            Status::Valid { summary } => (IconName::Check, Color::Success, summary.clone()),
            Status::Invalid(message) => (IconName::XCircle, Color::Error, message.clone()),
        };

        Some(
            h_flex()
                .min_w_0()
                .gap_1()
                .child(Icon::new(icon).size(IconSize::XSmall).color(color))
                .child(
                    Label::new(detail)
                        .size(LabelSize::Small)
                        .color(color)
                        .truncate(),
                ),
        )
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let secondary = |id: &'static str, label: SharedString| {
            Button::new(id, label)
                .style(ButtonStyle::Outlined)
                .size(ButtonSize::Medium)
                .label_size(LabelSize::Small)
        };

        h_flex()
            .flex_none()
            .justify_between()
            .gap_2()
            .px_2()
            .py_1()
            .border_t_1()
            .border_color(cx.theme().colors().border_variant)
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        secondary("paste-json", tr(cx, "toolbox.json.paste", "Paste").into())
                            .on_click(cx.listener(|this, _, window, cx| this.paste(window, cx))),
                    )
                    .child(
                        secondary("clear-json", tr(cx, "toolbox.json.clear", "Clear").into())
                            .on_click(cx.listener(|this, _, window, cx| this.clear(window, cx))),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        secondary("copy-json", tr(cx, "toolbox.json.copy", "Copy").into())
                            .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
                    )
                    .child(
                        Button::new("apply-json", self.mode.label(cx))
                            .style(ButtonStyle::Filled)
                            .size(ButtonSize::Medium)
                            .label_size(LabelSize::Small)
                            .start_icon(
                                Icon::new(IconName::FileCode)
                                    .size(IconSize::XSmall)
                                    .color(Color::Default),
                            )
                            .tooltip(Tooltip::for_action_title(
                                tr(cx, "toolbox.json.apply", "Apply"),
                                &FormatJson,
                            ))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.apply_mode(window, cx)),
                            ),
                    ),
            )
    }
}

impl Render for JsonPrettierPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let card_border = cx.theme().colors().border;
        let card_background = cx.theme().colors().editor_background;

        v_flex()
            .key_context("JsonPrettierPanel")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &FormatJson, window, cx| this.apply_mode(window, cx)))
            .size_full()
            .child(self.render_mode_tabs(cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .p_2()
                    .gap_1()
                    .children(self.render_status(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .rounded_lg()
                            .border_1()
                            .border_color(card_border)
                            .bg(card_background)
                            .child(self.editor.clone()),
                    ),
            )
            .child(self.render_footer(cx))
    }
}

impl Focusable for JsonPrettierPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for JsonPrettierPanel {}

impl Panel for JsonPrettierPanel {
    fn persistent_name() -> &'static str {
        "JsonPrettierPanel"
    }

    fn panel_key() -> &'static str {
        "JsonPrettierPanel"
    }

    /// Typing is the point of this panel, so activating it lands in the editor.
    fn activation_focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Right
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        position == DockPosition::Right
    }

    fn set_position(
        &mut self,
        _position: DockPosition,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn default_size(&self, _window: &Window, _cx: &App) -> Pixels {
        px(480.)
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<IconName> {
        None
    }

    fn icon_tooltip(&self, _window: &Window, cx: &App) -> Option<SharedString> {
        Some(tr(cx, "toolbox.json.title", "JSON Prettier").into())
    }

    fn toggle_action(&self) -> Box<dyn gpui::Action> {
        ToggleFocus.boxed_clone()
    }

    fn activation_priority(&self) -> u32 {
        8
    }
}

#[cfg(test)]
mod tests {
    use super::{format_json, minify_json, parse_json};

    #[test]
    fn formats_inline_json() {
        assert_eq!(
            format_json(r#"{"a":1,"b":[2,3]}"#).unwrap(),
            "{\n  \"a\": 1,\n  \"b\": [\n    2,\n    3\n  ]\n}"
        );
    }

    #[test]
    fn minifies_json() {
        assert_eq!(
            minify_json("{\n  \"a\": 1,\n  \"b\": [2, 3]\n}").unwrap(),
            r#"{"a":1,"b":[2,3]}"#
        );
    }

    #[test]
    fn preserves_key_order() {
        // Alphabetical sorting here would mean the tool rewrites documents it was only
        // asked to indent.
        let formatted = format_json(r#"{"zebra":1,"apple":2}"#).unwrap();
        assert!(
            formatted.find("zebra") < formatted.find("apple"),
            "keys were reordered: {formatted}"
        );
    }

    #[test]
    fn reports_where_parsing_failed() {
        let error = parse_json("{\n  \"a\": 1,\n}").unwrap_err();
        assert_eq!(error.line, 3);
        let message = error.to_string();
        assert!(
            message.starts_with("line 3, column 1: "),
            "unhelpful message: {message}"
        );
        assert!(
            !message.contains(" at line "),
            "position repeated: {message}"
        );
    }
}
