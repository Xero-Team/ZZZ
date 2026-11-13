use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use acp_thread::MentionUri;
use editor::{
    ClipboardSelection, Editor, EditorEvent, EditorMode, FoldPlaceholder, SelectionEffects,
    actions::{Copy, Cut, Paste},
    display_map::{Crease, CreaseId, FoldId},
    scroll::Autoscroll,
};
use git::{BuildPermalinkParams, GitHostingProviderRegistry, parse_git_remote_url};
use gpui::{
    Action, AnyElement, App, Bounds, ClickEvent, ClipboardEntry, ClipboardItem, Context, ElementId,
    Entity, EventEmitter, ExternalPaths, FocusHandle, Focusable, FollowMode, Global, Hsla, Image,
    IntoElement, ListAlignment, ListState, ObjectFit, ParentElement, Pixels, Render, RenderImage,
    SharedString, Styled, Subscription, Task, TextStyleRefinement, WeakEntity, Window, actions,
    canvas, div, fill, img, list, point, px, relative, size,
};
use i18n::tr;
use language::Buffer;
use markdown::{Markdown, MarkdownElement, MarkdownFont, MarkdownStyle};
use multi_buffer::{MultiBuffer, MultiBufferOffset, ToOffset as _};
use rope::Point;
use settings::Settings as _;
use telegram::{
    AttachmentKind, AuthState, ChatSnapshot, Command, ComposerBlock, ConnectionState,
    DownloadState, EngineConfig, EngineError, EngineHandle, MediaKind, MediaSnapshot,
    MessageSnapshot, StagedAttachment, TelegramCredentials, ViewModel, code_reference_markdown,
    outgoing_message_count, plan_outgoing,
};
use text::ToOffset as _;
use time::UtcOffset;
use time_format::{TimestampFormat, format_local_timestamp, format_time};
use ui::prelude::*;
use ui::{
    AvatarStyle, Button, ButtonCommon, Callout, Clickable, CountBadge, Disableable, Disclosure,
    Icon, IconButton, Label, LabelCommon, ListItem, ListItemSpacing, Severity, Tab, Tooltip,
};
use ui::{ContextMenu, ContextMenuEntry, right_click_menu};
use util::ResultExt as _;
use workspace::dock::{DockPosition, Panel, PanelEvent};
use workspace::notifications::NotificationId;
use workspace::{
    DraggedSelection, DraggedTab, HideStatusItem, ItemHandle, NotificationWindowData,
    NotificationWindowDisplay, NotificationWindowManager, OpenOptions, Toast, Workspace,
};

use crate::telegram_panel_settings::{TelegramPanelSettings, TelegramSettings};

actions!(
    telegram_panel,
    [
        ToggleFocus,
        Send,
        NextChat,
        PreviousChat,
        Refresh,
        Back,
        InsertCodeReference,
        PastePlain,
    ]
);

const PANEL_KEY: &str = "TelegramPanel";
const SAME_SENDER_GROUP_SECONDS: i64 = 5 * 60;
/// The id of the General topic in a forum supergroup. It is not always part of
/// `messages.getForumTopics`, so the panel adds it as a leading chip.
const GENERAL_TOPIC_ID: i32 = 1;
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(150);
const HISTORY_AUTOLOAD_THRESHOLD: usize = 3;
/// Height cap for a photo or video preview before it is allowed to shrink the
/// bubble; taller media is scaled down to this height.
const MEDIA_MAX_HEIGHT: f32 = 320.;
/// The fixed height of one tile in an album grid.
const ALBUM_TILE_HEIGHT: f32 = 160.;
/// Stickers are small by nature and get a much shorter cap.
const STICKER_MAX_HEIGHT: f32 = 120.;
/// Very tall media is widened to this floor so a portrait photo never becomes
/// a sliver.
const MEDIA_MIN_WIDTH: f32 = 120.;
/// Very wide media is capped at this aspect ratio so a panorama does not ask
/// for an absurdly wide bubble.
const MEDIA_MAX_ASPECT: f32 = 2.5;
/// Horizontal space a media card adds around its preview (1px border each side
/// plus `p_2` padding). The card width is the preview width plus this inset so
/// the preview is not clipped.
const MEDIA_CARD_HORIZONTAL_INSET: f32 = 18.;
/// The voice bar keeps a fixed, Telegram-like width instead of sizing to its
/// content. It is derived from the panel's content width when one is set.
const VOICE_BAR_FALLBACK_WIDTH: f32 = 200.;
const VOICE_BAR_MIN_WIDTH: f32 = 160.;
const VOICE_BAR_MAX_WIDTH: f32 = 280.;
/// How often the playing voice bar refreshes its playhead.
const VOICE_TICK: Duration = Duration::from_millis(100);
/// Telegram rejects uploads larger than this.
const MAX_ATTACHMENT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// The prefix used for inline composer tokens, e.g. `[#3]`.
const COMPOSER_TOKEN_PREFIX: &str = "[#";

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

/// One transcript row. Album items that Telegram created together share a row
/// so they can be laid out as a grid instead of separate bubbles.
#[derive(Clone, Debug)]
struct HistoryRow {
    messages: Vec<MessageSnapshot>,
    /// The index of the first message in the underlying history, used to look
    /// up its [`MessageLayout`].
    start_index: usize,
}

impl HistoryRow {
    fn key(&self) -> MessageKey {
        self.messages
            .first()
            .map_or(MessageKey::Remote(0, 0), MessageKey::for_message)
    }
}

/// Identifies a message's cached Markdown entity. Optimistic sends share id `0`,
/// so they are keyed by their local id to keep concurrent sends apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MessageKey {
    Remote(i64, i32),
    Local(i64, u64),
}

impl MessageKey {
    fn for_message(message: &MessageSnapshot) -> Self {
        match message.local_id {
            Some(local_id) => Self::Local(message.chat_id, local_id),
            None => Self::Remote(message.chat_id, message.id),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingScroll {
    conversation_chat_id: i64,
    message_chat_id: i64,
    message_id: i32,
}

/// The message currently being edited and the composer text to restore if the
/// edit is cancelled.
#[derive(Clone, Debug)]
struct EditingMessage {
    chat_id: i64,
    message_id: i32,
    original_text: String,
}

/// A code reference staged in the composer. Its [`target`](Self::target) is a
/// remote permalink when the file belongs to a repository with a remote, or a
/// worktree-relative path otherwise.
#[derive(Clone, Debug)]
struct CodeReference {
    display_name: String,
    language: Option<String>,
    target: String,
    text: String,
}

/// Something staged in the composer, positioned in the document by a `[#id]`
/// token in the editor's text.
#[derive(Clone, Debug)]
enum ComposerItem {
    Code(CodeReference),
    Attachment(StagedAttachment),
}

/// The Telegram panel view.
pub struct TelegramPanel {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    fs: Arc<dyn fs::Fs>,
    engine: Arc<EngineHandle>,
    view_model: Arc<ViewModel>,
    mode: ViewMode,
    chat_list_state: ListState,
    history_list_state: ListState,
    search_list_state: ListState,
    chat_count: usize,
    search_count: usize,
    history_row_ids: Vec<MessageKey>,
    history_rows: Arc<Vec<HistoryRow>>,
    history_layout: Arc<Vec<MessageLayout>>,
    markdown_cache: HashMap<MessageKey, Entity<Markdown>>,
    composer: Entity<Editor>,
    composer_expanded: bool,
    /// Code references and attachments staged in the composer, keyed by a
    /// monotonic id that appears as a hidden `[#id]` token in the editor text.
    composer_items: BTreeMap<u64, ComposerItem>,
    /// The editor crease that renders each staged item's inline chip.
    composer_creases: HashMap<u64, CreaseId>,
    composer_next_id: u64,
    phone_editor: Entity<Editor>,
    code_editor: Entity<Editor>,
    password_editor: Entity<Editor>,
    search_editor: Entity<Editor>,
    search_open: bool,
    qr_cache: Option<(String, Arc<RenderImage>)>,
    _composer_subscription: Subscription,
    _search_subscription: Subscription,
    _notification_release: Subscription,
    _activation_subscription: Option<Subscription>,
    _search_task: Option<Task<()>>,
    _snapshot_task: Task<()>,
    voice_playback: Option<audio::PlaybackHandle>,
    playing_voice: Option<MessageKey>,
    /// Drives playhead updates and resets the bar when playback ends.
    _voice_task: Option<Task<()>>,
    /// The moment the current flood-wait pause ends, when one is active.
    flood_wait_deadline: Option<std::time::Instant>,
    /// Ticks the flood-wait countdown and dismisses the error once it ends.
    _flood_task: Option<Task<()>>,
    /// A voice message the user asked to play before its file was downloaded.
    pending_voice: Option<(i64, i32)>,
    /// Media attachments whose card is expanded, keyed by chat and message.
    expanded_media: HashSet<(i64, i32)>,
    /// Album messages whose thumbnail was already requested, so a missing or
    /// failed preview does not retry on every snapshot.
    requested_thumbnails: HashSet<(i64, i32)>,
    last_unread_total: u32,
    /// The selected chat and topic of the previous snapshot, to detect
    /// conversation switches and reload the matching draft.
    selected_chat: Option<i64>,
    selected_topic: Option<i32>,
    /// A message to reveal once its chat history has loaded.
    pending_scroll: Option<PendingScroll>,
    notification_windows: NotificationWindowManager,
    /// The message the next send replies to.
    reply_to: Option<MessageSnapshot>,
    /// The message being edited, together with the text it had when editing
    /// started so cancelling can restore the composer.
    editing: Option<EditingMessage>,
    /// Selected messages while multi-select is active. An empty set means
    /// multi-select is off.
    selection: HashSet<i32>,
    /// The id of the oldest unread incoming message when the chat was opened,
    /// used to draw the unread divider until it is marked read.
    unread_divider: Option<(i64, i32)>,
    /// The last visibility tuple reported to the engine, to avoid re-sending it
    /// on every render.
    reported_viewing: Option<(Option<i64>, bool, bool)>,
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
        // Read through the borrowed workspace: `load` runs inside
        // `Workspace::update`, and re-entering the workspace's entity from the
        // constructor would panic while it is leased.
        let language_registry = workspace.project().read(cx).languages().clone();
        cx.new(|cx| Self::new_inner(workspace.weak_handle(), fs, language_registry, window, cx))
    }

    fn new_inner(
        workspace: WeakEntity<Workspace>,
        fs: Arc<dyn fs::Fs>,
        language_registry: Arc<language::LanguageRegistry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let engine = engine(cx);
        // Subscribe before reading the current value: a publish that lands in
        // between would otherwise be seen by the receiver as already handled.
        let mut snapshot_rx = engine.subscribe();
        let snapshot = engine.snapshot();

        let composer_min_lines = TelegramPanelSettings::get_global(cx)
            .composer_min_lines
            .max(1);
        let composer = cx.new(|cx| build_composer(composer_min_lines, window, cx));
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
                    this.prune_composer_items(cx);
                    if let Some(chat_id) = this.view_model.selected_chat {
                        let text = this.composer.read(cx).text(cx);
                        let draft = composer_draft_text(&text, &this.composer_items);
                        this.engine.send(Command::SetDraft {
                            chat_id,
                            topic_id: this.view_model.selected_topic,
                            text: draft,
                        });
                        this.engine.send(Command::SetTyping {
                            chat_id,
                            typing: !text.trim().is_empty(),
                        });
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

        let _notification_release = cx.on_release(|this, cx| {
            this.notification_windows.dismiss_all(cx);
        });

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

        let mut this = Self {
            focus_handle: cx.focus_handle(),
            workspace,
            fs,
            engine,
            view_model: snapshot,
            mode: ViewMode::ChatList,
            chat_list_state: ListState::new(0, ListAlignment::Top, px(100.)),
            history_list_state: ListState::new(0, ListAlignment::Bottom, px(2048.)),
            search_list_state: ListState::new(0, ListAlignment::Top, px(100.)),
            chat_count: 0,
            search_count: 0,
            history_row_ids: Vec::new(),
            history_rows: Arc::new(Vec::new()),
            history_layout: Arc::new(Vec::new()),
            markdown_cache: HashMap::new(),
            composer,
            composer_expanded: false,
            composer_items: BTreeMap::new(),
            composer_creases: HashMap::new(),
            composer_next_id: 0,
            phone_editor,
            code_editor,
            password_editor,
            search_editor,
            search_open: false,
            qr_cache: None,
            _composer_subscription,
            _search_subscription,
            _notification_release,
            _activation_subscription: None,
            _search_task: None,
            _snapshot_task,
            voice_playback: None,
            playing_voice: None,
            _voice_task: None,
            flood_wait_deadline: None,
            _flood_task: None,
            pending_voice: None,
            expanded_media: HashSet::new(),
            requested_thumbnails: HashSet::new(),
            last_unread_total: 0,
            selected_chat: None,
            selected_topic: None,
            pending_scroll: None,
            notification_windows: NotificationWindowManager::default(),
            reply_to: None,
            editing: None,
            selection: HashSet::new(),
            unread_divider: None,
            reported_viewing: None,
        };
        let panel = cx.entity().downgrade();
        this.history_list_state
            .set_scroll_handler(move |event, window, cx| {
                let should_load_older = event.visible_range.start <= HISTORY_AUTOLOAD_THRESHOLD;
                if should_load_older {
                    let panel = panel.clone();
                    cx.defer(move |cx| {
                        panel
                            .update(cx, |panel, cx| panel.maybe_load_older(cx))
                            .log_err();
                    });
                }
                // The scroll handler runs while ListState's RefCell is mutably
                // borrowed, so report_viewing (which reads is_following_tail)
                // must be deferred until the borrow is released.
                let panel = panel.clone();
                window.defer(cx, move |window, cx| {
                    panel
                        .update(cx, |panel, cx| panel.report_viewing(window, cx))
                        .log_err();
                });
            });
        // Adopt the state the engine already has. A panel is created after the
        // engine has connected in every window after the first, and the engine
        // only publishes on changes, so waiting for the next snapshot would
        // leave this panel looking unconfigured.
        this.sync_chat_list();
        this.sync_history();
        this.request_album_thumbnails();
        this.rebuild_markdown_with(Some(language_registry), cx);
        this.sync_draft(window, cx);
        this.sync_flood_wait(cx);
        this.last_unread_total = this.view_model.unread_total;
        if this.view_model.selected_chat.is_some() {
            this.mode = ViewMode::Conversation;
        }
        this._activation_subscription =
            Some(cx.observe_window_activation(window, |this, window, cx| {
                this.report_viewing(window, cx);
            }));
        this.report_viewing(window, cx);
        this
    }

    fn apply_snapshot(
        &mut self,
        snapshot: Arc<ViewModel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_chats = self.view_model.chats.clone();
        self.view_model = snapshot;
        self.notify_new_messages(&previous_chats, window, cx);
        self.sync_chat_list();
        self.sync_history();
        self.request_album_thumbnails();
        self.rebuild_markdown(cx);
        self.prune_expanded_media();
        self.sync_draft(window, cx);
        self.reveal_pending_message();
        self.maybe_autoplay_voice(cx);
        self.sync_flood_wait(cx);
        // Mark the divider consumed once the chat is no longer unread.
        if self.unread_divider.is_some_and(|(chat_id, _)| {
            self.view_model
                .chats
                .iter()
                .find(|chat| chat.id == chat_id)
                .is_none_or(|chat| chat.unread_count == 0)
        }) {
            self.unread_divider = None;
        }
        self.report_viewing(window, cx);
        cx.notify();
    }

    /// Reports focus and scroll position to the engine. The engine only marks a
    /// chat read while the panel is focused on it and following the newest
    /// message, so background messages keep their unread badge.
    fn report_viewing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let search_visible =
            self.search_open && !self.search_editor.read(cx).text(cx).trim().is_empty();
        let chat_id = if self.mode == ViewMode::Conversation
            && !search_visible
            && self.view_model.auth.is_signed_in()
        {
            self.view_model.selected_chat
        } else {
            None
        };
        let focused = window.is_window_active() && self.focus_handle.contains_focused(window, cx);
        let at_bottom = self.history_list_state.is_following_tail();
        let state = (chat_id, focused, at_bottom);
        if self.reported_viewing == Some(state) {
            return;
        }
        self.reported_viewing = Some(state);
        self.engine.send(Command::SetViewing {
            chat_id,
            focused,
            at_bottom,
        });
    }

    /// Loads the stored draft into the composer when the chat or forum topic
    /// changes, so text never leaks from one conversation into another.
    fn sync_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.view_model.selected_chat;
        let selected_topic = self.view_model.selected_topic;
        if selected == self.selected_chat && selected_topic == self.selected_topic {
            return;
        }
        let chat_changed = selected != self.selected_chat;
        self.selected_chat = selected;
        self.selected_topic = selected_topic;
        self.reply_to = None;
        self.editing = None;
        self.selection.clear();
        if chat_changed {
            self.unread_divider = self.view_model.selected_chat_snapshot().and_then(|chat| {
                (chat.unread_count > 0).then_some((chat.id, chat.read_inbox_message_id))
            });
        }
        // Staged attachments belong to the conversation they were added in;
        // like the text draft, they must not follow into a different chat or
        // forum topic.
        self.clear_composer_items(cx);
        let draft = self.view_model.draft.clone();
        if self.composer.read(cx).text(cx) != draft {
            self.composer
                .update(cx, |editor, cx| editor.set_text(draft, window, cx));
        }
    }

    /// Drops every staged composer item and its inline chip crease.
    fn clear_composer_items(&mut self, cx: &mut Context<Self>) {
        if self.composer_items.is_empty() && self.composer_creases.is_empty() {
            return;
        }
        self.composer_items.clear();
        let crease_ids: Vec<CreaseId> = self.composer_creases.drain().map(|(_, id)| id).collect();
        if !crease_ids.is_empty() {
            self.composer.update(cx, |editor, cx| {
                editor.remove_creases(crease_ids, cx);
            });
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
        let Some(pending) = self.pending_scroll else {
            return;
        };
        if self.view_model.selected_chat != Some(pending.conversation_chat_id) {
            return;
        }
        if let Some(index) = history_row_index(
            &self.history_rows,
            pending.message_chat_id,
            pending.message_id,
        ) {
            self.history_list_state.scroll_to_reveal_item(index);
            self.pending_scroll = None;
        }
    }

    fn notify_new_messages(
        &mut self,
        previous_chats: &[ChatSnapshot],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_total = self.last_unread_total;
        self.last_unread_total = self.view_model.unread_total;
        if !self.view_model.auth.is_signed_in() {
            return;
        }
        if self.view_model.unread_total <= previous_total {
            return;
        }
        if (window.is_window_active() && self.focus_handle.contains_focused(window, cx))
            || !self.notification_windows.is_empty()
        {
            return;
        }
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
        let dismiss_label = tr(cx, "telegram_panel.toast.dismiss", "Dismiss");
        let workspace_handle = self.workspace.clone();
        self.notification_windows.show(
            NotificationWindowData {
                title: title.into(),
                caption: body.into(),
                context: None,
                icon: IconName::Telegram,
                view_label: open_label.into(),
                dismiss_label: dismiss_label.into(),
            },
            NotificationWindowDisplay::Primary,
            false,
            window,
            cx,
            move |this, event, window, cx| match event {
                ui::NotificationWindowEvent::Accepted => {
                    cx.activate(true).log_err();
                    // This callback runs with the panel already on the update
                    // stack, so revealing it here would re-enter its own update
                    // through `PanelHandle::set_active`.
                    let workspace_handle = workspace_handle.clone();
                    cx.defer_in(window, move |_, window, cx| {
                        window.activate_window();
                        let Some(workspace) = workspace_handle.upgrade() else {
                            return;
                        };
                        workspace.update(cx, |workspace, cx| {
                            workspace.reveal_panel::<TelegramPanel>(window, cx);
                            workspace.focus_panel::<TelegramPanel>(window, cx);
                        });
                    });
                    this.notification_windows.dismiss_all(cx);
                }
                ui::NotificationWindowEvent::Dismissed => {
                    this.notification_windows.dismiss_all(cx);
                }
            },
            |_, _, _| Vec::new(),
        );
    }

    fn sync_chat_list(&mut self) {
        let new_count = self.view_model.chats.len();
        if new_count != self.chat_count {
            self.chat_list_state.splice(0..self.chat_count, new_count);
            self.chat_count = new_count;
        }
    }

    fn sync_history(&mut self) {
        let rows = build_history_rows(&self.view_model.history);
        let new_ids: Vec<MessageKey> = rows.iter().map(HistoryRow::key).collect();
        let previous = std::mem::replace(&mut self.history_row_ids, new_ids.clone());
        self.history_rows = Arc::new(rows);
        self.history_layout = Arc::new(build_history_layout(&self.view_model.history));
        if previous == new_ids {
            self.history_list_state.remeasure_items(0..new_ids.len());
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
    }

    /// Requests thumbnails for album tiles, which have no expandable media card
    /// to trigger the usual on-demand preview. The set of requested messages
    /// prevents a retry on every snapshot when Telegram has no thumbnail.
    fn request_album_thumbnails(&mut self) {
        let mut requests = Vec::new();
        for message in &self.view_model.history {
            if message.grouped_id.is_none() {
                continue;
            }
            let Some(media) = &message.media else {
                continue;
            };
            if !media.is_previewable()
                || (media.downloaded_path.is_some() && !uses_thumbnail_only(media))
                || media.thumbnail_state == DownloadState::Downloading
                || media
                    .thumbnail_path
                    .as_deref()
                    .is_some_and(|path| path.is_file())
            {
                continue;
            }
            let key = (message.chat_id, message.id);
            if self.requested_thumbnails.insert(key) {
                requests.push(key);
            }
        }
        for (chat_id, message_id) in requests {
            self.engine.send(Command::DownloadThumbnail {
                chat_id,
                message_id,
            });
        }
    }

    /// Keeps the markdown entity for each formatted message in sync with its
    /// source, replacing the entity when a message is edited.
    fn rebuild_markdown(&mut self, cx: &mut Context<Self>) {
        // The language registry resolves fenced code block languages so the
        // renderer can syntax-highlight them. Without it every code block is
        // drawn as plain text.
        let language_registry = self
            .workspace
            .upgrade()
            .map(|workspace| workspace.read(cx).project().read(cx).languages().clone());
        self.rebuild_markdown_with(language_registry, cx);
    }

    fn rebuild_markdown_with(
        &mut self,
        language_registry: Option<Arc<language::LanguageRegistry>>,
        cx: &mut Context<Self>,
    ) {
        let sources: Vec<(MessageKey, SharedString)> = self
            .view_model
            .history
            .iter()
            .chain(
                self.view_model
                    .search_results
                    .iter()
                    .filter_map(|hit| hit.message.as_ref()),
            )
            .filter_map(|message| {
                message.markdown.as_ref().map(|markdown| {
                    (
                        MessageKey::for_message(message),
                        SharedString::from(markdown.clone()),
                    )
                })
            })
            .collect();

        let live: HashSet<MessageKey> = sources.iter().map(|(key, _)| *key).collect();
        for (key, source) in sources {
            let needs_replace = self
                .markdown_cache
                .get(&key)
                .is_none_or(|entity| entity.read(cx).source() != source.as_str());
            if needs_replace {
                let entity =
                    cx.new(|cx| Markdown::new(source, language_registry.clone(), None, cx));
                self.markdown_cache.insert(key, entity);
            }
        }
        self.markdown_cache.retain(|key, _| live.contains(key));
    }

    fn start_engine(&mut self) {
        self.engine.send(Command::Start);
    }

    /// Sends a command to the engine and schedules a re-render.
    fn dispatch(&mut self, command: Command, cx: &mut Context<Self>) {
        self.engine.send(command);
        cx.notify();
    }

    /// The current chat list, used by the forward picker.
    pub fn chats(&self) -> &[ChatSnapshot] {
        &self.view_model.chats
    }

    fn forward_message(
        &mut self,
        chat_id: i64,
        message_ids: Vec<i32>,
        drop_author: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let message_ids: Vec<i32> = message_ids
            .into_iter()
            .filter(|message_id| *message_id != 0)
            .collect();
        if message_ids.is_empty() {
            return;
        }
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        workspace.update(cx, |workspace, cx| {
            crate::telegram_forward_picker::open(
                workspace,
                chat_id,
                message_ids,
                drop_author,
                window,
                cx,
            );
        });
    }

    fn open_chat(&mut self, chat_id: i64, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .pending_scroll
            .is_some_and(|pending| pending.conversation_chat_id != chat_id)
        {
            self.pending_scroll = None;
        }
        self.start_engine();
        self.mode = ViewMode::Conversation;
        if self.search_open {
            self.search_open = false;
            self.engine.send(Command::ClearSearch);
        }
        self.reply_to = None;
        self.editing = None;
        self.selection.clear();
        self.requested_thumbnails.clear();
        window.focus(&self.focus_handle, cx);
        self.engine.send(Command::SelectChat { chat_id });
        self.history_list_state.set_follow_mode(FollowMode::Tail);
        self.history_list_state.scroll_to_end();
        cx.notify();
    }

    /// Opens a chat from a search hit and reveals the matched message once its
    /// history is loaded.
    fn open_chat_at(
        &mut self,
        chat_id: i64,
        message: MessageSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_open = false;
        self.engine.send(Command::ClearSearch);
        self.pending_scroll = Some(PendingScroll {
            conversation_chat_id: chat_id,
            message_chat_id: message.chat_id,
            message_id: message.id,
        });
        self.start_engine();
        self.mode = ViewMode::Conversation;
        self.reply_to = None;
        self.editing = None;
        self.selection.clear();
        self.requested_thumbnails.clear();
        window.focus(&self.focus_handle, cx);
        self.engine.send(Command::SelectChatAt { chat_id, message });
        self.history_list_state.set_follow_mode(FollowMode::Tail);
        cx.notify();
        self.reveal_pending_message();
    }

    fn maybe_load_older(&mut self, cx: &mut Context<Self>) {
        if !self.view_model.has_more_history || self.view_model.loading_history {
            return;
        }
        let Some(chat_id) = self.view_model.selected_chat else {
            return;
        };
        self.engine.send(Command::LoadOlder { chat_id });
        cx.notify();
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == ViewMode::Conversation {
            self.mode = ViewMode::ChatList;
            self.reply_to = None;
            self.editing = None;
            self.selection.clear();
            self.report_viewing(window, cx);
            cx.notify();
        } else if let Some(workspace) = self.workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                workspace.close_panel::<TelegramPanel>(window, cx);
            });
        }
    }

