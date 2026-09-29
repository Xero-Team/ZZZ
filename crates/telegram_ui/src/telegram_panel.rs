use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use editor::{Editor, EditorEvent, EditorMode};
use gpui::{
    Action, AnyElement, App, ClickEvent, Context, ElementId, Entity, EventEmitter, FocusHandle,
    Focusable, FollowMode, Global, IntoElement, ListAlignment, ListState, ParentElement, Render,
    RenderImage, SharedString, Styled, Subscription, Task, TextStyleRefinement, WeakEntity, Window,
    actions, div, img, list, px, relative,
};
use i18n::tr;
use language::Buffer;
use markdown::{Markdown, MarkdownElement, MarkdownFont, MarkdownStyle};
use multi_buffer::MultiBuffer;
use settings::Settings as _;
use telegram::{
    AuthState, ChatSnapshot, Command, ConnectionState, DownloadState, EngineConfig, EngineError,
    EngineHandle, MediaKind, MediaSnapshot, MessageSnapshot, TelegramCredentials, ViewModel,
};
use time::UtcOffset;
use time_format::{TimestampFormat, format_local_timestamp, format_time};
use ui::prelude::*;
use ui::{
    AvatarStyle, Button, ButtonCommon, Callout, Clickable, CountBadge, Disableable, Disclosure,
    Icon, IconButton, Label, LabelCommon, ListItem, ListItemSpacing, Severity, Tab, Tooltip,
};
use ui::{ContextMenu, ContextMenuEntry, right_click_menu};
use workspace::dock::{DockPosition, Panel, PanelEvent};
use workspace::notifications::NotificationId;
use workspace::{HideStatusItem, Toast, Workspace};

use crate::telegram_panel_settings::{TelegramPanelSettings, TelegramSettings};

actions!(
    telegram_panel,
    [ToggleFocus, Send, NextChat, PreviousChat, Refresh, Back,]
);

const PANEL_KEY: &str = "TelegramPanel";
const COMPOSER_MIN_LINES: usize = 1;
const COMPOSER_MAX_LINES: usize = 8;
const SAME_SENDER_GROUP_SECONDS: i64 = 5 * 60;
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(150);

/// The application-global Telegram engine.
pub struct GlobalTelegramEngine(pub Arc<EngineHandle>);

impl Global for GlobalTelegramEngine {}

/// Reads the engine configuration from settings. The engine is not started here.
pub(crate) fn engine_config(cx: &App) -> EngineConfig {
    let dedicated = TelegramSettings::try_get(cx).and_then(|settings| settings.proxy.clone());
    let proxy_url = dedicated.or_else(|| {
        client::ProxySettings::try_get(cx)
            .and_then(|settings| settings.proxy_url())
            .map(|url| url.to_string())
    });
    EngineConfig {
        session_dir: paths::telegram_dir().join("session"),
        cache_dir: paths::telegram_cache_dir().clone(),
        credentials: TelegramCredentials::resolve(),
        proxy_url,
    }
}

