use gpui::{App, AppContext, ClipboardItem, Context, Entity, Window, div, prelude::*};
use language::Buffer;
use markdown::{Markdown, MarkdownElement, MarkdownFont, MarkdownStyle};

use crate::outputs::OutputContent;

pub struct MarkdownView {
    markdown: Entity<Markdown>,
    clipboard_html: Option<String>,
}

impl MarkdownView {
    pub fn from(text: String, cx: &mut Context<Self>) -> Self {
        Self::from_with_clipboard_html(text, None, cx)
    }

    pub fn from_with_clipboard_html(
        text: String,
        clipboard_html: Option<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        let markdown = cx.new(|cx| Markdown::new(text.clone().into(), None, None, cx));

        Self {
            markdown,
            clipboard_html,
        }
    }
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