    /// Starts editing one of the account's own text messages.
    fn start_edit(
        &mut self,
        message: &MessageSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !message.outgoing || message.id == 0 {
            return;
        }
        if self.composer.read(cx).text(cx).trim().is_empty() {
            let text = message
                .markdown
                .clone()
                .unwrap_or_else(|| message.text.clone());
            self.composer
                .update(cx, |editor, cx| editor.set_text(text.clone(), window, cx));
            self.editing = Some(EditingMessage {
                chat_id: message.chat_id,
                message_id: message.id,
                original_text: String::new(),
            });
        } else {
            // Do not silently replace an unsent draft.
            self.editing = Some(EditingMessage {
                chat_id: message.chat_id,
                message_id: message.id,
                original_text: self.composer.read(cx).text(cx),
            });
            let text = message
                .markdown
                .clone()
                .unwrap_or_else(|| message.text.clone());
            self.composer
                .update(cx, |editor, cx| editor.set_text(text, window, cx));
        }
        self.reply_to = None;
        window.focus(&self.composer.focus_handle(cx), cx);
        cx.notify();
    }

    fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editing) = self.editing.take() else {
            return;
        };
        self.composer.update(cx, |editor, cx| {
            editor.set_text(editing.original_text, window, cx)
        });
        cx.notify();
    }

    /// Sets the message the next send replies to.
    fn start_reply(
        &mut self,
        message: &MessageSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if message.id == 0 {
            return;
        }
        self.editing = None;
        self.reply_to = Some(message.clone());
        window.focus(&self.composer.focus_handle(cx), cx);
        cx.notify();
    }

    fn clear_reply(&mut self, cx: &mut Context<Self>) {
        self.reply_to = None;
        cx.notify();
    }

    /// Scrolls to a message in the loaded history, e.g. from a reply preview.
    fn scroll_to_message(&mut self, message_chat_id: i64, message_id: i32, cx: &mut Context<Self>) {
        if let Some(index) = history_row_index(&self.history_rows, message_chat_id, message_id) {
            self.history_list_state.scroll_to_reveal_item(index);
            cx.notify();
        }
    }

    fn enter_selection(&mut self, message_id: i32, cx: &mut Context<Self>) {
        if message_id == 0 {
            return;
        }
        self.selection.clear();
        self.selection.insert(message_id);
        self.reply_to = None;
        self.editing = None;
        cx.notify();
    }

    fn toggle_selection(&mut self, message_id: i32, cx: &mut Context<Self>) {
        if message_id == 0 {
            return;
        }
        if !self.selection.remove(&message_id) {
            self.selection.insert(message_id);
        }
        cx.notify();
    }

    fn exit_selection(&mut self, cx: &mut Context<Self>) {
        if self.selection.is_empty() {
            return;
        }
        self.selection.clear();
        cx.notify();
    }

    fn selected_message_ids(&self) -> Vec<i32> {
        let mut ids: Vec<i32> = self
            .view_model
            .history
            .iter()
            .filter(|message| self.selection.contains(&message.id))
            .map(|message| message.id)
            .collect();
        ids.sort_unstable();
        ids
    }

    fn delete_selection(&mut self, revoke: bool, cx: &mut Context<Self>) {
        let Some(chat_id) = self.view_model.selected_chat else {
            return;
        };
        let message_ids = self.selected_message_ids();
        if message_ids.is_empty() {
            return;
        }
        self.dispatch(
            Command::DeleteMessages {
                chat_id,
                message_ids,
                revoke,
            },
            cx,
        );
        self.selection.clear();
        cx.notify();
    }

    fn forward_selection(
        &mut self,
        drop_author: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(chat_id) = self.view_model.selected_chat else {
            return;
        };
        let message_ids = self.selected_message_ids();
        if message_ids.is_empty() {
            return;
        }
        self.forward_message(chat_id, message_ids, drop_author, window, cx);
        self.selection.clear();
        cx.notify();
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(chat_id) = self.view_model.selected_chat else {
            return;
        };
        if self.view_model.connection != ConnectionState::Connected {
            self.start_engine();
            return;
        }
        if self
            .view_model
            .selected_chat_snapshot()
            .is_some_and(|chat| !chat.can_send)
            || self.selected_topic_closed()
        {
            return;
        }
        let text = self.composer.read(cx).text(cx);
        self.start_engine();

        if let Some(editing) = self.editing.clone() {
            if text.trim().is_empty() {
                self.cancel_edit(window, cx);
                return;
            }
            self.engine.send(Command::EditMessage {
                chat_id: editing.chat_id,
                message_id: editing.message_id,
                text,
            });
            self.editing = None;
            self.clear_composer_items(cx);
            self.composer
                .update(cx, |editor, cx| editor.set_text("", window, cx));
            cx.notify();
            return;
        }

        if text.trim().is_empty() && self.composer_items.is_empty() {
            return;
        }
        let blocks = composer_blocks(&text, &self.composer_items);
        let items = plan_outgoing(blocks);
        if items.is_empty() {
            return;
        }
        let reply_to = self.reply_to.as_ref().map(|message| message.id);
        self.engine.send(Command::SendOutgoing {
            chat_id,
            items,
            reply_to,
        });
        self.reply_to = None;
        self.clear_composer_items(cx);
        self.composer
            .update(cx, |editor, cx| editor.set_text("", window, cx));
        self.unread_divider = None;
        self.history_list_state.set_follow_mode(FollowMode::Tail);
        self.history_list_state.scroll_to_end();
        cx.notify();
    }

    /// Stages an item, inserting a hidden token and an inline chip crease over
    /// it at the cursor.
    fn stage_composer_item(
        &mut self,
        item: ComposerItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.composer_next_id;
        self.composer_next_id = self.composer_next_id.wrapping_add(1);
        let token = format!("{COMPOSER_TOKEN_PREFIX}{id}]");
        let (label, icon) = match &item {
            ComposerItem::Code(reference) => (
                SharedString::from(reference.display_name.clone()),
                IconName::Code,
            ),
            ComposerItem::Attachment(attachment) => (
                SharedString::from(attachment.file_name.clone()),
                attachment_icon(attachment.kind),
            ),
        };
        self.composer_items.insert(id, item);
        let panel = cx.entity().downgrade();
        let crease_id = self.composer.update(cx, |editor, cx| {
            editor.insert(&format!("{token} "), window, cx);
            let snapshot = editor.buffer().read(cx).snapshot(cx);
            // Locate the token we just inserted from the cursor rather than the
            // first textual match, which could be an older literal `[#id]`.
            let head = editor
                .selections
                .newest::<MultiBufferOffset>(&editor.display_snapshot(cx))
                .head();
            let offset = head.0.checked_sub(token.len() + 1)?;
            let start = snapshot.anchor_after(MultiBufferOffset(offset));
            let end = snapshot.anchor_before(MultiBufferOffset(offset + token.len()));
            let crease = Crease::Inline {
                range: start..end,
                placeholder: composer_chip_placeholder(id, label, icon, panel),
                render_toggle: None,
                render_trailer: None,
                metadata: None,
            };
            let ids = editor.insert_creases(vec![crease.clone()], cx);
            editor.fold_creases(vec![crease], false, window, cx);
            ids.first().copied()
        });
        if let Some(crease_id) = crease_id {
            self.composer_creases.insert(id, crease_id);
        }
        window.focus(&self.composer.focus_handle(cx), cx);
        cx.notify();
    }

    /// Removes a staged item, its inline chip crease, and its hidden token.
    fn remove_composer_item(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.composer_items.remove(&id);
        let crease_id = self.composer_creases.remove(&id);
        let token = format!("{COMPOSER_TOKEN_PREFIX}{id}]");
        let text = self.composer.read(cx).text(cx);
        self.composer.update(cx, |editor, cx| {
            let mut range: Option<std::ops::Range<MultiBufferOffset>> = None;
            if let Some(crease_id) = crease_id {
                if let Some((_, removed)) =
                    editor.remove_creases([crease_id], cx).into_iter().next()
                {
                    let snapshot = editor.buffer().read(cx).snapshot(cx);
                    range =
                        Some(removed.start.to_offset(&snapshot)..removed.end.to_offset(&snapshot));
                }
            }
            if let Some(range) = range.or_else(|| {
                text.find(&token)
                    .map(|start| MultiBufferOffset(start)..MultiBufferOffset(start + token.len()))
            }) {
                editor.edit([(range, "")], cx);
            }
        });
        window.focus(&self.composer.focus_handle(cx), cx);
        cx.notify();
    }

    /// Drops staged items whose tokens the user has edited away.
    fn prune_composer_items(&mut self, cx: &mut Context<Self>) {
        if self.composer_items.is_empty() {
            return;
        }
        let text = self.composer.read(cx).text(cx);
        let stale: Vec<u64> = self
            .composer_items
            .keys()
            .copied()
            .filter(|id| !text.contains(&format!("{COMPOSER_TOKEN_PREFIX}{id}]")))
            .collect();
        if stale.is_empty() {
            return;
        }
        let crease_ids: Vec<CreaseId> = stale
            .iter()
            .filter_map(|id| self.composer_creases.remove(id))
            .collect();
        for id in &stale {
            self.composer_items.remove(id);
        }
        if !crease_ids.is_empty() {
            self.composer.update(cx, |editor, cx| {
                editor.remove_creases(crease_ids, cx);
            });
        }
    }

    /// Stages a code reference from the active editor's selection.
    fn insert_code_reference(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let Some(reference) = active_editor_code_reference(&workspace, cx) else {
            self.notify_message(
                tr(
                    cx,
                    "telegram_panel.composer.no_selection",
                    "Select code in an editor first.",
                ),
                cx,
            );
            return;
        };
        self.stage_composer_item(ComposerItem::Code(reference), window, cx);
    }

    /// Handles a paste in the composer. A selection copied from a ZZZ editor is
    /// staged as a code reference, pasted images and files become attachments,
    /// and everything else falls through to the editor's normal paste.
    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        let Some(clipboard) = cx.read_from_clipboard() else {
            return;
        };
        cx.stop_propagation();
        self.paste_item(&clipboard, window, cx);
    }

    /// Pastes the clipboard as-is, without turning editor selections into code
    /// references or images into attachments.
    fn paste_plain(&mut self, _: &PastePlain, window: &mut Window, cx: &mut Context<Self>) {
        let Some(clipboard) = cx.read_from_clipboard() else {
            return;
        };
        cx.stop_propagation();
        self.composer.update(cx, |editor, cx| {
            editor.paste_item(&clipboard, window, cx);
        });
    }

    fn paste_item(
        &mut self,
        clipboard: &ClipboardItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A single selection copied from a ZZZ editor carries its file and line
        // range as metadata. Stage it as a code reference, matching the Agent
        // panel's selection mentions, instead of pasting the raw text.
        let editor_selection = clipboard.entries().iter().find_map(|entry| match entry {
            ClipboardEntry::String(text) => {
                let selections = text.metadata_json::<Vec<ClipboardSelection>>()?;
                let [selection] = selections.as_slice() else {
                    return None;
                };
                Some((selection.clone(), text.text().clone()))
            }
            _ => None,
        });
        if let Some((selection, text)) = editor_selection
            && self.paste_code_reference(selection, text, window, cx)
        {
            return;
        }

        let images = clipboard
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                ClipboardEntry::Image(image) if !image.bytes().is_empty() => Some(image.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        if !images.is_empty() {
            self.stage_clipboard_images(images, window, cx);
            return;
        }

        let external_paths = clipboard
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                ClipboardEntry::ExternalPaths(paths) => Some(paths.paths().to_vec()),
                _ => None,
            })
            .flatten()
            .collect::<Vec<_>>();
        if !external_paths.is_empty() {
            let is_local = self
                .workspace
                .upgrade()
                .is_some_and(|workspace| workspace.read(cx).project().read(cx).is_local());
            if is_local {
                self.stage_attachment_paths(external_paths, window, cx);
            } else {
                self.notify_message(
                    tr(
                        cx,
                        "telegram_panel.composer.remote_attachment",
                        "Only local files can be attached.",
                    ),
                    cx,
                );
            }
            return;
        }

        self.composer.update(cx, |editor, cx| {
            editor.paste_item(clipboard, window, cx);
        });
    }

    /// Stages a selection copied from a ZZZ editor as a code reference.
    /// Returns `true` when it was staged, `false` to fall back to a text paste.
    fn paste_code_reference(
        &mut self,
        selection: ClipboardSelection,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let (Some(absolute_path), Some(line_range)) = (selection.file_path, selection.line_range)
        else {
            return false;
        };
        let Some(workspace) = self.workspace.upgrade() else {
            return false;
        };
        let project = workspace.read(cx).project().clone();
        let language_registry = project.read(cx).languages().clone();
        cx.spawn_in(window, async move |this, cx| {
            let language = language_registry
                .load_language_for_file_path(&absolute_path)
                .await
                .ok()
                .map(|language| language.code_fence_block_name().to_string())
                .or_else(|| extension_language(&absolute_path));
            this.update_in(cx, |this, window, cx| {
                let relative_path = project
                    .read(cx)
                    .project_path_for_absolute_path(&absolute_path, cx)
                    .map_or_else(
                        || absolute_path.to_string_lossy().into_owned(),
                        |project_path| project_path.path.as_unix_str().to_owned(),
                    );
                let file_name = absolute_path.file_name().map_or_else(
                    || relative_path.clone(),
                    |name| name.to_string_lossy().into_owned(),
                );
                let start = line_range.start() + 1;
                let end = line_range.end() + 1;
                let display_name = if start == end {
                    format!("{file_name}:{start}")
                } else {
                    format!("{file_name}:{start}-{end}")
                };
                let target = code_target(&project, &absolute_path, &relative_path, &line_range, cx);
                this.stage_composer_item(
                    ComposerItem::Code(CodeReference {
                        display_name,
                        language,
                        target,
                        text,
                    }),
                    window,
                    cx,
                );
            })
            .log_err();
        })
        .detach();
        true
    }

    /// Persists pasted clipboard images under the cache and stages them as
    /// attachments, because the engine uploads from a path.
    fn stage_clipboard_images(
        &mut self,
        images: Vec<Image>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let directory = paths::telegram_cache_dir().join("outgoing");
        cx.spawn_in(window, async move |this, cx| {
            let staged = cx
                .background_spawn(async move {
                    if let Err(error) = std::fs::create_dir_all(&directory) {
                        log::warn!("failed to create the outgoing cache: {error}");
                        return Vec::new();
                    }
                    let mut staged = Vec::with_capacity(images.len());
                    for image in images {
                        let path = directory.join(format!(
                            "clipboard-{}.{}",
                            unique_suffix(),
                            image.format().extension()
                        ));
                        let bytes = image.bytes();
                        match std::fs::write(&path, bytes) {
                            Ok(()) => staged.push((path, bytes.len() as u64)),
                            Err(error) => log::warn!("failed to stage a pasted image: {error}"),
                        }
                    }
                    staged
                })
                .await;
            this.update_in(cx, |this, window, cx| {
                for (path, size) in staged {
                    if this.is_attachment_staged(&path) {
                        continue;
                    }
                    let Some(attachment) = StagedAttachment::from_path(path, size) else {
                        continue;
                    };
                    this.stage_composer_item(ComposerItem::Attachment(attachment), window, cx);
                }
            })
            .log_err();
        })
        .detach();
    }

    /// Stages a single file, reading its metadata through the workspace's
    /// filesystem so remote files work too. Directories are ignored and
    /// oversized files are rejected with a toast.
    fn stage_attachment_path(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_attachment_staged(&path) {
            return;
        }
        let fs = self.fs.clone();
        cx.spawn_in(window, async move |this, cx| {
            let metadata = match fs.metadata(&path).await {
                Ok(Some(metadata)) => metadata,
                Ok(None) => return,
                Err(error) => {
                    log::warn!("failed to read attachment {}: {error}", path.display());
                    return;
                }
            };
            if metadata.is_dir {
                return;
            }
            let size = metadata.len;
            this.update_in(cx, |this, window, cx| {
                if this.is_attachment_staged(&path) {
                    return;
                }
                if size > MAX_ATTACHMENT_BYTES {
                    this.notify_message(
                        tr(
                            cx,
                            "telegram_panel.composer.attachment_too_large",
                            "This file is too large to send.",
                        ),
                        cx,
                    );
                    return;
                }
                let Some(attachment) = StagedAttachment::from_path(path, size) else {
                    return;
                };
                this.stage_composer_item(ComposerItem::Attachment(attachment), window, cx);
            })
            .log_err();
        })
        .detach();
    }

    fn is_attachment_staged(&self, path: &Path) -> bool {
        self.composer_items
            .values()
            .any(|item| matches!(item, ComposerItem::Attachment(existing) if existing.path == path))
    }

    fn stage_attachment_paths(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for path in paths {
            self.stage_attachment_path(path, window, cx);
        }
    }

    /// Opens the platform file picker and stages the chosen files.
    fn pick_attachments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(tr(cx, "telegram_panel.composer.attach", "Attach files").into()),
        });
        cx.spawn_in(window, async move |this, cx| match receiver.await {
            Ok(Ok(Some(paths))) => {
                this.update_in(cx, |this, window, cx| {
                    this.stage_attachment_paths(paths, window, cx);
                })
                .log_err();
            }
            Ok(Ok(None)) => {}
            Ok(Err(error)) => log::warn!("failed to open the file picker: {error}"),
            Err(error) => log::warn!("the file picker was cancelled: {error}"),
        })
        .detach();
    }

    /// Shows a transient toast in the workspace.
    fn notify_message(&self, message: String, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        workspace.update(cx, |workspace, cx| {
            let toast = Toast::new(NotificationId::unique::<TelegramPanel>(), message).autohide();
            workspace.show_toast(toast, cx);
        });
    }

    fn toggle_composer_expanded(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer_expanded.toggle();
        let min_lines = TelegramPanelSettings::get_global(cx)
            .composer_min_lines
            .max(1);
        let max_lines = if self.composer_expanded {
            None
        } else {
            Some(min_lines * 2)
        };
        self.composer.update(cx, |editor, cx| {
            editor.set_mode(EditorMode::AutoHeight {
                min_lines,
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
                let filter = this.view_model.search_filter;
                this.engine.send(Command::Search {
                    query,
                    chat_id: None,
                    filter,
                });
                cx.notify();
            })
            .log_err();
        }));
    }

    fn logout(&mut self, cx: &mut Context<Self>) {
        self.engine.send(Command::Logout);
        self.start_engine();
        cx.notify();
    }

    fn select_relative_chat(&mut self, delta: i64, window: &mut Window, cx: &mut Context<Self>) {
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
        self.open_chat(chat_id, window, cx);
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
                media.is_previewable()
                    // A video-like file is never rendered inline, so its poster
                    // is still worth fetching after the original arrives.
                    && (media.downloaded_path.is_none() || uses_thumbnail_only(media))
                    && media
                        .thumbnail_path
                        .as_deref()
                        .is_none_or(|path| !path.is_file())
                    && media.thumbnail_state != DownloadState::Downloading
            });
        if needs_preview {
            self.engine.send(Command::DownloadThumbnail {
                chat_id,
                message_id,
            });
        }
    }

    /// Plays a downloaded voice message, replacing any current playback.
    fn start_voice_playback(
        &mut self,
        chat_id: i64,
        message_id: i32,
        path: &Path,
        cx: &mut Context<Self>,
    ) {
        self.stop_voice_playback();
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) => {
                log::warn!("failed to open voice message {}: {error}", path.display());
                cx.notify();
                return;
            }
        };
        let source = match rodio::Decoder::new(std::io::BufReader::new(file)) {
            Ok(source) => source,
            Err(error) => {
                log::warn!("failed to decode voice message {}: {error}", path.display());
                cx.notify();
                return;
            }
        };
        match audio::Audio::play_source(source, cx) {
            Ok(playback) => {
                let message_key = MessageKey::Remote(chat_id, message_id);
                self.voice_playback = Some(playback);
                self.playing_voice = Some(message_key);
                self.spawn_voice_tick(message_key, cx);
            }
            Err(error) => log::warn!("failed to play voice message: {error}"),
        }
        cx.notify();
    }

    /// Stops the current voice playback, if any, without notifying. The tick
    /// task ends on its own once `playing_voice` changes.
    fn stop_voice_playback(&mut self) {
        if let Some(playback) = self.voice_playback.take() {
            playback.stop();
        }
        self.playing_voice = None;
    }

    /// Refreshes the voice playhead while playing and clears it once playback
    /// finishes so the bar returns to its idle state.
    fn spawn_voice_tick(&mut self, message_key: MessageKey, cx: &mut Context<Self>) {
        self._voice_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(VOICE_TICK).await;
                let keep_going = this
                    .update(cx, |panel, cx| {
                        if panel.playing_voice != Some(message_key) {
                            return false;
                        }
                        let (finished, paused) = match panel.voice_playback.as_ref() {
                            Some(playback) => (playback.is_finished(), playback.is_paused()),
                            None => (true, false),
                        };
                        if finished {
                            panel.stop_voice_playback();
                            cx.notify();
                            return false;
                        }
                        if !paused {
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        }));
    }

    /// Toggles playback of a voice message, downloading it first when needed.
    fn toggle_voice(
        &mut self,
        chat_id: i64,
        message_id: i32,
        path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let message_key = MessageKey::Remote(chat_id, message_id);
        if self.playing_voice == Some(message_key) {
            if let Some(playback) = &self.voice_playback {
                if playback.is_paused() {
                    playback.resume();
                } else {
                    playback.pause();
                }
            }
            cx.notify();
            return;
        }
        if let Some(path) = path {
            self.start_voice_playback(chat_id, message_id, &path, cx);
        } else {
            self.pending_voice = Some((chat_id, message_id));
            self.engine.send(Command::DownloadMedia {
                chat_id,
                message_id,
            });
            cx.notify();
        }
    }

    /// Plays a voice message whose download the user requested, once its file
    /// arrives in a snapshot.
    fn maybe_autoplay_voice(&mut self, cx: &mut Context<Self>) {
        let Some((chat_id, message_id)) = self.pending_voice else {
            return;
        };
        let media = self
            .view_model
            .history
            .iter()
            .find(|message| message.chat_id == chat_id && message.id == message_id)
            .and_then(|message| message.media.as_ref());
        let Some(media) = media else {
            self.pending_voice = None;
            return;
        };
        match (&media.downloaded_path, media.download_state) {
            (Some(path), _) => {
                let path = path.clone();
                self.pending_voice = None;
                self.start_voice_playback(chat_id, message_id, &path, cx);
            }
            (None, DownloadState::Failed) => self.pending_voice = None,
            _ => {}
        }
    }

    fn sync_flood_wait(&mut self, cx: &mut Context<Self>) {
        if !matches!(
            self.view_model.error,
            Some(EngineError::FloodWait { .. } | EngineError::SlowMode { .. })
        ) {
            self.flood_wait_deadline = None;
            return;
        }
        if self.flood_wait_deadline.is_some() {
            return;
        }
        let seconds = self.view_model.flood_wait_seconds.unwrap_or(0).max(1);
        self.flood_wait_deadline = Some(std::time::Instant::now() + Duration::from_secs(seconds));
        self._flood_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let keep_going = this
                    .update(cx, |panel, cx| {
                        let Some(deadline) = panel.flood_wait_deadline else {
                            return false;
                        };
                        if std::time::Instant::now() >= deadline {
                            panel.flood_wait_deadline = None;
                            panel.engine.send(Command::DismissError);
                            cx.notify();
                            return false;
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        }));
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
                        this.search_open.toggle();
                        if this.search_open {
                            window.focus(&this.search_editor.focus_handle(cx), cx);
                            let query = this.search_editor.read(cx).text(cx);
                            if !query.trim().is_empty() {
                                let filter = this.view_model.search_filter;
                                this.engine.send(Command::Search {
                                    query,
                                    chat_id: None,
                                    filter,
                                });
                            }
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
                    Button::new(
                        "telegram-search-filter",
                        search_filter_label(self.view_model.search_filter, cx),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        let filter = next_search_filter(this.view_model.search_filter);
                        let query = this.search_editor.read(cx).text(cx);
                        this.dispatch(
                            Command::Search {
                                query,
                                chat_id: None,
                                filter,
                            },
                            cx,
                        );
                    })),
                )
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
            );
        }

        if !self.view_model.auth.is_signed_in() {
            return self.render_auth(cx);
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
        let folder_tabs = self.render_folder_tabs(cx);
        if self.view_model.chats.is_empty() {
            return v_flex()
                .size_full()
                .child(folder_tabs)
                .child(self.render_message_state(if self.view_model.loading_chats {
                    tr(cx, "telegram_panel.states.loading_chats", "Loading chats…")
                } else {
                    tr(cx, "telegram_panel.states.no_chats", "No chats yet")
                }))
                .into_any_element();
        }

        let chats = Arc::new(self.view_model.chats.clone());
        let selected = self.view_model.selected_chat;
        let panel = cx.entity().downgrade();

        let list_element = list(self.chat_list_state.clone(), move |index, _window, cx| {
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
            if chat.unread_mentions > 0 {
                end_slot = end_slot.child(
                    Label::new(format!("@{}", chat.unread_mentions))
                        .size(LabelSize::XSmall)
                        .color(Color::Accent),
                );
            }

            let panel_for_menu = panel.clone();
            let row = ListItem::new(ElementId::Name(format!("telegram-chat-{chat_id}").into()))
                .spacing(ListItemSpacing::Sparse)
                .toggle_state(is_selected)
                .on_click(move |_, window, cx| {
                    panel
                        .update(cx, |panel, cx| panel.open_chat(chat_id, window, cx))
                        .log_err();
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
                );
            let chat_for_menu = chat;
            right_click_menu(ElementId::Name(
                format!("telegram-chat-menu-{chat_id}").into(),
            ))
            .trigger(move |_, _, _| row.into_any_element())
            .menu(move |window, cx| {
                chat_context_menu(chat_for_menu.clone(), panel_for_menu.clone(), window, cx)
            })
            .into_any_element()
        })
        .size_full()
        .into_any_element();

        v_flex()
            .size_full()
            .child(folder_tabs)
            .child(list_element)
            .into_any_element()
    }

    /// The folder selector above the chat list: all chats, custom folders, and
    /// the archive.
    fn render_folder_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.view_model.selected_folder;
        let mut entries: Vec<(telegram::FolderSelection, String)> = vec![(
            telegram::FolderSelection::All,
            tr(cx, "telegram_panel.folder.all", "All"),
        )];
        entries.extend(self.view_model.folders.iter().map(|folder| {
            (
                telegram::FolderSelection::Folder(folder.id),
                folder.title.clone(),
            )
        }));
        entries.push((
            telegram::FolderSelection::Archive,
            tr(cx, "telegram_panel.folder.archive", "Archive"),
        ));

        let mut row = h_flex()
            .w_full()
            .flex_none()
            .px_2()
            .py_1()
            .gap_1()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().panel_background);
        for (index, (selection, title)) in entries.into_iter().enumerate() {
            let is_selected = selection == selected;
            row = row.child(
                div()
                    .id(ElementId::Name(
                        format!("telegram-folder-tab-{index}").into(),
                    ))
                    .px_2()
                    .py_0p5()
                    .rounded_md()
                    .cursor_pointer()
                    .when(is_selected, |this| {
                        this.bg(cx.theme().colors().element_selected)
                    })
                    .hover(|this| this.bg(cx.theme().colors().element_hover))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.dispatch(Command::SelectFolder { folder: selection }, cx)
                    }))
                    .child(
                        Label::new(title)
                            .size(LabelSize::XSmall)
                            .color(if is_selected {
                                Color::Default
                            } else {
                                Color::Muted
                            }),
                    ),
            );
        }
        row.into_any_element()
    }

    /// The forum topic selector for a supergroup. The General chip is always
    /// present so a topic view can be left without going back to the chat list.
    fn render_topic_bar(&self, chat: &ChatSnapshot, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !chat.is_forum {
            return None;
        }
        let selected = self.view_model.selected_topic;
        let panel = cx.entity().downgrade();
        let listed_unread: i32 = self
            .view_model
            .topics
            .iter()
            .filter(|topic| topic.id != GENERAL_TOPIC_ID)
            .map(|topic| topic.unread_count.max(0))
            .sum();
        let general_unread = if self.view_model.loading_topics && self.view_model.topics.is_empty()
        {
            // Until the topic list arrives, the chat total cannot be split.
            0
        } else {
            self.view_model
                .topics
                .iter()
                .find(|topic| topic.id == GENERAL_TOPIC_ID)
                .map_or_else(
                    || chat.unread_count.max(0).saturating_sub(listed_unread),
                    |topic| topic.unread_count.max(0),
                )
        };
        let general_title = if general_unread > 0 {
            format!(
                "{}  {general_unread}",
                tr(cx, "telegram_panel.topics.general", "General")
            )
        } else {
            tr(cx, "telegram_panel.topics.general", "General")
        };
        let mut bar = h_flex()
            .id("telegram-topic-bar")
            .w_full()
            .flex_none()
            .px_2()
            .py_1()
            .gap_1()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().panel_background)
            .overflow_x_scroll();
        bar = bar.child(render_topic_chip(
            panel.clone(),
            "telegram-topic-general".to_owned(),
            general_title,
            selected.is_none(),
            Command::SelectTopic {
                chat_id: chat.id,
                topic_id: None,
            },
            cx,
        ));
        for topic in &self.view_model.topics {
            // The General topic can be part of the server response; the
            // synthetic chip above already represents it.
            if topic.id == GENERAL_TOPIC_ID {
                continue;
            }
            let title = if topic.unread_count > 0 {
                format!("{}  {}", topic.title, topic.unread_count)
            } else {
                topic.title.clone()
            };
            bar = bar.child(render_topic_chip(
                panel.clone(),
                format!("telegram-topic-{}", topic.id),
                title,
                selected == Some(topic.id),
                Command::SelectTopic {
                    chat_id: chat.id,
                    topic_id: Some(topic.id),
                },
                cx,
            ));
        }
        if self.view_model.loading_topics {
            bar = bar.child(Icon::new(IconName::LoadCircle).size(IconSize::XSmall));
        }
        Some(bar.into_any_element())
    }

    fn render_conversation(
        &mut self,
        narrow: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(chat) = self.view_model.selected_chat_snapshot().cloned() else {
            return self.render_message_state(tr(
                cx,
                "telegram_panel.states.select_chat",
                "Select a chat to start messaging",
            ));
        };

        let rows = self.history_rows.clone();
        let layout = self.history_layout.clone();
        let markdown = Arc::new(self.markdown_cache.clone());
        let panel = cx.entity().downgrade();
        let colors = MessageBubbleColors {
            outgoing: cx.theme().colors().element_background,
            incoming: cx.theme().colors().editor_background,
        };
        let max_content_width = TelegramPanelSettings::get_global(cx).max_content_width;
        let selection_mode = !self.selection.is_empty();
        let selection = Arc::new(self.selection.clone());
        // Broadcast channels report view counts instead of delivery ticks; the
        // outbox read position is not a per-post read receipt there.
        let read_outbox_message_id =
            (chat.kind != telegram::ChatKind::Channel).then_some(chat.read_outbox_message_id);
        let unread_divider = self
            .unread_divider
            .filter(|(chat_id, _)| *chat_id == chat.id)
            .map(|(_, after_message_id)| after_message_id);

        let transcript = list(self.history_list_state.clone(), move |index, window, cx| {
            let Some(row) = rows.get(index).cloned() else {
                return div().into_any_element();
            };
            let Some(first) = row.messages.first().cloned() else {
                return div().into_any_element();
            };
            let first_layout = layout.get(row.start_index).cloned().unwrap_or_default();
            let previous_id = index
                .checked_sub(1)
                .and_then(|index| rows.get(index))
                .and_then(|row| row.messages.last())
                .map(|message| message.id);
            // `w_full().min_w_0()` passes the list's definite width down the
            // whole bubble chain. Without it the auto-width column makes every
            // percentage width below resolve against max-content, so long
            // single-line messages grow past the panel instead of wrapping.
            let mut column = v_flex().w_full().min_w_0();
            if unread_divider
                .is_some_and(|after| first.id > after && previous_id.is_none_or(|id| id <= after))
            {
                column = column.child(
                    h_flex().w_full().justify_center().py_1().child(
                        Label::new(tr(
                            cx,
                            "telegram_panel.message.unread_divider",
                            "Unread messages",
                        ))
                        .size(LabelSize::XSmall)
                        .color(Color::Accent),
                    ),
                );
            }
            let content = if selection_mode {
                let mut stack = v_flex().w_full();
                for (offset, message) in row.messages.iter().cloned().enumerate() {
                    let message_layout = layout
                        .get(row.start_index + offset)
                        .cloned()
                        .unwrap_or_default();
                    let selected = selection.contains(&message.id);
                    let message_id = message.id;
                    let panel = panel.clone();
                    let element = render_message(
                        &message,
                        &message_layout,
                        &markdown,
                        &panel,
                        colors,
                        true,
                        selected,
                        read_outbox_message_id,
                        window,
                        cx,
                    );
                    stack = stack.child(
                        div()
                            .id(ElementId::Name(
                                format!("telegram-message-select-{}", message.id).into(),
                            ))
                            .w_full()
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                panel
                                    .update(cx, |panel, cx| panel.toggle_selection(message_id, cx))
                                    .log_err();
                            })
                            .child(element),
                    );
                }
                stack.into_any_element()
            } else if row.messages.len() > 1 {
                render_album_row(
                    &row,
                    &first_layout,
                    &markdown,
                    &panel,
                    colors,
                    read_outbox_message_id,
                    window,
                    cx,
                )
            } else {
                render_message(
                    &first,
                    &first_layout,
                    &markdown,
                    &panel,
                    colors,
                    false,
                    false,
                    read_outbox_message_id,
                    window,
                    cx,
                )
            };
            column
                .child(
                    h_flex().w_full().justify_center().child(
                        div()
                            .when_some(max_content_width, |this, max| this.max_w(max))
                            .w_full()
                            .min_w_0()
                            .child(content),
                    ),
                )
                .into_any_element()
        })
        .size_full();

        let composer = self.render_composer(window, cx);

        let subtitle = chat_subtitle(&chat, cx);

        let header = if selection_mode {
            h_flex()
                .w_full()
                .px_2()
                .gap_1p5()
                .items_center()
                .child(
                    Label::new(
                        tr(cx, "telegram_panel.selection.count", "{count} selected")
                            .replace("{count}", &self.selection.len().to_string()),
                    )
                    .size(LabelSize::Small)
                    .weight(gpui::FontWeight::MEDIUM),
                )
                .child(div().flex_1())
                .child(
                    IconButton::new("telegram-selection-forward", IconName::ArrowRight)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text(tr(
                            cx,
                            "telegram_panel.menu.forward",
                            "Forward",
                        )))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.forward_selection(false, window, cx)
                        })),
                )
                .child(
                    IconButton::new("telegram-selection-delete", IconName::Trash)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text(tr(
                            cx,
                            "telegram_panel.menu.delete_for_me",
                            "Delete for Me",
                        )))
                        .on_click(cx.listener(|this, _, _, cx| this.delete_selection(false, cx))),
                )
                .child(
                    IconButton::new("telegram-selection-cancel", IconName::Close)
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text(tr(
                            cx,
                            "telegram_panel.selection.cancel",
                            "Cancel selection",
                        )))
                        .on_click(cx.listener(|this, _, _, cx| this.exit_selection(cx))),
                )
        } else {
            h_flex()
                .w_full()
                .px_2()
                .py_1()
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
                            .on_click(cx.listener(|this, _, window, cx| this.back(window, cx))),
                    )
                })
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            Label::new(chat.title.clone())
                                .size(LabelSize::Small)
                                .weight(gpui::FontWeight::MEDIUM)
                                .truncate(),
                        )
                        .when_some(subtitle, |this, subtitle| {
                            this.child(
                                Label::new(subtitle)
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted)
                                    .truncate(),
                            )
                        }),
                )
                .when(self.view_model.has_more_history, |this| {
                    this.child(
                        IconButton::new("telegram-load-older", IconName::ChevronUp)
                            .icon_size(IconSize::XSmall)
                            .tooltip(Tooltip::text(tr(
                                cx,
                                "telegram_panel.load_older",
                                "Load older messages",
                            )))
                            .on_click(cx.listener(|this, _, _, cx| this.maybe_load_older(cx))),
                    )
                })
                .when(self.view_model.loading_history, |this| {
                    this.child(Icon::new(IconName::LoadCircle).size(IconSize::XSmall))
                })
        };

        let pinned = chat.pinned_message.as_ref().map(|pinned| {
            let panel = cx.entity().downgrade();
            let chat_id = chat.id;
            let message_chat_id = pinned.chat_id;
            let message_id = pinned.id;
            let preview = pinned.preview_text();
            h_flex()
                .id("telegram-pinned-message-bar")
                .w_full()
                .px_2()
                .py_1()
                .gap_1()
                .items_center()
                .border_b_1()
                .border_color(cx.theme().colors().border)
                .bg(cx.theme().colors().panel_background)
                .cursor_pointer()
                .on_click(move |_, window, cx| {
                    panel
                        .update(cx, |panel, cx| {
                            if panel.view_model.selected_chat != Some(chat_id) {
                                panel.open_chat(chat_id, window, cx);
                            }
                            panel.scroll_to_message(message_chat_id, message_id, cx);
                        })
                        .log_err();
                })
                .child(Icon::new(IconName::Pin).size(IconSize::XSmall))
                .child(
                    Label::new(if preview.is_empty() {
                        tr(cx, "telegram_panel.message.pinned", "Pinned message")
                    } else {
                        preview
                    })
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .truncate(),
                )
        });

        let show_jump_to_bottom = !self.history_list_state.is_following_tail();
        let topic_bar = self.render_topic_bar(&chat, cx);

        let conversation = v_flex()
            .size_full()
            .min_h_0()
            .child(
                h_flex()
                    .flex_none()
                    .w_full()
                    .min_h(Tab::container_height(cx))
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .child(self.render_centered(header, cx)),
            )
            .when_some(topic_bar, |this, bar| this.child(bar))
            .when_some(pinned, |this, pinned| this.child(pinned))
            .child(div().relative().flex_1().min_h_0().child(transcript).when(
                show_jump_to_bottom,
                |this| {
                    this.child(
                        div().absolute().bottom_2().right_4().child(
                            IconButton::new("telegram-jump-to-bottom", IconName::ChevronDown)
                                .icon_size(IconSize::Small)
                                .tooltip(Tooltip::text(tr(
                                    cx,
                                    "telegram_panel.jump_to_latest",
                                    "Jump to latest",
                                )))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.history_list_state.set_follow_mode(FollowMode::Tail);
                                    this.history_list_state.scroll_to_end();
                                    if let Some(chat_id) = this.view_model.selected_chat {
                                        this.engine.send(Command::SelectChat { chat_id });
                                    }
                                    cx.notify();
                                })),
                        ),
                    )
                },
            ))
            .child(composer)
            .into_any_element();

        div()
            .relative()
            .size_full()
            .child(conversation)
            .child(self.render_drop_target(cx))
            .into_any_element()
    }

    /// The bar above the editor that shows the message being replied to or
    /// edited. Returns `None` when neither is active.
    fn render_composer_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let mut banner = v_flex()
            .w_full()
            .flex_none()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().panel_background);
        let mut has_content = false;

        let chat_read_only = self
            .view_model
            .selected_chat_snapshot()
            .is_some_and(|chat| !chat.can_send);
        let topic_closed = self.selected_topic_closed();
        if chat_read_only || topic_closed {
            has_content = true;
            let message = if chat_read_only {
                tr(
                    cx,
                    "telegram_panel.composer.read_only",
                    "You cannot send messages in this chat",
                )
            } else {
                tr(
                    cx,
                    "telegram_panel.composer.topic_closed",
                    "This topic is closed",
                )
            };
            banner = banner.child(
                h_flex()
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_1()
                    .items_center()
                    .child(Icon::new(IconName::FileLock).size(IconSize::XSmall))
                    .child(
                        Label::new(message)
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
            );
        }

        if let Some(editing) = &self.editing {
            has_content = true;
            let target_chat_id = editing.chat_id;
            let target_message_id = editing.message_id;
            let preview = self
                .view_model
                .history
                .iter()
                .find(|message| {
                    message.chat_id == target_chat_id && message.id == target_message_id
                })
                .map_or_default(|message| message.preview_text());
            banner = banner.child(
                h_flex()
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_1()
                    .items_center()
                    .child(Icon::new(IconName::Pencil).size(IconSize::XSmall))
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .gap_0p5()
                            .child(
                                Label::new(tr(
                                    cx,
                                    "telegram_panel.composer.editing",
                                    "Editing message",
                                ))
                                .size(LabelSize::XSmall)
                                .weight(gpui::FontWeight::MEDIUM),
                            )
                            .child(
                                Label::new(preview)
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted)
                                    .truncate(),
                            ),
                    )
                    .child(
                        IconButton::new("telegram-composer-cancel-edit", IconName::Close)
                            .icon_size(IconSize::XSmall)
                            .tooltip(Tooltip::text(tr(
                                cx,
                                "telegram_panel.composer.cancel_edit",
                                "Cancel edit",
                            )))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.cancel_edit(window, cx)),
                            ),
                    ),
            );
        } else if let Some(reply) = &self.reply_to {
            has_content = true;
            let preview = reply.preview_text();
            let sender = reply.sender_name.clone();
            let label = if preview.is_empty() {
                tr(
                    cx,
                    "telegram_panel.composer.replying_to",
                    "Replying to {sender}",
                )
                .replace("{sender}", &sender)
            } else {
                format!("{sender}: {preview}")
            };
            banner = banner.child(
                h_flex()
                    .w_full()
                    .px_2()
                    .py_1()
                    .gap_1()
                    .items_center()
                    .child(Icon::new(IconName::Chat).size(IconSize::XSmall))
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .gap_0p5()
                            .child(
                                Label::new(tr(cx, "telegram_panel.composer.reply", "Reply"))
                                    .size(LabelSize::XSmall)
                                    .weight(gpui::FontWeight::MEDIUM),
                            )
                            .child(
                                Label::new(label)
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted)
                                    .truncate(),
                            ),
                    )
                    .child(
                        IconButton::new("telegram-composer-cancel-reply", IconName::Close)
                            .icon_size(IconSize::XSmall)
                            .tooltip(Tooltip::text(tr(
                                cx,
                                "telegram_panel.composer.cancel_reply",
                                "Cancel reply",
                            )))
                            .on_click(cx.listener(|this, _, _, cx| this.clear_reply(cx))),
                    ),
            );
        }

        has_content.then(|| banner.into_any_element())
    }

    /// Whether the open forum topic no longer accepts replies. The chat-level
    /// `can_send` stays true for a forum even when one topic is closed.
    fn selected_topic_closed(&self) -> bool {
        let Some(topic_id) = self.view_model.selected_topic else {
            return false;
        };
        self.view_model
            .topics
            .iter()
            .find(|topic| topic.id == topic_id)
            .is_some_and(|topic| topic.closed)
    }

    fn render_composer(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let expanded = self.composer_expanded;
        let text = self.composer.read(cx).text(cx);
        let can_send = self
            .view_model
            .selected_chat_snapshot()
            .is_none_or(|chat| chat.can_send)
            && !self.selected_topic_closed();
        let composer_empty = text.trim().is_empty() && self.composer_items.is_empty() || !can_send;
        let message_count = if composer_empty {
            0
        } else {
            outgoing_message_count(&plan_outgoing(composer_blocks(&text, &self.composer_items)))
        };
        let reply_banner = self.render_composer_banner(cx);
        let focused = self.composer.read(cx).focus_handle(cx).is_focused(window);
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
        let colors = cx.theme().colors();
        let border = colors.border;
        let editor_background = colors.editor_background;
        let editor_border = if focused {
            colors.border_focused
        } else {
            colors.border_variant
        };

        let mut editor_container = v_flex()
            .relative()
            .w_full()
            .min_h_0()
            .when(expanded, |this| this.flex_1())
            .rounded_md()
            .border_1()
            .border_color(editor_border)
            .bg(editor_background)
            .overflow_hidden();

        editor_container = editor_container
            .child(
                div()
                    .w_full()
                    .p_1()
                    .key_context("TelegramComposer")
                    .capture_action(cx.listener(Self::paste))
                    .on_action(cx.listener(Self::paste_plain))
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
                        IconButton::new("telegram-composer-height", expand_icon)
                            .icon_size(IconSize::Small)
                            .icon_color(Color::Muted)
                            .tooltip(Tooltip::text(expand_tooltip))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_composer_expanded(window, cx)
                            })),
                    ),
            );

        v_flex()
            .flex_none()
            .w_full()
            .when(expanded, |this| this.h(vh(0.6, window)))
            .border_t_1()
            .border_color(border)
            .when_some(reply_banner, |this, banner| this.child(banner))
            .child(
                h_flex()
                    .w_full()
                    .justify_center()
                    .p_2()
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
                                    .gap_1()
                                    .child(editor_container)
                                    .child(
                                        h_flex()
                                            .w_full()
                                            .flex_none()
                                            .items_center()
                                            .gap_0p5()
                                            .child(
                                                IconButton::new(
                                                    "telegram-composer-attach",
                                                    IconName::Attach,
                                                )
                                                .icon_size(IconSize::Small)
                                                .icon_color(Color::Muted)
                                                .tooltip(Tooltip::text(tr(
                                                    cx,
                                                    "telegram_panel.composer.attach",
                                                    "Attach files",
                                                )))
                                                .on_click(cx.listener(|this, _, window, cx| {
                                                    this.pick_attachments(window, cx)
                                                })),
                                            )
                                            .child(div().flex_1())
                                            .when(message_count > 1, |this| {
                                                this.child(
                                                    Label::new(
                                                        tr(
                                                            cx,
                                                            "telegram_panel.composer.will_split",
                                                            "Will send as {} messages",
                                                        )
                                                        .replacen(
                                                            "{}",
                                                            &message_count.to_string(),
                                                            1,
                                                        ),
                                                    )
                                                    .size(LabelSize::XSmall)
                                                    .color(Color::Muted),
                                                )
                                            })
                                            .child(
                                                IconButton::new("telegram-send", IconName::Send)
                                                    .style(ButtonStyle::Filled)
                                                    .map(|this| {
                                                        if composer_empty {
                                                            this.disabled(true)
                                                                .icon_color(Color::Muted)
                                                        } else {
                                                            this.icon_color(Color::Accent)
                                                        }
                                                    })
                                                    .tooltip(Tooltip::text(tr(
                                                        cx,
                                                        "telegram_panel.composer.send",
                                                        "Send",
                                                    )))
                                                    .on_click(cx.listener(
                                                        |this, _, window, cx| this.send(window, cx),
                                                    )),
                                            ),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// An invisible full-panel overlay that stages dropped files as
    /// attachments.
    fn render_drop_target(&self, cx: &mut Context<Self>) -> AnyElement {
        let project = self
            .workspace
            .upgrade()
            .map(|workspace| workspace.read(cx).project().clone());
        let is_local = project
            .as_ref()
            .is_some_and(|project| project.read(cx).is_local());
        div()
            .invisible()
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .left_0()
            .bg(cx.theme().colors().drop_target_background)
            .drag_over::<DraggedTab>(|this, _, _, _| this.visible())
            .drag_over::<DraggedSelection>(|this, _, _, _| this.visible())
            .when(is_local, |this| {
                this.drag_over::<ExternalPaths>(|this, _, _, _| this.visible())
            })
            .on_drop(cx.listener(|this, tab: &DraggedTab, window, cx| {
                let path = tab.item.project_path(cx).and_then(|project_path| {
                    let project = this.workspace.upgrade()?.read(cx).project().clone();
                    project.read(cx).absolutize(&project_path, cx)
                });
                this.stage_attachment_paths(path.into_iter().collect(), window, cx);
            }))
            .on_drop(
                cx.listener(|this, selection: &DraggedSelection, window, cx| {
                    let Some(project) = this
                        .workspace
                        .upgrade()
                        .map(|workspace| workspace.read(cx).project().clone())
                    else {
                        return;
                    };
                    let paths = selection
                        .items()
                        .filter_map(|entry| {
                            let project_path =
                                project.read(cx).path_for_entry(entry.entry_id, cx)?;
                            project.read(cx).absolutize(&project_path, cx)
                        })
                        .collect();
                    this.stage_attachment_paths(paths, window, cx);
                }),
            )
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.stage_attachment_paths(paths.paths().to_vec(), window, cx);
            }))
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

    fn render_auth(&mut self, cx: &mut Context<Self>) -> AnyElement {
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
                    .on_click(
                        cx.listener(|this, _, _, cx| this.dispatch(Command::CancelLogin, cx)),
                    ),
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
                column =
                    column.child(div().child(self.code_editor.clone())).child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new(
                                    "telegram-submit-code",
                                    tr(cx, "telegram_panel.auth.submit_code", "Submit code"),
                                )
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.submit_code(window, cx)),
                                ),
                            )
                            .child(
                                Button::new(
                                    "telegram-resend-code",
                                    tr(cx, "telegram_panel.auth.resend_code", "Resend code"),
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| this.dispatch(Command::ResendCode, cx),
                                )),
                            )
                            .child(
                                Button::new(
                                    "telegram-cancel-code",
                                    tr(cx, "telegram_panel.auth.cancel", "Cancel"),
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| this.dispatch(Command::CancelLogin, cx),
                                )),
                            ),
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
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new(
                                    "telegram-submit-password",
                                    tr(
                                        cx,
                                        "telegram_panel.auth.submit_password",
                                        "Submit password",
                                    ),
                                )
                                .on_click(cx.listener(|this, _, _, cx| this.submit_password(cx))),
                            )
                            .child(
                                Button::new(
                                    "telegram-cancel-password",
                                    tr(cx, "telegram_panel.auth.cancel", "Cancel"),
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| this.dispatch(Command::CancelLogin, cx),
                                )),
                            ),
                    );
            }
            AuthState::SignedIn { .. } => {}
        }

        column.into_any_element()
    }

    fn render_error_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let error = self.view_model.error.as_ref()?;
        let flood_wait_seconds = match error {
            EngineError::FloodWait { seconds } | EngineError::SlowMode { seconds } => {
                Some(self.flood_wait_deadline.map_or(*seconds, |deadline| {
                    deadline
                        .saturating_duration_since(std::time::Instant::now())
                        .as_secs()
                }))
            }
            _ => self.view_model.flood_wait_seconds,
        };
        let message = error_text(error, flood_wait_seconds, cx);
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
                        .on_click(
                            cx.listener(|this, _, _, cx| this.dispatch(Command::DismissError, cx)),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_message_state(&self, message: String) -> AnyElement {
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
                    let matched_message = hit.message.clone();
                    let panel = panel.clone();
                    let click_panel = panel.clone();
                    let markdown = markdown.clone();
                    ListItem::new(ElementId::Name(
                        format!("telegram-search-hit-{index}").into(),
                    ))
                    .spacing(ListItemSpacing::Sparse)
                    .on_click(move |_, window, cx| match matched_message.clone() {
                        Some(message) => {
                            click_panel
                                .update(cx, |panel, cx| {
                                    panel.open_chat_at(chat_id, message, window, cx)
                                })
                                .log_err();
                        }
                        None => {
                            click_panel
                                .update(cx, |panel, cx| {
                                    panel.search_open = false;
                                    panel.engine.send(Command::ClearSearch);
                                    panel.open_chat(chat_id, window, cx);
                                })
                                .log_err();
                        }
                    })
                    .child(
                        v_flex()
                            .min_w_0()
                            .max_w_full()
                            .gap_0p5()
                            .child(
                                Label::new(hit.chat_title.clone())
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            )
                            .when_some(hit.message.as_ref(), |this, message| {
                                this.child(render_body_text(message, &markdown, &panel, window, cx))
                            }),
                    )
                    .into_any_element()
                })
                .flex_1(),
            );
            if self.view_model.has_more_search && !searching {
                column = column.child(
                    h_flex().flex_none().w_full().justify_center().p_2().child(
                        Button::new(
                            "telegram-search-more",
                            tr(cx, "telegram_panel.search.load_more", "Load more"),
                        )
                        .on_click(
                            cx.listener(|this, _, _, cx| this.dispatch(Command::SearchMore, cx)),
                        ),
                    ),
                );
            }
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
    markdown: &HashMap<MessageKey, Entity<Markdown>>,
    panel: &WeakEntity<TelegramPanel>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    if let Some(entity) = markdown.get(&MessageKey::for_message(message)) {
        let panel = panel.clone();
        return MarkdownElement::new(entity.clone(), telegram_markdown_style(window, cx))
            .on_url_click(move |url, window, cx| {
                open_message_link(url, &panel, window, cx);
            })
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

/// The bubble background for outgoing and incoming messages.
#[derive(Clone, Copy)]
struct MessageBubbleColors {
    outgoing: Hsla,
    incoming: Hsla,
}

/// Lays a message bubble out on its side of the row while keeping it inside the
/// panel width.
///
/// `right_click_menu` wraps its trigger in a `Style::default()` block element,
/// whose auto width severs the definite width coming from the list item. Any
/// percentage width *inside* that wrapper therefore resolves against
/// max-content and cannot cap a long message. Putting the constraint one level
/// *outside* the wrapper restores a definite basis, so the bubble's own
/// `max_w_full()` and the wrapping of its text/markdown/media children work.
/// `justify_end`/`justify_start` replace the outer column's `items_end`/
/// `items_start`, which no longer align a full-width item.
fn align_message_bubble(
    bubble: AnyElement,
    outgoing: bool,
    selection_icon: Option<AnyElement>,
) -> AnyElement {
    let constrained = div()
        .max_w_full()
        .min_w_0()
        .child(bubble)
        .into_any_element();

    let mut item = h_flex().w_full().min_w_0().gap_1().items_end();
    item = if outgoing {
        item.justify_end()
    } else {
        item.justify_start()
    };
    if let Some(icon) = selection_icon {
        item = item.child(icon);
    }
    item.child(constrained).into_any_element()
}

fn render_message(
    message: &MessageSnapshot,
    layout: &MessageLayout,
    markdown: &HashMap<MessageKey, Entity<Markdown>>,
    panel: &WeakEntity<TelegramPanel>,
    colors: MessageBubbleColors,
    selection_mode: bool,
    selected: bool,
    read_outbox_message_id: Option<i32>,
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

    if message.is_service() {
        return column
            .child(
                h_flex().w_full().justify_center().py_1().child(
                    Label::new(service_message_text(message, cx))
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                ),
            )
            .into_any_element();
    }

    let background = if message.outgoing {
        colors.outgoing
    } else {
        colors.incoming
    };

    let mut bubble = v_flex()
        .min_w_0()
        .max_w_full()
        .gap_1()
        .px_2()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().colors().border)
        .overflow_hidden()
        .bg(background)
        .when(selected, |this| {
            this.border_color(cx.theme().colors().border_focused)
        });

    if let Some(forwarded_from) = &message.forwarded_from {
        bubble = bubble.child(
            Label::new(
                tr(
                    cx,
                    "telegram_panel.message.forwarded_from",
                    "Forwarded from {sender}",
                )
                .replace("{sender}", forwarded_from),
            )
            .size(LabelSize::XSmall)
            .color(Color::Muted),
        );
    }

    if layout.show_sender && !message.outgoing && !message.sender_name.is_empty() {
        bubble = bubble.child(
            Label::new(message.sender_name.clone())
                .size(LabelSize::XSmall)
                .weight(gpui::FontWeight::MEDIUM),
        );
    }

    if let Some(reply) = &message.reply_to {
        bubble = bubble.child(render_reply_preview(message, reply, panel, cx));
    }

    if let Some(media) = &message.media {
        bubble = bubble.child(render_media(message, media, panel, cx));
    }

    let body = render_body_text(message, markdown, panel, window, cx);
    bubble = bubble.child(body);

    if !message.reactions.is_empty() {
        bubble = bubble.child(render_reactions(message, panel, cx));
    }

    bubble = bubble.child(render_message_footer(
        message,
        read_outbox_message_id,
        panel,
        cx,
    ));

    let bubble = wrap_in_context_menu(message, bubble.into_any_element(), panel);
    let selection_icon = selection_mode.then(|| {
        Icon::new(if selected {
            IconName::Check
        } else {
            IconName::Circle
        })
        .size(IconSize::XSmall)
        .color(if selected {
            Color::Accent
        } else {
            Color::Muted
        })
        .into_any_element()
    });
    column = column.child(align_message_bubble(
        bubble,
        message.outgoing,
        selection_icon,
    ));

    column.into_any_element()
}