fn engine(cx: &App) -> Arc<EngineHandle> {
    cx.global::<GlobalTelegramEngine>().0.clone()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ViewMode {
    ChatList,
    Conversation,
}

/// Per-message layout metadata computed when the history changes.
#[derive(Clone, Debug, Default)]
struct MessageLayout {
    day_separator: Option<String>,
    show_sender: bool,
}

/// The Telegram panel view.
pub struct TelegramPanel {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    fs: Arc<dyn fs::Fs>,
    engine: Arc<EngineHandle>,
    view_model: Arc<ViewModel>,
    mode: ViewMode,
    started: bool,
    chat_list_state: ListState,
    history_list_state: ListState,
    search_list_state: ListState,
    chat_count: usize,
    search_count: usize,
    history_ids: Vec<i32>,
    history_layout: Arc<Vec<MessageLayout>>,
    markdown_cache: HashMap<(i64, i32), Entity<Markdown>>,
    composer: Entity<Editor>,
    composer_expanded: bool,
    phone_editor: Entity<Editor>,
    code_editor: Entity<Editor>,
    password_editor: Entity<Editor>,
    search_editor: Entity<Editor>,
    search_open: bool,
    qr_cache: Option<(String, Arc<RenderImage>)>,
    _composer_subscription: Subscription,
    _search_subscription: Subscription,
    _search_task: Option<Task<()>>,
    _snapshot_task: Task<()>,
    voice_playback: Option<audio::PlaybackHandle>,
    playing_voice: Option<i32>,
    /// Media attachments whose card is expanded, keyed by chat and message.
    expanded_media: HashSet<(i64, i32)>,
    last_unread_total: u32,
    /// The selected chat of the previous snapshot, to detect chat switches.
    selected_chat: Option<i64>,
    /// A message to reveal once its chat history has loaded.
    pending_scroll: Option<(i64, i32)>,
}

impl TelegramPanel {
    /// Loads the panel for a workspace.
    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: gpui::AsyncWindowContext,
    ) -> anyhow::Result<Entity<Self>> {
        let panel = workspace.update_in(&mut cx, |workspace, window, cx| {
            Self::new(workspace, window, cx)
        })?;
        anyhow::Ok(panel)
    }

    fn new(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) -> Entity<Self> {
        let fs = workspace.app_state().fs.clone();
        cx.new(|cx| Self::new_inner(workspace.weak_handle(), fs, window, cx))
    }

    fn new_inner(
        workspace: WeakEntity<Workspace>,
        fs: Arc<dyn fs::Fs>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let engine = engine(cx);
        let mut snapshot_rx = engine.subscribe();

        let composer = cx.new(|cx| build_composer(window, cx));
        let phone_editor = cx.new(|cx| {
            single_line_editor(
                tr(cx, "telegram_panel.auth.phone_placeholder", "Phone number"),
                window,
                cx,
            )
        });
        let code_editor = cx.new(|cx| {
            single_line_editor(
                tr(cx, "telegram_panel.auth.code_placeholder", "Login code"),
                window,
                cx,
            )
        });
        let password_editor = cx.new(|cx| {
            single_line_editor(
                tr(cx, "telegram_panel.auth.password_placeholder", "Password"),
                window,
                cx,
            )
        });
        let search_editor = cx.new(|cx| {
            single_line_editor(
                tr(cx, "telegram_panel.search_placeholder", "Search"),
                window,
                cx,
            )
        });

        let _composer_subscription = cx.subscribe_in(
            &composer,
            window,
            |this, _editor, event: &EditorEvent, _window, cx| {
                if matches!(event, EditorEvent::BufferEdited) {
                    if let Some(chat_id) = this.view_model.selected_chat {
                        let text = this.composer.read(cx).text(cx);
                        this.engine.send(Command::SetDraft { chat_id, text });
                    }
                    cx.notify();
                }
            },
        );

        let _search_subscription = cx.subscribe_in(
            &search_editor,
            window,
            |this, _editor, event: &EditorEvent, window, cx| {
                if matches!(event, EditorEvent::BufferEdited) {
                    this.schedule_search(window, cx);
                }
            },
        );

        let _snapshot_task = cx.spawn_in(window, async move |this, cx| {
            loop {
                if snapshot_rx.changed().await.is_err() {
                    break;
                }
                let snapshot = snapshot_rx.borrow_and_update().clone();
                if this
                    .update_in(cx, |this, window, cx| {
                        this.apply_snapshot(snapshot, window, cx)
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        Self {
            focus_handle: cx.focus_handle(),
            workspace,
            fs,
            engine,
            view_model: Arc::new(ViewModel::default()),
            mode: ViewMode::ChatList,
            started: false,
            chat_list_state: ListState::new(0, ListAlignment::Top, px(100.)),
            history_list_state: ListState::new(0, ListAlignment::Bottom, px(2048.)),
            search_list_state: ListState::new(0, ListAlignment::Top, px(100.)),
            chat_count: 0,
            search_count: 0,
            history_ids: Vec::new(),
            history_layout: Arc::new(Vec::new()),
            markdown_cache: HashMap::new(),
            composer,
            composer_expanded: false,
            phone_editor,
            code_editor,
            password_editor,
            search_editor,
            search_open: false,
            qr_cache: None,
            _composer_subscription,
            _search_subscription,
            _search_task: None,
            _snapshot_task,
            voice_playback: None,
            playing_voice: None,
            expanded_media: HashSet::new(),
            last_unread_total: 0,
            selected_chat: None,
            pending_scroll: None,
        }
    }

    fn apply_snapshot(
        &mut self,
        snapshot: Arc<ViewModel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_chats = self.view_model.chats.clone();
        self.view_model = snapshot;
        self.notify_new_messages(&previous_chats, cx);
        self.sync_chat_list();
        self.sync_history();
        self.rebuild_markdown(cx);
        self.prune_expanded_media();
        self.sync_draft(window, cx);
        self.reveal_pending_message();
        cx.notify();
    }

    /// Loads the stored draft into the composer when the chat changes, so text
    /// never leaks from one chat into another.
    fn sync_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.view_model.selected_chat;
        if selected == self.selected_chat {
            return;
        }
        self.selected_chat = selected;
        let draft = self.view_model.draft.clone();
        if self.composer.read(cx).text(cx) != draft {
            self.composer
                .update(cx, |editor, cx| editor.set_text(draft, window, cx));
        }
    }

    /// Drops expanded-media entries for messages no longer in view so the set
    /// does not grow without bound.
    fn prune_expanded_media(&mut self) {
        if self.expanded_media.is_empty() {
            return;
        }
        let live: HashSet<(i64, i32)> = self
            .view_model
            .history
            .iter()
            .map(|message| (message.chat_id, message.id))
            .collect();
        self.expanded_media.retain(|key| live.contains(key));
    }

    /// Scrolls the history list to a message requested from search once it has
    /// loaded into history.
    fn reveal_pending_message(&mut self) {
        let Some((chat_id, message_id)) = self.pending_scroll else {
            return;
        };
        if self.view_model.selected_chat != Some(chat_id) {
            return;
        }
        if let Some(index) = self
            .view_model
            .history
            .iter()
            .position(|message| message.id == message_id)
        {
            self.history_list_state.scroll_to_reveal_item(index);
            self.pending_scroll = None;
        }
    }

    fn notify_new_messages(&mut self, previous_chats: &[ChatSnapshot], cx: &mut Context<Self>) {
        let previous_total = self.last_unread_total;
        self.last_unread_total = self.view_model.unread_total;
        if !self.view_model.auth.is_signed_in() {
            return;
        }
        if self.view_model.unread_total <= previous_total {
            return;
        }
        if cx.active_window().is_none() {
            return;
        }
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let Some(chat) = self
            .view_model
            .chats
            .iter()
            .filter(|chat| !chat.muted)
            .find(|chat| {
                previous_chats
                    .iter()
                    .find(|previous| previous.id == chat.id)
                    .is_none_or(|previous| chat.unread_count > previous.unread_count)
            })
            .cloned()
        else {
            return;
        };
        let title = chat.title.clone();
        let body = chat_preview_text(&chat, cx);
        let open_label = tr(cx, "telegram_panel.toast.open", "Open");
        workspace.update(cx, |workspace, cx| {
            let toast = Toast::new(
                NotificationId::unique::<TelegramPanel>(),
                format!("{title}: {body}"),
            )
            .on_click(open_label, move |_, cx| {
                cx.dispatch_action(&ToggleFocus);
            })
            .autohide();
            workspace.show_toast(toast, cx);
        });
    }

    fn sync_chat_list(&mut self) {
        let new_count = self.view_model.chats.len();
        if new_count != self.chat_count {
            self.chat_list_state.splice(0..self.chat_count, new_count);
            self.chat_count = new_count;
        }
    }

    fn sync_history(&mut self) {
        let new_ids: Vec<i32> = self
            .view_model
            .history
            .iter()
            .map(|message| message.id)
            .collect();
        let previous = std::mem::replace(&mut self.history_ids, new_ids.clone());
        if previous == new_ids {
            self.history_layout = Arc::new(build_history_layout(&self.view_model.history));
            return;
        }
        let previous_len = previous.len();
        let new_len = new_ids.len();
        if previous.is_empty() {
            self.history_list_state.splice(0..0, new_len);
        } else if new_ids.starts_with(&previous) {
            self.history_list_state
                .splice(previous_len..previous_len, new_len - previous_len);
        } else if new_ids.ends_with(&previous) {
            self.history_list_state.splice(0..0, new_len - previous_len);
        } else {
            self.history_list_state.splice(0..previous_len, new_len);
        }
        self.history_list_state.remeasure_items(0..new_len);
        self.history_layout = Arc::new(build_history_layout(&self.view_model.history));
    }

    /// Keeps the markdown entity for each formatted message in sync with its
    /// source, replacing the entity when a message is edited.
    fn rebuild_markdown(&mut self, cx: &mut Context<Self>) {
        let mut sources: Vec<((i64, i32), SharedString)> = Vec::new();
        for message in &self.view_model.history {
            if let Some(markdown) = &message.markdown {
                sources.push((
                    (message.chat_id, message.id),
                    SharedString::from(markdown.clone()),
                ));
            }
        }
        for message in self
            .view_model
            .search_results
            .iter()
            .filter_map(|hit| hit.message.as_ref())
        {
            if let Some(markdown) = &message.markdown {
                sources.push((
                    (message.chat_id, message.id),
                    SharedString::from(markdown.clone()),
                ));
            }
        }

        let live: HashSet<(i64, i32)> = sources.iter().map(|(key, _)| *key).collect();
        for (key, source) in sources {
            let needs_replace = self
                .markdown_cache
                .get(&key)
                .is_none_or(|entity| entity.read(cx).source() != source.as_str());
            if needs_replace {
                let entity = cx.new(|cx| Markdown::new(source, None, None, cx));
                self.markdown_cache.insert(key, entity);
            }
        }
        self.markdown_cache.retain(|key, _| live.contains(key));
    }

    fn start_engine(&mut self) {
        if !self.started {
            self.started = true;
        }
        self.engine.send(Command::Start);
    }

    /// The current chat list, used by the forward picker.
    pub fn chats(&self) -> &[ChatSnapshot] {
        &self.view_model.chats
    }

    fn forward_message(
        &mut self,
        chat_id: i64,
        message_id: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if message_id == 0 {
            return;
        }
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        workspace.update(cx, |workspace, cx| {
            crate::telegram_forward_picker::open(workspace, chat_id, vec![message_id], window, cx);
        });
    }

    fn open_chat(&mut self, chat_id: i64, cx: &mut Context<Self>) {
        self.start_engine();
        self.mode = ViewMode::Conversation;
        self.engine.send(Command::SelectChat { chat_id });
        self.history_list_state.set_follow_mode(FollowMode::Tail);
        cx.notify();
    }

    /// Opens a chat from a search hit and reveals the matched message once its
    /// history is loaded.
    fn open_chat_at(&mut self, chat_id: i64, message_id: i32, cx: &mut Context<Self>) {
        self.search_open = false;
        self.engine.send(Command::ClearSearch);
        self.pending_scroll = Some((chat_id, message_id));
        self.open_chat(chat_id, cx);
        self.reveal_pending_message();
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == ViewMode::Conversation {
            self.mode = ViewMode::ChatList;
            cx.notify();
        } else if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                workspace.close_panel::<TelegramPanel>(window, cx);
            });
        }
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(chat_id) = self.view_model.selected_chat else {
            return;
        };
        let text = self.composer.read(cx).text(cx);
        if text.trim().is_empty() {
            return;
        }
        self.start_engine();
        self.engine.send(Command::SendMessage { chat_id, text });
        self.composer
            .update(cx, |editor, cx| editor.set_text("", window, cx));
        self.history_list_state.set_follow_mode(FollowMode::Tail);
        self.history_list_state.scroll_to_end();
        cx.notify();
    }

    fn toggle_composer_expanded(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer_expanded = !self.composer_expanded;
        let max_lines = if self.composer_expanded {
            None
        } else {
            Some(COMPOSER_MAX_LINES)
        };
        self.composer.update(cx, |editor, cx| {
            editor.set_mode(EditorMode::AutoHeight {
                min_lines: COMPOSER_MIN_LINES,
                max_lines,
            });
            cx.notify();
        });
        window.focus(&self.composer.focus_handle(cx), cx);
        cx.notify();
    }

    fn submit_phone(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let phone = self.phone_editor.read(cx).text(cx);
        if phone.trim().is_empty() {
            return;
        }
        self.started = true;
        self.engine.send(Command::Start);
        self.engine.send(Command::SubmitPhone {
            phone: phone.trim().to_owned(),
        });
        window.focus(&self.code_editor.focus_handle(cx), cx);
        cx.notify();
    }

    fn submit_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let code = self.code_editor.read(cx).text(cx);
        if code.trim().is_empty() {
            return;
        }
        self.engine.send(Command::SubmitCode {
            code: code.trim().to_owned(),
        });
        window.focus(&self.password_editor.focus_handle(cx), cx);
        cx.notify();
    }

    fn submit_password(&mut self, cx: &mut Context<Self>) {
        let password = self.password_editor.read(cx).text(cx);
        if password.is_empty() {
            return;
        }
        self.engine.send(Command::SubmitPassword { password });
        cx.notify();
    }

    /// Debounces the search editor and forwards the query to the engine.
    fn schedule_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self._search_task = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(SEARCH_DEBOUNCE).await;
            this.update_in(cx, |this, _window, cx| {
                let query = this.search_editor.read(cx).text(cx);
                this.engine.send(Command::Search {
                    query,
                    chat_id: None,
                });
                cx.notify();
            })
            .ok();
        }));
    }

    fn logout(&mut self, cx: &mut Context<Self>) {
        self.engine.send(Command::Logout);
        self.start_engine();
        cx.notify();
    }

    fn select_relative_chat(&mut self, delta: i64, cx: &mut Context<Self>) {
        let chats = &self.view_model.chats;
        if chats.is_empty() {
            return;
        }
        let current = self
            .view_model
            .selected_chat
            .and_then(|id| chats.iter().position(|chat| chat.id == id));
        let next = match current {
            Some(index) => (index as i64 + delta).rem_euclid(chats.len() as i64) as usize,
            None if delta >= 0 => 0,
            None => chats.len() - 1,
        };
        let chat_id = chats[next].id;
        self.open_chat(chat_id, cx);
    }

    /// Requests the thumbnail for an image-like attachment the first time its
    /// card is expanded, so expanding is enough to see a preview without
    /// downloading the original.
    fn maybe_download_preview(&mut self, chat_id: i64, message_id: i32) {
        let needs_preview = self
            .view_model
            .history
            .iter()
            .find(|message| message.chat_id == chat_id && message.id == message_id)
            .and_then(|message| message.media.as_ref())
            .is_some_and(|media| {
                matches!(media.kind, MediaKind::Photo | MediaKind::Sticker)
                    && media.downloaded_path.is_none()
                    && media.thumbnail_path.is_none()
                    && media.thumbnail_state != DownloadState::Downloading
            });
        if needs_preview {
            self.engine.send(Command::DownloadThumbnail {
                chat_id,
                message_id,
            });
        }
    }

    fn toggle_voice(&mut self, message_id: i32, path: PathBuf, cx: &mut Context<Self>) {
        if self.playing_voice == Some(message_id) {
            if let Some(playback) = self.voice_playback.take() {
                playback.stop();
            }
            self.playing_voice = None;
            cx.notify();
            return;
        }
        if self.playing_voice.is_some() {
            if let Some(playback) = self.voice_playback.take() {
                playback.stop();
            }
            self.playing_voice = None;
        }
        let file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(error) => {
                log::warn!("failed to open voice message {}: {error}", path.display());
                return;
            }
        };
        let source = match rodio::Decoder::new(std::io::BufReader::new(file)) {
            Ok(source) => source,
            Err(error) => {
                log::warn!("failed to decode voice message {}: {error}", path.display());
                return;
            }
        };
        match audio::Audio::play_source(source, cx) {
            Ok(playback) => {
                self.voice_playback = Some(playback);
                self.playing_voice = Some(message_id);
            }
            Err(error) => log::warn!("failed to play voice message: {error}"),
        }
        cx.notify();
    }

    /// Wraps content so it stays readable: full width when the panel is narrow,
    /// centered at [`TelegramPanelSettings::max_content_width`] when it is wide.
    fn render_centered<E: IntoElement>(&self, content: E, cx: &App) -> AnyElement {
        let max_content_width = TelegramPanelSettings::get_global(cx).max_content_width;
        h_flex()
            .w_full()
            .justify_center()
            .child(
                div()
                    .when_some(max_content_width, |this, max| this.max_w(max))
                    .w_full()
                    .min_w_0()
                    .child(content),
            )
            .into_any_element()
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let vm = &self.view_model;
        let status = connection_label(vm, cx);
        h_flex()
            .w_full()
            .h(Tab::container_height(cx))
            .px_2()
            .gap_1p5()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().tab_bar_background)
            .child(Icon::new(IconName::Telegram).size(IconSize::Small))
            .child(
                Label::new(tr(cx, "telegram_panel.title", "Telegram"))
                    .size(LabelSize::Small)
                    .weight(gpui::FontWeight::MEDIUM),
            )
            .child(
                Label::new(status)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
            .child(div().flex_1())
            .child(
                IconButton::new("telegram-search", IconName::MagnifyingGlass)
                    .icon_size(IconSize::XSmall)
                    .tooltip(Tooltip::text(tr(
                        cx,
                        "telegram_panel.search",
                        "Search messages",
                    )))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.search_open = !this.search_open;
                        if this.search_open {
                            window.focus(&this.search_editor.focus_handle(cx), cx);
                        } else {
                            this.engine.send(Command::ClearSearch);
                        }
                        cx.notify();
                    })),
            )
            .child(
                IconButton::new("telegram-refresh", IconName::RefreshTitle)
                    .icon_size(IconSize::XSmall)
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action(
                            tr(cx, "telegram_panel.refresh", "Refresh"),
                            &Refresh,
                            cx,
                        )
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.start_engine();
                        this.engine.send(Command::RefreshChats);
                        cx.notify();
                    })),
            )
            .when(vm.auth.is_signed_in(), |this| {
                this.child(
                    IconButton::new("telegram-logout", IconName::Exit)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text(tr(cx, "telegram_panel.logout", "Log out")))
                        .on_click(cx.listener(|this, _, _, cx| this.logout(cx))),
                )
            })
    }

    /// A top search bar mirroring the Agent panel's thread search, shown while
    /// search is open.
    fn render_search_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.search_open {
            return None;
        }
        Some(
            h_flex()
                .w_full()
                .flex_none()
                .gap_2()
                .px_2()
                .py_1()
                .border_b_1()
                .border_color(cx.theme().colors().border)
                .bg(cx.theme().colors().panel_background)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .px_1p5()
                        .py_0p5()
                        .border_1()
                        .border_color(cx.theme().colors().border)
                        .bg(cx.theme().colors().editor_background)
                        .rounded_md()
                        .child(self.search_editor.clone()),
                )
                .when(self.view_model.searching, |this| {
                    this.child(Icon::new(IconName::LoadCircle).size(IconSize::XSmall))
                })
                .child(
                    IconButton::new("telegram-search-close", IconName::Close)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text(tr(
                            cx,
                            "telegram_panel.search.close",
                            "Close search",
                        )))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.search_open = false;
                            this.engine.send(Command::ClearSearch);
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    fn render_body(
        &mut self,
        narrow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.view_model.configured {
            return self.render_message_state(
                tr(
                    cx,
                    "telegram_panel.states.not_configured",
                    "This build has no embedded Telegram credentials. Build from a checkout with ZZZ_TELEGRAM_API_ID and ZZZ_TELEGRAM_API_HASH set to sign in.",
                ),
                false,
                cx,
            );
        }

        if !self.view_model.auth.is_signed_in() {
            return self.render_auth(window, cx);
        }

        if self.search_open && !self.search_editor.read(cx).text(cx).is_empty() {
            return self.render_search_results(cx);
        }

        let show_conversation = !narrow || self.mode == ViewMode::Conversation;
        if narrow && self.mode == ViewMode::Conversation {
            return self.render_conversation(true, window, cx);
        }
        if show_conversation && !narrow {
            return h_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(
                    div()
                        .w(rems_from_px(260.))
                        .h_full()
                        .flex_none()
                        .border_r_1()
                        .border_color(cx.theme().colors().border)
                        .child(self.render_chat_list(cx)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .child(self.render_conversation(false, window, cx)),
                )
                .into_any_element();
        }
        self.render_chat_list(cx)
    }

    fn render_chat_list(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.view_model.chats.is_empty() {
            return self.render_message_state(
                if self.view_model.loading_chats {
                    tr(cx, "telegram_panel.states.loading_chats", "Loading chats…")
                } else {
                    tr(cx, "telegram_panel.states.no_chats", "No chats yet")
                },
                false,
                cx,
            );
        }

        let chats = Arc::new(self.view_model.chats.clone());
        let selected = self.view_model.selected_chat;
        let panel = cx.entity().downgrade();

        list(self.chat_list_state.clone(), move |index, _window, cx| {
            let Some(chat) = chats.get(index).cloned() else {
                return div().into_any_element();
            };
            let is_selected = selected == Some(chat.id);
            let is_unread = chat.unread_count > 0;
            let chat_id = chat.id;
            let panel = panel.clone();
            let title = chat.title.clone();
            let preview = chat_preview_text(&chat, cx);
            let initials = chat.avatar_initials.clone();
            let timestamp = relative_time(chat.timestamp_unix);
            let style = AvatarStyle::new(&chat.title);

            let avatar = div()
                .flex_none()
                .size(rems_from_px(34.))
                .rounded_full()
                .bg(style.background(cx.theme().colors().element_active))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Label::new(initials)
                        .size(LabelSize::XSmall)
                        .color(style.foreground(Color::Default)),
                );

            let mut end_slot = v_flex().flex_none().items_end().gap_1().child(
                Label::new(timestamp)
                    .size(LabelSize::XSmall)
                    .color(if is_unread {
                        Color::Accent
                    } else {
                        Color::Muted
                    }),
            );
            if is_unread {
                end_slot = end_slot.child(
                    div()
                        .relative()
                        .h(rems_from_px(18.))
                        .w(rems_from_px(24.))
                        .child(CountBadge::new(chat.unread_count.max(0) as usize)),
                );
            }

            ListItem::new(ElementId::Name(format!("telegram-chat-{chat_id}").into()))
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(is_selected)
                .on_click(move |_, _, cx| {
                    panel
                        .update(cx, |panel, cx| panel.open_chat(chat_id, cx))
                        .ok();
                })
                .start_slot(avatar)
                .end_slot(end_slot)
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            Label::new(title)
                                .size(LabelSize::Small)
                                .weight(if is_unread {
                                    gpui::FontWeight::SEMIBOLD
                                } else {
                                    gpui::FontWeight::MEDIUM
                                })
                                .truncate(),
                        )
                        .child(
                            Label::new(preview)
                                .size(LabelSize::XSmall)
                                .color(if is_unread {
                                    Color::Default
                                } else {
                                    Color::Muted
                                })
                                .truncate(),
                        ),
                )
                .into_any_element()
        })
        .size_full()
        .into_any_element()
    }

    fn render_conversation(
        &mut self,
        narrow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(chat) = self.view_model.selected_chat_snapshot().cloned() else {
            return self.render_message_state(
                tr(
                    cx,
                    "telegram_panel.states.select_chat",
                    "Select a chat to start messaging",
                ),
                false,
                cx,
            );
        };

        let history = Arc::new(self.view_model.history.clone());
        let layout = self.history_layout.clone();
        let markdown = Arc::new(self.markdown_cache.clone());
        let panel = cx.entity().downgrade();
        let outgoing_background = cx.theme().colors().element_background;
        let incoming_background = cx.theme().colors().editor_background;
        let max_content_width = TelegramPanelSettings::get_global(cx).max_content_width;

        let transcript = list(self.history_list_state.clone(), move |index, window, cx| {
            let Some(message) = history.get(index).cloned() else {
                return div().into_any_element();
            };
            let message_layout = layout.get(index).cloned().unwrap_or_default();
            let message = render_message(
                &message,
                &message_layout,
                &markdown,
                &panel,
                outgoing_background,
                incoming_background,
                window,
                cx,
            );
            h_flex()
                .w_full()
                .justify_center()
                .child(
                    div()
                        .when_some(max_content_width, |this, max| this.max_w(max))
                        .w_full()
                        .min_w_0()
                        .child(message),
                )
                .into_any_element()
        })
        .size_full();

        let composer = self.render_composer(window, cx);

        v_flex()
            .size_full()
            .min_h_0()
            .child(
                h_flex()
                    .flex_none()
                    .w_full()
                    .h(Tab::container_height(cx))
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .child(
                        self.render_centered(
                            h_flex()
                                .w_full()
                                .px_2()
                                .gap_1p5()
                                .items_center()
                                .when(narrow, |this| {
                                    this.child(
                                        IconButton::new("telegram-back", IconName::ChevronLeft)
                                            .icon_size(IconSize::Small)
                                            .tooltip(move |_window, cx| {
                                                Tooltip::for_action(
                                                    tr(cx, "telegram_panel.back", "Back"),
                                                    &Back,
                                                    cx,
                                                )
                                            })
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.back(window, cx)
                                            })),
                                    )
                                })
                                .child(
                                    Label::new(chat.title)
                                        .size(LabelSize::Small)
                                        .weight(gpui::FontWeight::MEDIUM)
                                        .truncate(),
                                )
                                .child(div().flex_1())
                                .when(self.view_model.has_more_history, |this| {
                                    this.child(
                                        IconButton::new("telegram-load-older", IconName::ChevronUp)
                                            .icon_size(IconSize::XSmall)
                                            .tooltip(Tooltip::text(tr(
                                                cx,
                                                "telegram_panel.load_older",
                                                "Load older messages",
                                            )))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                if let Some(chat_id) = this.view_model.selected_chat
                                                {
                                                    this.engine
                                                        .send(Command::LoadOlder { chat_id });
                                                    cx.notify();
                                                }
                                            })),
                                    )
                                })
                                .when(self.view_model.loading_history, |this| {
                                    this.child(
                                        Icon::new(IconName::LoadCircle).size(IconSize::XSmall),
                                    )
                                }),
                            cx,
                        ),
                    ),
            )
            .child(div().flex_1().min_h_0().child(transcript))
            .child(composer)
            .into_any_element()
    }

    fn render_composer(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let expanded = self.composer_expanded;
        let composer_empty = self.composer.read(cx).text(cx).trim().is_empty();
        let (expand_icon, expand_tooltip) = if expanded {
            (
                IconName::Minimize,
                tr(cx, "telegram_panel.composer.minimize", "Minimize Composer"),
            )
        } else {
            (
                IconName::Maximize,
                tr(cx, "telegram_panel.composer.expand", "Expand Composer"),
            )
        };
        let max_content_width = TelegramPanelSettings::get_global(cx).max_content_width;

        v_flex()
            .flex_none()
            .w_full()
            .when(expanded, |this| this.h(vh(0.6, window)))
            .border_t_1()
            .border_color(cx.theme().colors().border)
            .child(
                h_flex()
                    .w_full()
                    .justify_center()
                    .when(expanded, |this| this.h_full())
                    .child(
                        div()
                            .when_some(max_content_width, |this, max| this.max_w(max))
                            .w_full()
                            .min_w_0()
                            .when(expanded, |this| this.h_full())
                            .child(
                                v_flex()
                                    .w_full()
                                    .min_w_0()
                                    .when(expanded, |this| this.h_full())
                                    .gap_1p5()
                                    .px_2()
                                    .py_1p5()
                                    .child(
                                        v_flex()
                                            .relative()
                                            .w_full()
                                            .min_h_0()
                                            .when(expanded, |this| this.flex_1())
                                            .child(
                                                div()
                                                    .w_full()
                                                    .key_context("TelegramComposer")
                                                    .child(self.composer.clone()),
                                            )
                                            .child(
                                                h_flex()
                                                    .absolute()
                                                    .top_0()
                                                    .right_0()
                                                    .opacity(0.5)
                                                    .hover(|this| this.opacity(1.0))
                                                    .child(
                                                        IconButton::new(
                                                            "telegram-composer-height",
                                                            expand_icon,
                                                        )
                                                        .icon_size(IconSize::Small)
                                                        .icon_color(Color::Muted)
                                                        .tooltip(Tooltip::text(expand_tooltip))
                                                        .on_click(cx.listener(
                                                            |this, _, window, cx| {
                                                                this.toggle_composer_expanded(
                                                                    window, cx,
                                                                )
                                                            },
                                                        )),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        h_flex().w_full().flex_none().justify_end().child(
                                            IconButton::new("telegram-send", IconName::Send)
                                                .style(ButtonStyle::Filled)
                                                .map(|this| {
                                                    if composer_empty {
                                                        this.disabled(true).icon_color(Color::Muted)
                                                    } else {
                                                        this.icon_color(Color::Accent)
                                                    }
                                                })
                                                .tooltip(Tooltip::text(tr(
                                                    cx,
                                                    "telegram_panel.composer.send",
                                                    "Send",
                                                )))
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.send(window, cx)
                                                })),
                                        ),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn qr_image_for(&mut self, url: &str) -> Option<Arc<RenderImage>> {
        if let Some((cached_url, image)) = &self.qr_cache {
            if cached_url == url {
                return Some(image.clone());
            }
        }
        let image = build_qr_image(url)?;
        self.qr_cache = Some((url.to_owned(), image.clone()));
        Some(image)
    }

    fn render_auth(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let auth = self.view_model.auth.clone();
        let mut column = v_flex().size_full().gap_3().p_4().justify_center().child(
            Label::new(tr(cx, "telegram_panel.auth.title", "Sign in to Telegram"))
                .size(LabelSize::Large)
                .weight(gpui::FontWeight::MEDIUM),
        );

        if self.view_model.connection == ConnectionState::Connecting {
            column = column.child(
                Label::new(tr(cx, "telegram_panel.auth.connecting", "Connecting…"))
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            );
        }

        match auth {
            AuthState::SignedOut => {
                column = column.child(div().child(self.phone_editor.clone())).child(
                    Button::new(
                        "telegram-sign-in",
                        tr(cx, "telegram_panel.auth.send_code", "Send code"),
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.submit_phone(window, cx))),
                );
                column = column.child(
                    Button::new(
                        "telegram-sign-in-qr",
                        tr(cx, "telegram_panel.auth.qr_sign_in", "Sign in with QR code"),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.started = true;
                        this.engine.send(Command::Start);
                        this.engine.send(Command::StartQrLogin);
                        cx.notify();
                    })),
                );
            }
            AuthState::AwaitingQr { url, .. } => {
                if let Some(image) = self.qr_image_for(&url) {
                    column = column.child(
                        div()
                            .flex()
                            .justify_center()
                            .child(img(image).w(rems_from_px(240.)).h(rems_from_px(240.))),
                    );
                }
                column = column.child(
                    Label::new(tr(
                        cx,
                        "telegram_panel.auth.qr_hint",
                        "Open Telegram on your phone, go to Settings, then Devices, and scan this code.",
                    ))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
                );
                column = column.child(
                    Button::new(
                        "telegram-use-phone",
                        tr(
                            cx,
                            "telegram_panel.auth.use_phone",
                            "Use phone number instead",
                        ),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.engine.send(Command::CancelLogin);
                        cx.notify();
                    })),
                );
            }
            AuthState::AwaitingCode { phone } => {
                column = column.child(
                    Label::new(
                        tr(
                            cx,
                            "telegram_panel.auth.code_sent",
                            "Enter the code sent to {}",
                        )
                        .replacen("{}", &phone, 1),
                    )
                    .size(LabelSize::Small)
                    .color(Color::Muted),
                );
                column = column.child(div().child(self.code_editor.clone())).child(
                    Button::new(
                        "telegram-submit-code",
                        tr(cx, "telegram_panel.auth.submit_code", "Submit code"),
                    )
                    .on_click(cx.listener(|this, _, window, cx| this.submit_code(window, cx))),
                );
            }
            AuthState::AwaitingPassword { hint } => {
                let hint = hint.unwrap_or_else(|| {
                    tr(
                        cx,
                        "telegram_panel.auth.password_default_hint",
                        "Two-factor password",
                    )
                });
                column = column.child(Label::new(hint).size(LabelSize::Small).color(Color::Muted));
                column = column
                    .child(div().child(self.password_editor.clone()))
                    .child(
                        Button::new(
                            "telegram-submit-password",
                            tr(cx, "telegram_panel.auth.submit_password", "Submit password"),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.submit_password(cx))),
                    );
            }
            AuthState::SignedIn { .. } => {}
        }

        let _ = window;
        column.into_any_element()
    }

    fn render_error_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let error = self.view_model.error.as_ref()?;
        let message = error_text(error, self.view_model.flood_wait_seconds, cx);
        Some(
            Callout::new()
                .severity(Severity::Error)
                .icon(IconName::Warning)
                .title(message)
                .dismiss_action(
                    IconButton::new("telegram-error-dismiss", IconName::Close)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text(tr(
                            cx,
                            "telegram_panel.error.dismiss",
                            "Dismiss",
                        )))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.engine.send(Command::DismissError);
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    fn render_message_state(
        &self,
        message: String,
        _is_error: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let _ = cx;
        v_flex()
            .size_full()
            .justify_center()
            .items_center()
            .p_6()
            .child(Label::new(message).color(Color::Muted))
            .into_any_element()
    }

    fn render_search_results(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let hits = self.view_model.search_results.clone();
        let total = hits.len();
        self.sync_search_list(total);
        let searching = self.view_model.searching;
        let panel = cx.entity().downgrade();
        let markdown = Arc::new(self.markdown_cache.clone());

        let mut column = v_flex().size_full().min_h_0();
        if searching {
            column = column.child(
                h_flex()
                    .flex_none()
                    .p_2()
                    .gap_1()
                    .items_center()
                    .child(Icon::new(IconName::LoadCircle).size(IconSize::XSmall))
                    .child(
                        Label::new(tr(cx, "telegram_panel.search.searching", "Searching…"))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
            );
        }

        if total == 0 {
            if !searching {
                column = column.child(
                    v_flex()
                        .flex_1()
                        .justify_center()
                        .items_center()
                        .p_6()
                        .child(
                            Label::new(tr(cx, "telegram_panel.search.no_results", "No results"))
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                );
            }
        } else {
            column = column.child(
                list(self.search_list_state.clone(), move |index, window, cx| {
                    let Some(hit) = hits.get(index).cloned() else {
                        return div().into_any_element();
                    };
                    let chat_id = hit.chat_id;
                    let message_id = hit.message.as_ref().map_or(0, |message| message.id);
                    let panel = panel.clone();
                    let markdown = markdown.clone();
                    ListItem::new(ElementId::Name(
                        format!("telegram-search-hit-{index}").into(),
                    ))
                    .spacing(ListItemSpacing::Sparse)
                    .on_click(move |_, _, cx| {
                        if message_id == 0 {
                            panel
                                .update(cx, |panel, cx| panel.open_chat(chat_id, cx))
                                .ok();
                        } else {
                            panel
                                .update(cx, |panel, cx| panel.open_chat_at(chat_id, message_id, cx))
                                .ok();
                        }
                    })
                    .child(
                        v_flex()
                            .min_w_0()
                            .gap_0p5()
                            .child(
                                Label::new(hit.chat_title.clone())
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            )
                            .when_some(hit.message.as_ref(), |this, message| {
                                this.child(render_body_text(message, &markdown, window, cx))
                            }),
                    )
                    .into_any_element()
                })
                .size_full(),
            );
        }

        column.into_any_element()
    }

    fn sync_search_list(&mut self, new_count: usize) {
        if new_count != self.search_count {
            self.search_list_state
                .splice(0..self.search_count, new_count);
            self.search_count = new_count;
        }
    }
}

/// Markdown style for chat messages. [`MarkdownFont::Agent`] uses the agent
/// panel's UI font size (16px by default), which is noticeably larger than the
/// small label used for plain-text messages, so both the base and container text
/// styles are pinned to [`TextSize::Small`] to keep formatted and unformatted
/// messages the same size. The container style is the one that drives layout
/// metrics; the base style only carries the run font and color.
fn telegram_markdown_style(window: &Window, cx: &App) -> MarkdownStyle {
    let mut style = MarkdownStyle::themed(MarkdownFont::Agent, window, cx);
    let font_size = TextSize::Small.pixels(cx);
    let line_height = font_size * 1.4;
    style.base_text_style.font_size = font_size.into();
    style.base_text_style.line_height = line_height.into();
    style.paragraph_line_height = line_height.into();
    style.paragraph_spacing = px(4.);
    // `base_text_style` only carries the run font/color; the size that actually
    // reaches layout comes from the container's text style, so set it there too.
    style.container_style.text.font_size = Some(font_size.into());
    style.container_style.text.line_height = Some(line_height.into());
    style
}

/// Renders a message body as Markdown when formatting is present, otherwise as
/// plain text.
fn render_body_text(
    message: &MessageSnapshot,
    markdown: &HashMap<(i64, i32), Entity<Markdown>>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    if let Some(entity) = markdown.get(&(message.chat_id, message.id)) {
        return MarkdownElement::new(entity.clone(), telegram_markdown_style(window, cx))
            .into_any_element();
    }
    if message.text.is_empty() {
        return div().into_any_element();
    }
    Label::new(message.text.clone())
        .size(LabelSize::Small)
        .into_any_element()
}

/// Encodes a `tg://login` URL into an RGBA bitmap for [`img`], with a quiet
/// zone and an integer scale so the modules stay crisp.
fn build_qr_image(url: &str) -> Option<Arc<RenderImage>> {
    use qrcodegen::{QrCode, QrCodeEcc};

    let code = QrCode::encode_text(url, QrCodeEcc::Low).ok()?;
    let border: i32 = 4;
    let scale: i32 = 8;
    let dimension = ((code.size() + border * 2) * scale) as u32;
    let mut buffer = image::RgbaImage::new(dimension, dimension);
    for y in 0..dimension {
        for x in 0..dimension {
            let module_x = x as i32 / scale - border;
            let module_y = y as i32 / scale - border;
            let dark = module_x >= 0
                && module_y >= 0
                && module_x < code.size()
                && module_y < code.size()
                && code.get_module(module_x, module_y);
            let color = if dark {
                image::Rgba([0, 0, 0, 255])
            } else {
                image::Rgba([255, 255, 255, 255])
            };
            buffer.put_pixel(x, y, color);
        }
    }
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

#[allow(clippy::too_many_arguments)]
fn render_message(
    message: &MessageSnapshot,
    layout: &MessageLayout,
    markdown: &HashMap<(i64, i32), Entity<Markdown>>,
    panel: &WeakEntity<TelegramPanel>,
    outgoing_background: gpui::Hsla,
    incoming_background: gpui::Hsla,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let mut column = v_flex().w_full().px_3().py_1().gap_1();

    if let Some(day) = &layout.day_separator {
        column = column.child(
            h_flex().w_full().justify_center().py_1().child(
                Label::new(day.clone())
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            ),
        );
    }

    let background = if message.outgoing {
        outgoing_background
    } else {
        incoming_background
    };

    let mut bubble = v_flex()
        .min_w_0()
        .gap_1()
        .px_2()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().colors().border)
        .overflow_hidden()
        .bg(background);

    if layout.show_sender && !message.outgoing && !message.sender_name.is_empty() {
        bubble = bubble.child(
            Label::new(message.sender_name.clone())
                .size(LabelSize::XSmall)
                .weight(gpui::FontWeight::MEDIUM),
        );
    }

    if let Some(media) = &message.media {
        bubble = bubble.child(render_media(message, media, panel, cx));
    }

    let body = render_body_text(message, markdown, window, cx);
    bubble = bubble.child(body);

    bubble = bubble.child(
        h_flex()
            .justify_end()
            .gap_1()
            .child(
                Label::new(clock_time(message.timestamp_unix))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
            .when(message.edited, |this| {
                this.child(
                    Label::new(tr(cx, "telegram_panel.message.edited", "edited"))
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
            })
            .when(message.send_state == telegram::SendState::Sending, |this| {
                this.child(Icon::new(IconName::LoadCircle).size(IconSize::XSmall))
            })
            .when(message.send_state == telegram::SendState::Failed, |this| {
                let panel = panel.clone();
                let chat_id = message.chat_id;
                let local_id = message.local_id;
                this.child(
                    Button::new(
                        ElementId::Name(
                            format!("telegram-retry-{}", message.local_id.unwrap_or(0)).into(),
                        ),
                        tr(cx, "telegram_panel.message.retry", "Retry"),
                    )
                    .label_size(LabelSize::XSmall)
                    .on_click(move |_, _, cx| {
                        if let Some(local_id) = local_id {
                            panel
                                .update(cx, |panel, cx| {
                                    panel.engine.send(Command::RetrySend { chat_id, local_id });
                                    cx.notify();
                                })
                                .ok();
                        }
                    }),
                )
            }),
    );

    let bubble = wrap_in_context_menu(message, bubble.into_any_element(), panel);
    let item = div().min_w_0().child(bubble).into_any_element();
    if message.outgoing {
        column = column.items_end().child(item);
    } else {
        column = column.items_start().child(item);
    }

    column.into_any_element()
}

/// Wraps a message bubble in a right-click menu with message actions.
fn wrap_in_context_menu(
    message: &MessageSnapshot,
    bubble: AnyElement,
    panel: &WeakEntity<TelegramPanel>,
) -> AnyElement {
    let chat_id = message.chat_id;
    let message_id = message.id;
    let text = message.text.clone();
    let is_sendable = message_id != 0;
    let panel = panel.clone();
    right_click_menu(ElementId::Name(
        format!("telegram-message-menu-{chat_id}-{message_id}").into(),
    ))
    .trigger(move |_, _, _| bubble)
    .menu(move |window, cx| {
        let panel = panel.clone();
        let text = text.clone();
        ContextMenu::build(window, cx, move |menu, _, cx| {
            let is_pinned = panel.upgrade().is_some_and(|panel| {
                panel.read_with(cx, |panel, _| {
                    panel
                        .view_model
                        .selected_chat_snapshot()
                        .is_some_and(|chat| chat.pinned)
                })
            });
            let copy_text = text.clone();
            let copy = ContextMenuEntry::new(tr(cx, "telegram_panel.menu.copy", "Copy")).handler(
                move |_, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy_text.clone()));
                },
            );

            let mark_read =
                ContextMenuEntry::new(tr(cx, "telegram_panel.menu.mark_read", "Mark as Read"))
                    .disabled(message_id == 0)
                    .handler({
                        let panel = panel.clone();
                        move |_, cx| {
                            panel
                                .update(cx, |panel, cx| {
                                    panel.engine.send(Command::MarkRead { chat_id });
                                    cx.notify();
                                })
                                .ok();
                        }
                    });

            let forward = ContextMenuEntry::new(tr(cx, "telegram_panel.menu.forward", "Forward"))
                .disabled(!is_sendable)
                .handler({
                    let panel = panel.clone();
                    move |window, cx| {
                        panel
                            .update(cx, |panel, cx| {
                                panel.forward_message(chat_id, message_id, window, cx)
                            })
                            .ok();
                    }
                });

            let delete = ContextMenuEntry::new(tr(cx, "telegram_panel.menu.delete", "Delete"))
                .disabled(!is_sendable)
                .handler({
                    let panel = panel.clone();
                    move |_, cx| {
                        panel
                            .update(cx, |panel, cx| {
                                panel.engine.send(Command::DeleteMessage {
                                    chat_id,
                                    message_id,
                                });
                                cx.notify();
                            })
                            .ok();
                    }
                });

            let pin = ContextMenuEntry::new(if is_pinned {
                tr(cx, "telegram_panel.menu.unpin_chat", "Unpin Chat")
            } else {
                tr(cx, "telegram_panel.menu.pin_chat", "Pin Chat")
            })
            .handler({
                let panel = panel.clone();
                move |_, cx| {
                    panel
                        .update(cx, |panel, cx| {
                            panel.engine.send(Command::SetPinned {
                                chat_id,
                                pinned: !is_pinned,
                            });
                            cx.notify();
                        })
                        .ok();
                }
            });

            menu.item(copy)
                .item(mark_read)
                .separator()
                .item(forward)
                .item(delete)
                .separator()
                .item(pin)
        })
    })
    .into_any_element()
}

/// Toggles whether a media card is expanded. Shared by the card header and the
/// disclosure chevron.
fn media_toggle(
    panel: &WeakEntity<TelegramPanel>,
    chat_id: i64,
    message_id: i32,
) -> Arc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static> {
    let panel = panel.clone();
    Arc::new(move |_, _, cx| {
        panel
            .update(cx, |panel, cx| {
                let key = (chat_id, message_id);
                let now_expanded = if panel.expanded_media.remove(&key) {
                    false
                } else {
                    panel.expanded_media.insert(key);
                    true
                };
                if now_expanded {
                    panel.maybe_download_preview(chat_id, message_id);
                }
                cx.notify();
            })
            .ok();
    })
}

/// Renders an attachment as a card that is collapsed by default and expands on
/// demand, mirroring the agent panel's tool-call cards.
fn render_media(
    message: &MessageSnapshot,
    media: &MediaSnapshot,
    panel: &WeakEntity<TelegramPanel>,
    cx: &mut App,
) -> AnyElement {
    let chat_id = message.chat_id;
    let message_id = message.id;

    if media.kind == MediaKind::WebPage {
        return render_webpage(media, message_id, cx);
    }

    let icon = match media.kind {
        MediaKind::Photo => IconName::Image,
        MediaKind::Video => IconName::PlayFilled,
        MediaKind::Voice => IconName::Mic,
        MediaKind::Sticker => IconName::Sparkle,
        _ => IconName::FileGeneric,
    };
    let is_expanded = panel.upgrade().is_some_and(|panel| {
        panel.read_with(cx, |panel, _| {
            panel.expanded_media.contains(&(chat_id, message_id))
        })
    });
    let metadata = media_metadata(media);
    let toggle = media_toggle(panel, chat_id, message_id);

    let mut card = v_flex()
        .w_full()
        .min_w_0()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().colors().border)
        .overflow_hidden()
        .child(
            h_flex()
                .id(ElementId::Name(
                    format!("telegram-media-header-{message_id}").into(),
                ))
                .w_full()
                .min_w_0()
                .px_2()
                .py_1()
                .gap_2()
                .items_center()
                .cursor_pointer()
                .bg(cx.theme().colors().editor_background)
                .hover(|this| this.bg(cx.theme().colors().element_hover))
                .on_click({
                    let toggle = toggle.clone();
                    move |event, window, cx| toggle(event, window, cx)
                })
                .child(Icon::new(icon).size(IconSize::Small).color(Color::Muted))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            Label::new(media_display_name(media, cx))
                                .size(LabelSize::Small)
                                .truncate(),
                        )
                        .when(!metadata.is_empty(), |this| {
                            this.child(
                                Label::new(metadata)
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted)
                                    .truncate(),
                            )
                        }),
                )
                .child(
                    div().flex_none().child(
                        Disclosure::new(
                            ElementId::Name(
                                format!("telegram-media-disclosure-{message_id}").into(),
                            ),
                            is_expanded,
                        )
                        .on_toggle_expanded(toggle),
                    ),
                ),
        );

    if is_expanded {
        card = card.child(
            v_flex()
                .w_full()
                .min_w_0()
                .p_2()
                .border_t_1()
                .border_color(cx.theme().colors().border)
                .child(render_media_content(media, chat_id, message_id, panel, cx)),
        );
    }

    card.into_any_element()
}

