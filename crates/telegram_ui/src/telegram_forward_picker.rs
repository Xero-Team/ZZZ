//! A modal picker for choosing the destination chat when forwarding messages.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use fuzzy::{StringMatch, StringMatchCandidate, match_strings};
use gpui::{App, Context, DismissEvent, SharedString, Task, Window};
use picker::{Picker, PickerDelegate};
use telegram::{ChatSnapshot, Command};
use ui::prelude::*;
use ui::{ListItem, ListItemSpacing};
use workspace::Workspace;

use crate::telegram_panel::{GlobalTelegramEngine, TelegramPanel};

/// Opens the forward picker, forwarding the selected messages to the chosen chat.
pub fn open(
    workspace: &mut Workspace,
    source_chat_id: i64,
    message_ids: Vec<i32>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(panel) = workspace.panel::<TelegramPanel>(cx) else {
        return;
    };
    let chats = panel.read(cx).chats().to_vec();
    workspace.toggle_modal(window, cx, move |window, cx| {
        let delegate = ForwardPickerDelegate::new(source_chat_id, message_ids, chats);
        Picker::uniform_list(delegate, window, cx)
    });
}

/// A picker delegate that lists chats as forward destinations.
pub struct ForwardPickerDelegate {
    source_chat_id: i64,
    message_ids: Vec<i32>,
    chats: Vec<ChatSnapshot>,
    matches: Vec<StringMatch>,
    selected_index: usize,
}

impl ForwardPickerDelegate {
    fn new(source_chat_id: i64, message_ids: Vec<i32>, chats: Vec<ChatSnapshot>) -> Self {
        let matches = chats
            .iter()
            .enumerate()
            .map(|(index, chat)| StringMatch {
                candidate_id: index,
                score: 0.,
                positions: Vec::new(),
                string: chat.title.clone(),
            })
            .collect();
        Self {
            source_chat_id,
            message_ids,
            chats,
            matches,
            selected_index: 0,
        }
    }
}

impl PickerDelegate for ForwardPickerDelegate {
    type ListItem = ListItem;

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        index: usize,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) {
        self.selected_index = index;
    }

    fn placeholder_text(&self, _window: &mut Window, cx: &mut App) -> Arc<str> {
        i18n::tr(cx, "telegram_panel.forward.placeholder", "Forward to chat…").into()
    }

    fn update_matches(
        &mut self,
        query: String,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let candidates: Vec<StringMatchCandidate> = self
            .chats
            .iter()
            .enumerate()
            .map(|(index, chat)| StringMatchCandidate::new(index, &chat.title))
            .collect();
        let executor = cx.background_executor().clone();
        let cancel_flag = AtomicBool::new(false);
        let matches = match_strings(
            &candidates,
            &query,
            false,
            true,
            100,
            &cancel_flag,
            executor,
        );
        self.matches = futures::executor::block_on(matches);
        self.selected_index = 0;
        Task::ready(())
    }

    fn confirm(&mut self, _secondary: bool, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(matched) = self.matches.get(self.selected_index) else {
            return;
        };
        let Some(target) = self.chats.get(matched.candidate_id) else {
            return;
        };
        let engine = cx.global::<GlobalTelegramEngine>().0.clone();
        engine.send(Command::ForwardMessages {
            source_chat_id: self.source_chat_id,
            message_ids: self.message_ids.clone(),
            target_chat_id: target.id,
        });
        cx.emit(DismissEvent);
    }

    fn dismissed(&mut self, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        cx.emit(DismissEvent);
    }

    fn render_match(
        &self,
        index: usize,
        selected: bool,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let matched = self.matches.get(index)?;
        let chat = self.chats.get(matched.candidate_id)?;
        Some(
            ListItem::new(index)
                .inset(true)
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(selected)
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            div()
                                .flex_none()
                                .size_6()
                                .rounded_full()
                                .bg(cx.theme().colors().element_active)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    Label::new(chat.avatar_initials.clone())
                                        .size(LabelSize::XSmall),
                                ),
                        )
                        .child(Label::new(SharedString::from(chat.title.clone()))),
                ),
        )
    }
}