/// The trailing footer of a bubble: views, time, delivery ticks, edits, and the
/// retry button for a failed send.
fn render_message_footer(
    message: &MessageSnapshot,
    read_outbox_message_id: Option<i32>,
    panel: &WeakEntity<TelegramPanel>,
    cx: &mut App,
) -> AnyElement {
    h_flex()
        .justify_end()
        .gap_1()
        .when_some(message.views, |this, views| {
            this.child(
                Label::new(format!("👁 {views}"))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            )
        })
        .child(
            Label::new(clock_time(message.timestamp_unix))
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        )
        .when_some(read_outbox_message_id, |this, read_outbox_message_id| {
            this.when(
                message.outgoing
                    && message.send_state == telegram::SendState::Sent
                    && message.local_id.is_none(),
                |this| {
                    let read = message.id != 0 && message.id <= read_outbox_message_id;
                    this.child(
                        Icon::new(if read {
                            IconName::CheckDouble
                        } else {
                            IconName::Check
                        })
                        .size(IconSize::XSmall)
                        .color(if read {
                            Color::Accent
                        } else {
                            Color::Muted
                        }),
                    )
                },
            )
        })
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
                                panel.dispatch(Command::RetrySend { chat_id, local_id }, cx)
                            })
                            .log_err();
                    }
                }),
            )
        })
        .into_any_element()
}