fn render_media_content(
    media: &MediaSnapshot,
    chat_id: i64,
    message_id: i32,
    panel: &WeakEntity<TelegramPanel>,
    cx: &mut App,
) -> AnyElement {
    match media.kind {
        MediaKind::Photo | MediaKind::Sticker => {
            let max_height = if media.kind == MediaKind::Sticker {
                rems_from_px(120.)
            } else {
                rems_from_px(320.)
            };
            let mut column = v_flex().w_full().min_w_0().gap_1();
            if let Some(path) = media
                .downloaded_path
                .as_ref()
                .or(media.thumbnail_path.as_ref())
            {
                column = column.child(
                    img(path.clone())
                        .max_h(max_height)
                        .max_w_full()
                        .rounded_md(),
                );
            }
            if media.downloaded_path.is_none() {
                if media.thumbnail_state == DownloadState::Downloading {
                    column = column.child(
                        Label::new(tr(
                            cx,
                            "telegram_panel.media.loading_preview",
                            "Loading preview…",
                        ))
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                    );
                } else {
                    column =
                        column.child(render_media_action(media, chat_id, message_id, panel, cx));
                }
            }
            column.into_any_element()
        }
        MediaKind::Voice => render_voice_action(media, chat_id, message_id, panel, cx),
        _ if media.is_downloadable() => render_media_action(media, chat_id, message_id, panel, cx),
        _ => div().into_any_element(),
    }
}

