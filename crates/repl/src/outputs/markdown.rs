use gpui::{App, AppContext, ClipboardItem, Context, Entity, Window, div, prelude::*};
use language::Buffer;
use markdown::{Markdown, MarkdownElement, MarkdownFont, MarkdownOptions, MarkdownStyle};

use crate::outputs::OutputContent;

pub struct MarkdownView {
    markdown: Entity<Markdown>,
    clipboard_html: Option<String>,
}

impl MarkdownView {
    pub fn from(text: String, cx: &mut Context<Self>) -> Self {
        Self::new(
            text,
            None,
            MarkdownOptions {
                render_math: true,
                ..Default::default()
            },
            cx,
        )
    }

    pub fn from_latex(text: String, cx: &mut Context<Self>) -> Self {
        let trimmed = text.trim();
        let markdown = if has_outer_math_delimiters(trimmed) {
            trimmed.to_owned()
        } else {
            format!("$$\n{trimmed}\n$$")
        };
        Self::from(markdown, cx)
    }

    pub fn from_with_clipboard_html(
        text: String,
        clipboard_html: Option<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new(text, clipboard_html, MarkdownOptions::default(), cx)
    }

    fn new(
        text: String,
        clipboard_html: Option<String>,
        options: MarkdownOptions,
        cx: &mut Context<Self>,
    ) -> Self {
        let markdown =
            cx.new(|cx| Markdown::new_with_options(text.clone().into(), None, None, options, cx));

        Self {
            markdown,
            clipboard_html,
        }
    }
}

fn has_outer_math_delimiters(text: &str) -> bool {
    let display_dollars = text.len() >= 4 && text.starts_with("$$") && text.ends_with("$$");
    let inline_dollars = text.len() >= 2
        && text.starts_with('$')
        && !text.starts_with("$$")
        && text.ends_with('$')
        && !text.ends_with("$$");
    let display_backslashes = text.len() >= 4 && text.starts_with(r"\[") && text.ends_with(r"\]");
    let inline_backslashes = text.len() >= 4 && text.starts_with(r"\(") && text.ends_with(r"\)");

    display_dollars || inline_dollars || display_backslashes || inline_backslashes
}

impl OutputContent for MarkdownView {
    fn clipboard_content(&self, _window: &Window, cx: &App) -> Option<ClipboardItem> {
        let item = self
            .markdown
            .read(cx)
            .rendered_clipboard_item_for_document(cx);
        if let Some(clipboard_html) = self.clipboard_html.as_ref() {
            Some(ClipboardItem::new_string_with_html(
                item.text().unwrap_or_default(),
                clipboard_html.clone(),
            ))
        } else {
            Some(item)
        }
    }

    fn has_clipboard_content(&self, _window: &Window, _cx: &App) -> bool {
        true
    }

    fn has_buffer_content(&self, _window: &Window, _cx: &App) -> bool {
        true
    }

    fn buffer_content(&mut self, _: &mut Window, cx: &mut App) -> Option<Entity<Buffer>> {
        let source = self.markdown.read(cx).source().to_owned();
        let buffer = cx.new(|cx| {
            let mut buffer =
                Buffer::local(source.clone(), cx).with_language(language::PLAIN_TEXT.clone(), cx);
            buffer.set_capability(language::Capability::ReadOnly, cx);
            buffer
        });
        Some(buffer)
    }
}

impl Render for MarkdownView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let style = markdown_style(window, cx);
        div()
            .w_full()
            .child(MarkdownElement::new(self.markdown.clone(), style))
    }
}

fn markdown_style(window: &Window, cx: &App) -> MarkdownStyle {
    MarkdownStyle::themed(MarkdownFont::Editor, window, cx)
}