/// Renders an album as one bubble with a compact grid, so a set of photos reads
/// as a single message instead of a stack of cards.
fn render_album_row(
    row: &HistoryRow,
    layout: &MessageLayout,
    markdown: &HashMap<MessageKey, Entity<Markdown>>,
    panel: &WeakEntity<TelegramPanel>,
    colors: MessageBubbleColors,
    read_outbox_message_id: Option<i32>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let first = &row.messages[0];
    let last = row.messages.last().unwrap_or(first);
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
    let background = if first.outgoing {
        colors.outgoing
    } else {
        colors.incoming
    };
    let mut bubble = v_flex()
        .min_w_0()
        .max_w_full()
        .gap_1()
        .px_2()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().colors().border)
        .overflow_hidden()
        .bg(background);
    if let Some(forwarded_from) = &first.forwarded_from {
        bubble = bubble.child(
            Label::new(
                tr(
                    cx,
                    "telegram_panel.message.forwarded_from",
                    "Forwarded from {sender}",
                )
                .replace("{sender}", forwarded_from),
            )
            .size(LabelSize::XSmall)
            .color(Color::Muted),
        );
    }
    if layout.show_sender && !first.outgoing && !first.sender_name.is_empty() {
        bubble = bubble.child(
            Label::new(first.sender_name.clone())
                .size(LabelSize::XSmall)
                .weight(gpui::FontWeight::MEDIUM),
        );
    }
    if let Some(reply) = &first.reply_to {
        bubble = bubble.child(render_reply_preview(first, reply, panel, cx));
    }
    bubble = bubble.child(render_album_grid(&row.messages, panel, cx));
    if let Some(caption) = row.messages.iter().find(|message| !message.text.is_empty()) {
        bubble = bubble.child(render_body_text(caption, markdown, panel, window, cx));
    }
    if let Some(reactions) = row
        .messages
        .iter()
        .find(|message| !message.reactions.is_empty())
    {
        bubble = bubble.child(render_reactions(reactions, panel, cx));
    }
    bubble = bubble.child(render_message_footer(
        last,
        read_outbox_message_id,
        panel,
        cx,
    ));
    let bubble = wrap_in_context_menu(first, bubble.into_any_element(), panel);
    column = column.child(align_message_bubble(bubble, first.outgoing, None));
    column.into_any_element()
}