fn render_media_action(
    media: &MediaSnapshot,
    chat_id: i64,
    message_id: i32,
    panel: &WeakEntity<TelegramPanel>,
    cx: &mut App,
) -> AnyElement {
    let downloading = media.download_state == DownloadState::Downloading;
    let path = media.downloaded_path.clone();
    let panel = panel.clone();
    Button::new(
        ElementId::Name(format!("telegram-media-action-{message_id}").into()),
        if downloading {
            tr(cx, "telegram_panel.media.downloading", "Downloading…")
        } else if media.downloaded_path.is_some() {
            tr(cx, "telegram_panel.media.open", "Open")
        } else {
            tr(cx, "telegram_panel.media.download", "Download")
        },
    )
    .label_size(LabelSize::XSmall)
    .disabled(downloading)
    .when_some(path, |this, path| {
        this.on_click(move |_, _, cx| cx.open_with_system(&path))
    })
    .when(media.downloaded_path.is_none(), |this| {
        this.on_click(move |_, _, cx| {
            panel
                .update(cx, |panel, cx| {
                    panel.engine.send(Command::DownloadMedia {
                        chat_id,
                        message_id,
                    });
                    cx.notify();
                })
                .ok();
        })
    })
    .into_any_element()
}

fn render_voice_action(
    media: &MediaSnapshot,
    chat_id: i64,
    message_id: i32,
    panel: &WeakEntity<TelegramPanel>,
    cx: &mut App,
) -> AnyElement {
    let path = media.downloaded_path.clone();
    let panel_entity = panel.clone();
    let is_playing = panel.upgrade().is_some_and(|panel| {
        panel.read_with(cx, |panel, _| panel.playing_voice == Some(message_id))
    });
    h_flex()
        .gap_2()
        .items_center()
        .child(
            Label::new(duration_label(media.duration_seconds))
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        )
        .child(
            Button::new(
                ElementId::Name(format!("telegram-voice-{message_id}").into()),
                if is_playing {
                    tr(cx, "telegram_panel.media.stop", "Stop")
                } else {
                    tr(cx, "telegram_panel.media.play", "Play")
                },
            )
            .label_size(LabelSize::XSmall)
            .on_click(move |_, _, cx| {
                if let Some(path) = path.clone() {
                    panel_entity
                        .update(cx, |panel, cx| panel.toggle_voice(message_id, path, cx))
                        .ok();
                } else {
                    panel_entity
                        .update(cx, |panel, cx| {
                            panel.engine.send(Command::DownloadMedia {
                                chat_id,
                                message_id,
                            });
                            cx.notify();
                        })
                        .ok();
                }
            }),
        )
        .into_any_element()
}

