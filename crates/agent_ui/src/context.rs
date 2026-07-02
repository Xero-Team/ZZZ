use crate::mention_set::Mention;
use gpui::{AppContext as _, Entity, Task};
use i18n as app_i18n;
use language_model::{LanguageModelImage, LanguageModelRequestMessage, MessageContent};
use ui::App;
use util::ResultExt as _;

use crate::mention_set::MentionSet;

#[derive(Debug, Clone, Default)]
pub struct LoadedContext {
    pub text: String,
    pub images: Vec<LanguageModelImage>,
    pub images_attached_by_user_text: String,
}

impl LoadedContext {
    pub fn add_to_request_message(&self, request_message: &mut LanguageModelRequestMessage) {
        if !self.text.is_empty() {
            request_message
                .content
                .push(MessageContent::Text(self.text.to_string()));
        }

        if !self.images.is_empty() {
            // Some providers only support image parts after an initial text part
            if request_message.content.is_empty() {
                let image_intro = if self.images_attached_by_user_text.is_empty() {
                    "Images attached by user:"
                } else {
                    &self.images_attached_by_user_text
                };
                request_message
                    .content
                    .push(MessageContent::Text(image_intro.to_owned()));
            }

            for image in &self.images {
                request_message
                    .content
                    .push(MessageContent::Image(image.clone()))
            }
        }
    }
}

/// Loads and formats a collection of contexts.
pub fn load_context(mention_set: &Entity<MentionSet>, cx: &mut App) -> Task<Option<LoadedContext>> {
    let task = mention_set.update(cx, |mention_set, cx| mention_set.contents(true, cx));
    let images_attached_by_user_text = app_i18n::tr(
        cx,
        "agent_ui.context.images_attached_by_user",
        "Images attached by user:",
    );
    let items_attached_by_user_text = app_i18n::tr(
        cx,
        "agent_ui.context.items_attached_by_user",
        "The following items were attached by the user.",
    );
    cx.background_spawn(async move {
        let mentions = task.await.log_err()?;
        let mut loaded_context = LoadedContext::default();
        loaded_context.images_attached_by_user_text = images_attached_by_user_text;
        loaded_context.text.push_str(&items_attached_by_user_text);
        loaded_context.text.push('\n');
        for (_, (_, mention)) in mentions {
            match mention {
                Mention::Text { content, .. } => {
                    loaded_context.text.push_str(&content);
                }
                Mention::Image(mention_image) => loaded_context.images.push(LanguageModelImage {
                    source: mention_image.data,
                }),
                Mention::Link => {}
            }
        }
        Some(loaded_context)
    })
}