/// Two tiles per line, with the last odd tile filling the line. Each tile is
/// cropped to a fixed height so mixed portrait and landscape items keep a
/// stable grid.
fn render_album_grid(
    messages: &[MessageSnapshot],
    panel: &WeakEntity<TelegramPanel>,
    cx: &mut App,
) -> AnyElement {
    let mut grid = v_flex().w_full().min_w_0().gap_1();
    for chunk in messages.chunks(2) {
        let mut line = h_flex().w_full().min_w_0().gap_1();
        for message in chunk {
            line = line.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h(px(ALBUM_TILE_HEIGHT))
                    .child(render_album_tile(message, panel, cx)),
            );
        }
        grid = grid.child(line);
    }
    grid.into_any_element()
}

fn render_album_tile(
    message: &MessageSnapshot,
    panel: &WeakEntity<TelegramPanel>,
    cx: &mut App,
) -> AnyElement {
    let Some(media) = message.media.as_ref() else {
        return div().into_any_element();
    };
    let mut media = media.clone();
    media.forget_missing_cache_files();
    let video_like = is_video_like(media.kind);
    let thumbnail_only = uses_thumbnail_only(&media);
    let previewable = media.is_previewable();
    let preview_path = if !previewable {
        None
    } else if thumbnail_only {
        media.thumbnail_path.clone()
    } else {
        media
            .downloaded_path
            .clone()
            .or_else(|| media.thumbnail_path.clone())
    };
    let chat_id = message.chat_id;
    let message_id = message.id;
    let tile = if let Some(path) = preview_path {
        let image = img(path)
            .size_full()
            .object_fit(ObjectFit::Cover)
            .rounded_sm();
        if video_like {
            let open_path = media.downloaded_path.clone();
            let panel = panel.clone();
            div()
                .id(ElementId::Name(
                    format!("telegram-album-tile-{chat_id}-{message_id}").into(),
                ))
                .relative()
                .size_full()
                .cursor_pointer()
                .child(image)
                .child(video_play_overlay(media.duration_seconds))
                .on_click(move |_, _, cx| {
                    if let Some(path) = open_path.clone().filter(|path| path.is_file()) {
                        cx.open_with_system(&path);
                    } else {
                        panel
                            .update(cx, |panel, cx| {
                                panel.dispatch(
                                    Command::DownloadMedia {
                                        chat_id,
                                        message_id,
                                    },
                                    cx,
                                )
                            })
                            .log_err();
                    }
                })
                .into_any_element()
        } else {
            image.into_any_element()
        }
    } else {
        let panel = panel.clone();
        let downloaded_path = media.downloaded_path.clone();
        let downloadable = media.is_downloadable();
        let icon = match media.kind {
            MediaKind::Photo | MediaKind::Animation | MediaKind::VideoNote => IconName::Image,
            MediaKind::Video => IconName::PlayFilled,
            MediaKind::Voice | MediaKind::Audio => IconName::Mic,
            MediaKind::Sticker => IconName::Sparkle,
            _ => IconName::FileGeneric,
        };
        div()
            .id(ElementId::Name(
                format!("telegram-album-tile-{chat_id}-{message_id}").into(),
            ))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .gap_1()
            .rounded_sm()
            .bg(cx.theme().colors().editor_background)
            .cursor_pointer()
            .on_click(move |_, _, cx| {
                if let Some(path) = downloaded_path.clone().filter(|path| path.is_file()) {
                    cx.open_with_system(&path);
                    return;
                }
                panel
                    .update(cx, |panel, cx| {
                        if previewable {
                            panel.maybe_download_preview(chat_id, message_id);
                        } else if downloadable {
                            panel.dispatch(
                                Command::DownloadMedia {
                                    chat_id,
                                    message_id,
                                },
                                cx,
                            );
                        }
                        cx.notify();
                    })
                    .log_err();
            })
            .child(Icon::new(icon).size(IconSize::Small))
            .child(
                Label::new(media_display_name(&media, cx))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .truncate(),
            )
            .into_any_element()
    };
    wrap_in_context_menu(message, tile, panel)
}

/// The localized one-line description of a service message.
fn service_message_text(message: &MessageSnapshot, cx: &mut App) -> SharedString {
    let sender = message.sender_name.clone();
    let action = match &message.action {
        Some(telegram::MessageAction::MemberJoined { .. }) => tr(
            cx,
            "telegram_panel.service.member_joined",
            "{actor} joined the group",
        ),
        Some(telegram::MessageAction::MemberAdded { .. }) => tr(
            cx,
            "telegram_panel.service.member_added",
            "{actor} added a member",
        ),
        Some(telegram::MessageAction::MemberLeft { .. }) => tr(
            cx,
            "telegram_panel.service.member_left",
            "{actor} left the group",
        ),
        Some(telegram::MessageAction::TitleChanged { .. }) => tr(
            cx,
            "telegram_panel.service.title_changed",
            "{actor} changed the group name",
        ),
        Some(telegram::MessageAction::PhotoChanged { .. }) => tr(
            cx,
            "telegram_panel.service.photo_changed",
            "{actor} changed the group photo",
        ),
        Some(telegram::MessageAction::PinnedMessage { .. }) => tr(
            cx,
            "telegram_panel.service.pinned_message",
            "{actor} pinned a message",
        ),
        Some(telegram::MessageAction::GroupCreated { .. }) => tr(
            cx,
            "telegram_panel.service.group_created",
            "{actor} created the group",
        ),
        Some(telegram::MessageAction::ChannelCreated { .. }) => tr(
            cx,
            "telegram_panel.service.channel_created",
            "Channel created",
        ),
        Some(telegram::MessageAction::Call { missed, .. }) => {
            if *missed {
                tr(cx, "telegram_panel.service.missed_call", "Missed call")
            } else {
                tr(cx, "telegram_panel.service.call", "Call")
            }
        }
        Some(telegram::MessageAction::Other) | None => {
            tr(cx, "telegram_panel.service.other", "Service message")
        }
    };
    let actor = if sender.is_empty() {
        match &message.action {
            Some(
                telegram::MessageAction::MemberJoined { actor }
                | telegram::MessageAction::MemberAdded { actor, .. }
                | telegram::MessageAction::MemberLeft { actor }
                | telegram::MessageAction::TitleChanged { actor, .. }
                | telegram::MessageAction::PhotoChanged { actor }
                | telegram::MessageAction::PinnedMessage { actor }
                | telegram::MessageAction::GroupCreated { actor, .. },
            ) => actor.clone(),
            _ => String::new(),
        }
    } else {
        sender
    };
    SharedString::from(action.replace("{actor}", &actor))
}

fn render_reply_preview(
    message: &MessageSnapshot,
    reply: &telegram::ReplySnapshot,
    panel: &WeakEntity<TelegramPanel>,
    cx: &mut App,
) -> AnyElement {
    let panel = panel.clone();
    let chat_id = message.chat_id;
    let message_id = reply.message_id;
    let text = if reply.text.is_empty() {
        tr(
            cx,
            "telegram_panel.message.reply_to_message",
            "Message #{id}",
        )
        .replace("{id}", &reply.message_id.to_string())
    } else if reply.sender_name.is_empty() {
        reply.text.clone()
    } else {
        format!("{}: {}", reply.sender_name, reply.text)
    };
    h_flex()
        .id(ElementId::Name(
            format!("telegram-reply-preview-{chat_id}-{message_id}").into(),
        ))
        .w_full()
        .min_w_0()
        .gap_1()
        .px_1()
        .py_0p5()
        .border_l_2()
        .border_color(cx.theme().colors().border_focused)
        .cursor_pointer()
        .on_click(move |_, _, cx| {
            panel
                .update(cx, |panel, cx| {
                    panel.scroll_to_message(chat_id, message_id, cx)
                })
                .log_err();
        })
        .child(
            Label::new(text)
                .size(LabelSize::XSmall)
                .color(Color::Muted)
                .truncate(),
        )
        .into_any_element()
}

fn render_reactions(
    message: &MessageSnapshot,
    panel: &WeakEntity<TelegramPanel>,
    cx: &mut App,
) -> AnyElement {
    let mut row = h_flex().flex_wrap().gap_1();
    let chat_id = message.chat_id;
    let message_id = message.id;
    for (index, reaction) in message.reactions.iter().enumerate() {
        let panel = panel.clone();
        let emoji = reaction.emoji.clone();
        let chosen = reaction.chosen;
        let label = format!("{} {}", reaction.emoji, reaction.count);
        row = row.child(
            div()
                .id(ElementId::Name(
                    format!("telegram-reaction-{chat_id}-{message_id}-{index}").into(),
                ))
                .cursor_pointer()
                .px_1()
                .rounded_full()
                .border_1()
                .border_color(if chosen {
                    cx.theme().colors().border_focused
                } else {
                    cx.theme().colors().border
                })
                .on_click(move |_, _, cx| {
                    let emoji = (!chosen).then(|| emoji.clone());
                    panel
                        .update(cx, |panel, cx| {
                            panel.dispatch(
                                Command::SetReaction {
                                    chat_id,
                                    message_id,
                                    emoji,
                                },
                                cx,
                            )
                        })
                        .log_err();
                })
                .child(Label::new(label).size(LabelSize::XSmall).color(if chosen {
                    Color::Accent
                } else {
                    Color::Muted
                })),
        );
    }
    row.into_any_element()
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
    let outgoing = message.outgoing;
    let is_service = message.is_service();
    let is_sendable = message_id != 0 && !is_service;
    let chosen_reactions: Vec<String> = message
        .reactions
        .iter()
        .filter(|reaction| reaction.chosen)
        .map(|reaction| reaction.emoji.clone())
        .collect();
    let message_for_actions = message.clone();
    let panel = panel.clone();
    right_click_menu(ElementId::Name(
        format!("telegram-message-menu-{chat_id}-{message_id}").into(),
    ))
    .trigger(move |_, _, _| bubble)
    .menu(move |window, cx| {
        let panel = panel.clone();
        let text = text.clone();
        let message_for_actions = message_for_actions.clone();
        let chosen_reactions = chosen_reactions.clone();
        ContextMenu::build(window, cx, move |menu, _, cx| {
            let chat = panel.upgrade().and_then(|panel| {
                panel.read_with(cx, |panel, _| {
                    panel.view_model.selected_chat_snapshot().cloned()
                })
            });
            let chat_kind = chat.as_ref().map(|chat| chat.kind);
            let can_revoke = outgoing || chat_kind == Some(telegram::ChatKind::Group);
            let pinned_message_id = chat
                .as_ref()
                .and_then(|chat| chat.pinned_message.as_ref())
                .map(|pinned| pinned.id);
            let chat_pinned = chat.as_ref().is_some_and(|chat| chat.pinned);

            let copy = ContextMenuEntry::new(tr(cx, "telegram_panel.menu.copy", "Copy"))
                .disabled(text.is_empty())
                .handler({
                    let text = text.clone();
                    move |_, cx| {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.clone()));
                    }
                });

            let reply = ContextMenuEntry::new(tr(cx, "telegram_panel.menu.reply", "Reply"))
                .disabled(!is_sendable)
                .handler({
                    let panel = panel.clone();
                    let message = message_for_actions.clone();
                    move |window, cx| {
                        panel
                            .update(cx, |panel, cx| panel.start_reply(&message, window, cx))
                            .log_err();
                    }
                });

            let edit = ContextMenuEntry::new(tr(cx, "telegram_panel.menu.edit", "Edit"))
                .disabled(!outgoing || !is_sendable || is_service)
                .handler({
                    let panel = panel.clone();
                    let message = message_for_actions.clone();
                    move |window, cx| {
                        panel
                            .update(cx, |panel, cx| panel.start_edit(&message, window, cx))
                            .log_err();
                    }
                });

            let select = ContextMenuEntry::new(tr(cx, "telegram_panel.menu.select", "Select"))
                .disabled(!is_sendable)
                .handler({
                    let panel = panel.clone();
                    move |_, cx| {
                        panel
                            .update(cx, |panel, cx| {
                                panel.enter_selection(message_id, cx);
                            })
                            .log_err();
                    }
                });

            let pin_message = ContextMenuEntry::new(if pinned_message_id == Some(message_id) {
                tr(cx, "telegram_panel.menu.unpin_message", "Unpin Message")
            } else {
                tr(cx, "telegram_panel.menu.pin_message", "Pin Message")
            })
            .disabled(!is_sendable)
            .handler({
                let panel = panel.clone();
                move |_, cx| {
                    panel
                        .update(cx, |panel, cx| {
                            let message_id = if pinned_message_id == Some(message_id) {
                                None
                            } else {
                                Some(message_id)
                            };
                            panel.dispatch(
                                Command::SetPinnedMessage {
                                    chat_id,
                                    message_id,
                                },
                                cx,
                            );
                        })
                        .log_err();
                }
            });

            let forward = ContextMenuEntry::new(tr(cx, "telegram_panel.menu.forward", "Forward"))
                .disabled(!is_sendable)
                .handler({
                    let panel = panel.clone();
                    move |window, cx| {
                        panel
                            .update(cx, |panel, cx| {
                                panel.forward_message(chat_id, vec![message_id], false, window, cx)
                            })
                            .log_err();
                    }
                });

            let forward_without_author = ContextMenuEntry::new(tr(
                cx,
                "telegram_panel.menu.forward_without_author",
                "Forward Without Author",
            ))
            .disabled(!is_sendable)
            .handler({
                let panel = panel.clone();
                move |window, cx| {
                    panel
                        .update(cx, |panel, cx| {
                            panel.forward_message(chat_id, vec![message_id], true, window, cx)
                        })
                        .log_err();
                }
            });

            let delete_for_me =
                ContextMenuEntry::new(tr(cx, "telegram_panel.menu.delete_for_me", "Delete for Me"))
                    .disabled(!is_sendable)
                    .handler({
                        let panel = panel.clone();
                        move |_, cx| {
                            panel
                                .update(cx, |panel, cx| {
                                    panel.dispatch(
                                        Command::DeleteMessages {
                                            chat_id,
                                            message_ids: vec![message_id],
                                            revoke: false,
                                        },
                                        cx,
                                    )
                                })
                                .log_err();
                        }
                    });

            let delete_for_everyone = ContextMenuEntry::new(tr(
                cx,
                "telegram_panel.menu.delete_for_everyone",
                "Delete for Everyone",
            ))
            .disabled(!is_sendable || !can_revoke)
            .handler({
                let panel = panel.clone();
                move |_, cx| {
                    panel
                        .update(cx, |panel, cx| {
                            panel.dispatch(
                                Command::DeleteMessages {
                                    chat_id,
                                    message_ids: vec![message_id],
                                    revoke: true,
                                },
                                cx,
                            )
                        })
                        .log_err();
                }
            });

            let pin_chat = ContextMenuEntry::new(if chat_pinned {
                tr(cx, "telegram_panel.menu.unpin_chat", "Unpin Chat")
            } else {
                tr(cx, "telegram_panel.menu.pin_chat", "Pin Chat")
            })
            .handler({
                let panel = panel.clone();
                move |_, cx| {
                    panel
                        .update(cx, |panel, cx| {
                            panel.dispatch(
                                Command::SetPinned {
                                    chat_id,
                                    pinned: !chat_pinned,
                                },
                                cx,
                            )
                        })
                        .log_err();
                }
            });

            let mark_unread =
                ContextMenuEntry::new(tr(cx, "telegram_panel.menu.mark_unread", "Mark as Unread"))
                    .handler({
                        let panel = panel.clone();
                        move |_, cx| {
                            panel
                                .update(cx, |panel, cx| {
                                    panel.dispatch(Command::MarkUnread { chat_id }, cx)
                                })
                                .log_err();
                        }
                    });

            let mut menu = menu
                .item(reply)
                .item(edit)
                .item(copy)
                .item(select)
                .item(pin_message);
            let chosen_reactions = chosen_reactions.clone();
            let react_panel = panel.clone();
            let react_label = tr(cx, "telegram_panel.menu.react", "React");
            let remove_reaction_label =
                tr(cx, "telegram_panel.menu.remove_reaction", "Remove Reaction");
            menu = menu.submenu(react_label, move |menu, _, _| {
                let mut menu = menu;
                for emoji in ["👍", "❤️", "😂", "🔥", "🎉", "😮"] {
                    let panel = react_panel.clone();
                    let emoji = emoji.to_owned();
                    menu = menu.item(ContextMenuEntry::new(emoji.clone()).handler(move |_, cx| {
                        panel
                            .update(cx, |panel, cx| {
                                panel.dispatch(
                                    Command::SetReaction {
                                        chat_id,
                                        message_id,
                                        emoji: Some(emoji.clone()),
                                    },
                                    cx,
                                )
                            })
                            .log_err();
                    }));
                }
                if !chosen_reactions.is_empty() {
                    let panel = react_panel.clone();
                    let label = remove_reaction_label.clone();
                    menu = menu.item(ContextMenuEntry::new(label).handler(move |_, cx| {
                        panel
                            .update(cx, |panel, cx| {
                                panel.dispatch(
                                    Command::SetReaction {
                                        chat_id,
                                        message_id,
                                        emoji: None,
                                    },
                                    cx,
                                )
                            })
                            .log_err();
                    }));
                }
                menu
            });
            menu.separator()
                .item(forward)
                .item(forward_without_author)
                .separator()
                .item(delete_for_me)
                .item(delete_for_everyone)
                .separator()
                .item(pin_chat)
                .item(mark_unread)
        })
    })
    .into_any_element()
}

/// Runs `command` after the user confirms a destructive action.
fn confirm_destructive(
    panel: WeakEntity<TelegramPanel>,
    message: String,
    command: Command,
    window: &mut Window,
    cx: &mut App,
) {
    let confirm = tr(cx, "telegram_panel.confirm.ok", "Confirm");
    let cancel = tr(cx, "telegram_panel.confirm.cancel", "Cancel");
    let receiver = window.prompt(
        gpui::PromptLevel::Warning,
        &message,
        None,
        &[confirm.as_str(), cancel.as_str()],
        cx,
    );
    cx.spawn(async move |cx| {
        if receiver.await == Ok(0) {
            panel
                .update(cx, |panel, cx| panel.dispatch(command, cx))
                .log_err();
        }
    })
    .detach();
}