fn render_webpage(media: &MediaSnapshot, message_id: i32, cx: &mut App) -> AnyElement {
    let Some(webpage) = &media.webpage else {
        return div().into_any_element();
    };
    let url = webpage.url.clone();
    let title = webpage.title.clone().unwrap_or_else(|| webpage.url.clone());
    let description = webpage.description.clone().unwrap_or_default();
    v_flex()
        .id(ElementId::Name(
            format!("telegram-link-{message_id}").into(),
        ))
        .w_full()
        .min_w_0()
        .gap_0p5()
        .p_2()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().colors().border)
        .child(
            Label::new(title)
                .size(LabelSize::Small)
                .weight(gpui::FontWeight::MEDIUM)
                .truncate(),
        )
        .when(!description.is_empty(), |this| {
            this.child(
                Label::new(description)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
        })
        .child(
            Label::new(webpage.url.clone())
                .size(LabelSize::XSmall)
                .color(Color::Accent)
                .truncate(),
        )
        .on_click(move |_, _, cx| cx.open_url(&url))
        .into_any_element()
}

fn media_metadata(media: &MediaSnapshot) -> String {
    let mut parts = Vec::new();
    if let Some(duration) = media.duration_seconds {
        parts.push(duration_label(Some(duration)));
    }
    if let (Some(width), Some(height)) = (media.width, media.height) {
        parts.push(format!("{width}×{height}"));
    }
    if let Some(size) = media.size {
        parts.push(util::size::format_file_size(size, false));
    }
    if let Some(mime) = &media.mime_type {
        parts.push(mime.clone());
    }
    parts.join(" • ")
}

fn duration_label(duration: Option<f64>) -> String {
    let seconds = duration
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
        .max(0.0);
    util::time::format_media_duration(std::time::Duration::from_secs_f64(seconds))
}

fn build_history_layout(history: &[MessageSnapshot]) -> Vec<MessageLayout> {
    let mut layout = Vec::with_capacity(history.len());
    let mut previous: Option<&MessageSnapshot> = None;
    for message in history {
        let day = day_label(message.timestamp_unix);
        let previous_day = previous.map(|message| day_label(message.timestamp_unix));
        let day_separator = if previous_day.as_deref() == Some(day.as_str()) {
            None
        } else {
            Some(day)
        };
        let grouped = previous.is_some_and(|previous| {
            previous.outgoing == message.outgoing
                && previous.sender_name == message.sender_name
                && message.timestamp_unix - previous.timestamp_unix <= SAME_SENDER_GROUP_SECONDS
        });
        layout.push(MessageLayout {
            day_separator,
            show_sender: !grouped,
        });
        previous = Some(message);
    }
    layout
}

fn connection_label(view_model: &ViewModel, cx: &App) -> String {
    if !view_model.configured {
        return tr(cx, "telegram_panel.status.not_configured", "Not configured");
    }
    match view_model.connection {
        ConnectionState::Offline => tr(cx, "telegram_panel.status.offline", "Offline"),
        ConnectionState::Connecting => tr(cx, "telegram_panel.status.connecting", "Connecting"),
        ConnectionState::Connected => {
            if view_model.auth.is_signed_in() {
                tr(cx, "telegram_panel.status.connected", "Connected")
            } else {
                tr(cx, "telegram_panel.status.signed_out", "Signed out")
            }
        }
    }
}