/// The menu shown when a chat row is right-clicked.
fn chat_context_menu(
    chat: ChatSnapshot,
    panel: WeakEntity<TelegramPanel>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<ContextMenu> {
    let chat_id = chat.id;
    ContextMenu::build(window, cx, move |menu, _, cx| {
        let panel_for_pin = panel.clone();
        let panel_for_mute = panel.clone();
        let panel_for_archive = panel.clone();
        let panel_for_unread = panel.clone();
        let panel_for_clear = panel.clone();
        let panel_for_leave = panel.clone();
        let muted = chat.muted;
        let archived = chat.archived;
        let pinned = chat.pinned;
        let title_for_clear = chat.title.clone();
        let title_for_leave = chat.title;

        menu.item(
            ContextMenuEntry::new(if pinned {
                tr(cx, "telegram_panel.menu.unpin_chat", "Unpin Chat")
            } else {
                tr(cx, "telegram_panel.menu.pin_chat", "Pin Chat")
            })
            .handler(move |_, cx| {
                panel_for_pin
                    .update(cx, |panel, cx| {
                        panel.dispatch(
                            Command::SetPinned {
                                chat_id,
                                pinned: !pinned,
                            },
                            cx,
                        )
                    })
                    .log_err();
            }),
        )
        .item(
            ContextMenuEntry::new(if muted {
                tr(cx, "telegram_panel.menu.unmute", "Unmute")
            } else {
                tr(cx, "telegram_panel.menu.mute", "Mute")
            })
            .handler(move |_, cx| {
                panel_for_mute
                    .update(cx, |panel, cx| {
                        panel.dispatch(
                            Command::SetMuted {
                                chat_id,
                                muted: !muted,
                            },
                            cx,
                        )
                    })
                    .log_err();
            }),
        )
        .item(
            ContextMenuEntry::new(if archived {
                tr(cx, "telegram_panel.menu.unarchive", "Unarchive")
            } else {
                tr(cx, "telegram_panel.menu.archive", "Archive")
            })
            .handler(move |_, cx| {
                panel_for_archive
                    .update(cx, |panel, cx| {
                        panel.dispatch(
                            Command::SetArchived {
                                chat_id,
                                archived: !archived,
                            },
                            cx,
                        )
                    })
                    .log_err();
            }),
        )
        .item(
            ContextMenuEntry::new(tr(cx, "telegram_panel.menu.mark_unread", "Mark as Unread"))
                .handler(move |_, cx| {
                    panel_for_unread
                        .update(cx, |panel, cx| {
                            panel.dispatch(Command::MarkUnread { chat_id }, cx)
                        })
                        .log_err();
                }),
        )
        .separator()
        .item(
            ContextMenuEntry::new(tr(cx, "telegram_panel.menu.clear_history", "Clear History"))
                .handler(move |window, cx| {
                    let message = tr(
                        cx,
                        "telegram_panel.confirm.clear_history",
                        "Clear the history of “{title}”? This cannot be undone.",
                    )
                    .replace("{title}", &title_for_clear);
                    confirm_destructive(
                        panel_for_clear.clone(),
                        message,
                        Command::ClearHistory { chat_id },
                        window,
                        cx,
                    );
                }),
        )
        .item(
            ContextMenuEntry::new(tr(cx, "telegram_panel.menu.leave", "Leave Chat")).handler(
                move |window, cx| {
                    let message = tr(
                        cx,
                        "telegram_panel.confirm.leave",
                        "Leave “{title}”? This cannot be undone.",
                    )
                    .replace("{title}", &title_for_leave);
                    confirm_destructive(
                        panel_for_leave.clone(),
                        message,
                        Command::LeaveChat { chat_id },
                        window,
                        cx,
                    );
                },
            ),
        )
    })
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
            .log_err();
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
        return render_webpage(media, chat_id, message_id, cx);
    }
    match media.kind {
        MediaKind::Poll => return render_poll(message, media, panel, cx),
        MediaKind::Contact => return render_contact(media, cx),
        MediaKind::Geo => return render_geo(media, cx),
        _ => {}
    }

    let icon = match media.kind {
        MediaKind::Photo | MediaKind::Animation => IconName::Image,
        MediaKind::Video | MediaKind::VideoNote => IconName::PlayFilled,
        MediaKind::Voice | MediaKind::Audio => IconName::Mic,
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
    let display_size = media_display_size(media);

    let mut card = v_flex()
        .min_w_0()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().colors().border)
        .overflow_hidden();
    // A collapsed card keeps the full width so its file name stays readable;
    // an expanded preview shrinks to the media so the bubble hugs the image.
    if let Some((width, _)) = display_size.filter(|_| is_expanded) {
        card = card.w(width + px(MEDIA_CARD_HORIZONTAL_INSET)).max_w_full();
    } else {
        card = card.w_full();
    }
    card = card.child(
        h_flex()
            .id(ElementId::Name(
                format!("telegram-media-header-{chat_id}-{message_id}").into(),
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
                            format!("telegram-media-disclosure-{chat_id}-{message_id}").into(),
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
                .child(render_media_content(
                    media,
                    chat_id,
                    message_id,
                    display_size,
                    panel,
                    cx,
                )),
        );
    }

    card.into_any_element()
}

fn render_media_content(
    media: &MediaSnapshot,
    chat_id: i64,
    message_id: i32,
    display_size: Option<(Pixels, Pixels)>,
    panel: &WeakEntity<TelegramPanel>,
    cx: &mut App,
) -> AnyElement {
    // The operating system may have deleted the cached files since the snapshot
    // was taken, so render from a copy that reflects what is still on disk.
    let mut media = media.clone();
    media.forget_missing_cache_files();
    match media.kind {
        MediaKind::Photo
        | MediaKind::Sticker
        | MediaKind::Video
        | MediaKind::Animation
        | MediaKind::VideoNote => {
            render_media_preview(&media, chat_id, message_id, display_size, panel, cx)
        }
        MediaKind::Voice => render_voice_action(&media, chat_id, message_id, panel, cx),
        _ if media.is_downloadable() => render_media_action(&media, chat_id, message_id, panel, cx),
        _ => div().into_any_element(),
    }
}

/// Kinds whose downloaded file is a video container rather than a cacheable
/// still image, so only their thumbnail can be shown inline.
fn is_video_like(kind: MediaKind) -> bool {
    matches!(
        kind,
        MediaKind::Video | MediaKind::Animation | MediaKind::VideoNote
    )
}

/// Media whose original file is not a still image: videos and animated
/// stickers. Only their thumbnail can be rendered inline.
fn uses_thumbnail_only(media: &MediaSnapshot) -> bool {
    is_video_like(media.kind)
        || (media.kind == MediaKind::Sticker
            && media.mime_type.as_deref().is_some_and(|mime| {
                mime == "application/x-tgsticker" || mime.starts_with("video/")
            }))
}

/// Renders a photo, sticker, or video poster, with a play overlay for videos.
fn render_media_preview(
    media: &MediaSnapshot,
    chat_id: i64,
    message_id: i32,
    display_size: Option<(Pixels, Pixels)>,
    panel: &WeakEntity<TelegramPanel>,
    cx: &mut App,
) -> AnyElement {
    let max_height = if media.kind == MediaKind::Sticker {
        rems_from_px(STICKER_MAX_HEIGHT)
    } else {
        rems_from_px(MEDIA_MAX_HEIGHT)
    };
    let video_like = is_video_like(media.kind);
    let thumbnail_only = uses_thumbnail_only(media);
    let mut column = v_flex().w_full().min_w_0().gap_1();
    // A downloaded video is not an image, so only its thumbnail can be shown.
    let preview_path = if thumbnail_only {
        media.thumbnail_path.as_ref()
    } else {
        media
            .downloaded_path
            .as_ref()
            .or(media.thumbnail_path.as_ref())
    };
    if let Some(path) = preview_path {
        let image = img(path.clone()).rounded_md().max_w_full();
        let image = if let Some((width, height)) = display_size {
            image.w(width).h(height)
        } else {
            image.max_h(max_height)
        };
        let preview = if video_like {
            let panel = panel.clone();
            let open_path = media.downloaded_path.clone();
            v_flex()
                .id(ElementId::Name(
                    format!("telegram-video-{chat_id}-{message_id}").into(),
                ))
                .relative()
                .cursor_pointer()
                .child(image)
                .child(video_play_overlay(media.duration_seconds))
                .on_click(move |_, _, cx| {
                    if let Some(path) = open_path.clone().filter(|path| path.is_file()) {
                        cx.open_with_system(&path);
                    } else {
                        panel
                            .update(cx, |panel, cx| {
                                panel.dispatch(
                                    Command::DownloadMedia {
                                        chat_id,
                                        message_id,
                                    },
                                    cx,
                                )
                            })
                            .log_err();
                    }
                })
                .into_any_element()
        } else {
            image.into_any_element()
        };
        column = column.child(preview);
    }
    if media.downloaded_path.is_none() && media.thumbnail_state == DownloadState::Downloading {
        column = column.child(
            Label::new(tr(
                cx,
                "telegram_panel.media.loading_preview",
                "Loading preview…",
            ))
            .size(LabelSize::XSmall)
            .color(Color::Muted),
        );
    } else if media.downloaded_path.is_none() || thumbnail_only {
        column = column.child(render_media_action(media, chat_id, message_id, panel, cx));
    }
    column.into_any_element()
}

/// A centered play badge, plus a duration chip on the bottom-right, shown over a
/// video poster.
fn video_play_overlay(duration: Option<f64>) -> AnyElement {
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .p_1p5()
                .rounded_full()
                .bg(gpui::black().opacity(0.45))
                .child(
                    Icon::new(IconName::PlayFilled)
                        .size(IconSize::Medium)
                        .color(Color::Custom(gpui::white())),
                ),
        )
        .when_some(duration, |this, duration| {
            this.child(
                div()
                    .absolute()
                    .bottom_1()
                    .right_1()
                    .px_1()
                    .rounded_sm()
                    .bg(gpui::black().opacity(0.45))
                    .child(
                        Label::new(duration_label(Some(duration)))
                            .size(LabelSize::XSmall)
                            .color(Color::Custom(gpui::white())),
                    ),
            )
        })
        .into_any_element()
}

fn render_media_action(
    media: &MediaSnapshot,
    chat_id: i64,
    message_id: i32,
    panel: &WeakEntity<TelegramPanel>,
    cx: &mut App,
) -> AnyElement {
    let downloading = media.download_state == DownloadState::Downloading;
    // `render_media_content` already cleared missing cache files, so the path
    // exists; the click handler re-checks it in case the file is deleted later.
    let cached_path = media.downloaded_path.clone();
    let label = if downloading {
        tr(cx, "telegram_panel.media.downloading", "Downloading…")
    } else if cached_path.is_some() {
        tr(cx, "telegram_panel.media.open", "Open")
    } else {
        tr(cx, "telegram_panel.media.download", "Download")
    };
    let button = Button::new(
        ElementId::Name(format!("telegram-media-action-{chat_id}-{message_id}").into()),
        label,
    )
    .label_size(LabelSize::XSmall)
    .disabled(downloading);

    if let Some(path) = cached_path {
        let panel = panel.clone();
        button.on_click(move |_, _, cx| {
            if path.is_file() {
                cx.open_with_system(&path);
            } else {
                panel
                    .update(cx, |panel, cx| {
                        panel.dispatch(
                            Command::DownloadMedia {
                                chat_id,
                                message_id,
                            },
                            cx,
                        )
                    })
                    .log_err();
            }
        })
    } else {
        let panel = panel.clone();
        button.on_click(move |_, _, cx| {
            panel
                .update(cx, |panel, cx| {
                    panel.dispatch(
                        Command::DownloadMedia {
                            chat_id,
                            message_id,
                        },
                        cx,
                    )
                })
                .log_err();
        })
    }
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
    let downloading = media.download_state == DownloadState::Downloading;
    let (is_playing, is_paused, playhead_ratio) =
        panel.upgrade().map_or((false, false, 0.0), |panel| {
            panel.read_with(cx, |panel, _| {
                let playing = panel.playing_voice == Some(MessageKey::Remote(chat_id, message_id));
                let playback = if playing {
                    panel.voice_playback.as_ref()
                } else {
                    None
                };
                let paused = playback.is_some_and(|playback| playback.is_paused());
                let ratio = playback.map_or(0.0, |playback| {
                    let total = media.duration_seconds.unwrap_or(0.0);
                    if total > 0.0 {
                        (playback.position().as_secs_f64() / total).clamp(0.0, 1.0) as f32
                    } else {
                        0.0
                    }
                });
                (playing, paused, ratio)
            })
        });
    let samples = Arc::new(voice_waveform_samples(media.waveform.as_deref()));
    let unplayed = cx.theme().colors().text.opacity(0.28);
    let played = cx.theme().status().info;
    let panel_entity = panel.clone();

    h_flex()
        .w(voice_bar_width(cx))
        .max_w_full()
        .gap_2()
        .items_center()
        .child(
            IconButton::new(
                ElementId::Name(format!("telegram-voice-{chat_id}-{message_id}").into()),
                if downloading {
                    IconName::LoadCircle
                } else if is_playing && !is_paused {
                    IconName::DebugPause
                } else {
                    IconName::PlayFilled
                },
            )
            .icon_size(IconSize::Small)
            .disabled(downloading)
            .tooltip(Tooltip::text(if is_playing && !is_paused {
                tr(cx, "telegram_panel.media.pause", "Pause")
            } else {
                tr(cx, "telegram_panel.media.play", "Play")
            }))
            .on_click(move |_, _, cx| {
                let path = path.clone();
                panel_entity
                    .update(cx, |panel, cx| {
                        panel.toggle_voice(chat_id, message_id, path, cx)
                    })
                    .log_err();
            }),
        )
        .child(
            div().flex_1().min_w_0().h(px(24.)).child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        paint_voice_waveform(
                            window,
                            bounds,
                            &samples,
                            playhead_ratio,
                            unplayed,
                            played,
                        );
                    },
                )
                .size_full(),
            ),
        )
        .child(
            Label::new(duration_label(media.duration_seconds))
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        )
        .into_any_element()
}

/// The fixed width of a voice bar: half the panel's content width when one is
/// configured, constrained to a sensible range so it fits in narrow panels.
fn voice_bar_width(cx: &App) -> Pixels {
    let max_content_width = TelegramPanelSettings::get_global(cx).max_content_width;
    let width = max_content_width.map_or(VOICE_BAR_FALLBACK_WIDTH, |max| f32::from(max) * 0.5);
    px(width.clamp(VOICE_BAR_MIN_WIDTH, VOICE_BAR_MAX_WIDTH))
}

/// Unpacks Telegram's voice waveform into normalized amplitudes. Each byte
/// holds two 4-bit levels, low nibble first.
fn voice_waveform_samples(waveform: Option<&[u8]>) -> Vec<f32> {
    let Some(waveform) = waveform else {
        return Vec::new();
    };
    let mut samples = Vec::with_capacity(waveform.len() * 2);
    for byte in waveform {
        samples.push(f32::from(byte & 0x0F));
        samples.push(f32::from((byte >> 4) & 0x0F));
    }
    let max = samples.iter().copied().fold(0.0_f32, f32::max).max(1.0);
    for sample in &mut samples {
        *sample /= max;
    }
    samples
}

/// Paints the voice waveform as centered bars, splitting the played portion
/// from the rest at `playhead_ratio`.
fn paint_voice_waveform(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    samples: &[f32],
    playhead_ratio: f32,
    unplayed: Hsla,
    played: Hsla,
) {
    if samples.is_empty() {
        return;
    }
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let slot = width / samples.len() as f32;
    let gap = (slot * 0.3).min(1.0);
    let bar_width = (slot - gap).max(0.5);
    let center_y = bounds.origin.y + bounds.size.height / 2.0;
    let half_height = bounds.size.height / 2.0;
    let playhead_index = (playhead_ratio.clamp(0.0, 1.0) * samples.len() as f32) as usize;
    for (index, sample) in samples.iter().enumerate() {
        let amplitude = half_height * sample.clamp(0.0, 1.0);
        let bar_height = (amplitude * 2.0).max(px(1.0));
        let x = bounds.origin.x + px(slot * index as f32 + gap / 2.0);
        let color = if index < playhead_index {
            played
        } else {
            unplayed
        };
        window.paint_quad(fill(
            Bounds::new(
                point(x, center_y - bar_height / 2.0),
                size(px(bar_width), bar_height),
            ),
            color,
        ));
    }
}

/// The display size, in pixels, for a media preview. The media is scaled to fit
/// within [`MEDIA_MAX_HEIGHT`], floored at [`MEDIA_MIN_WIDTH`] so a tall image
/// stays visible, and capped at [`MEDIA_MAX_ASPECT`] so a panorama does not ask
/// for an absurdly wide bubble. `None` when the dimensions are unknown.
fn media_display_size(media: &MediaSnapshot) -> Option<(Pixels, Pixels)> {
    let width = media.width?;
    let height = media.height?;
    if width <= 0 || height <= 0 {
        return None;
    }
    let aspect = width as f32 / height as f32;
    let max_height = if media.kind == MediaKind::Sticker {
        STICKER_MAX_HEIGHT
    } else {
        MEDIA_MAX_HEIGHT
    };
    let mut display_height = max_height;
    let mut display_width = max_height * aspect;
    if display_width < MEDIA_MIN_WIDTH {
        display_width = MEDIA_MIN_WIDTH;
        display_height = display_width / aspect;
    } else if aspect > MEDIA_MAX_ASPECT {
        display_width = max_height * MEDIA_MAX_ASPECT;
        display_height = max_height;
    }
    Some((px(display_width), px(display_height)))
}

fn render_poll(
    message: &MessageSnapshot,
    media: &MediaSnapshot,
    panel: &WeakEntity<TelegramPanel>,
    cx: &mut App,
) -> AnyElement {
    let Some(poll) = &media.poll else {
        return div().into_any_element();
    };
    let chat_id = message.chat_id;
    let message_id = message.id;
    let quiz_answered = poll.quiz && poll.options.iter().any(|option| option.chosen);
    let can_vote = !poll.closed && !quiz_answered && message.id != 0 && message.local_id.is_none();
    let chosen: Vec<Vec<u8>> = poll
        .options
        .iter()
        .filter(|option| option.chosen)
        .map(|option| option.option.clone())
        .collect();
    let mut card = v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .p_2()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().colors().border)
        .bg(cx.theme().colors().editor_background)
        .child(
            Label::new(poll.question.clone())
                .size(LabelSize::Small)
                .weight(gpui::FontWeight::MEDIUM),
        );
    for (index, option) in poll.options.iter().enumerate() {
        let ratio = if poll.total_voters > 0 {
            option.voters as f32 / poll.total_voters as f32
        } else {
            0.0
        };
        let option_bytes = option.option.clone();
        let mut row = div()
            .id(ElementId::Name(
                format!("telegram-poll-option-{chat_id}-{message_id}-{index}").into(),
            ))
            .relative()
            .w_full()
            .min_w_0()
            .rounded_sm()
            .overflow_hidden();
        if ratio > 0.0 {
            row = row.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(relative(ratio))
                    .bg(cx.theme().colors().element_active.opacity(0.5)),
            );
        }
        let label_color = if option.correct && (poll.closed || quiz_answered) {
            Color::Success
        } else if option.chosen {
            Color::Accent
        } else {
            Color::Default
        };
        let mut content = h_flex()
            .w_full()
            .min_w_0()
            .px_2()
            .py_1()
            .gap_2()
            .items_center()
            .child(
                div().flex_none().child(
                    Icon::new(if option.chosen {
                        IconName::Check
                    } else {
                        IconName::Circle
                    })
                    .size(IconSize::XSmall)
                    .color(label_color),
                ),
            )
            .child(
                Label::new(option.text.clone())
                    .size(LabelSize::XSmall)
                    .flex_1()
                    .truncate(),
            );
        if let Some(voters) = (option.voters > 0).then_some(option.voters) {
            content = content.child(
                Label::new(voters.to_string())
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            );
        }
        row = row.child(content);
        if can_vote && !option_bytes.is_empty() {
            let panel = panel.clone();
            let selected = if poll.multiple_choice {
                let mut selected = chosen.clone();
                if option.chosen {
                    selected.retain(|chosen| chosen != &option_bytes);
                } else {
                    selected.push(option_bytes);
                }
                selected
            } else if option.chosen {
                // Clicking the current single-choice answer retracts the vote.
                Vec::new()
            } else {
                vec![option_bytes]
            };
            row = row.cursor_pointer().on_click(move |_, _, cx| {
                panel
                    .update(cx, |panel, cx| {
                        panel.dispatch(
                            Command::VotePoll {
                                chat_id,
                                message_id,
                                options: selected.clone(),
                            },
                            cx,
                        )
                    })
                    .log_err();
            });
        }
        card = card.child(row);
    }
    let mut footer = h_flex().gap_2();
    footer = footer.child(
        Label::new(
            tr(cx, "telegram_panel.poll.votes", "{count} votes")
                .replace("{count}", &poll.total_voters.to_string()),
        )
        .size(LabelSize::XSmall)
        .color(Color::Muted),
    );
    if poll.quiz {
        footer = footer.child(
            Label::new(tr(cx, "telegram_panel.poll.quiz", "Quiz"))
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        );
    }
    if poll.closed {
        footer = footer.child(
            Label::new(tr(cx, "telegram_panel.poll.closed", "Closed"))
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        );
    }
    if poll.multiple_choice {
        footer = footer.child(
            Label::new(tr(
                cx,
                "telegram_panel.poll.multiple_choice",
                "Multiple choice",
            ))
            .size(LabelSize::XSmall)
            .color(Color::Muted),
        );
    }
    card.child(footer).into_any_element()
}

fn render_contact(media: &MediaSnapshot, cx: &mut App) -> AnyElement {
    let Some(contact) = &media.contact else {
        return div().into_any_element();
    };
    let name = format!("{} {}", contact.first_name, contact.last_name);
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .p_2()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().colors().border)
        .bg(cx.theme().colors().editor_background)
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(Icon::new(IconName::Person).size(IconSize::Small))
                .child(
                    Label::new(name.trim().to_owned())
                        .size(LabelSize::Small)
                        .weight(gpui::FontWeight::MEDIUM),
                ),
        )
        .child(
            Label::new(contact.phone.clone())
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        )
        .into_any_element()
}

fn render_geo(media: &MediaSnapshot, cx: &mut App) -> AnyElement {
    let Some(geo) = &media.geo else {
        return div().into_any_element();
    };
    let coordinates = format!("{:.5}, {:.5}", geo.latitude, geo.longitude);
    let mut card = v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .p_2()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().colors().border)
        .bg(cx.theme().colors().editor_background)
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(Icon::new(IconName::Pin).size(IconSize::Small))
                .child(
                    Label::new(
                        geo.title
                            .clone()
                            .unwrap_or_else(|| tr(cx, "telegram_panel.media.geo", "Location")),
                    )
                    .size(LabelSize::Small)
                    .weight(gpui::FontWeight::MEDIUM),
                ),
        );
    if let Some(address) = &geo.address
        && !address.is_empty()
    {
        card = card.child(
            Label::new(address.clone())
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        );
    }
    card.child(
        Label::new(coordinates)
            .size(LabelSize::XSmall)
            .color(Color::Muted),
    )
    .into_any_element()
}