fn build_composer(window: &mut Window, cx: &mut Context<Editor>) -> Editor {
    let buffer = cx.new(|cx| Buffer::local("", cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let mut editor = Editor::new(
        EditorMode::AutoHeight {
            min_lines: COMPOSER_MIN_LINES,
            max_lines: Some(COMPOSER_MAX_LINES),
        },
        buffer,
        None,
        window,
        cx,
    );
    editor.set_placeholder_text(
        &tr(cx, "telegram_panel.composer.placeholder", "Message"),
        window,
        cx,
    );
    editor.set_use_modal_editing(true);
    // `AutoHeight` editors default to `rems(0.875)` (14px at the default 16px
    // rem). Match the message text instead: `LabelSize::Small` is
    // `rems_from_px(12.)` (0.75rem), so both scale with the user's `ui_font_size`.
    editor.set_text_style_refinement(TextStyleRefinement {
        font_size: Some(rems_from_px(12.).into()),
        line_height: Some(relative(1.4)),
        ..Default::default()
    });
    editor
}

fn single_line_editor(
    placeholder: String,
    window: &mut Window,
    cx: &mut Context<Editor>,
) -> Editor {
    let mut editor = Editor::single_line(window, cx);
    editor.set_placeholder_text(&placeholder, window, cx);
    editor.set_use_modal_editing(true);
    editor
}

fn local_offset() -> UtcOffset {
    UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC)
}

fn clock_time(unix: i64) -> String {
    match time::OffsetDateTime::from_unix_timestamp(unix) {
        Ok(timestamp) => format_time(timestamp.to_offset(local_offset())),
        Err(_) => String::new(),
    }
}

fn relative_time(unix: i64) -> String {
    let offset = local_offset();
    let reference = time::OffsetDateTime::now_utc().to_offset(offset);
    match time::OffsetDateTime::from_unix_timestamp(unix) {
        Ok(timestamp) => format_local_timestamp(
            timestamp.to_offset(offset),
            reference,
            TimestampFormat::Relative,
        ),
        Err(_) => String::new(),
    }
}

fn day_label(unix: i64) -> String {
    let offset = local_offset();
    let reference = time::OffsetDateTime::now_utc().to_offset(offset);
    match time::OffsetDateTime::from_unix_timestamp(unix) {
        Ok(timestamp) => {
            time_format::format_date_medium(timestamp.to_offset(offset), reference, true)
        }
        Err(_) => String::new(),
    }
}

/// The chat-list preview text. Media-only messages fall back to a localized
/// name for the attachment kind instead of an empty line.
fn chat_preview_text(chat: &ChatSnapshot, cx: &App) -> String {
    if !chat.preview.is_empty() {
        return chat.preview.clone();
    }
    chat.preview_media
        .map(|kind| media_kind_label(kind, cx))
        .unwrap_or_default()
}

fn media_display_name(media: &MediaSnapshot, cx: &App) -> String {
    if let Some(file_name) = &media.file_name {
        return file_name.clone();
    }
    media_kind_label(media.kind, cx)
}

fn media_kind_label(kind: MediaKind, cx: &App) -> String {
    let (key, fallback) = match kind {
        MediaKind::Photo => ("telegram_panel.media.photo", "Photo"),
        MediaKind::Video => ("telegram_panel.media.video", "Video"),
        MediaKind::Voice => ("telegram_panel.media.voice", "Voice message"),
        MediaKind::Audio => ("telegram_panel.media.audio", "Audio"),
        MediaKind::Sticker => ("telegram_panel.media.sticker", "Sticker"),
        MediaKind::Document => ("telegram_panel.media.document", "Document"),
        MediaKind::WebPage => ("telegram_panel.media.web_page", "Link"),
        MediaKind::Contact => ("telegram_panel.media.contact", "Contact"),
        MediaKind::Poll => ("telegram_panel.media.poll", "Poll"),
        MediaKind::Geo => ("telegram_panel.media.geo", "Location"),
        MediaKind::Other => ("telegram_panel.media.attachment", "Attachment"),
    };
    tr(cx, key, fallback)
}

fn error_text(error: &EngineError, flood_wait_seconds: Option<u64>, cx: &App) -> String {
    match error {
        EngineError::NotConfigured => tr(
            cx,
            "telegram_panel.error.not_configured",
            "This build has no embedded Telegram credentials.",
        ),
        EngineError::NotConnected => tr(
            cx,
            "telegram_panel.error.not_connected",
            "Not connected to Telegram.",
        ),
        EngineError::NoCredentials => tr(
            cx,
            "telegram_panel.error.no_credentials",
            "This build has no embedded Telegram credentials.",
        ),
        EngineError::InvalidPhone => tr(
            cx,
            "telegram_panel.error.invalid_phone",
            "Enter the phone number in international format, including the country code.",
        ),
        EngineError::RequestCodeFailed => tr(
            cx,
            "telegram_panel.error.request_code_failed",
            "Could not request a login code.",
        ),
        EngineError::InvalidCode => tr(
            cx,
            "telegram_panel.error.invalid_code",
            "The login code is invalid.",
        ),
        EngineError::SignUpRequired => tr(
            cx,
            "telegram_panel.error.sign_up_required",
            "Sign up with an official Telegram app first.",
        ),
        EngineError::InvalidPassword => tr(
            cx,
            "telegram_panel.error.invalid_password",
            "The password is invalid.",
        ),
        EngineError::SignInFailed => {
            tr(cx, "telegram_panel.error.sign_in_failed", "Sign in failed.")
        }
        EngineError::QrFailed => tr(cx, "telegram_panel.error.qr_failed", "QR sign in failed."),
        EngineError::QrTooManyMigrations => tr(
            cx,
            "telegram_panel.error.qr_too_many_migrations",
            "QR sign in failed: too many data-center migrations.",
        ),
        EngineError::NoPendingPassword => tr(
            cx,
            "telegram_panel.error.no_pending_password",
            "No password step is pending.",
        ),
        EngineError::ChatUnavailable => tr(
            cx,
            "telegram_panel.error.chat_unavailable",
            "This chat is not available.",
        ),
        EngineError::ChatUnavailableOffline => tr(
            cx,
            "telegram_panel.error.chat_unavailable_offline",
            "This chat is not available offline.",
        ),
        EngineError::SendFailed => tr(
            cx,
            "telegram_panel.error.send_failed",
            "Failed to send the message.",
        ),
        EngineError::LoadChatsFailed => tr(
            cx,
            "telegram_panel.error.load_chats_failed",
            "Failed to load chats.",
        ),
        EngineError::LoadMessagesFailed => tr(
            cx,
            "telegram_panel.error.load_messages_failed",
            "Failed to load messages.",
        ),
        EngineError::DeleteFailed => tr(
            cx,
            "telegram_panel.error.delete_failed",
            "Failed to delete the message.",
        ),
        EngineError::ForwardFailed => tr(
            cx,
            "telegram_panel.error.forward_failed",
            "Failed to forward messages.",
        ),
        EngineError::PinFailed => tr(
            cx,
            "telegram_panel.error.pin_failed",
            "Failed to update the chat pin.",
        ),
        EngineError::SearchFailed => tr(cx, "telegram_panel.error.search_failed", "Search failed."),
        EngineError::FloodWait { .. } => {
            let seconds = flood_wait_seconds.unwrap_or(0);
            tr(
                cx,
                "telegram_panel.error.flood_wait",
                "Too many requests. Try again in {} seconds.",
            )
            .replacen("{}", &seconds.to_string(), 1)
        }
        EngineError::Other => tr(cx, "telegram_panel.error.other", "Something went wrong."),
    }
}

impl Render for TelegramPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let narrow = matches!(
            self.position(window, cx),
            DockPosition::Left | DockPosition::Right
        );
        let body = self.render_body(narrow, window, cx);
        v_flex()
            .id("telegram-panel")
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().colors().panel_background)
            .key_context("TelegramPanel")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &Back, window, cx| this.back(window, cx)))
            .on_action(cx.listener(|this, _: &Send, window, cx| this.send(window, cx)))
            .on_action(cx.listener(|this, _: &NextChat, _, cx| this.select_relative_chat(1, cx)))
            .on_action(
                cx.listener(|this, _: &PreviousChat, _, cx| this.select_relative_chat(-1, cx)),
            )
            .child(self.render_header(cx))
            .when_some(self.render_search_bar(cx), |this, search| {
                this.child(search)
            })
            .when_some(self.render_error_banner(cx), |this, banner| {
                this.child(banner)
            })
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }
}

impl Focusable for TelegramPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for TelegramPanel {}

impl Panel for TelegramPanel {
    fn persistent_name() -> &'static str {
        "TelegramPanel"
    }

    fn panel_key() -> &'static str {
        PANEL_KEY
    }

    fn position(&self, _window: &Window, cx: &App) -> DockPosition {
        TelegramPanelSettings::get_global(cx).dock
    }

    fn position_is_valid(&self, _position: DockPosition) -> bool {
        true
    }

    fn set_position(
        &mut self,
        position: DockPosition,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        settings::update_settings_file(self.fs.clone(), cx, move |settings, _| {
            settings.telegram_panel.get_or_insert_default().dock = Some(position.into());
        });
    }

    fn default_size(&self, window: &Window, cx: &App) -> gpui::Pixels {
        let settings = TelegramPanelSettings::get_global(cx);
        match self.position(window, cx) {
            DockPosition::Left | DockPosition::Right => settings.default_width,
            DockPosition::Bottom => settings.default_height,
        }
    }

    fn supports_flexible_size(&self) -> bool {
        true
    }

    fn has_flexible_size(&self, _window: &Window, cx: &App) -> bool {
        TelegramPanelSettings::get_global(cx).flexible
    }

    fn set_flexible_size(&mut self, flexible: bool, _window: &mut Window, cx: &mut Context<Self>) {
        settings::update_settings_file(self.fs.clone(), cx, move |settings, _| {
            settings.telegram_panel.get_or_insert_default().flexible = Some(flexible);
        });
    }

    fn icon(&self, _window: &Window, cx: &App) -> Option<IconName> {
        TelegramPanelSettings::get_global(cx)
            .button
            .then_some(IconName::Telegram)
    }

    fn icon_tooltip(&self, _window: &Window, cx: &App) -> Option<SharedString> {
        Some(tr(cx, "workspace.dock.panel.telegram", "Telegram Panel").into())
    }

    fn icon_label(&self, _window: &Window, cx: &App) -> Option<String> {
        if !TelegramPanelSettings::get_global(cx).show_unread_badge {
            return None;
        }
        (self.view_model.unread_total > 0).then(|| self.view_model.unread_total.to_string())
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn starts_open(&self, _window: &Window, cx: &App) -> bool {
        TelegramPanelSettings::get_global(cx).starts_open
    }

    fn set_active(&mut self, active: bool, _window: &mut Window, cx: &mut Context<Self>) {
        if active {
            self.start_engine();
        }
        cx.notify();
    }

    fn activation_priority(&self) -> u32 {
        4
    }

    fn hide_button_setting(&self, _cx: &App) -> Option<HideStatusItem> {
        Some(HideStatusItem::new(|settings| {
            settings.telegram_panel.get_or_insert_default().button = Some(false);
        }))
    }
}

/// Registers the panel's actions on a workspace.
pub fn register(workspace: &mut Workspace, _cx: &mut Context<Workspace>) {
    workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
        workspace.toggle_panel_focus::<TelegramPanel>(window, cx);
    });
    workspace.register_action(|workspace, _: &Refresh, window, cx| {
        with_panel(workspace, window, cx, |panel, _window, cx| {
            panel.start_engine();
            panel.engine.send(Command::RefreshChats);
            cx.notify();
        });
    });
    workspace.register_action(|workspace, _: &Send, window, cx| {
        with_panel(workspace, window, cx, |panel, window, cx| {
            panel.send(window, cx)
        });
    });
    workspace.register_action(|workspace, _: &NextChat, window, cx| {
        with_panel(workspace, window, cx, |panel, _window, cx| {
            panel.select_relative_chat(1, cx)
        });
    });
    workspace.register_action(|workspace, _: &PreviousChat, window, cx| {
        with_panel(workspace, window, cx, |panel, _window, cx| {
            panel.select_relative_chat(-1, cx)
        });
    });
    workspace.register_action(|workspace, _: &Back, window, cx| {
        with_panel(workspace, window, cx, |panel, window, cx| {
            panel.back(window, cx)
        });
    });
}

fn with_panel(
    workspace: &mut Workspace,
    _window: &mut Window,
    cx: &mut Context<Workspace>,
    f: impl FnOnce(&mut TelegramPanel, &mut Window, &mut Context<TelegramPanel>),
) {
    if let Some(panel) = workspace.panel::<TelegramPanel>(cx) {
        panel.update(cx, |panel, cx| f(panel, _window, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::{build_history_layout, duration_label, error_text, media_kind_label};
    use gpui::TestAppContext;
    use settings::SettingsStore;
    use telegram::{EngineError, MediaKind, MessageSnapshot, SendState};

    fn message(id: i32, sender: &str, timestamp_unix: i64) -> MessageSnapshot {
        MessageSnapshot {
            id,
            chat_id: 1,
            outgoing: false,
            sender_name: sender.to_owned(),
            timestamp_unix,
            text: String::new(),
            markdown: None,
            media: None,
            send_state: SendState::Sent,
            local_id: None,
            edited: false,
        }
    }

    #[test]
    fn duration_rolls_into_hours() {
        assert_eq!(duration_label(None), "0:00");
        assert_eq!(duration_label(Some(65.0)), "1:05");
        assert_eq!(duration_label(Some(3600.0)), "1:00:00");
    }

    #[test]
    fn history_layout_groups_consecutive_sends_and_marks_days() {
        let day = 86_400;
        let history = vec![
            message(1, "Ada", day),
            message(2, "Ada", day + 60),
            message(3, "Ada", day + 120),
            message(4, "Ada", day + 5 * 86_400),
        ];
        let layout = build_history_layout(&history);
        assert!(layout[0].day_separator.is_some());
        assert!(layout[0].show_sender);
        assert!(layout[1].day_separator.is_none());
        assert!(!layout[1].show_sender);
        assert!(!layout[2].show_sender);
        assert!(layout[3].day_separator.is_some());
        assert!(layout[3].show_sender);
    }

    fn init_i18n(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
            i18n::init(cx);
        });
    }

    #[gpui::test]
    fn error_text_maps_variants_to_locale_keys(cx: &mut TestAppContext) {
        init_i18n(cx);
        cx.update(|cx| {
            assert_eq!(
                error_text(&EngineError::InvalidPhone, None, cx),
                i18n::tr(
                    cx,
                    "telegram_panel.error.invalid_phone",
                    "Enter the phone number in international format, including the country code.",
                )
            );
            assert_eq!(
                error_text(&EngineError::FloodWait { seconds: 5 }, Some(5), cx),
                i18n::tr(
                    cx,
                    "telegram_panel.error.flood_wait",
                    "Too many requests. Try again in {} seconds.",
                )
                .replacen("{}", "5", 1)
            );
        });
    }

    #[gpui::test]
    fn media_kind_labels_use_locale_keys(cx: &mut TestAppContext) {
        init_i18n(cx);
        cx.update(|cx| {
            assert_eq!(
                media_kind_label(MediaKind::Voice, cx),
                i18n::tr(cx, "telegram_panel.media.voice", "Voice message")
            );
            assert_eq!(
                media_kind_label(MediaKind::Geo, cx),
                i18n::tr(cx, "telegram_panel.media.geo", "Location")
            );
        });
    }
}