fn render_webpage(
    media: &MediaSnapshot,
    chat_id: i64,
    message_id: i32,
    cx: &mut App,
) -> AnyElement {
    let Some(webpage) = &media.webpage else {
        return div().into_any_element();
    };
    let url = webpage.url.clone();
    let title = webpage.title.clone().unwrap_or_else(|| webpage.url.clone());
    let description = webpage.description.clone().unwrap_or_default();
    v_flex()
        .id(ElementId::Name(
            format!("telegram-link-{chat_id}-{message_id}").into(),
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

fn render_topic_chip(
    panel: WeakEntity<TelegramPanel>,
    id: String,
    title: String,
    is_selected: bool,
    command: Command,
    cx: &mut App,
) -> AnyElement {
    div()
        .id(ElementId::Name(id.into()))
        .flex_none()
        .px_2()
        .py_0p5()
        .rounded_md()
        .cursor_pointer()
        .when(is_selected, |this| {
            this.bg(cx.theme().colors().element_selected)
        })
        .hover(|this| this.bg(cx.theme().colors().element_hover))
        .on_click(move |_, _, cx| {
            panel
                .update(cx, |panel, cx| panel.dispatch(command.clone(), cx))
                .log_err();
        })
        .child(
            Label::new(title)
                .size(LabelSize::XSmall)
                .color(if is_selected {
                    Color::Default
                } else {
                    Color::Muted
                }),
        )
        .into_any_element()
}

/// Groups consecutive messages that Telegram delivered as one album. Service
/// messages never share a group, and a group must stay within one sender and
/// direction so its bubble has a single author.
fn build_history_rows(history: &[MessageSnapshot]) -> Vec<HistoryRow> {
    let mut rows: Vec<HistoryRow> = Vec::with_capacity(history.len());
    for (index, message) in history.iter().enumerate() {
        if let Some(grouped_id) = message.grouped_id
            && let Some(row) = rows.last_mut()
            && row.messages.last().is_some_and(|last| {
                last.grouped_id == Some(grouped_id)
                    && last.chat_id == message.chat_id
                    && last.outgoing == message.outgoing
                    && last.sender_name == message.sender_name
            })
        {
            row.messages.push(message.clone());
            continue;
        }
        rows.push(HistoryRow {
            messages: vec![message.clone()],
            start_index: index,
        });
    }
    rows
}

fn history_row_index(rows: &[HistoryRow], chat_id: i64, message_id: i32) -> Option<usize> {
    rows.iter().position(|row| {
        row.messages
            .iter()
            .any(|message| message.chat_id == chat_id && message.id == message_id)
    })
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

fn build_composer(min_lines: usize, window: &mut Window, cx: &mut Context<Editor>) -> Editor {
    let buffer = cx.new(|cx| Buffer::local("", cx));
    let buffer = cx.new(|cx| MultiBuffer::singleton(buffer, cx));
    let mut editor = Editor::new(
        EditorMode::AutoHeight {
            min_lines,
            max_lines: Some(min_lines * 2),
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
    // The default editor menu cannot offer the panel's code-reference action, so
    // the composer keeps Cut/Copy/Paste and adds the reference and paste-as-text
    // entries.
    editor.set_custom_context_menu(|editor, _point, window, cx| {
        let has_selection = editor.has_non_empty_selection(&editor.display_snapshot(cx));
        let cut = tr(cx, "telegram_panel.composer.cut", "Cut");
        let copy = tr(cx, "telegram_panel.composer.copy", "Copy");
        let paste = tr(cx, "telegram_panel.composer.paste", "Paste");
        let paste_plain = tr(
            cx,
            "telegram_panel.composer.paste_plain",
            "Paste as Plain Text",
        );
        let insert_code = tr(
            cx,
            "telegram_panel.composer.code_block",
            "Insert Code Reference from Selection",
        );

        Some(ContextMenu::build(window, cx, move |menu, _, _| {
            menu.action_disabled_when(!has_selection, cut.clone(), Box::new(Cut))
                .action_disabled_when(!has_selection, copy.clone(), Box::new(Copy))
                .action(paste.clone(), Box::new(Paste))
                .action(paste_plain.clone(), Box::new(PastePlain))
                .separator()
                .action(insert_code, Box::new(InsertCodeReference))
        }))
    });
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

/// The icon shown in an attachment chip for the given kind.
fn attachment_icon(kind: AttachmentKind) -> IconName {
    match kind {
        AttachmentKind::Photo => IconName::Image,
        AttachmentKind::Video => IconName::File,
        AttachmentKind::Document => IconName::FileDoc,
    }
}

/// A process-unique suffix for a staged clipboard image's cache file.
fn unique_suffix() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_default(|duration| duration.as_nanos());
    format!("{nanos}-{count}")
}

/// Builds the fold placeholder that renders a staged item as an inline chip
/// with a remove button.
fn composer_chip_placeholder(
    id: u64,
    label: SharedString,
    icon: IconName,
    panel: WeakEntity<TelegramPanel>,
) -> FoldPlaceholder {
    FoldPlaceholder {
        render: Arc::new(move |fold_id: FoldId, _range, cx| {
            let remove_panel = panel.clone();
            FoldPlaceholder::fold_element(fold_id, cx)
                .flex()
                .items_center()
                .gap_1()
                .px_1()
                .child(Icon::new(icon).size(IconSize::XSmall).color(Color::Muted))
                .child(Label::new(label.clone()).size(LabelSize::XSmall))
                .child(
                    IconButton::new(("telegram-composer-chip-remove", id), IconName::Close)
                        .icon_size(IconSize::XSmall)
                        .icon_color(Color::Muted)
                        .tooltip(Tooltip::text(tr(
                            cx,
                            "telegram_panel.composer.remove_attachment",
                            "Remove",
                        )))
                        .on_click(move |_, window, cx| {
                            remove_panel
                                .update(cx, |panel, cx| panel.remove_composer_item(id, window, cx))
                                .log_err();
                        }),
                )
                .into_any_element()
        }),
        constrain_width: false,
        merge_adjacent: false,
        type_tag: None,
        collapsed_text: None,
    }
}

/// Expands a staged code reference to its outgoing Markdown.
fn expand_code_reference(reference: &CodeReference) -> String {
    // The trailing newline keeps text typed after the code chip on the next
    // line. Without it the closing fence fuses with that text (`\`\`\`测试`),
    // which is not a valid fence, so the text is swallowed into the code block.
    format!(
        "{}\n",
        code_reference_markdown(
            &reference.display_name,
            &reference.target,
            reference.language.as_deref(),
            &reference.text,
        )
    )
}

/// A piece of composer text: a literal run or a `[#id]` token. The id is `None`
/// when the digits do not parse; consumers keep such a token literal.
enum ComposerSegment<'a> {
    Text(&'a str),
    Token { id: Option<u64>, literal: &'a str },
}

/// Walks composer text, splitting it into literal runs and `[#id]` tokens. An
/// unterminated `[#` and the rest of the text are yielded as one final literal
/// run.
fn composer_segments(text: &str) -> Vec<ComposerSegment<'_>> {
    let mut segments = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(COMPOSER_TOKEN_PREFIX) {
        if start > 0 {
            segments.push(ComposerSegment::Text(&rest[..start]));
        }
        let after = &rest[start + COMPOSER_TOKEN_PREFIX.len()..];
        let Some(close) = after.find(']') else {
            segments.push(ComposerSegment::Text(&rest[start..]));
            return segments;
        };
        let token_end = start + COMPOSER_TOKEN_PREFIX.len() + close + 1;
        segments.push(ComposerSegment::Token {
            id: after[..close].parse::<u64>().ok(),
            literal: &rest[start..token_end],
        });
        rest = &rest[token_end..];
    }
    if !rest.is_empty() {
        segments.push(ComposerSegment::Text(rest));
    }
    segments
}

/// Splits the composer's text into ordered blocks, expanding staged code
/// references and lifting out attachments at their inline token positions.
fn composer_blocks(text: &str, items: &BTreeMap<u64, ComposerItem>) -> Vec<ComposerBlock> {
    let mut blocks = Vec::new();
    for segment in composer_segments(text) {
        match segment {
            ComposerSegment::Text(text) => blocks.push(ComposerBlock::Text(text.to_owned())),
            ComposerSegment::Token { id, literal } => match id.and_then(|id| items.get(&id)) {
                Some(ComposerItem::Code(reference)) => {
                    blocks.push(ComposerBlock::Text(expand_code_reference(reference)));
                }
                Some(ComposerItem::Attachment(attachment)) => {
                    blocks.push(ComposerBlock::Attachment(attachment.clone()));
                }
                None => blocks.push(ComposerBlock::Text(literal.to_owned())),
            },
        }
    }
    blocks
}

/// Removes staged items' hidden tokens before storing the composer text as a
/// draft. Drafts outlive the staged items (which are cleared when the selected
/// chat changes), so leaving the tokens in would let a later restore send them
/// verbatim as literal `[#id]` text.
fn composer_draft_text(text: &str, items: &BTreeMap<u64, ComposerItem>) -> String {
    if items.is_empty() || !text.contains(COMPOSER_TOKEN_PREFIX) {
        return text.to_owned();
    }
    let mut result = String::with_capacity(text.len());
    for segment in composer_segments(text) {
        match segment {
            ComposerSegment::Text(text) => result.push_str(text),
            ComposerSegment::Token { id, literal } => {
                if !id.is_some_and(|id| items.contains_key(&id)) {
                    result.push_str(literal);
                }
            }
        }
    }
    result
}

/// Builds a code reference from the active editor's non-empty selection.
fn active_editor_code_reference(
    workspace: &Entity<Workspace>,
    cx: &mut App,
) -> Option<CodeReference> {
    let editor = workspace.read(cx).active_item_as::<Editor>(cx)?;
    let (buffer, text, start_row, end_row) = editor.update(cx, |editor, cx| {
        let selections = editor.selections.all_adjusted(&editor.display_snapshot(cx));
        let selection = selections
            .into_iter()
            .find(|selection| !selection.is_empty())?;
        let multi_buffer = editor.buffer().read(cx);
        let multi_buffer_snapshot = multi_buffer.snapshot(cx);
        let start_anchor = multi_buffer_snapshot.anchor_after(selection.start);
        let end_anchor = multi_buffer_snapshot.anchor_before(selection.end);
        let (start_buffer, start) = multi_buffer.text_anchor_for_position(start_anchor, cx)?;
        let (end_buffer, end) = multi_buffer.text_anchor_for_position(end_anchor, cx)?;
        if start_buffer != end_buffer {
            return None;
        }
        let snapshot = start_buffer.read(cx).snapshot();
        let start_offset = start.to_offset(&snapshot);
        let end_offset = end.to_offset(&snapshot);
        if start_offset >= end_offset {
            return None;
        }
        let text = snapshot
            .text_for_range(start_offset..end_offset)
            .collect::<String>();
        let start_point = snapshot.offset_to_point(start_offset);
        let end_point = snapshot.offset_to_point(end_offset - 1);
        Some((start_buffer, text, start_point.row, end_point.row))
    })?;

    let (file_name, language, absolute_path, relative_path) = {
        let buffer = buffer.read(cx);
        let file = buffer.file()?;
        let absolute_path = file.full_path(cx);
        let language = buffer
            .language()
            .map(|language| language.code_fence_block_name().to_string())
            .or_else(|| extension_language(&absolute_path));
        (
            file.file_name(cx).to_owned(),
            language,
            absolute_path,
            file.path().as_unix_str().to_owned(),
        )
    };
    let line_range = start_row..=end_row;
    let display_name = format!("{file_name}:{}-{}", start_row + 1, end_row + 1);
    let project = workspace.read(cx).project().clone();
    let target = code_target(&project, &absolute_path, &relative_path, &line_range, cx);
    Some(CodeReference {
        display_name,
        language,
        target,
        text,
    })
}

/// The lowercase file extension, used as a code fence language when a buffer
/// has no language of its own.
fn extension_language(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|extension| !extension.is_empty())
}

/// Builds the target for a code reference: a commit permalink when the file
/// belongs to a repository with a remote, otherwise a worktree-relative path
/// with a line fragment.
fn code_target(
    project: &Entity<project::Project>,
    absolute_path: &Path,
    relative_path: &str,
    line_range: &RangeInclusive<u32>,
    cx: &App,
) -> String {
    if let Some(project_path) = project.read(cx).find_project_path(absolute_path, cx) {
        let git_store = project.read(cx).git_store().clone();
        let resolved = git_store
            .read(cx)
            .repository_and_path_for_project_path(&project_path, cx);
        if let Some((repository, repo_path)) = resolved {
            let remote_url = repository.read(cx).default_remote_url();
            let head_commit = repository.read(cx).snapshot().head_commit;
            if let (Some(remote_url), Some(head_commit)) = (remote_url, head_commit)
                && let Some(registry) = GitHostingProviderRegistry::try_global(cx)
                && let Some((provider, parsed)) = parse_git_remote_url(registry, &remote_url)
            {
                let selection = Some(*line_range.start()..*line_range.end());
                let params =
                    BuildPermalinkParams::new(head_commit.sha.as_ref(), &repo_path, selection);
                return provider.build_permalink(parsed, params).to_string();
            }
        }
    }
    let start = line_range.start() + 1;
    let end = line_range.end() + 1;
    if start == end {
        format!("{relative_path}#L{start}")
    } else {
        format!("{relative_path}#L{start}-L{end}")
    }
}

/// Splits a `path#Lx-Ly` reference into its path and (1-based) start line.
fn split_line_fragment(target: &str) -> (&str, Option<u32>) {
    let Some((path, fragment)) = target.split_once('#') else {
        return (target, None);
    };
    let line = fragment
        .strip_prefix('L')
        .and_then(|rest| rest.split(['-', 'L']).next())
        .and_then(|line| line.parse::<u32>().ok());
    (path, line)
}

/// Opens a link from a Telegram message. Absolute file mentions and non-HTTP
/// targets are opened in the workspace; anything unresolved falls back to the
/// system URL handler.
fn open_message_link(
    url: SharedString,
    panel: &WeakEntity<TelegramPanel>,
    window: &mut Window,
    cx: &mut App,
) {
    if url.starts_with("http://") || url.starts_with("https://") {
        cx.open_url(&url);
        return;
    }
    let Some(workspace) = panel
        .upgrade()
        .and_then(|panel| panel.read(cx).workspace.upgrade())
    else {
        cx.open_url(&url);
        return;
    };
    if open_mention_link(&workspace, &url, window, cx)
        || open_project_reference(&workspace, &url, window, cx)
    {
        return;
    }
    cx.open_url(&url);
}

/// Opens an absolute-path or `zzz://` mention link via [`MentionUri`]. Returns
/// `false` when the target is not a recognized mention.
fn open_mention_link(
    workspace: &Entity<Workspace>,
    url: &str,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let path_style = workspace.read(cx).path_style(cx);
    let Ok(mention) = MentionUri::parse_hyperlink(url, path_style) else {
        return false;
    };
    match mention {
        MentionUri::File { abs_path } => {
            open_abs_path_at_point(workspace, abs_path, None, window, cx)
        }
        MentionUri::Directory { abs_path } => {
            reveal_in_project_panel(workspace, &abs_path, cx);
        }
        MentionUri::Symbol {
            abs_path,
            line_range,
            ..
        } => open_abs_path_at_point(
            workspace,
            abs_path,
            Some(Point::new(*line_range.start(), 0)),
            window,
            cx,
        ),
        MentionUri::Selection {
            abs_path: Some(abs_path),
            line_range,
            column,
        } => open_abs_path_at_point(
            workspace,
            abs_path,
            Some(Point::new(*line_range.start(), column.unwrap_or(0))),
            window,
            cx,
        ),
        MentionUri::Fetch { url } => cx.open_url(url.as_str()),
        _ => return false,
    }
    true
}

/// Reveals a directory in the project panel.
fn reveal_in_project_panel(workspace: &Entity<Workspace>, abs_path: &Path, cx: &mut App) {
    let project = workspace.read(cx).project().clone();
    let entry_id = project
        .read(cx)
        .find_project_path(abs_path, cx)
        .and_then(|project_path| {
            project
                .read(cx)
                .entry_for_path(&project_path, cx)
                .map(|entry| entry.id)
        });
    if let Some(entry_id) = entry_id {
        project.update(cx, |_, cx| {
            cx.emit(project::Event::RevealInProjectPanel(entry_id))
        });
    }
}

/// Opens an absolute path, preferring its project path so the file opens in the
/// existing buffer, then optionally places the cursor at `point`.
fn open_abs_path_at_point(
    workspace: &Entity<Workspace>,
    abs_path: PathBuf,
    point: Option<Point>,
    window: &mut Window,
    cx: &mut App,
) {
    let project = workspace.read(cx).project().clone();
    let project_path = project.read(cx).find_project_path(&abs_path, cx);
    let fs = project.read(cx).fs().clone();
    let workspace = workspace.downgrade();
    window
        .spawn(cx, async move |cx| {
            let item = if let Some(project_path) = project_path {
                workspace
                    .update_in(cx, |workspace, window, cx| {
                        workspace.open_path(project_path, None, true, window, cx)
                    })?
                    .await?
            } else {
                let metadata = fs.metadata(&abs_path).await?;
                anyhow::ensure!(
                    metadata.is_some_and(|metadata| !metadata.is_dir),
                    "no file found at path {abs_path:?}"
                );
                workspace
                    .update_in(cx, |workspace, window, cx| {
                        workspace.open_abs_path(
                            abs_path,
                            OpenOptions {
                                focus: Some(true),
                                ..Default::default()
                            },
                            window,
                            cx,
                        )
                    })?
                    .await?
            };
            if let Some(point) = point {
                reveal_point_in_item(item.as_ref(), point, cx);
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
}

/// Resolves a worktree-relative reference against the project's worktrees and
/// opens it, returning `false` when it does not resolve.
fn open_project_reference(
    workspace: &Entity<Workspace>,
    url: &str,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let (path_part, line) = split_line_fragment(url);
    if path_part.is_empty() || Path::new(path_part).is_absolute() {
        return false;
    }
    if path_part
        .split(['/', '\\'])
        .any(|component| component == "..")
    {
        return false;
    }

    let project = workspace.read(cx).project().clone();
    let roots = project
        .read(cx)
        .worktrees(cx)
        .map(|worktree| worktree.read(cx).abs_path().to_path_buf())
        .collect::<Vec<_>>();
    let Some(absolute_path) = roots
        .into_iter()
        .map(|root| root.join(path_part))
        .find(|candidate| candidate.is_file())
    else {
        return false;
    };

    let Some(project_path) = project.update(cx, |project, cx| {
        project.find_project_path(&absolute_path, cx)
    }) else {
        return false;
    };

    let point = line.map(|line| Point::new(line.saturating_sub(1), 0));
    let workspace = workspace.downgrade();
    window
        .spawn(cx, async move |cx| {
            let item = workspace
                .update_in(cx, |workspace, window, cx| {
                    workspace.open_path(project_path, None, true, window, cx)
                })?
                .await?;
            if let Some(point) = point {
                reveal_point_in_item(item.as_ref(), point, cx);
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    true
}

/// Places the cursor at `point` in the opened item, scrolling it into view.
/// Does nothing when the opened item is not an editor.
fn reveal_point_in_item(item: &dyn ItemHandle, point: Point, cx: &mut gpui::AsyncWindowContext) {
    let Some(editor) = item.downcast::<Editor>() else {
        return;
    };
    editor
        .update_in(cx, |editor, window, cx| {
            editor.change_selections(
                SelectionEffects::scroll(Autoscroll::center()),
                window,
                cx,
                |selections| selections.select_ranges([point..point]),
            );
        })
        .log_err();
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
    if let Some(typing) = &chat.typing {
        return tr(cx, "telegram_panel.typing", "{name} is typing…")
            .replace("{name}", &typing.user_name);
    }
    if !chat.preview.is_empty() {
        return chat.preview.clone();
    }
    chat.preview_media
        .map_or_default(|kind| media_kind_label(kind, cx))
}

/// Cycles the search filter in the order shown in the search bar.
fn next_search_filter(filter: telegram::SearchFilter) -> telegram::SearchFilter {
    match filter {
        telegram::SearchFilter::All => telegram::SearchFilter::Text,
        telegram::SearchFilter::Text => telegram::SearchFilter::Media,
        telegram::SearchFilter::Media => telegram::SearchFilter::Links,
        telegram::SearchFilter::Links => telegram::SearchFilter::Files,
        telegram::SearchFilter::Files => telegram::SearchFilter::All,
    }
}

/// The localized label of the active search filter.
fn search_filter_label(filter: telegram::SearchFilter, cx: &App) -> String {
    let (key, fallback) = match filter {
        telegram::SearchFilter::All => ("telegram_panel.search.filter.all", "All"),
        telegram::SearchFilter::Text => ("telegram_panel.search.filter.text", "Text"),
        telegram::SearchFilter::Media => ("telegram_panel.search.filter.media", "Media"),
        telegram::SearchFilter::Links => ("telegram_panel.search.filter.links", "Links"),
        telegram::SearchFilter::Files => ("telegram_panel.search.filter.files", "Files"),
    };
    tr(cx, key, fallback)
}

/// The muted second line of the conversation header: typing state, presence,
/// slow mode, or the public username.
fn chat_subtitle(chat: &ChatSnapshot, cx: &App) -> Option<String> {
    if let Some(typing) = &chat.typing {
        return Some(
            tr(cx, "telegram_panel.typing", "{name} is typing…")
                .replace("{name}", &typing.user_name),
        );
    }
    match &chat.presence {
        telegram::Presence::Online => {
            return Some(tr(cx, "telegram_panel.presence.online", "online"));
        }
        telegram::Presence::Recently => {
            return Some(tr(
                cx,
                "telegram_panel.presence.recently",
                "recently online",
            ));
        }
        telegram::Presence::LastSeen(unix) if *unix > 0 => {
            return Some(
                tr(cx, "telegram_panel.presence.last_seen", "last seen {time}")
                    .replace("{time}", &relative_time(*unix)),
            );
        }
        _ => {}
    }
    if chat.slow_mode_seconds > 0 {
        return Some(
            tr(cx, "telegram_panel.slow_mode", "slow mode: {seconds}s")
                .replace("{seconds}", &chat.slow_mode_seconds.to_string()),
        );
    }
    chat.username
        .as_ref()
        .map(|username| format!("@{username}"))
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
        MediaKind::VideoNote => ("telegram_panel.media.video_note", "Video message"),
        MediaKind::Animation => ("telegram_panel.media.animation", "Animation"),
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
        EngineError::ReadOnlyChat => tr(
            cx,
            "telegram_panel.error.read_only_chat",
            "You cannot send messages in this chat.",
        ),
        EngineError::EditFailed => tr(
            cx,
            "telegram_panel.error.edit_failed",
            "Failed to edit the message.",
        ),
        EngineError::PinMessageFailed => tr(
            cx,
            "telegram_panel.error.pin_message_failed",
            "Failed to update the pinned message.",
        ),
        EngineError::ReactionFailed => tr(
            cx,
            "telegram_panel.error.reaction_failed",
            "Failed to update the reaction.",
        ),
        EngineError::MuteFailed => tr(
            cx,
            "telegram_panel.error.mute_failed",
            "Failed to update the notification settings.",
        ),
        EngineError::ArchiveFailed => tr(
            cx,
            "telegram_panel.error.archive_failed",
            "Failed to archive the chat.",
        ),
        EngineError::MarkUnreadFailed => tr(
            cx,
            "telegram_panel.error.mark_unread_failed",
            "Failed to mark the chat as unread.",
        ),
        EngineError::ClearHistoryFailed => tr(
            cx,
            "telegram_panel.error.clear_history_failed",
            "Failed to clear the chat history.",
        ),
        EngineError::LeaveFailed => tr(
            cx,
            "telegram_panel.error.leave_failed",
            "Failed to leave the chat.",
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
        EngineError::SlowMode { .. } => {
            let seconds = flood_wait_seconds.unwrap_or(0);
            tr(
                cx,
                "telegram_panel.error.slow_mode",
                "This chat is in slow mode. Try again in {} seconds.",
            )
            .replacen("{}", &seconds.to_string(), 1)
        }
        EngineError::MediaUnavailable => tr(
            cx,
            "telegram_panel.error.media_unavailable",
            "This media can no longer be downloaded.",
        ),
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
            .on_action(cx.listener(|this, _: &NextChat, window, cx| {
                this.select_relative_chat(1, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PreviousChat, window, cx| {
                this.select_relative_chat(-1, window, cx)
            }))
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
            self.notification_windows.dismiss_all(cx);
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
        with_panel(workspace, window, cx, |panel, window, cx| {
            panel.select_relative_chat(1, window, cx)
        });
    });
    workspace.register_action(|workspace, _: &PreviousChat, window, cx| {
        with_panel(workspace, window, cx, |panel, window, cx| {
            panel.select_relative_chat(-1, window, cx)
        });
    });
    workspace.register_action(|workspace, _: &Back, window, cx| {
        with_panel(workspace, window, cx, |panel, window, cx| {
            panel.back(window, cx)
        });
    });
    workspace.register_action(|workspace, _: &InsertCodeReference, window, cx| {
        with_panel(workspace, window, cx, |panel, window, cx| {
            panel.insert_code_reference(window, cx)
        });
    });
}

fn with_panel(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
    f: impl FnOnce(&mut TelegramPanel, &mut Window, &mut Context<TelegramPanel>),
) {
    if let Some(panel) = workspace.panel::<TelegramPanel>(cx) {
        panel.update(cx, |panel, cx| f(panel, window, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        align_message_bubble, build_history_layout, build_history_rows, chat_subtitle,
        duration_label, error_text, media_display_size, media_kind_label, next_search_filter,
        search_filter_label, service_message_text, voice_waveform_samples,
    };
    use gpui::{
        AppContext, AvailableSpace, InteractiveElement, IntoElement, ParentElement, Styled,
        TestAppContext, div, point, px, size,
    };
    use settings::SettingsStore;
    use telegram::{
        ChatSnapshot, EngineError, MediaKind, MediaSnapshot, MessageAction, MessageSnapshot,
        Presence, SendState, TypingSnapshot,
    };
    use ui::{h_flex, v_flex};

    fn message(id: i32, sender: &str, timestamp_unix: i64) -> MessageSnapshot {
        MessageSnapshot {
            id,
            chat_id: 1,
            outgoing: false,
            sender_name: sender.to_owned(),
            sender_id: None,
            timestamp_unix,
            text: String::new(),
            markdown: None,
            media: None,
            reply_to: None,
            forwarded_from: None,
            grouped_id: None,
            topic_id: None,
            thread_root_id: None,
            views: None,
            action: None,
            reactions: Vec::new(),
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

    fn media_snapshot(kind: MediaKind, width: i32, height: i32) -> MediaSnapshot {
        MediaSnapshot {
            kind,
            width: Some(width),
            height: Some(height),
            ..Default::default()
        }
    }

    #[test]
    fn media_display_size_preserves_aspect_and_caps() {
        assert_eq!(
            media_display_size(&media_snapshot(MediaKind::Photo, 400, 200)),
            Some((px(640.), px(320.)))
        );
        assert_eq!(
            media_display_size(&media_snapshot(MediaKind::Photo, 200, 400)),
            Some((px(160.), px(320.)))
        );
    }

    #[test]
    fn media_display_size_floors_tall_and_caps_wide_media() {
        assert_eq!(
            media_display_size(&media_snapshot(MediaKind::Photo, 100, 1000)),
            Some((px(120.), px(1200.)))
        );
        assert_eq!(
            media_display_size(&media_snapshot(MediaKind::Video, 1000, 200)),
            Some((px(800.), px(320.)))
        );
    }

    #[test]
    fn media_display_size_is_none_without_dimensions() {
        assert_eq!(media_display_size(&MediaSnapshot::default()), None);
        assert_eq!(
            media_display_size(&media_snapshot(MediaKind::Photo, 0, 100)),
            None
        );
    }

    #[test]
    fn voice_waveform_unpacks_nibbles_and_normalizes() {
        assert!(voice_waveform_samples(None).is_empty());
        assert_eq!(
            voice_waveform_samples(Some(&[0x0F, 0xF0])),
            vec![1.0, 0.0, 0.0, 1.0]
        );
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

    #[test]
    fn history_rows_group_only_consecutive_album_items() {
        let mut first = message(1, "Ada", 0);
        first.grouped_id = Some(7);
        let mut second = message(2, "Ada", 1);
        second.grouped_id = Some(7);
        let standalone = message(3, "Ada", 2);
        let mut third = message(4, "Ada", 3);
        third.grouped_id = Some(7);
        let rows = build_history_rows(&[first, second, standalone, third]);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].messages.len(), 2);
        assert_eq!(rows[0].start_index, 0);
        assert_eq!(rows[1].messages.len(), 1);
        assert_eq!(rows[1].start_index, 2);
        assert_eq!(rows[2].messages.len(), 1);
        assert_eq!(rows[2].start_index, 3);
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
            assert_eq!(
                error_text(&EngineError::SlowMode { seconds: 5 }, Some(5), cx),
                i18n::tr(
                    cx,
                    "telegram_panel.error.slow_mode",
                    "This chat is in slow mode. Try again in {} seconds.",
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
            assert_eq!(
                media_kind_label(MediaKind::Animation, cx),
                i18n::tr(cx, "telegram_panel.media.animation", "Animation")
            );
            assert_eq!(
                media_kind_label(MediaKind::VideoNote, cx),
                i18n::tr(cx, "telegram_panel.media.video_note", "Video message")
            );
        });
    }

    #[gpui::test]
    fn chat_subtitle_prefers_typing_then_presence(cx: &mut TestAppContext) {
        init_i18n(cx);
        cx.update(|cx| {
            let mut chat = ChatSnapshot {
                presence: Presence::Online,
                ..Default::default()
            };
            assert_eq!(
                chat_subtitle(&chat, cx),
                Some(i18n::tr(cx, "telegram_panel.presence.online", "online"))
            );

            chat.typing = Some(TypingSnapshot {
                user_name: "Ada".to_owned(),
                until_unix: 0,
            });
            assert_eq!(
                chat_subtitle(&chat, cx),
                Some(
                    i18n::tr(cx, "telegram_panel.typing", "{name} is typing…")
                        .replace("{name}", "Ada")
                )
            );

            chat.typing = None;
            chat.presence = Presence::Unknown;
            chat.slow_mode_seconds = 30;
            assert_eq!(
                chat_subtitle(&chat, cx),
                Some(
                    i18n::tr(cx, "telegram_panel.slow_mode", "slow mode: {seconds}s")
                        .replace("{seconds}", "30")
                )
            );

            chat.slow_mode_seconds = 0;
            chat.username = Some("ada".to_owned());
            assert_eq!(chat_subtitle(&chat, cx), Some("@ada".to_owned()));
        });
    }

    #[gpui::test]
    fn service_message_text_fills_the_actor(cx: &mut TestAppContext) {
        init_i18n(cx);
        cx.update(|cx| {
            let mut service = message(10, "Ada", 0);
            service.action = Some(MessageAction::MemberJoined {
                actor: "Ada".to_owned(),
            });
            assert_eq!(
                service_message_text(&service, cx).as_ref(),
                i18n::tr(
                    cx,
                    "telegram_panel.service.member_joined",
                    "{actor} joined the group"
                )
                .replace("{actor}", "Ada")
            );
        });
    }

    #[gpui::test]
    fn search_filter_cycles_through_every_value(cx: &mut TestAppContext) {
        init_i18n(cx);
        let start = telegram::SearchFilter::All;
        let mut filter = start;
        for _ in 0..5 {
            filter = next_search_filter(filter);
        }
        assert_eq!(filter, start);
        cx.update(|cx| {
            for filter in [
                telegram::SearchFilter::All,
                telegram::SearchFilter::Text,
                telegram::SearchFilter::Media,
                telegram::SearchFilter::Links,
                telegram::SearchFilter::Files,
            ] {
                assert!(!search_filter_label(filter, cx).is_empty());
            }
        });
    }

    #[test]
    fn composer_blocks_expand_code_and_lift_attachments() {
        use std::collections::BTreeMap;

        use super::{CodeReference, ComposerItem, composer_blocks};
        use telegram::{AttachmentKind, ComposerBlock, StagedAttachment};

        let mut items = BTreeMap::new();
        items.insert(
            0,
            ComposerItem::Code(CodeReference {
                display_name: "a.rs:1-2".into(),
                language: Some("rust".into()),
                target: "a.rs#L1-L2".into(),
                text: "fn a() {}".into(),
            }),
        );
        items.insert(
            1,
            ComposerItem::Attachment(StagedAttachment {
                path: "/tmp/photo.png".into(),
                kind: AttachmentKind::Photo,
                file_name: "photo.png".into(),
                size: 10,
            }),
        );

        let blocks = composer_blocks("see [#0] then [#1] ok", &items);
        assert_eq!(
            blocks,
            vec![
                ComposerBlock::Text("see ".into()),
                ComposerBlock::Text("[a.rs:1-2](a.rs#L1-L2)\n```rust\nfn a() {}\n```\n".into()),
                ComposerBlock::Text(" then ".into()),
                ComposerBlock::Attachment(StagedAttachment {
                    path: "/tmp/photo.png".into(),
                    kind: AttachmentKind::Photo,
                    file_name: "photo.png".into(),
                    size: 10,
                }),
                ComposerBlock::Text(" ok".into()),
            ]
        );
    }

    #[test]
    fn composer_blocks_keep_unknown_tokens_literal() {
        use std::collections::BTreeMap;

        use super::composer_blocks;
        use telegram::ComposerBlock;

        let blocks = composer_blocks("x [#9] y", &BTreeMap::new());
        assert_eq!(
            blocks,
            vec![
                ComposerBlock::Text("x ".into()),
                ComposerBlock::Text("[#9]".into()),
                ComposerBlock::Text(" y".into()),
            ]
        );
    }

    #[test]
    fn composer_draft_text_drops_known_tokens_only() {
        use std::collections::BTreeMap;

        use super::{ComposerItem, composer_draft_text};
        use telegram::{AttachmentKind, StagedAttachment};

        let mut items = BTreeMap::new();
        items.insert(
            3,
            ComposerItem::Attachment(StagedAttachment {
                path: "/tmp/photo.png".into(),
                kind: AttachmentKind::Photo,
                file_name: "photo.png".into(),
                size: 10,
            }),
        );

        assert_eq!(
            composer_draft_text("hello [#3] world [#9]", &items),
            "hello  world [#9]"
        );
        assert_eq!(
            composer_draft_text("no items [#3]", &BTreeMap::new()),
            "no items [#3]"
        );
    }

    #[test]
    fn split_line_fragment_parses_ranges() {
        use super::split_line_fragment;

        assert_eq!(split_line_fragment("a/b.rs#L9-L16"), ("a/b.rs", Some(9)));
        assert_eq!(split_line_fragment("a/b.rs#L5"), ("a/b.rs", Some(5)));
        assert_eq!(split_line_fragment("a/b.rs"), ("a/b.rs", None));
    }

    #[test]
    fn message_keys_separate_peer_local_and_optimistic_ids() {
        use super::MessageKey;

        let remote = message(7, "Ada", 0);
        assert_eq!(MessageKey::for_message(&remote), MessageKey::Remote(1, 7));
        let mut migrated_remote = remote.clone();
        migrated_remote.chat_id = 2;
        assert_ne!(
            MessageKey::for_message(&remote),
            MessageKey::for_message(&migrated_remote)
        );

        let mut first = message(0, "", 0);
        first.local_id = Some(1);
        let mut second = message(0, "", 0);
        second.local_id = Some(2);
        assert_eq!(MessageKey::for_message(&first), MessageKey::Local(1, 1));
        assert_eq!(MessageKey::for_message(&second), MessageKey::Local(1, 2));
        assert_ne!(
            MessageKey::for_message(&first),
            MessageKey::for_message(&second)
        );
    }

    #[test]
    fn unique_suffix_is_unique_per_call() {
        use super::unique_suffix;

        assert_ne!(unique_suffix(), unique_suffix());
    }

    #[test]
    fn extension_language_uses_lowercase_extension() {
        use super::extension_language;
        use std::path::Path;

        assert_eq!(
            extension_language(Path::new("/tmp/keymap.json")),
            Some("json".to_owned())
        );
        assert_eq!(
            extension_language(Path::new("/tmp/Script.PY")),
            Some("py".to_owned())
        );
        assert_eq!(extension_language(Path::new("/tmp/README")), None);
        assert_eq!(extension_language(Path::new("/tmp/.gitignore")), None);
    }

    /// Builds the transcript row the way `render_conversation` does, with a
    /// `right_click_menu`-style `Style::default()` block wrapper standing in for
    /// the real context menu, and measures the bubble under a definite width.
    fn measure_bubble(
        cx: &mut gpui::VisualTestContext,
        selector: &'static str,
        panel_width: f32,
        content_width: f32,
        outgoing: bool,
        selection_icon: bool,
    ) -> gpui::Bounds<gpui::Pixels> {
        cx.draw(
            point(px(0.), px(0.)),
            size(
                AvailableSpace::Definite(px(panel_width)),
                AvailableSpace::MinContent,
            ),
            |_, _| {
                // A wide child stands in for a long single-line label, a media
                // preview, a poll, etc. All of them used to overflow the panel.
                let bubble = v_flex()
                    .min_w_0()
                    .max_w_full()
                    .px_2()
                    .py_1()
                    .debug_selector(move || selector.into())
                    .child(div().w(px(content_width)).h(px(20.)))
                    .into_any_element();
                // right_click_menu wraps the trigger in a Style::default() block.
                let wrapped = div().child(bubble).into_any_element();
                let icon = selection_icon.then(|| div().w(px(16.)).h(px(16.)).into_any_element());
                let item = align_message_bubble(wrapped, outgoing, icon);

                // render_conversation's list item: w_full column -> centered
                // layer (max_w_full) -> render_message's w_full column.
                v_flex()
                    .w_full()
                    .min_w_0()
                    .child(
                        h_flex().w_full().justify_center().child(
                            div()
                                .max_w_full()
                                .w_full()
                                .min_w_0()
                                .child(v_flex().w_full().min_w_0().px_3().py_1().child(item)),
                        ),
                    )
                    .into_any_element()
            },
        );
        cx.debug_bounds(selector)
            .expect("bubble should be rendered")
    }

    #[gpui::test]
    fn long_message_bubbles_stay_within_panel_width(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();

        // A bubble whose content is far wider than the panel must be clamped and
        // wrap regardless of message side or selection mode. This covers the
        // long-text, media, poll, contact, geo, webpage and album/caption paths,
        // which all go through align_message_bubble.
        for (panel_width, content_width) in [(300., 500.), (360., 900.), (600., 1200.)] {
            for (outgoing, selection_icon) in [(true, false), (false, false), (true, true)] {
                let selector: &'static str = Box::leak(
                    format!("bubble-{panel_width}-{outgoing}-{selection_icon}").into_boxed_str(),
                );
                let bounds = measure_bubble(
                    cx,
                    selector,
                    panel_width,
                    content_width,
                    outgoing,
                    selection_icon,
                );
                assert!(
                    bounds.size.width <= px(panel_width),
                    "bubble {bounds:?} overflows panel width {panel_width}"
                );
                assert!(
                    bounds.origin.x >= px(0.) && bounds.origin.x < px(panel_width),
                    "bubble {bounds:?} is outside the panel horizontally"
                );
            }
        }
    }

    #[gpui::test]
    fn short_message_bubbles_still_hug_content_and_align(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();

        let short_out = measure_bubble(cx, "short-out", 300., 40., true, false);
        let short_in = measure_bubble(cx, "short-in", 300., 40., false, false);

        // A short bubble must not be stretched to the full panel width.
        assert!(
            short_out.size.width < px(100.),
            "short outgoing bubble was stretched: {short_out:?}"
        );
        // Outgoing bubbles sit on the right, incoming on the left.
        assert!(
            short_out.origin.x > short_in.origin.x,
            "outgoing bubble {short_out:?} is not right of incoming {short_in:?}"
        );
    }

    /// A Markdown-formatted message goes through `MarkdownElement`, which adds
    /// another `Style::default()` block wrapper. Verify the bubble constraint
    /// still bounds it (the same class of bug as plain-text messages).
    #[gpui::test]
    fn markdown_message_bubble_stays_within_panel_width(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
            theme_settings::init(theme::LoadThemes::JustBase, cx);
        });

        let source = "**bold** and a very long unbroken token ".to_owned()
            + &"超长中文内容".repeat(40)
            + " and more text that keeps going without any line breaks";
        let entity = cx.new(|cx| markdown::Markdown::new(source.into(), None, None, cx));
        cx.run_until_parked();

        let cx = cx.add_empty_window();
        let panel_width = 300.;

        cx.draw(
            point(px(0.), px(0.)),
            size(
                AvailableSpace::Definite(px(panel_width)),
                AvailableSpace::MinContent,
            ),
            |_, _| {
                let body = markdown::MarkdownElement::new(
                    entity.clone(),
                    markdown::MarkdownStyle::default(),
                )
                .into_any_element();

                let bubble = v_flex()
                    .min_w_0()
                    .max_w_full()
                    .px_2()
                    .py_1()
                    .debug_selector(|| "markdown-bubble".into())
                    .child(body)
                    .into_any_element();
                let wrapped = div().child(bubble).into_any_element();
                let item = align_message_bubble(wrapped, true, None);

                v_flex()
                    .w_full()
                    .min_w_0()
                    .child(
                        h_flex().w_full().justify_center().child(
                            div()
                                .max_w_full()
                                .w_full()
                                .min_w_0()
                                .child(v_flex().w_full().min_w_0().px_3().py_1().child(item)),
                        ),
                    )
                    .into_any_element()
            },
        );

        let bounds = cx
            .debug_bounds("markdown-bubble")
            .expect("markdown bubble should be rendered");
        assert!(
            bounds.size.width <= px(panel_width),
            "markdown bubble {bounds:?} overflows panel width {panel_width}"
        );
        // The long source must actually have been laid out (not an empty,
        // not-yet-parsed element), otherwise this test proves nothing.
        assert!(
            bounds.size.height > px(20.),
            "markdown bubble {bounds:?} did not render its text"
        );
    }
}
