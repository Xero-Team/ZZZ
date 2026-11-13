//! The Telegram engine: a grammers client driven on a dedicated runtime thread.
//!
//! The UI sends [`Command`]s through an `mpsc` channel and subscribes to
//! immutable [`ViewModel`] snapshots. All networking, file, and media work
//! happens on the engine thread.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use base64::Engine as _;
use grammers_client::client::{GlobalSearchIter, LoginToken, PasswordToken, SearchIter};
use grammers_client::media::{Downloadable, InputMedia, Media, PhotoSize};
use grammers_client::message::{InputMessage, Message};
use grammers_client::peer::{Dialog, Peer};
use grammers_client::sender::{ConnectionParams, SenderPool, UpdatesConfiguration};
use grammers_client::session::SessionData;
use grammers_client::session::types::{PeerAuth, PeerId, PeerKind, PeerRef};
use grammers_client::tl;
use grammers_client::update::Update;
use grammers_client::{Client, InvocationError, SignInError};
use tokio::sync::{mpsc, watch};

use crate::compose::{
    AttachmentKind, OutgoingItem, StagedAttachment, markdown_caption, markdown_message,
};
use crate::credentials::TelegramCredentials;
use crate::error::EngineError;
use crate::markdown::entities_to_markdown;
use crate::model::{
    AuthState, ChatFolderSnapshot, ChatKind, ChatSnapshot, ConnectionState, ContactSnapshot,
    DownloadState, FolderSelection, GeoSnapshot, MediaKind, MediaSnapshot, MessageAction,
    MessageSnapshot, PollOptionSnapshot, PollSnapshot, Presence, ReactionSnapshot, ReplySnapshot,
    SearchFilter, SearchHit, SendState, TopicSnapshot, TypingSnapshot, ViewModel, WebPageSnapshot,
    avatar_initials,
};
use crate::proxy::socks5_url;
use crate::session::{SessionStore, TelegramSession};

/// Number of messages fetched per history page.
const HISTORY_PAGE_SIZE: usize = 40;
/// Number of messages returned by a server-side search.
const SEARCH_LIMIT: usize = 50;
/// The minimum query length that triggers a server-side search. Shorter
/// queries only filter the loaded chat titles.
const SEARCH_MIN_QUERY_CHARS: usize = 2;
/// Telegram's reserved folder id for the archive.
const ARCHIVE_FOLDER_ID: i32 = 1;
/// The id of the General topic in a forum supergroup.
const GENERAL_TOPIC_ID: i32 = 1;
/// How many pages of archived dialogs are fetched at most. The archive is
/// normally small; the bound only prevents an unbounded loop on a broken server.
const ARCHIVE_PAGE_LIMIT: usize = 20;
/// How long a "typing" indicator stays visible without a refresh.
const TYPING_TTL: Duration = Duration::from_secs(6);
/// The engine does not resend the typing action more often than this; Telegram
/// itself expires it after roughly five seconds.
const TYPING_REFRESH_INTERVAL: Duration = Duration::from_secs(4);
/// How long to wait after the last keystroke before syncing a draft to the
/// server, so typing does not issue one request per character.
const DRAFT_SYNC_DELAY: Duration = Duration::from_millis(600);
/// Upper bound for the on-disk media cache. Older entries are evicted first.
const MEDIA_CACHE_MAX_BYTES: u64 = 512 * 1024 * 1024;
/// Maximum age for on-disk media before it is evicted.
const MEDIA_CACHE_MAX_AGE: Duration = Duration::from_hours(30 * 24);
/// How many times a media download is retried after Telegram reports an
/// expired file reference. One refetch is enough to mint a fresh reference;
/// a refetch that returns the same reference is not retried at all, because
/// the next request would be rejected identically.
/// <https://core.telegram.org/api/file-references>
const MEDIA_FILE_REFERENCE_RETRIES: usize = 1;
/// How many data-center migrations a QR login token may trigger before the
/// engine gives up and reports [`EngineError::QrTooManyMigrations`].
const QR_MIGRATION_LIMIT: usize = 3;

/// Requests sent from the UI thread to the engine thread.
#[derive(Clone)]
pub enum Command {
    /// Lazily start the engine and connect if credentials exist.
    Start,
    RefreshChats,
    SelectFolder {
        folder: FolderSelection,
    },
    StartQrLogin,
    SubmitPhone {
        phone: String,
    },
    SubmitCode {
        code: String,
    },
    SubmitPassword {
        password: String,
    },
    /// Requests a fresh login code for the phone number already submitted.
    ResendCode,
    CancelLogin,
    Logout,
    SelectChat {
        chat_id: i64,
    },
    SelectChatAt {
        chat_id: i64,
        message: MessageSnapshot,
    },
    LoadOlder {
        chat_id: i64,
    },
    /// Sends a planned sequence of text and media messages.
    SendOutgoing {
        chat_id: i64,
        items: Vec<OutgoingItem>,
        reply_to: Option<i32>,
    },
    RetrySend {
        chat_id: i64,
        local_id: u64,
    },
    /// Edits one of the account's own text messages.
    EditMessage {
        chat_id: i64,
        message_id: i32,
        text: String,
    },
    SetDraft {
        chat_id: i64,
        /// The forum topic the draft belongs to; `None` is the General topic.
        topic_id: Option<i32>,
        text: String,
    },
    /// Shows the General topic, or the replies of one forum topic.
    SelectTopic {
        chat_id: i64,
        topic_id: Option<i32>,
    },
    MarkRead {
        chat_id: i64,
        /// Mark messages up to this id as read. `None` means the newest message.
        max_message_id: Option<i32>,
    },
    MarkUnread {
        chat_id: i64,
    },
    /// Tells the engine whether the panel is currently focused on a chat and
    /// scrolled to the bottom, so it can decide when to report read state.
    SetViewing {
        chat_id: Option<i64>,
        focused: bool,
        at_bottom: bool,
    },
    /// Sends the "typing" action for a chat. The engine throttles repeated calls.
    SetTyping {
        chat_id: i64,
        typing: bool,
    },
    Search {
        query: String,
        chat_id: Option<i64>,
        filter: SearchFilter,
    },
    SearchMore,
    ClearSearch,
    DownloadMedia {
        chat_id: i64,
        message_id: i32,
    },
    /// Fetch only a small preview for an image-like attachment.
    DownloadThumbnail {
        chat_id: i64,
        message_id: i32,
    },
    DeleteMessages {
        chat_id: i64,
        message_ids: Vec<i32>,
        /// `true` deletes for everyone, `false` only for this account.
        revoke: bool,
    },
    ForwardMessages {
        source_chat_id: i64,
        message_ids: Vec<i32>,
        target_chat_id: i64,
        /// `true` forwards the content without the original author header.
        drop_author: bool,
    },
    SetPinned {
        chat_id: i64,
        pinned: bool,
    },
    SetPinnedMessage {
        chat_id: i64,
        message_id: Option<i32>,
    },
    SetReaction {
        chat_id: i64,
        message_id: i32,
        /// The emoji to set, or `None` to remove the account's reaction.
        emoji: Option<String>,
    },
    /// Casts or updates the account's vote on a poll message.
    VotePoll {
        chat_id: i64,
        message_id: i32,
        /// Opaque option bytes from [`crate::model::PollOptionSnapshot::option`].
        options: Vec<Vec<u8>>,
    },
    SetMuted {
        chat_id: i64,
        muted: bool,
    },
    SetArchived {
        chat_id: i64,
        archived: bool,
    },
    ClearHistory {
        chat_id: i64,
    },
    LeaveChat {
        chat_id: i64,
    },
    DismissError,
    Shutdown,
}

impl fmt::Debug for Command {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start => formatter.write_str("Start"),
            Self::RefreshChats => formatter.write_str("RefreshChats"),
            Self::SelectFolder { folder } => formatter
                .debug_struct("SelectFolder")
                .field("folder", folder)
                .finish(),
            Self::StartQrLogin => formatter.write_str("StartQrLogin"),
            Self::SubmitPhone { phone } => formatter
                .debug_struct("SubmitPhone")
                .field("phone", &redact_phone(phone))
                .finish(),
            Self::SubmitCode { .. } => formatter.write_str("SubmitCode(<redacted>)"),
            Self::SubmitPassword { .. } => formatter.write_str("SubmitPassword(<redacted>)"),
            Self::ResendCode => formatter.write_str("ResendCode"),
            Self::CancelLogin => formatter.write_str("CancelLogin"),
            Self::Logout => formatter.write_str("Logout"),
            Self::SelectChat { chat_id } => formatter
                .debug_struct("SelectChat")
                .field("chat_id", chat_id)
                .finish(),
            Self::SelectChatAt { chat_id, message } => formatter
                .debug_struct("SelectChatAt")
                .field("chat_id", chat_id)
                .field("message_chat_id", &message.chat_id)
                .field("message_id", &message.id)
                .finish(),
            Self::LoadOlder { chat_id } => formatter
                .debug_struct("LoadOlder")
                .field("chat_id", chat_id)
                .finish(),
            Self::SendOutgoing {
                chat_id,
                items,
                reply_to,
            } => formatter
                .debug_struct("SendOutgoing")
                .field("chat_id", chat_id)
                .field("items", &items.len())
                .field("reply_to", reply_to)
                .finish(),
            Self::RetrySend { chat_id, local_id } => formatter
                .debug_struct("RetrySend")
                .field("chat_id", chat_id)
                .field("local_id", local_id)
                .finish(),
            Self::EditMessage {
                chat_id,
                message_id,
                ..
            } => formatter
                .debug_struct("EditMessage")
                .field("chat_id", chat_id)
                .field("message_id", message_id)
                .field("text", &"<redacted>")
                .finish(),
            Self::SetDraft {
                chat_id, topic_id, ..
            } => formatter
                .debug_struct("SetDraft")
                .field("chat_id", chat_id)
                .field("topic_id", topic_id)
                .field("text", &"<redacted>")
                .finish(),
            Self::SelectTopic { chat_id, topic_id } => formatter
                .debug_struct("SelectTopic")
                .field("chat_id", chat_id)
                .field("topic_id", topic_id)
                .finish(),
            Self::MarkRead {
                chat_id,
                max_message_id,
            } => formatter
                .debug_struct("MarkRead")
                .field("chat_id", chat_id)
                .field("max_message_id", max_message_id)
                .finish(),
            Self::MarkUnread { chat_id } => formatter
                .debug_struct("MarkUnread")
                .field("chat_id", chat_id)
                .finish(),
            Self::SetViewing {
                chat_id,
                focused,
                at_bottom,
            } => formatter
                .debug_struct("SetViewing")
                .field("chat_id", chat_id)
                .field("focused", focused)
                .field("at_bottom", at_bottom)
                .finish(),
            Self::SetTyping { chat_id, typing } => formatter
                .debug_struct("SetTyping")
                .field("chat_id", chat_id)
                .field("typing", typing)
                .finish(),
            Self::Search {
                chat_id, filter, ..
            } => formatter
                .debug_struct("Search")
                .field("chat_id", chat_id)
                .field("filter", filter)
                .field("query", &"<redacted>")
                .finish(),
            Self::SearchMore => formatter.write_str("SearchMore"),
            Self::ClearSearch => formatter.write_str("ClearSearch"),
            Self::DownloadMedia {
                chat_id,
                message_id,
            } => formatter
                .debug_struct("DownloadMedia")
                .field("chat_id", chat_id)
                .field("message_id", message_id)
                .finish(),
            Self::DownloadThumbnail {
                chat_id,
                message_id,
            } => formatter
                .debug_struct("DownloadThumbnail")
                .field("chat_id", chat_id)
                .field("message_id", message_id)
                .finish(),
            Self::DeleteMessages {
                chat_id,
                message_ids,
                revoke,
            } => formatter
                .debug_struct("DeleteMessages")
                .field("chat_id", chat_id)
                .field("message_count", &message_ids.len())
                .field("revoke", revoke)
                .finish(),
            Self::ForwardMessages {
                source_chat_id,
                message_ids,
                target_chat_id,
                drop_author,
            } => formatter
                .debug_struct("ForwardMessages")
                .field("source_chat_id", source_chat_id)
                .field("message_count", &message_ids.len())
                .field("target_chat_id", target_chat_id)
                .field("drop_author", drop_author)
                .finish(),
            Self::SetPinned { chat_id, pinned } => formatter
                .debug_struct("SetPinned")
                .field("chat_id", chat_id)
                .field("pinned", pinned)
                .finish(),
            Self::SetPinnedMessage {
                chat_id,
                message_id,
            } => formatter
                .debug_struct("SetPinnedMessage")
                .field("chat_id", chat_id)
                .field("message_id", message_id)
                .finish(),
            Self::SetReaction {
                chat_id,
                message_id,
                emoji,
            } => formatter
                .debug_struct("SetReaction")
                .field("chat_id", chat_id)
                .field("message_id", message_id)
                .field("emoji", emoji)
                .finish(),
            Self::VotePoll {
                chat_id,
                message_id,
                options,
            } => formatter
                .debug_struct("VotePoll")
                .field("chat_id", chat_id)
                .field("message_id", message_id)
                .field("option_count", &options.len())
                .finish(),
            Self::SetMuted { chat_id, muted } => formatter
                .debug_struct("SetMuted")
                .field("chat_id", chat_id)
                .field("muted", muted)
                .finish(),
            Self::SetArchived { chat_id, archived } => formatter
                .debug_struct("SetArchived")
                .field("chat_id", chat_id)
                .field("archived", archived)
                .finish(),
            Self::ClearHistory { chat_id } => formatter
                .debug_struct("ClearHistory")
                .field("chat_id", chat_id)
                .finish(),
            Self::LeaveChat { chat_id } => formatter
                .debug_struct("LeaveChat")
                .field("chat_id", chat_id)
                .finish(),
            Self::DismissError => formatter.write_str("DismissError"),
            Self::Shutdown => formatter.write_str("Shutdown"),
        }
    }
}

/// Keeps only the leading country code and trailing digits of a phone number so
/// logs stay useful without recording the whole number.
fn redact_phone(phone: &str) -> String {
    let digits: String = phone.chars().filter(char::is_ascii_digit).collect();
    match digits.len() {
        0..=3 => "<redacted>".to_owned(),
        _ => format!(
            "{}…{}",
            &digits[..2.min(digits.len())],
            &digits[digits.len() - 2..]
        ),
    }
}

/// Runtime configuration for the engine.
#[derive(Clone)]
pub struct EngineConfig {
    /// Persistent directory for the encrypted session. Never cleared by cache
    /// eviction.
    pub session_dir: PathBuf,
    /// Regenerable media cache root. Safe for the OS to delete at any time.
    pub cache_dir: PathBuf,
    pub credentials: Option<TelegramCredentials>,
    /// Raw proxy URL from settings. Only `socks5://` is used.
    pub proxy_url: Option<String>,
}

/// A handle to the engine thread.
pub struct EngineHandle {
    command_tx: mpsc::UnboundedSender<Command>,
    snapshot_rx: watch::Receiver<Arc<ViewModel>>,
    engine: Mutex<EngineThread>,
}

/// The lazily-started state needed to run the engine thread. The thread is only
/// spawned once a command is sent, so an unused engine never wakes the UI from
/// its own thread.
struct EngineThread {
    config: Option<EngineConfig>,
    command_rx: Option<mpsc::UnboundedReceiver<Command>>,
    snapshot_tx: watch::Sender<Arc<ViewModel>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl EngineHandle {
    /// Creates a handle without starting the engine thread. The engine is
    /// spawned on the first call to [`EngineHandle::send`] and does not connect
    /// until it receives [`Command::Start`].
    pub fn spawn(config: EngineConfig) -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (snapshot_tx, snapshot_rx) = watch::channel(Arc::new(ViewModel::default()));
        Self {
            command_tx,
            snapshot_rx,
            engine: Mutex::new(EngineThread {
                config: Some(config),
                command_rx: Some(command_rx),
                snapshot_tx,
                thread: None,
            }),
        }
    }

    /// Sends a command to the engine, starting the engine thread if needed.
    /// Returns `false` when the engine stopped.
    pub fn send(&self, command: Command) -> bool {
        if !matches!(command, Command::Shutdown) {
            self.ensure_started();
        }
        self.command_tx.send(command).is_ok()
    }

    /// Subscribes to engine snapshots. Awaiting `changed()` on the receiver and
    /// then notifying is how the panel re-renders.
    pub fn subscribe(&self) -> watch::Receiver<Arc<ViewModel>> {
        self.snapshot_rx.clone()
    }

    /// Returns the most recent snapshot without waiting.
    pub fn snapshot(&self) -> Arc<ViewModel> {
        self.snapshot_rx.borrow().clone()
    }

    fn ensure_started(&self) {
        let mut engine = self
            .engine
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if engine.thread.is_some() {
            return;
        }
        let (Some(config), Some(command_rx)) = (engine.config.take(), engine.command_rx.take())
        else {
            return;
        };
        engine.thread = spawn_engine_thread(config, command_rx, engine.snapshot_tx.clone());
    }
}

impl Drop for EngineHandle {
    fn drop(&mut self) {
        self.command_tx.send(Command::Shutdown).ok();
        let mut engine = self
            .engine
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(thread) = engine.thread.take() {
            if thread.join().is_err() {
                log::warn!("telegram engine thread panicked during shutdown");
            }
        }
    }
}

fn spawn_engine_thread(
    config: EngineConfig,
    command_rx: mpsc::UnboundedReceiver<Command>,
    snapshot_tx: watch::Sender<Arc<ViewModel>>,
) -> Option<std::thread::JoinHandle<()>> {
    match std::thread::Builder::new()
        .name("telegram-engine".to_owned())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(2)
                .thread_name("telegram-worker")
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    log::error!("failed to start the Telegram runtime: {error}");
                    return;
                }
            };
            runtime.block_on(run_engine(config, command_rx, snapshot_tx));
        }) {
        Ok(thread) => Some(thread),
        Err(error) => {
            log::error!("failed to spawn the Telegram engine thread: {error}");
            None
        }
    }
}

async fn run_engine(
    config: EngineConfig,
    mut command_rx: mpsc::UnboundedReceiver<Command>,
    snapshot_tx: watch::Sender<Arc<ViewModel>>,
) {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut engine = EngineState::new(config, snapshot_tx, event_tx);
    engine.publish();

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                match command {
                    Some(Command::Shutdown) | None => break,
                    Some(command) => engine.handle_command(command).await,
                }
            }
            event = event_rx.recv() => {
                if let Some(event) = event {
                    engine.handle_event(event).await;
                }
            }
        }
    }

    engine.persist_session();
    if let Some(client) = &engine.client {
        client.disconnect();
    }
}

enum EngineEvent {
    Update(Update),
    Disconnected {
        error: String,
        epoch: u64,
    },
    QrToken(tl::enums::auth::LoginToken),
    QrError(EngineError),
    /// A debounced request to reload the chat list.
    RefreshChats,
    /// A debounced draft save for a chat.
    DraftSync {
        chat_id: i64,
        topic_id: Option<i32>,
        epoch: u64,
        text: String,
    },
    /// The topic list of a forum supergroup finished loading.
    TopicsLoaded {
        chat_id: i64,
        result: Result<Vec<TopicSnapshot>, EngineError>,
    },
    /// A page of replies in one forum topic finished loading.
    TopicHistoryLoaded {
        chat_id: i64,
        topic_id: i32,
        epoch: u64,
        offset_id: i32,
        result: Result<(Vec<MessageSnapshot>, bool), EngineError>,
    },
    /// A scheduled reconnection attempt after a drop.
    Reconnect,
    /// Fetch the messages between two known ids for the selected chat.
    FillGap {
        chat_id: i64,
        after_id: i32,
        before_id: i32,
    },
    /// The full chat list finished loading on a background task.
    ChatsLoaded {
        result: Result<LoadedChatList, EngineError>,
    },
    /// A history page finished loading.
    HistoryLoaded {
        conversation_chat_id: i64,
        epoch: u64,
        request: HistoryRequest,
        result: Result<(Vec<MessageSnapshot>, bool), EngineError>,
    },
    /// A gap-fill page finished loading.
    GapLoaded {
        chat_id: i64,
        epoch: u64,
        messages: Vec<MessageSnapshot>,
    },
    /// A server-side search finished.
    SearchLoaded {
        epoch: u64,
        append: bool,
        next_offset: Option<i32>,
        result: Result<Vec<SearchHit>, EngineError>,
    },
    /// An outgoing batch item finished sending or failed.
    SendFinished {
        chat_id: i64,
        local_id: u64,
        result: Result<Vec<MessageSnapshot>, EngineError>,
    },
    /// A remote "typing" indicator expired.
    TypingExpired {
        chat_id: i64,
        until_unix: i64,
    },
    /// An original media download finished.
    MediaLoaded {
        chat_id: i64,
        message_id: i32,
        result: Result<Option<PathBuf>, EngineError>,
    },
    /// A thumbnail download finished.
    ThumbnailLoaded {
        chat_id: i64,
        message_id: i32,
        result: Result<Option<PathBuf>, EngineError>,
    },
}

struct LoadedDialog {
    snapshot: ChatSnapshot,
    peer_ref: PeerRef,
    migrated_to_chat_id: Option<i64>,
}

struct LoadedChatList {
    chats: Vec<ChatSnapshot>,
    peer_refs: HashMap<i64, PeerRef>,
    migrated_to: HashMap<i64, i64>,
    migrated_from: HashMap<i64, i64>,
    folders: Vec<ChatFolderSnapshot>,
    /// The custom folder each chat belongs to. `None` means the main list;
    /// archived chats are tracked by `ChatSnapshot::archived` instead.
    folder_membership: HashMap<i64, Option<i32>>,
}

/// A send that is still in flight or failed, kept so the panel can retry it.
#[derive(Clone, Debug)]
struct PendingOutgoing {
    chat_id: i64,
    item: OutgoingItem,
    reply_to: Option<i32>,
    /// The forum topic the message was composed in, so a retry posts there too.
    topic_id: Option<i32>,
}

/// What the panel reports about its own visibility. Messages are only reported
/// as read while the panel is focused on the chat and scrolled to the bottom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ViewingState {
    chat_id: Option<i64>,
    focused: bool,
    at_bottom: bool,
}

/// One page request of a server-side search.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SearchRequest {
    query: String,
    chat_id: Option<i64>,
    filter: SearchFilter,
    offset_id: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HistoryCursor {
    source_chat_id: i64,
    before_message_id: Option<i32>,
}

impl HistoryCursor {
    fn is_initial_for(self, chat_id: i64) -> bool {
        self.source_chat_id == chat_id && self.before_message_id.is_none()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryRequest {
    Page(HistoryCursor),
    RefreshTail { source_chat_id: i64 },
}

impl HistoryRequest {
    fn source_chat_id(self) -> i64 {
        match self {
            Self::Page(cursor) => cursor.source_chat_id,
            Self::RefreshTail { source_chat_id } => source_chat_id,
        }
    }

    fn before_message_id(self) -> Option<i32> {
        match self {
            Self::Page(cursor) => cursor.before_message_id,
            Self::RefreshTail { .. } => None,
        }
    }
}

/// The in-memory history for one chat. Kept across chat switches so selecting a
/// chat again renders instantly before its tail is refreshed.
#[derive(Clone, Default)]
struct CachedHistory {
    messages: Vec<MessageSnapshot>,
    next_page: Option<HistoryCursor>,
    loaded: bool,
}

/// The reply cache for one forum topic. Topic pages are paged by the oldest
/// loaded message id rather than by a chat-level cursor.
#[derive(Clone, Default)]
struct TopicHistoryCache {
    messages: Vec<MessageSnapshot>,
    next_offset: Option<i32>,
    loaded: bool,
}

struct EngineState {
    config: EngineConfig,
    view: ViewModel,
    snapshot_tx: watch::Sender<Arc<ViewModel>>,
    event_tx: mpsc::UnboundedSender<EngineEvent>,
    store: SessionStore,
    media_dir: PathBuf,
    thumbnail_dir: PathBuf,
    session: Option<Arc<TelegramSession>>,
    client: Option<Client>,
    proxy_url: Option<String>,
    login_token: Option<LoginToken>,
    password_token: Option<PasswordToken>,
    /// The normalized phone number of the last code request, for `ResendCode`.
    pending_phone: Option<String>,
    qr_task: Option<tokio::task::JoinHandle<()>>,
    /// The sender-pool runner for the live connection, aborted when the
    /// connection is torn down so a dropped session does not linger.
    runner_task: Option<tokio::task::JoinHandle<()>>,
    peer_refs: HashMap<i64, PeerRef>,
    /// Every loaded chat, before folder filtering. `view.chats` is a derived
    /// view of this list so switching folders never refetches.
    all_chats: Vec<ChatSnapshot>,
    folders: Vec<ChatFolderSnapshot>,
    folder_membership: HashMap<i64, Option<i32>>,
    /// Old basic-group chat id to the visible supergroup chat id.
    migrated_to: HashMap<i64, i64>,
    /// Visible supergroup chat id to its old basic-group chat id.
    migrated_from: HashMap<i64, i64>,
    /// Drafts keyed by chat and forum topic; `None` is the General topic.
    drafts: HashMap<(i64, Option<i32>), String>,
    /// The latest scheduled draft-save generation per conversation, so only
    /// the last keystroke in a burst reaches the server.
    draft_epochs: HashMap<(i64, Option<i32>), u64>,
    /// The last draft text confirmed by the server, to skip redundant saves.
    saved_drafts: HashMap<(i64, Option<i32>), String>,
    /// Per-forum-topic reply cache, keyed by chat and topic id.
    topic_histories: HashMap<(i64, i32), TopicHistoryCache>,
    /// Per-chat history cache, so switching chats does not refetch.
    histories: HashMap<i64, CachedHistory>,
    /// Bumped whenever the displayed chat changes, so stale page loads are dropped.
    history_epoch: u64,
    /// Bumped on every search request, so stale results are dropped.
    search_epoch: u64,
    /// Bumped on every connection attempt, so a stale stream cannot tear down a
    /// newer connection.
    connection_epoch: u64,
    chats_refresh_scheduled: bool,
    reconnect_scheduled: bool,
    reconnect_attempts: u32,
    /// The planned item for each optimistic message, so a failed send can be
    /// retried with its attachment intact.
    pending_outgoing: HashMap<u64, PendingOutgoing>,
    /// The last reported panel visibility, used to decide when to mark read.
    viewing: ViewingState,
    /// The last time a typing action was sent per chat, to throttle refreshes.
    typing_sent_at: HashMap<i64, Instant>,
    /// Sender display names learned from messages, for typing indicators whose
    /// raw update only carries a user id.
    known_names: HashMap<i64, String>,
    /// The cursor for the next page of the current server-side search.
    search_next_offset: Option<i32>,
}

impl EngineState {
    fn new(
        config: EngineConfig,
        snapshot_tx: watch::Sender<Arc<ViewModel>>,
        event_tx: mpsc::UnboundedSender<EngineEvent>,
    ) -> Self {
        let store = SessionStore::new(config.session_dir.clone());
        if let Err(error) = store.ensure_directory() {
            log::warn!("failed to create the Telegram session directory: {error:#}");
        }
        let media_dir = config.cache_dir.join("media");
        let thumbnail_dir = config.cache_dir.join("thumbnails");
        let proxy_url = config.proxy_url.as_deref().and_then(socks5_url);
        let view = ViewModel {
            configured: config.credentials.is_some(),
            ..Default::default()
        };
        if config.proxy_url.is_some() && proxy_url.is_none() {
            log::warn!("the configured proxy is not SOCKS5; Telegram will connect directly");
        }
        Self {
            config,
            view,
            snapshot_tx,
            event_tx,
            store,
            media_dir,
            thumbnail_dir,
            session: None,
            client: None,
            proxy_url,
            login_token: None,
            password_token: None,
            pending_phone: None,
            qr_task: None,
            runner_task: None,
            peer_refs: HashMap::new(),
            all_chats: Vec::new(),
            folders: Vec::new(),
            folder_membership: HashMap::new(),
            migrated_to: HashMap::new(),
            migrated_from: HashMap::new(),
            drafts: HashMap::new(),
            draft_epochs: HashMap::new(),
            saved_drafts: HashMap::new(),
            topic_histories: HashMap::new(),
            histories: HashMap::new(),
            history_epoch: 0,
            search_epoch: 0,
            connection_epoch: 0,
            chats_refresh_scheduled: false,
            reconnect_scheduled: false,
            reconnect_attempts: 0,
            pending_outgoing: HashMap::new(),
            viewing: ViewingState::default(),
            typing_sent_at: HashMap::new(),
            known_names: HashMap::new(),
            search_next_offset: None,
        }
    }

    fn publish(&self) {
        if self.snapshot_tx.send(Arc::new(self.view.clone())).is_err() {
            log::debug!("telegram snapshot receiver has been dropped");
        }
    }

    fn send_event(&self, event: EngineEvent) {
        if self.event_tx.send(event).is_err() {
            log::debug!("telegram event receiver has been dropped");
        }
    }

    fn set_error(&mut self, error: EngineError) {
        log::warn!("Telegram engine error: {error:?}");
        self.view.flood_wait_seconds = match &error {
            EngineError::FloodWait { seconds } | EngineError::SlowMode { seconds } => {
                Some(*seconds)
            }
            _ => None,
        };
        self.view.error = Some(error);
        self.publish();
    }

    fn clear_error(&mut self) {
        self.view.error = None;
        self.view.flood_wait_seconds = None;
    }

    /// Resolves the peer and connected client for a chat, returning `None` when
    /// the chat is unknown or the engine is offline.
    fn chat_context(&self, chat_id: i64) -> Option<(PeerRef, Client)> {
        let peer_ref = self.peer_refs.get(&chat_id).copied()?;
        let client = self.client.clone()?;
        Some((peer_ref, client))
    }

    /// Schedules a server-side draft save after the typing burst settles. The
    /// panel sends one `SetDraft` per keystroke, so the delay collapses a burst
    /// into a single request.
    fn schedule_draft_sync(&mut self, chat_id: i64, topic_id: Option<i32>, text: &str) {
        let key = (chat_id, topic_id);
        if self
            .saved_drafts
            .get(&key)
            .is_some_and(|saved| saved == text)
        {
            return;
        }
        let epoch = self.draft_epochs.entry(key).or_default();
        *epoch = epoch.wrapping_add(1);
        let epoch = *epoch;
        let text = text.to_owned();
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(DRAFT_SYNC_DELAY).await;
            let event = EngineEvent::DraftSync {
                chat_id,
                topic_id,
                epoch,
                text,
            };
            if event_tx.send(event).is_err() {
                log::debug!("telegram event receiver has been dropped");
            }
        });
    }

    async fn apply_draft_sync(
        &mut self,
        chat_id: i64,
        topic_id: Option<i32>,
        epoch: u64,
        text: String,
    ) {
        let key = (chat_id, topic_id);
        if self.draft_epochs.get(&key) != Some(&epoch) {
            return;
        }
        // An empty draft only needs a request when there is something server
        // side to clear. The epoch bump above still invalidates a pending save
        // for text the user already deleted.
        if text.is_empty() && !self.saved_drafts.contains_key(&key) {
            return;
        }
        if self.saved_drafts.get(&key) == Some(&text) {
            return;
        }
        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            return;
        };
        let reply_to = topic_id.map(|topic_id| {
            tl::enums::InputReplyTo::Message(tl::types::InputReplyToMessage {
                reply_to_msg_id: 0,
                top_msg_id: Some(topic_id),
                reply_to_peer_id: None,
                quote_text: None,
                quote_entities: None,
                quote_offset: None,
                monoforum_peer_id: None,
                todo_item_id: None,
                poll_option: None,
                quote_rich: None,
            })
        });
        let request = tl::functions::messages::SaveDraft {
            no_webpage: false,
            invert_media: false,
            reply_to,
            peer: peer_ref.into(),
            message: text.clone(),
            entities: None,
            media: None,
            effect: None,
            suggested_post: None,
            rich_message: None,
        };
        match client.invoke(&request).await {
            Ok(_) => {
                self.saved_drafts.insert(key, text);
            }
            Err(error) => log::debug!("failed to save a Telegram draft: {error}"),
        }
    }

    /// Like [`Self::chat_context`], but reports the missing half to the panel as
    /// a user-visible error. Used by the send paths, where a dropped message
    /// must be surfaced.
    fn sending_context(&mut self, chat_id: i64) -> Option<(PeerRef, Client)> {
        let chat_id = self.canonical_chat_id(chat_id);
        let Some(peer_ref) = self.peer_refs.get(&chat_id).copied() else {
            self.set_error(EngineError::ChatUnavailable);
            return None;
        };
        let Some(client) = self.client.clone() else {
            self.set_error(EngineError::NotConnected);
            return None;
        };
        if self
            .all_chats
            .iter()
            .find(|chat| chat.id == chat_id)
            .is_some_and(|chat| !chat.can_send)
        {
            self.set_error(EngineError::ReadOnlyChat);
            return None;
        }
        Some((peer_ref, client))
    }

    async fn handle_command(&mut self, command: Command) {
        match command {
            Command::Start => self.ensure_connected().await,
            Command::RefreshChats => {
                self.ensure_connected().await;
                if self.view.auth.is_signed_in() {
                    self.spawn_load_chats();
                }
            }
            Command::SelectFolder { folder } => self.select_folder(folder),
            Command::StartQrLogin => self.start_qr_login().await,
            Command::SubmitPhone { phone } => self.submit_phone(phone).await,
            Command::SubmitCode { code } => self.submit_code(code).await,
            Command::SubmitPassword { password } => self.submit_password(password).await,
            Command::ResendCode => self.resend_code().await,
            Command::CancelLogin => {
                self.stop_qr();
                self.login_token = None;
                self.password_token = None;
                self.pending_phone = None;
                self.view.auth = AuthState::SignedOut;
                self.clear_error();
                self.publish();
            }
            Command::Logout => self.logout().await,
            Command::SelectChat { chat_id } => self.select_chat(chat_id),
            Command::SelectChatAt { chat_id, message } => self.select_chat_at(chat_id, message),
            Command::LoadOlder { chat_id } => self.load_older(chat_id),
            Command::SendOutgoing {
                chat_id,
                items,
                reply_to,
            } => self.send_outgoing(chat_id, items, reply_to),
            Command::RetrySend { chat_id, local_id } => self.retry_send(chat_id, local_id),
            Command::EditMessage {
                chat_id,
                message_id,
                text,
            } => self.edit_message(chat_id, message_id, text).await,
            Command::SetDraft {
                chat_id,
                topic_id,
                text,
            } => {
                let chat_id = self.canonical_chat_id(chat_id);
                let key = (chat_id, topic_id);
                let was_empty = self.drafts.get(&key).is_none_or(|draft| draft.is_empty());
                self.drafts.insert(key, text.clone());
                let preview_draft = self.chat_draft(chat_id);
                if let Some(chat) = self.all_chats.iter_mut().find(|chat| chat.id == chat_id) {
                    chat.draft = preview_draft;
                }
                if self.view.selected_chat == Some(chat_id) && self.view.selected_topic == topic_id
                {
                    // The panel keeps its own copy while typing, so publishing on
                    // every keystroke would clone the whole view model for no
                    // visible change. Only the empty/non-empty transition matters
                    // to other consumers, such as a second window.
                    self.view.draft = text.clone();
                }
                if was_empty != text.is_empty() {
                    self.publish();
                }
                self.schedule_draft_sync(chat_id, topic_id, &text);
            }
            Command::SelectTopic { chat_id, topic_id } => self.select_topic(chat_id, topic_id),
            Command::MarkRead {
                chat_id,
                max_message_id,
            } => self.mark_read(chat_id, max_message_id),
            Command::MarkUnread { chat_id } => self.mark_unread(chat_id).await,
            Command::SetViewing {
                chat_id,
                focused,
                at_bottom,
            } => self.set_viewing(chat_id, focused, at_bottom),
            Command::SetTyping { chat_id, typing } => self.set_typing(chat_id, typing),
            Command::Search {
                query,
                chat_id,
                filter,
            } => self.search(query, chat_id, filter),
            Command::SearchMore => self.search_more(),
            Command::ClearSearch => {
                self.search_epoch = self.search_epoch.wrapping_add(1);
                self.search_next_offset = None;
                self.view.searching = false;
                self.view.has_more_search = false;
                self.view.search_query.clear();
                self.view.search_results.clear();
                self.publish();
            }
            Command::DownloadMedia {
                chat_id,
                message_id,
            } => self.download(chat_id, message_id, DownloadTarget::Original),
            Command::DownloadThumbnail {
                chat_id,
                message_id,
            } => self.download(chat_id, message_id, DownloadTarget::Thumbnail),
            Command::DeleteMessages {
                chat_id,
                message_ids,
                revoke,
            } => self.delete_messages(chat_id, message_ids, revoke).await,
            Command::ForwardMessages {
                source_chat_id,
                message_ids,
                target_chat_id,
                drop_author,
            } => {
                self.forward_messages(source_chat_id, message_ids, target_chat_id, drop_author)
                    .await
            }
            Command::SetPinned { chat_id, pinned } => self.set_pinned(chat_id, pinned).await,
            Command::SetPinnedMessage {
                chat_id,
                message_id,
            } => self.set_pinned_message(chat_id, message_id).await,
            Command::SetReaction {
                chat_id,
                message_id,
                emoji,
            } => self.set_reaction(chat_id, message_id, emoji).await,
            Command::VotePoll {
                chat_id,
                message_id,
                options,
            } => self.vote_poll(chat_id, message_id, options).await,
            Command::SetMuted { chat_id, muted } => self.set_muted(chat_id, muted).await,
            Command::SetArchived { chat_id, archived } => {
                self.set_archived(chat_id, archived).await
            }
            Command::ClearHistory { chat_id } => self.clear_history(chat_id).await,
            Command::LeaveChat { chat_id } => self.leave_chat(chat_id).await,
            Command::DismissError => {
                self.clear_error();
                self.publish();
            }
            Command::Shutdown => {}
        }
    }

    async fn ensure_connected(&mut self) {
        if self.client.is_some() {
            // A second panel (or a reopened one) subscribes after the last
            // snapshot, so answer `Start` with the current state even when the
            // connection already exists.
            self.publish();
            return;
        }
        let Some(credentials) = self.config.credentials.clone() else {
            self.view.configured = false;
            self.publish();
            return;
        };

        self.view.connection = ConnectionState::Connecting;
        self.publish();

        let session_data = match self.store.load() {
            Ok(Some(data)) => data,
            Ok(None) => SessionData::default(),
            Err(error) => {
                log::warn!("failed to load the Telegram session: {error:#}");
                SessionData::default()
            }
        };

        let session = Arc::new(TelegramSession::new(session_data));
        let mut params = ConnectionParams::default();
        params.device_model = "ZZZ".to_owned();
        params.app_version = env!("CARGO_PKG_VERSION").to_owned();
        params.proxy_url = self.proxy_url.clone();

        let pool = SenderPool::with_configuration(session.clone(), credentials.api_id, params);
        let SenderPool {
            runner,
            handle,
            updates,
        } = pool;
        let runner_task = tokio::spawn(runner.run());
        self.runner_task = Some(runner_task);

        let client = Client::new(handle);
        self.session = Some(session);
        self.client = Some(client.clone());

        self.connection_epoch = self.connection_epoch.wrapping_add(1);
        let connection_epoch = self.connection_epoch;
        match client
            .stream_updates(updates, UpdatesConfiguration::default())
            .await
        {
            Ok(mut stream) => {
                let event_tx = self.event_tx.clone();
                tokio::spawn(async move {
                    loop {
                        match stream.next().await {
                            Ok(update) => {
                                if event_tx.send(EngineEvent::Update(update)).is_err() {
                                    break;
                                }
                            }
                            Err(error) => {
                                if event_tx
                                    .send(EngineEvent::Disconnected {
                                        error: error.to_string(),
                                        epoch: connection_epoch,
                                    })
                                    .is_err()
                                {
                                    break;
                                }
                                break;
                            }
                        }
                    }
                });
            }
            Err(error) => {
                log::warn!("failed to start the Telegram update stream: {error}");
            }
        }

        match client.is_authorized().await {
            Ok(true) => {
                self.reconnect_attempts = 0;
                self.view.connection = ConnectionState::Connected;
                self.restore_profile(&client).await;
                self.spawn_load_chats();
            }
            Ok(false) => {
                self.view.connection = ConnectionState::Connected;
                self.view.auth = AuthState::SignedOut;
            }
            Err(error) => {
                // Keep the failed client around and the reconnect timer never
                // fires, so drop it before scheduling a retry. Abort the pool
                // runner too, otherwise the failed connection lingers.
                if let Some(task) = self.runner_task.take() {
                    task.abort();
                }
                self.client = None;
                self.session = None;
                self.view.connection = ConnectionState::Offline;
                log::warn!("failed to connect to Telegram: {error}");
                self.set_error(EngineError::from_invocation_with(
                    &error,
                    EngineError::NotConnected,
                ));
                self.publish();
                self.schedule_reconnect();
                return;
            }
        }
        self.publish();
    }

    async fn restore_profile(&mut self, client: &Client) {
        match client.get_me().await {
            Ok(_) => {
                self.view.auth = AuthState::SignedIn;
            }
            Err(error) => {
                log::warn!("failed to fetch the Telegram profile: {error}");
                self.view.auth = AuthState::SignedOut;
            }
        }
    }

    async fn start_qr_login(&mut self) {
        let Some(client) = self.client.clone() else {
            self.set_error(EngineError::NotConnected);
            return;
        };
        let Some(credentials) = self.config.credentials.clone() else {
            self.set_error(EngineError::NoCredentials);
            return;
        };
        self.stop_qr();
        self.clear_error();

        match export_qr_token(&client, &credentials).await {
            Ok(result) => {
                self.handle_qr_result(Ok(result)).await;
                if matches!(self.view.auth, AuthState::AwaitingQr { .. }) {
                    self.spawn_qr_poll(client, credentials);
                }
            }
            Err(error) => self.set_error(error),
        }
    }

    fn spawn_qr_poll(&mut self, client: Client, credentials: TelegramCredentials) {
        self.stop_qr();
        let event_tx = self.event_tx.clone();
        self.qr_task = Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(2));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                match export_qr_token(&client, &credentials).await {
                    Ok(result) => {
                        let complete = matches!(result, tl::enums::auth::LoginToken::Success(_));
                        if event_tx.send(EngineEvent::QrToken(result)).is_err() || complete {
                            break;
                        }
                    }
                    Err(error) => {
                        if event_tx.send(EngineEvent::QrError(error)).is_err() {
                            break;
                        }
                        break;
                    }
                }
            }
        }));
    }

    fn stop_qr(&mut self) {
        if let Some(task) = self.qr_task.take() {
            task.abort();
        }
    }

    async fn handle_qr_result(
        &mut self,
        mut result: Result<tl::enums::auth::LoginToken, EngineError>,
    ) {
        for _ in 0..QR_MIGRATION_LIMIT {
            match result {
                Ok(tl::enums::auth::LoginToken::Token(token)) => {
                    let url = qr_login_url(&token.token);
                    self.view.auth = AuthState::AwaitingQr { url };
                    self.publish();
                    return;
                }
                Ok(tl::enums::auth::LoginToken::Success(_)) => {
                    self.finish_qr_login().await;
                    return;
                }
                Ok(tl::enums::auth::LoginToken::MigrateTo(migrate)) => {
                    let Some(client) = self.client.clone() else {
                        return;
                    };
                    let request = tl::functions::auth::ImportLoginToken {
                        token: migrate.token,
                    };
                    result = client
                        .invoke(&request)
                        .await
                        .map_err(|error| EngineError::from_invocation(&error));
                }
                Err(error) => {
                    self.fail_qr(error);
                    return;
                }
            }
        }
        self.fail_qr(EngineError::QrTooManyMigrations);
    }

    /// Clears the QR state so a stale code never stays on screen after an error.
    fn fail_qr(&mut self, error: EngineError) {
        self.stop_qr();
        self.login_token = None;
        self.password_token = None;
        self.view.auth = AuthState::SignedOut;
        self.set_error(error);
    }

    async fn finish_qr_login(&mut self) {
        self.stop_qr();
        let Some(client) = self.client.clone() else {
            return;
        };
        match client.get_me().await {
            Ok(_) => self.finish_login().await,
            Err(error) => {
                log::warn!("signed in on the phone but fetching the account failed: {error}");
                self.fail_qr(EngineError::QrFailed);
            }
        }
    }

    async fn submit_phone(&mut self, phone: String) {
        let Some(client) = self.client.clone() else {
            self.set_error(EngineError::NotConnected);
            return;
        };
        let Some(credentials) = self.config.credentials.clone() else {
            self.set_error(EngineError::NoCredentials);
            return;
        };
        let Some(phone) = normalize_phone(&phone) else {
            self.set_error(EngineError::InvalidPhone);
            return;
        };
        self.clear_error();
        match client
            .request_login_code(&phone, &credentials.api_hash)
            .await
        {
            Ok(token) => {
                log::info!("Telegram login code requested successfully");
                self.login_token = Some(token);
                self.password_token = None;
                self.pending_phone = Some(phone.clone());
                self.view.auth = AuthState::AwaitingCode {
                    phone: phone.clone(),
                };
            }
            Err(error) => {
                self.set_error(EngineError::from_invocation(&error));
            }
        }
        self.publish();
    }

    /// Requests a fresh login code for the phone number already submitted.
    async fn resend_code(&mut self) {
        match self.pending_phone.clone() {
            Some(phone) => self.submit_phone(phone).await,
            None => self.set_error(EngineError::RequestCodeFailed),
        }
    }

    async fn submit_code(&mut self, code: String) {
        let Some(client) = self.client.clone() else {
            self.set_error(EngineError::NotConnected);
            return;
        };
        let Some(token) = self.login_token.take() else {
            self.set_error(EngineError::RequestCodeFailed);
            return;
        };
        self.clear_error();
        match client.sign_in(&token, code.trim()).await {
            Ok(_) => self.finish_login().await,
            Err(SignInError::PasswordRequired(password_token)) => {
                let hint = password_token.hint().map(ToOwned::to_owned);
                self.password_token = Some(password_token);
                self.view.auth = AuthState::AwaitingPassword { hint };
                self.publish();
            }
            Err(SignInError::InvalidCode) => {
                self.login_token = Some(token);
                self.set_error(EngineError::InvalidCode);
            }
            Err(SignInError::SignUpRequired) => self.set_error(EngineError::SignUpRequired),
            Err(SignInError::InvalidPassword(_)) => self.set_error(EngineError::InvalidPassword),
            Err(SignInError::Other(error)) => self.set_error(EngineError::from_invocation(&error)),
        }
    }

    async fn submit_password(&mut self, password: String) {
        let Some(client) = self.client.clone() else {
            self.set_error(EngineError::NotConnected);
            return;
        };
        let Some(password_token) = self.password_token.take() else {
            self.set_error(EngineError::NoPendingPassword);
            return;
        };
        self.clear_error();
        match client.check_password(password_token, password).await {
            Ok(_) => self.finish_login().await,
            Err(SignInError::InvalidPassword(password_token)) => {
                self.password_token = Some(password_token);
                self.set_error(EngineError::InvalidPassword);
            }
            Err(SignInError::Other(error)) => self.set_error(EngineError::from_invocation(&error)),
            Err(_) => self.set_error(EngineError::SignInFailed),
        }
    }

    async fn finish_login(&mut self) {
        self.login_token = None;
        self.password_token = None;
        self.pending_phone = None;
        self.view.auth = AuthState::SignedIn;
        self.view.connection = ConnectionState::Connected;
        self.persist_session();
        self.publish();
        self.spawn_load_chats();
    }

    fn persist_session(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        match session.snapshot() {
            Ok(data) => {
                if let Err(error) = self.store.save(&data) {
                    log::warn!("failed to save the Telegram session: {error:#}");
                }
            }
            Err(error) => log::warn!("failed to snapshot the Telegram session: {error}"),
        }
    }

    async fn logout(&mut self) {
        self.stop_qr();
        if let Some(client) = self.client.clone() {
            if let Err(error) = client.sign_out().await {
                log::warn!("failed to sign out of Telegram: {error}");
            }
        }
        if let Err(error) = self.store.clear() {
            log::warn!("failed to clear the Telegram session: {error:#}");
        }
        self.peer_refs.clear();
        self.all_chats.clear();
        self.folders.clear();
        self.folder_membership.clear();
        self.migrated_to.clear();
        self.migrated_from.clear();
        self.drafts.clear();
        self.draft_epochs.clear();
        self.saved_drafts.clear();
        self.histories.clear();
        self.topic_histories.clear();
        self.pending_outgoing.clear();
        self.typing_sent_at.clear();
        self.known_names.clear();
        self.viewing = ViewingState::default();
        self.history_epoch = self.history_epoch.wrapping_add(1);
        self.search_next_offset = None;
        self.reconnect_scheduled = false;
        self.reconnect_attempts = 0;
        self.clear_cache().await;
        self.view = ViewModel {
            configured: self.config.credentials.is_some(),
            connection: if self.client.is_some() {
                ConnectionState::Connected
            } else {
                ConnectionState::Offline
            },
            auth: AuthState::SignedOut,
            ..Default::default()
        };
        self.publish();
    }

    /// Removes the regenerable media cache. The encrypted session lives in a
    /// separate root and is never touched here.
    async fn clear_cache(&self) {
        let directories = [self.media_dir.clone(), self.thumbnail_dir.clone()];
        let cleared = tokio::task::spawn_blocking(move || {
            for directory in &directories {
                match std::fs::remove_dir_all(directory) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => log::warn!(
                        "failed to clear the Telegram cache at {}: {error}",
                        directory.display()
                    ),
                }
            }
        })
        .await;
        if let Err(error) = cleared {
            log::warn!("the Telegram cache cleanup task was cancelled: {error}");
        }
    }

    /// Schedules a chat-list reload on a background task, so the select loop
    /// keeps servicing commands and incoming updates.
    fn spawn_load_chats(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        if self.view.loading_chats {
            return;
        }
        self.view.loading_chats = true;
        self.publish();
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let result = fetch_dialogs(&client).await;
            if event_tx.send(EngineEvent::ChatsLoaded { result }).is_err() {
                log::debug!("telegram event receiver has been dropped");
            }
        });
    }

    fn apply_chats_loaded(&mut self, result: Result<LoadedChatList, EngineError>) {
        self.view.loading_chats = false;
        match result {
            Ok(loaded) => {
                let selected_chat = self.view.selected_chat;
                self.peer_refs = loaded.peer_refs;
                self.migrated_to = loaded.migrated_to;
                self.migrated_from = loaded.migrated_from;
                self.all_chats = loaded.chats;
                self.folders = loaded.folders;
                self.folder_membership = loaded.folder_membership;
                self.remap_migrated_state();
                // Seed drafts the server knows about, without overwriting text
                // the user typed while the list was loading.
                for chat in &self.all_chats {
                    if let Some(draft) = chat.draft.clone().filter(|draft| !draft.is_empty()) {
                        self.drafts
                            .entry((chat.id, None))
                            .or_insert_with(|| draft.clone());
                        self.saved_drafts.entry((chat.id, None)).or_insert(draft);
                    }
                }
                self.apply_known_drafts();
                self.refresh_visible_chats();
                self.recompute_unread();

                let redirected_chat = selected_chat
                    .map(|chat_id| self.canonical_chat_id(chat_id))
                    .filter(|chat_id| Some(*chat_id) != selected_chat);
                if let Some(chat_id) = redirected_chat {
                    self.view.selected_chat = None;
                    self.select_chat(chat_id);
                } else {
                    // A chat can disappear between reloads (left, deleted, or
                    // blocked). Do not keep rendering a conversation that has no
                    // peer reference any more.
                    if self.view.selected_chat.is_some_and(|chat_id| {
                        !self.all_chats.iter().any(|chat| chat.id == chat_id)
                    }) {
                        self.view.selected_chat = None;
                        self.view.history.clear();
                        self.view.draft.clear();
                        self.view.topics.clear();
                        self.view.loading_topics = false;
                        self.view.selected_topic = None;
                        self.view.has_more_history = false;
                        self.history_epoch = self.history_epoch.wrapping_add(1);
                    }
                    self.publish();
                }
            }
            Err(error) => self.set_error(error),
        }
    }

    /// Copies the in-memory drafts onto the freshly loaded chat snapshots so a
    /// reload does not drop an unsent composer.
    fn apply_known_drafts(&mut self) {
        let drafts: Vec<(i64, Option<String>)> = self
            .all_chats
            .iter()
            .map(|chat| (chat.id, self.chat_draft(chat.id)))
            .collect();
        for (chat_id, draft) in drafts {
            if let Some(draft) = draft
                && let Some(chat) = self.all_chats.iter_mut().find(|chat| chat.id == chat_id)
            {
                chat.draft = Some(draft);
            }
        }
    }

    /// The draft the chat list should preview: the General topic first, then
    /// the lowest topic id with text. The fallback is stable across snapshots,
    /// unlike iterating the hash map directly.
    fn chat_draft(&self, chat_id: i64) -> Option<String> {
        self.drafts
            .get(&(chat_id, None))
            .filter(|draft| !draft.is_empty())
            .cloned()
            .or_else(|| {
                self.drafts
                    .iter()
                    .filter(|((id, _), draft)| *id == chat_id && !draft.is_empty())
                    .min_by_key(|((_, topic), _)| topic.unwrap_or(i32::MAX))
                    .map(|(_, draft)| draft.clone())
            })
    }

    fn select_folder(&mut self, folder: FolderSelection) {
        if self.view.selected_folder == folder {
            return;
        }
        self.view.selected_folder = folder;
        self.refresh_visible_chats();
        self.recompute_unread();
        self.publish();
    }

    /// Recomputes the folder-filtered chat list from `all_chats`.
    fn refresh_visible_chats(&mut self) {
        let folder = self.view.selected_folder;
        let membership = &self.folder_membership;
        let mut chats: Vec<ChatSnapshot> = self
            .all_chats
            .iter()
            .filter(|chat| match folder {
                FolderSelection::All => !chat.archived,
                FolderSelection::Archive => chat.archived,
                FolderSelection::Folder(id) => {
                    !chat.archived && membership.get(&chat.id).copied().flatten() == Some(id)
                }
            })
            .cloned()
            .collect();
        chats.sort_by(|left, right| {
            right
                .pinned
                .cmp(&left.pinned)
                .then_with(|| right.timestamp_unix.cmp(&left.timestamp_unix))
        });
        self.view.chats = chats;
    }

    /// The title shown for a chat, falling back to the id for unknown peers.
    fn chat_title(&self, chat_id: i64) -> String {
        self.all_chats
            .iter()
            .find(|chat| chat.id == chat_id)
            .map_or_else(|| chat_id.to_string(), |chat| chat.title.clone())
    }

    fn canonical_chat_id(&self, chat_id: i64) -> i64 {
        self.migrated_to.get(&chat_id).copied().unwrap_or(chat_id)
    }

    fn remap_migrated_state(&mut self) {
        let migrations: Vec<(i64, i64)> = self
            .migrated_to
            .iter()
            .map(|(&source_chat_id, &target_chat_id)| (source_chat_id, target_chat_id))
            .collect();
        for (source_chat_id, target_chat_id) in migrations {
            if let Some(draft) = self.drafts.remove(&(source_chat_id, None)) {
                self.drafts.entry((target_chat_id, None)).or_insert(draft);
            }

            let Some(source_history) = self.histories.remove(&source_chat_id) else {
                continue;
            };
            let target_history = self.histories.remove(&target_chat_id).unwrap_or_default();
            let target_was_loaded = target_history.loaded;
            let messages = merge_history(
                &target_history.messages,
                source_history.messages,
                target_chat_id,
                Some(source_chat_id),
            );
            let loaded = target_was_loaded || source_history.loaded;
            let next_page = if target_was_loaded {
                target_history.next_page.or(source_history.next_page)
            } else if loaded {
                Some(HistoryCursor {
                    source_chat_id: target_chat_id,
                    before_message_id: None,
                })
            } else {
                None
            };
            self.histories.insert(
                target_chat_id,
                CachedHistory {
                    messages,
                    next_page,
                    loaded,
                },
            );
        }
    }

    /// Sums unread counts for the dock badge. Muted chats are excluded, matching
    /// the badge behavior of Telegram's own clients. The count spans every
    /// folder so the badge does not shrink when a folder is selected.
    fn recompute_unread(&mut self) {
        self.view.unread_total = self
            .all_chats
            .iter()
            .filter(|chat| !chat.muted)
            .map(|chat| chat.unread_count.max(0) as u32)
            .sum();
    }

    fn select_chat(&mut self, chat_id: i64) {
        let chat_id = self.canonical_chat_id(chat_id);
        if !self.peer_refs.contains_key(&chat_id) {
            self.set_error(EngineError::ChatUnavailableOffline);
            return;
        }
        if self.view.selected_chat == Some(chat_id) {
            // Selecting the already-open chat means "show the newest messages",
            // which is how the panel returns from a search jump. A topic stays
            // open and refreshes its own replies.
            if self.view.selected_topic.is_some() {
                self.refresh_topic_tail(chat_id);
            } else {
                self.refresh_tail(chat_id);
            }
            return;
        }
        self.history_epoch = self.history_epoch.wrapping_add(1);
        let epoch = self.history_epoch;
        self.view.selected_chat = Some(chat_id);
        self.view.topics.clear();
        self.view.loading_topics = false;
        self.view.selected_topic = None;
        self.view.draft = self
            .drafts
            .get(&(chat_id, None))
            .cloned()
            .unwrap_or_default();
        self.view.search_results.clear();
        self.view.searching = false;
        self.search_next_offset = None;
        let loaded_history = self.histories.get_mut(&chat_id).and_then(|cached| {
            if !cached.loaded {
                return None;
            }
            // The operating system may have deleted the cache since this history
            // was stored; do not render a stale path as a cached file.
            forget_missing_cached_media(&mut cached.messages);
            Some((cached.messages.clone(), cached.next_page))
        });
        let request = if let Some((messages, next_page)) = loaded_history {
            self.view.history = messages;
            self.view.has_more_history = next_page.is_some();
            match next_page {
                Some(cursor) if cursor.is_initial_for(chat_id) => HistoryRequest::Page(cursor),
                _ => HistoryRequest::RefreshTail {
                    source_chat_id: chat_id,
                },
            }
        } else {
            self.view.history.clear();
            self.view.has_more_history = true;
            let cursor = HistoryCursor {
                source_chat_id: chat_id,
                before_message_id: None,
            };
            self.histories.entry(chat_id).or_default().next_page = Some(cursor);
            HistoryRequest::Page(cursor)
        };
        self.publish();

        self.spawn_history_page(chat_id, epoch, request);
        if self.chat_is_forum(chat_id) {
            self.load_topics(chat_id);
        }
    }

    /// Reloads the newest page of the selected chat without dropping older
    /// messages, so returning from a jump keeps the whole conversation.
    fn refresh_tail(&mut self, chat_id: i64) {
        let epoch = self.history_epoch;
        self.spawn_history_page(
            chat_id,
            epoch,
            HistoryRequest::RefreshTail {
                source_chat_id: chat_id,
            },
        );
    }

    /// Fetches the topic list for a forum supergroup. The list is only kept for
    /// the selected chat, because the panel shows one chat at a time.
    fn load_topics(&mut self, chat_id: i64) {
        let chat_id = self.canonical_chat_id(chat_id);
        if self.view.selected_chat != Some(chat_id) || !self.chat_is_forum(chat_id) {
            return;
        }
        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            return;
        };
        self.view.topics.clear();
        self.view.loading_topics = true;
        self.publish();
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let request = tl::functions::messages::GetForumTopics {
                peer: peer_ref.into(),
                q: None,
                offset_date: 0,
                offset_id: 0,
                offset_topic: 0,
                limit: 100,
            };
            let result = match client.invoke(&request).await {
                Ok(tl::enums::messages::ForumTopics::Topics(topics)) => Ok(topics
                    .topics
                    .into_iter()
                    .filter_map(|topic| {
                        let tl::enums::ForumTopic::Topic(topic) = topic else {
                            return None;
                        };
                        Some(TopicSnapshot {
                            id: topic.id,
                            title: topic.title,
                            unread_count: topic.unread_count.max(0),
                            unread_mentions: topic.unread_mentions_count.max(0),
                            closed: topic.closed,
                            pinned: topic.pinned,
                            draft: match topic.draft.as_ref() {
                                Some(tl::enums::DraftMessage::Message(draft))
                                    if !draft.message.is_empty() =>
                                {
                                    Some(draft.message.clone())
                                }
                                _ => None,
                            },
                        })
                    })
                    .collect()),
                Err(error) => {
                    log::warn!("failed to load Telegram forum topics: {error}");
                    Err(EngineError::from_invocation(&error))
                }
            };
            let event = EngineEvent::TopicsLoaded { chat_id, result };
            if event_tx.send(event).is_err() {
                log::debug!("telegram event receiver has been dropped");
            }
        });
    }

    fn apply_topics_loaded(
        &mut self,
        chat_id: i64,
        result: Result<Vec<TopicSnapshot>, EngineError>,
    ) {
        if self.view.selected_chat != Some(chat_id) {
            return;
        }
        self.view.loading_topics = false;
        match result {
            Ok(topics) => {
                for topic in &topics {
                    if let Some(draft) = topic.draft.as_ref().filter(|draft| !draft.is_empty()) {
                        // The General topic uses the chat-level draft, which is
                        // the same key the composer sends for it.
                        let key = if topic.id == GENERAL_TOPIC_ID {
                            (chat_id, None)
                        } else {
                            (chat_id, Some(topic.id))
                        };
                        self.drafts.entry(key).or_insert_with(|| draft.clone());
                        self.saved_drafts
                            .entry(key)
                            .or_insert_with(|| draft.clone());
                    }
                }
                self.view.topics = topics;
                self.view.draft = self
                    .drafts
                    .get(&(chat_id, self.view.selected_topic))
                    .cloned()
                    .unwrap_or_default();
                self.publish();
                self.maybe_mark_read();
            }
            // A topic list is supplementary; a failure must not replace the
            // conversation with an error banner, but the spinner must stop.
            Err(error) => {
                log::warn!("failed to load Telegram forum topics: {error:?}");
                self.publish();
                self.maybe_mark_read();
            }
        }
    }

    fn chat_is_forum(&self, chat_id: i64) -> bool {
        self.all_chats
            .iter()
            .any(|chat| chat.id == chat_id && chat.is_forum)
    }

    /// Switches between the General topic and one forum topic, keeping the
    /// previous history in its cache so returning is instant.
    fn select_topic(&mut self, chat_id: i64, topic_id: Option<i32>) {
        self.open_topic(chat_id, topic_id, None);
    }

    /// Opens a topic at one of its replies, so a search hit below the tail is
    /// fetched instead of waiting for the user to page up to it.
    fn select_topic_at(&mut self, chat_id: i64, topic_id: i32, message_id: i32) {
        self.open_topic(chat_id, Some(topic_id), Some(message_id));
    }

    fn open_topic(&mut self, chat_id: i64, topic_id: Option<i32>, anchor: Option<i32>) {
        let chat_id = self.canonical_chat_id(chat_id);
        if self.view.selected_chat != Some(chat_id) || self.view.selected_topic == topic_id {
            return;
        }
        self.cache_selected_history();
        self.history_epoch = self.history_epoch.wrapping_add(1);
        let epoch = self.history_epoch;
        self.view.selected_topic = topic_id;
        self.view.draft = self
            .drafts
            .get(&(chat_id, topic_id))
            .cloned()
            .unwrap_or_default();
        self.view.search_results.clear();
        self.view.searching = false;
        self.search_next_offset = None;
        match topic_id {
            None => self.restore_general_history(chat_id, epoch),
            Some(topic_id) => self.restore_topic_history(chat_id, topic_id, epoch, anchor),
        }
        self.publish();
    }

    fn restore_general_history(&mut self, chat_id: i64, epoch: u64) {
        let loaded = self.histories.get_mut(&chat_id).and_then(|cached| {
            if !cached.loaded {
                return None;
            }
            forget_missing_cached_media(&mut cached.messages);
            Some((cached.messages.clone(), cached.next_page))
        });
        if let Some((messages, next_page)) = loaded {
            self.view.history = messages;
            self.view.has_more_history = next_page.is_some();
            self.view.loading_history = false;
            let request = match next_page {
                Some(cursor) if cursor.is_initial_for(chat_id) => HistoryRequest::Page(cursor),
                _ => HistoryRequest::RefreshTail {
                    source_chat_id: chat_id,
                },
            };
            self.spawn_history_page(chat_id, epoch, request);
        } else {
            self.view.history.clear();
            self.view.has_more_history = true;
            let cursor = HistoryCursor {
                source_chat_id: chat_id,
                before_message_id: None,
            };
            self.histories.entry(chat_id).or_default().next_page = Some(cursor);
            self.spawn_history_page(chat_id, epoch, HistoryRequest::Page(cursor));
        }
    }

    fn restore_topic_history(
        &mut self,
        chat_id: i64,
        topic_id: i32,
        epoch: u64,
        anchor: Option<i32>,
    ) {
        let loaded = self
            .topic_histories
            .get_mut(&(chat_id, topic_id))
            .and_then(|cached| {
                if !cached.loaded {
                    return None;
                }
                forget_missing_cached_media(&mut cached.messages);
                Some((cached.messages.clone(), cached.next_offset))
            });
        if let Some((messages, next_offset)) = loaded {
            self.view.history = messages;
            self.view.has_more_history = next_offset.is_some();
            self.view.loading_history = false;
        } else {
            self.view.history.clear();
            self.view.has_more_history = false;
        }
        // A search jump fetches the page ending at the hit. Otherwise refresh
        // the newest page even for a cached topic, so replies that arrived while
        // another topic was open appear without reopening.
        let offset_id = anchor.map_or(0, |message_id| message_id.saturating_add(1));
        self.spawn_topic_history_page(chat_id, topic_id, epoch, offset_id);
    }

    fn refresh_topic_tail(&mut self, chat_id: i64) {
        let Some(topic_id) = self.view.selected_topic else {
            return;
        };
        let epoch = self.history_epoch;
        self.spawn_topic_history_page(chat_id, topic_id, epoch, 0);
    }

    fn spawn_topic_history_page(
        &mut self,
        chat_id: i64,
        topic_id: i32,
        epoch: u64,
        offset_id: i32,
    ) {
        let Some(client) = self.client.clone() else {
            self.view.loading_history = false;
            self.publish();
            return;
        };
        let Some(peer_ref) = self.peer_refs.get(&chat_id).copied() else {
            self.view.loading_history = false;
            self.set_error(EngineError::ChatUnavailableOffline);
            return;
        };
        self.view.loading_history = true;
        self.publish();
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let result = fetch_topic_page(&client, peer_ref, chat_id, topic_id, offset_id).await;
            let event = EngineEvent::TopicHistoryLoaded {
                chat_id,
                topic_id,
                epoch,
                offset_id,
                result,
            };
            if event_tx.send(event).is_err() {
                log::debug!("telegram event receiver has been dropped");
            }
        });
    }

    fn apply_topic_history_loaded(
        &mut self,
        chat_id: i64,
        topic_id: i32,
        epoch: u64,
        offset_id: i32,
        result: Result<(Vec<MessageSnapshot>, bool), EngineError>,
    ) {
        if epoch != self.history_epoch
            || self.view.selected_chat != Some(chat_id)
            || self.view.selected_topic != Some(topic_id)
        {
            return;
        }
        match result {
            Ok((page, has_more)) => {
                let previous_next = self
                    .topic_histories
                    .get(&(chat_id, topic_id))
                    .and_then(|history| history.next_offset);
                let previous_newest = self
                    .view
                    .history
                    .iter()
                    .filter(|message| message.id != 0)
                    .map(|message| message.id)
                    .max();
                let page_oldest = page
                    .iter()
                    .filter(|message| message.id != 0)
                    .map(|message| message.id)
                    .min();
                // A tail refresh that starts above the cached newest leaves a
                // hole. Drop the stale page and page down from the fresh tail,
                // because the reply endpoint has no gap-fill request.
                let has_gap = offset_id == 0
                    && previous_newest
                        .zip(page_oldest)
                        .is_some_and(|(previous, oldest)| oldest > previous.saturating_add(1));
                if has_gap {
                    self.view.history = page;
                } else {
                    self.view.history = merge_history(&self.view.history, page, chat_id, None);
                }
                self.view.loading_history = false;
                self.enrich_reply_previews();
                let oldest = self
                    .view
                    .history
                    .iter()
                    .find(|message| message.id != 0)
                    .map(|message| message.id);
                let next_offset = if offset_id == 0 {
                    if has_gap {
                        has_more.then_some(page_oldest).flatten()
                    } else {
                        previous_next.or_else(|| has_more.then_some(oldest).flatten())
                    }
                } else if has_more {
                    oldest
                } else {
                    None
                };
                self.view.has_more_history = next_offset.is_some();
                let entry = self.topic_histories.entry((chat_id, topic_id)).or_default();
                entry.messages = self.view.history.clone();
                entry.loaded = true;
                entry.next_offset = next_offset;
                self.publish();
                self.maybe_mark_read();
            }
            Err(error) => {
                self.view.loading_history = false;
                if offset_id == 0 {
                    self.set_error(error);
                } else {
                    log::warn!("failed to load older topic replies: {error:?}");
                    self.publish();
                }
            }
        }
    }

    fn select_chat_at(&mut self, chat_id: i64, message: MessageSnapshot) {
        let chat_id = self.canonical_chat_id(chat_id);
        self.select_chat(chat_id);
        if self.view.selected_chat != Some(chat_id) {
            return;
        }
        match message.topic_id {
            Some(topic_id) => self.select_topic_at(chat_id, topic_id, message.id),
            // A General hit must leave any open topic, or the message would be
            // inserted into another topic's history.
            None => self.select_topic(chat_id, None),
        }
        if self.view.selected_topic.is_none() {
            // A search hit usually lives outside the loaded tail. Fetch the page
            // that ends at the hit so the panel scrolls into a real conversation
            // instead of a single isolated message.
            let cursor = HistoryCursor {
                source_chat_id: message.chat_id,
                before_message_id: Some(message.id.saturating_add(1)),
            };
            let epoch = self.history_epoch;
            self.spawn_history_page(chat_id, epoch, HistoryRequest::Page(cursor));
        }
        self.upsert_history_message(message);
        self.sort_history();
        self.cache_selected_history();
        self.publish();
    }

    /// Schedules a history page fetch, returning immediately so the select
    /// loop is not blocked by a slow (or large) download.
    fn spawn_history_page(
        &mut self,
        conversation_chat_id: i64,
        epoch: u64,
        request: HistoryRequest,
    ) {
        let Some(client) = self.client.clone() else {
            self.view.loading_history = false;
            self.publish();
            return;
        };
        let source_chat_id = request.source_chat_id();
        let Some(peer_ref) = self.peer_refs.get(&source_chat_id).copied() else {
            self.view.loading_history = false;
            self.set_error(EngineError::ChatUnavailableOffline);
            return;
        };
        let before_message_id = request.before_message_id();
        self.view.loading_history = true;
        self.publish();
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let result =
                fetch_history_page(&client, peer_ref, source_chat_id, before_message_id).await;
            if event_tx
                .send(EngineEvent::HistoryLoaded {
                    conversation_chat_id,
                    epoch,
                    request,
                    result,
                })
                .is_err()
            {
                log::debug!("telegram event receiver has been dropped");
            }
        });
    }

    fn apply_history_loaded(
        &mut self,
        conversation_chat_id: i64,
        epoch: u64,
        request: HistoryRequest,
        result: Result<(Vec<MessageSnapshot>, bool), EngineError>,
    ) {
        if epoch != self.history_epoch || self.view.selected_chat != Some(conversation_chat_id) {
            return;
        }
        match result {
            Ok((page, has_more)) => {
                let source_chat_id = request.source_chat_id();
                let oldest_message_id = page
                    .iter()
                    .find(|message| message.chat_id == source_chat_id && message.id != 0)
                    .map(|message| message.id);
                let migrated_from_chat_id = self.migrated_from.get(&conversation_chat_id).copied();
                self.view.history = merge_history(
                    &self.view.history,
                    page,
                    conversation_chat_id,
                    migrated_from_chat_id,
                );
                self.enrich_reply_previews();

                let mut continue_with = None;
                if let HistoryRequest::Page(cursor) = request {
                    let next_page = next_history_page(
                        conversation_chat_id,
                        cursor,
                        has_more,
                        oldest_message_id,
                        migrated_from_chat_id,
                    );
                    if !has_more && source_chat_id == conversation_chat_id {
                        continue_with = next_page;
                    }
                    self.set_next_history_page(conversation_chat_id, next_page);
                }

                self.cache_selected_history();
                if let Some(cursor) = continue_with {
                    self.spawn_history_page(
                        conversation_chat_id,
                        epoch,
                        HistoryRequest::Page(cursor),
                    );
                    return;
                }
                self.view.loading_history = false;
                self.publish();
                self.maybe_mark_read();
            }
            Err(error) => {
                self.view.loading_history = false;
                self.set_error(error);
            }
        }
    }

    /// Loads older messages and prepends them. History is stored oldest-first,
    /// so the first non-optimistic message is the oldest one.
    fn load_older(&mut self, chat_id: i64) {
        let chat_id = self.canonical_chat_id(chat_id);
        if self.view.selected_chat != Some(chat_id)
            || !self.view.has_more_history
            || self.view.loading_history
        {
            return;
        }
        if let Some(topic_id) = self.view.selected_topic {
            let next = self
                .topic_histories
                .get(&(chat_id, topic_id))
                .and_then(|history| history.next_offset);
            if let Some(offset_id) = next {
                let epoch = self.history_epoch;
                self.spawn_topic_history_page(chat_id, topic_id, epoch, offset_id);
            } else {
                self.view.has_more_history = false;
                self.publish();
            }
            return;
        }
        let Some(cursor) = self
            .histories
            .get(&chat_id)
            .and_then(|history| history.next_page)
        else {
            self.view.has_more_history = false;
            self.publish();
            return;
        };
        let epoch = self.history_epoch;
        self.spawn_history_page(chat_id, epoch, HistoryRequest::Page(cursor));
    }

    /// Fetches the messages between two known ids for the selected chat and
    /// merges them into history, closing an out-of-order gap.
    fn load_gap(&mut self, chat_id: i64, after_id: i32, before_id: i32) {
        if self.view.selected_topic.is_some()
            || self.view.selected_chat != Some(self.canonical_chat_id(chat_id))
        {
            return;
        }
        let Some(peer_ref) = self.peer_refs.get(&chat_id).copied() else {
            return;
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        let epoch = self.history_epoch;
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let messages = fetch_gap(&client, peer_ref, chat_id, after_id, before_id).await;
            if event_tx
                .send(EngineEvent::GapLoaded {
                    chat_id,
                    epoch,
                    messages,
                })
                .is_err()
            {
                log::debug!("telegram event receiver has been dropped");
            }
        });
    }

    fn apply_gap_loaded(&mut self, chat_id: i64, epoch: u64, messages: Vec<MessageSnapshot>) {
        if epoch != self.history_epoch
            || self.view.selected_topic.is_some()
            || self.view.selected_chat != Some(self.canonical_chat_id(chat_id))
        {
            return;
        }
        for snapshot in messages {
            self.upsert_history_message(snapshot);
        }
        self.sort_history();
        self.enrich_reply_previews();
        self.cache_selected_history();
        self.publish();
    }

    /// Stores the displayed history in the per-chat or per-topic cache.
    fn cache_selected_history(&mut self) {
        let Some(chat_id) = self.view.selected_chat else {
            return;
        };
        if let Some(topic_id) = self.view.selected_topic {
            let entry = self.topic_histories.entry((chat_id, topic_id)).or_default();
            entry.messages = self.view.history.clone();
            entry.loaded = true;
            return;
        }
        let entry = self.histories.entry(chat_id).or_default();
        entry.messages = self.view.history.clone();
        entry.loaded = true;
        self.view.has_more_history = entry.next_page.is_some();
    }

    fn set_next_history_page(&mut self, chat_id: i64, next_page: Option<HistoryCursor>) {
        self.histories.entry(chat_id).or_default().next_page = next_page;
        if self.view.selected_chat == Some(chat_id) {
            self.view.has_more_history = next_page.is_some();
        }
    }

    /// Fills reply previews from the loaded history. `message_to_snapshot` only
    /// knows the reply target's id; the target itself is usually loaded a moment
    /// later, so resolve the name and text once it is available.
    fn enrich_reply_previews(&mut self) {
        let mut lookup: HashMap<i32, (i64, String, String, Option<MediaKind>)> = HashMap::new();
        for message in &self.view.history {
            if message.id == 0 {
                continue;
            }
            lookup.entry(message.id).or_insert_with(|| {
                (
                    message.chat_id,
                    message.sender_name.clone(),
                    message.preview_text(),
                    message.media.as_ref().map(|media| media.kind),
                )
            });
        }
        for message in &mut self.view.history {
            let Some(reply) = message.reply_to.as_mut() else {
                continue;
            };
            if !reply.text.is_empty() || !reply.sender_name.is_empty() {
                continue;
            }
            let Some((_, sender_name, text, media)) = lookup.get(&reply.message_id) else {
                continue;
            };
            reply.sender_name = sender_name.clone();
            reply.text = text.clone();
            reply.media = *media;
        }
    }

    /// Sorts history oldest-first, keeping optimistic (id `0`) sends at the end.
    fn sort_history(&mut self) {
        let Some(conversation_chat_id) = self.view.selected_chat else {
            return;
        };
        let migrated_from_chat_id = self.migrated_from.get(&conversation_chat_id).copied();
        sort_history_messages(
            &mut self.view.history,
            conversation_chat_id,
            migrated_from_chat_id,
        );
    }

    fn schedule_gap_fill(&mut self, chat_id: i64, after_id: i32, before_id: i32) {
        self.send_event(EngineEvent::FillGap {
            chat_id,
            after_id,
            before_id,
        });
    }

    /// Reloads the chat list once, shortly after the first new-chat update,
    /// instead of refetching on every message.
    fn schedule_chats_refresh(&mut self) {
        if self.chats_refresh_scheduled {
            return;
        }
        self.chats_refresh_scheduled = true;
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if event_tx.send(EngineEvent::RefreshChats).is_err() {
                log::debug!("telegram event receiver has been dropped");
            }
        });
    }

    /// Schedules a reconnection attempt with exponential backoff. A first
    /// connection failure also schedules a retry, even though the account is
    /// not signed in yet.
    fn schedule_reconnect(&mut self) {
        if self.reconnect_scheduled || !self.view.configured {
            return;
        }
        self.reconnect_scheduled = true;
        let exponent = self.reconnect_attempts.min(5);
        let delay = Duration::from_secs(1u64 << exponent).min(Duration::from_secs(30));
        self.reconnect_attempts = self.reconnect_attempts.saturating_add(1);
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            if event_tx.send(EngineEvent::Reconnect).is_err() {
                log::debug!("telegram event receiver has been dropped");
            }
        });
    }

    async fn reconnect(&mut self) {
        self.reconnect_scheduled = false;
        if self.client.is_some() || !self.view.configured {
            return;
        }
        self.ensure_connected().await;
    }

    /// Stages every outgoing message optimistically, then hands the network
    /// work to a background task. The select loop stays responsive while a
    /// large upload runs, and a dropped connection leaves a retryable message.
    fn send_outgoing(&mut self, chat_id: i64, items: Vec<OutgoingItem>, reply_to: Option<i32>) {
        if items.is_empty() {
            return;
        }
        let chat_id = self.canonical_chat_id(chat_id);
        if self.sending_context(chat_id).is_none() {
            return;
        }
        self.view.draft.clear();
        let topic_id = self.view.selected_topic;
        let cleared_draft = self.drafts.remove(&(chat_id, topic_id));
        let remaining_draft = self.chat_draft(chat_id);
        if let Some(chat) = self.all_chats.iter_mut().find(|chat| chat.id == chat_id) {
            chat.draft = remaining_draft;
        }
        if cleared_draft.is_some() {
            self.schedule_draft_sync(chat_id, topic_id, "");
        }

        let mut local_ids = Vec::with_capacity(items.len());
        for (index, item) in items.into_iter().enumerate() {
            // A reply points at the first message of the batch, matching what
            // Telegram's own clients do for multi-message sends.
            let reply_to = if index == 0 { reply_to } else { None };
            let local_id = rand::random::<u64>();
            self.push_optimistic(chat_id, local_id, &item, reply_to);
            self.pending_outgoing.insert(
                local_id,
                PendingOutgoing {
                    chat_id,
                    item,
                    reply_to,
                    topic_id,
                },
            );
            local_ids.push(local_id);
        }
        self.publish();
        self.spawn_send_batch(chat_id, &local_ids);
    }

    /// Sends the pending items for `local_ids` in order on a background task,
    /// reporting each result back through the event channel.
    fn spawn_send_batch(&mut self, chat_id: i64, local_ids: &[u64]) {
        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            for local_id in local_ids {
                self.fail_send(*local_id, EngineError::NotConnected);
            }
            self.publish();
            return;
        };
        let batch: Vec<(u64, OutgoingItem, Option<i32>, Option<i32>)> = local_ids
            .iter()
            .filter_map(|local_id| {
                self.pending_outgoing.get(local_id).map(|pending| {
                    (
                        *local_id,
                        pending.item.clone(),
                        pending.reply_to,
                        pending.topic_id,
                    )
                })
            })
            .collect();
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            for (local_id, item, reply_to, topic_id) in batch {
                let result =
                    perform_send(&client, peer_ref, chat_id, item, reply_to, topic_id).await;
                let event = EngineEvent::SendFinished {
                    chat_id,
                    local_id,
                    result,
                };
                if event_tx.send(event).is_err() {
                    log::debug!("telegram event receiver has been dropped");
                    break;
                }
            }
        });
    }

    fn apply_send_finished(
        &mut self,
        chat_id: i64,
        local_id: u64,
        result: Result<Vec<MessageSnapshot>, EngineError>,
    ) {
        match result {
            Ok(messages) => {
                let pending = self.pending_outgoing.remove(&local_id);
                let preview_source = messages
                    .last()
                    .map(|message| (snapshot_preview(message), message.timestamp_unix));
                for snapshot in &messages {
                    self.remember_sender(snapshot);
                }
                let conversation_chat_id = self.canonical_chat_id(chat_id);
                let migrated_from_chat_id = self.migrated_from.get(&conversation_chat_id).copied();
                let pending_topic = pending.as_ref().and_then(|pending| pending.topic_id);
                if self.view.selected_chat == Some(conversation_chat_id)
                    && self.view.selected_topic == pending_topic
                {
                    replace_sent_messages(
                        &mut self.view.history,
                        local_id,
                        messages,
                        conversation_chat_id,
                        migrated_from_chat_id,
                    );
                    self.enrich_reply_previews();
                    self.cache_selected_history();
                } else if let Some(topic_id) = pending_topic {
                    if let Some(entry) = self
                        .topic_histories
                        .get_mut(&(conversation_chat_id, topic_id))
                    {
                        replace_sent_messages(
                            &mut entry.messages,
                            local_id,
                            messages,
                            conversation_chat_id,
                            migrated_from_chat_id,
                        );
                    }
                } else {
                    let entry = self.histories.entry(conversation_chat_id).or_default();
                    replace_sent_messages(
                        &mut entry.messages,
                        local_id,
                        messages,
                        conversation_chat_id,
                        migrated_from_chat_id,
                    );
                }
                if let Some((preview, timestamp_unix)) = preview_source {
                    self.update_chat_preview(chat_id, preview, timestamp_unix);
                }
                if let Some(chat) = self.all_chats.iter_mut().find(|chat| chat.id == chat_id) {
                    chat.slow_mode_seconds = 0;
                }
                self.persist_session();
                self.publish();
            }
            Err(error) => {
                self.fail_send(local_id, error);
                self.publish();
            }
        }
    }

    /// Marks one optimistic message as failed and keeps its planned item so the
    /// panel can retry without re-staging the attachment.
    fn fail_send(&mut self, local_id: u64, error: EngineError) {
        let Some(pending) = self.pending_outgoing.get(&local_id).cloned() else {
            self.set_error(error);
            return;
        };
        let chat_id = pending.chat_id;
        if self.view.selected_chat == Some(chat_id) && self.view.selected_topic == pending.topic_id
        {
            if let Some(index) = self.message_index(local_id) {
                self.view.history[index].send_state = SendState::Failed;
            }
            self.cache_selected_history();
        } else if let Some(topic_id) = pending.topic_id {
            if let Some(entry) = self.topic_histories.get_mut(&(chat_id, topic_id))
                && let Some(message) = entry
                    .messages
                    .iter_mut()
                    .find(|message| message.local_id == Some(local_id))
            {
                message.send_state = SendState::Failed;
            }
        } else if let Some(entry) = self.histories.get_mut(&chat_id) {
            if let Some(message) = entry
                .messages
                .iter_mut()
                .find(|message| message.local_id == Some(local_id))
            {
                message.send_state = SendState::Failed;
            }
        }
        if let EngineError::SlowMode { seconds } = &error {
            let seconds = (*seconds).min(u64::from(u32::MAX)) as u32;
            if let Some(chat) = self.all_chats.iter_mut().find(|chat| chat.id == chat_id) {
                chat.slow_mode_seconds = seconds;
            }
            self.refresh_visible_chats();
        }
        self.set_error(error);
    }

    fn retry_send(&mut self, chat_id: i64, local_id: u64) {
        let chat_id = self.canonical_chat_id(chat_id);
        let Some(pending) = self.pending_outgoing.get(&local_id).cloned() else {
            return;
        };
        if pending.chat_id != chat_id {
            return;
        }
        if self.view.selected_chat == Some(chat_id) && self.view.selected_topic == pending.topic_id
        {
            if let Some(index) = self.message_index(local_id) {
                self.view.history[index].send_state = SendState::Sending;
            }
            self.cache_selected_history();
        } else if let Some(topic_id) = pending.topic_id {
            if let Some(entry) = self.topic_histories.get_mut(&(chat_id, topic_id))
                && let Some(message) = entry
                    .messages
                    .iter_mut()
                    .find(|message| message.local_id == Some(local_id))
            {
                message.send_state = SendState::Sending;
            }
        } else if let Some(entry) = self.histories.get_mut(&chat_id)
            && let Some(message) = entry
                .messages
                .iter_mut()
                .find(|message| message.local_id == Some(local_id))
        {
            message.send_state = SendState::Sending;
        }
        self.publish();
        self.spawn_send_batch(chat_id, &[local_id]);
    }

    /// Pushes an optimistic history entry for a message being sent.
    fn push_optimistic(
        &mut self,
        chat_id: i64,
        local_id: u64,
        item: &OutgoingItem,
        reply_to: Option<i32>,
    ) {
        let (text, markdown, media) = match item {
            OutgoingItem::Text(text) => {
                let text = text.trim_end().to_owned();
                (text.clone(), Some(text), None)
            }
            OutgoingItem::Media {
                attachment,
                caption,
            } => (
                caption.clone().unwrap_or_default(),
                caption.clone(),
                Some(optimistic_media(attachment)),
            ),
            OutgoingItem::Album { items } => match items.first() {
                Some(first) => (
                    first.caption.clone().unwrap_or_default(),
                    first.caption.clone(),
                    Some(optimistic_media(&first.attachment)),
                ),
                None => (String::new(), None, None),
            },
        };
        let reply_to = reply_to.and_then(|message_id| self.reply_snapshot(chat_id, message_id));
        let optimistic = MessageSnapshot {
            id: 0,
            chat_id,
            outgoing: true,
            sender_name: String::new(),
            sender_id: None,
            timestamp_unix: current_unix_timestamp(),
            text,
            markdown,
            media,
            reply_to,
            forwarded_from: None,
            grouped_id: None,
            topic_id: self.view.selected_topic,
            thread_root_id: None,
            views: None,
            action: None,
            reactions: Vec::new(),
            send_state: SendState::Sending,
            local_id: Some(local_id),
            edited: false,
        };
        if self.view.selected_chat == Some(chat_id) {
            self.view.history.push(optimistic);
        } else {
            self.histories
                .entry(chat_id)
                .or_default()
                .messages
                .push(optimistic);
        }
    }

    /// Builds the reply preview for a message in the loaded history.
    fn reply_snapshot(&self, chat_id: i64, message_id: i32) -> Option<Box<ReplySnapshot>> {
        let history: &[MessageSnapshot] = if self.view.selected_chat == Some(chat_id) {
            &self.view.history
        } else {
            self.histories
                .get(&chat_id)
                .map_or(&[], |entry| entry.messages.as_slice())
        };
        let message = history.iter().find(|message| message.id == message_id)?;
        Some(Box::new(ReplySnapshot {
            message_id,
            sender_name: message.sender_name.clone(),
            text: message.preview_text(),
            media: message.media.as_ref().map(|media| media.kind),
        }))
    }

    /// Edits the account's own text message and mirrors the change locally.
    async fn edit_message(&mut self, chat_id: i64, message_id: i32, text: String) {
        let text = text.trim_end().to_owned();
        if text.is_empty() {
            self.set_error(EngineError::EditFailed);
            return;
        }
        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            self.set_error(EngineError::NotConnected);
            return;
        };
        match client
            .edit_message(peer_ref, message_id, markdown_message(&text))
            .await
        {
            Ok(()) => {
                if let Some(snapshot) = self
                    .view
                    .history
                    .iter_mut()
                    .find(|message| message.chat_id == chat_id && message.id == message_id)
                {
                    snapshot.text = text.clone();
                    snapshot.markdown = Some(text.clone());
                    snapshot.edited = true;
                }
                // Editing the newest message changes the chat list preview too.
                let newest = self
                    .view
                    .history
                    .iter()
                    .filter(|message| message.chat_id == chat_id && message.id != 0)
                    .max_by_key(|message| message.id)
                    .map_or_default(snapshot_preview);
                let timestamp = self
                    .view
                    .history
                    .iter()
                    .find(|message| message.chat_id == chat_id && message.id == message_id)
                    .map(|message| message.timestamp_unix);
                if let Some(timestamp) = timestamp {
                    self.update_chat_preview(chat_id, newest, timestamp);
                }
                self.cache_selected_history();
                self.publish();
            }
            Err(error) => {
                log::warn!("failed to edit a Telegram message: {error}");
                self.set_error(EngineError::from_invocation_with(
                    &error,
                    EngineError::EditFailed,
                ));
            }
        }
    }

    fn message_index(&self, local_id: u64) -> Option<usize> {
        self.view
            .history
            .iter()
            .position(|message| message.local_id == Some(local_id))
    }

    /// Inserts a server message, replacing the same message from the same peer.
    /// Message ids from the two sides of a group migration may overlap.
    fn upsert_history_message(&mut self, snapshot: MessageSnapshot) {
        if let Some(existing) = self
            .view
            .history
            .iter_mut()
            .find(|message| message.chat_id == snapshot.chat_id && message.id == snapshot.id)
        {
            *existing = snapshot;
        } else {
            self.view.history.push(snapshot);
        }
    }

    fn update_chat_preview(
        &mut self,
        chat_id: i64,
        preview: (String, Option<MediaKind>),
        timestamp_unix: i64,
    ) {
        let chat_id = self.canonical_chat_id(chat_id);
        if let Some(chat) = self.all_chats.iter_mut().find(|chat| chat.id == chat_id) {
            chat.preview = preview.0;
            chat.preview_media = preview.1;
            chat.timestamp_unix = timestamp_unix;
        }
        self.refresh_visible_chats();
    }

    /// Records a sender name so a typing indicator, whose raw update only
    /// carries a user id, can show something better than a number.
    fn remember_sender(&mut self, message: &MessageSnapshot) {
        if let Some(sender_id) = message.sender_id
            && !message.sender_name.is_empty()
        {
            self.known_names
                .insert(sender_id, message.sender_name.clone());
        }
    }

    fn apply_remote_typing(
        &mut self,
        chat_id: i64,
        user_id: Option<i64>,
        action: &tl::enums::SendMessageAction,
    ) {
        let chat_id = self.canonical_chat_id(chat_id);
        let typing = !matches!(
            action,
            tl::enums::SendMessageAction::SendMessageCancelAction
        );
        let user_name = user_id
            .and_then(|user_id| self.known_names.get(&user_id).cloned())
            .unwrap_or_else(|| self.chat_title(chat_id));
        let until_unix =
            current_unix_timestamp() + i64::try_from(TYPING_TTL.as_secs()).unwrap_or_default();
        let next = typing.then_some(TypingSnapshot {
            user_name,
            until_unix,
        });
        let changed = match self.all_chats.iter_mut().find(|chat| chat.id == chat_id) {
            Some(chat) => {
                let changed = chat.typing != next;
                chat.typing = next;
                changed
            }
            None => false,
        };
        if !changed {
            return;
        }
        self.refresh_visible_chats();
        self.publish();
        if typing {
            self.schedule_typing_expiry(chat_id, until_unix);
        }
    }

    fn schedule_typing_expiry(&self, chat_id: i64, until_unix: i64) {
        let event_tx = self.event_tx.clone();
        let delay = TYPING_TTL + Duration::from_secs(1);
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            if event_tx
                .send(EngineEvent::TypingExpired {
                    chat_id,
                    until_unix,
                })
                .is_err()
            {
                log::debug!("telegram event receiver has been dropped");
            }
        });
    }

    fn apply_typing_expired(&mut self, chat_id: i64, until_unix: i64) {
        let chat_id = self.canonical_chat_id(chat_id);
        let changed = match self.all_chats.iter_mut().find(|chat| chat.id == chat_id) {
            Some(chat)
                if chat
                    .typing
                    .as_ref()
                    .is_some_and(|typing| typing.until_unix <= until_unix) =>
            {
                chat.typing = None;
                true
            }
            _ => false,
        };
        if changed {
            self.refresh_visible_chats();
            self.publish();
        }
    }

    /// Updates read state synchronously and reports it to the server on a
    /// background task, so incoming updates are never blocked by a read call.
    fn mark_read(&mut self, chat_id: i64, max_message_id: Option<i32>) {
        let chat_id = self.canonical_chat_id(chat_id);
        let peer_refs = self.conversation_sources(chat_id);
        let newest = max_message_id.or_else(|| {
            self.view
                .history
                .iter()
                .filter(|message| message.chat_id == chat_id && message.id != 0)
                .map(|message| message.id)
                .max()
        });
        // Without a loaded message there is nothing to report as read; the
        // history load calls back once it has a real position.
        let Some(newest) = newest else {
            return;
        };
        let changed = match self.all_chats.iter_mut().find(|chat| chat.id == chat_id) {
            Some(chat) => {
                let changed = chat.unread_count != 0 || chat.unread_mentions != 0;
                chat.unread_count = 0;
                chat.unread_mentions = 0;
                chat.read_inbox_message_id = chat.read_inbox_message_id.max(newest);
                changed
            }
            None => false,
        };
        if changed {
            self.refresh_visible_chats();
            self.recompute_unread();
            self.publish();
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        let max_id = newest;
        tokio::spawn(async move {
            for peer_ref in peer_refs {
                let result = if peer_ref.id.kind() == PeerKind::Channel {
                    client
                        .invoke(&tl::functions::channels::ReadHistory {
                            channel: peer_ref.into(),
                            max_id,
                        })
                        .await
                        .map(drop)
                } else {
                    client
                        .invoke(&tl::functions::messages::ReadHistory {
                            peer: peer_ref.into(),
                            max_id,
                        })
                        .await
                        .map(drop)
                };
                if let Err(error) = result {
                    log::debug!("failed to mark a Telegram chat as read: {error}");
                }
            }
        });
    }

    async fn mark_unread(&mut self, chat_id: i64) {
        let chat_id = self.canonical_chat_id(chat_id);
        if let Some(chat) = self.all_chats.iter_mut().find(|chat| chat.id == chat_id) {
            // Telegram models "mark unread" as a flag; one unread message is the
            // closest visible equivalent in this model.
            chat.unread_count = chat.unread_count.max(1);
            self.refresh_visible_chats();
            self.recompute_unread();
            self.publish();
        }
        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            return;
        };
        let request = tl::functions::messages::MarkDialogUnread {
            unread: true,
            parent_peer: None,
            peer: tl::enums::InputDialogPeer::Peer(tl::types::InputDialogPeer {
                peer: peer_ref.into(),
            }),
        };
        if let Err(error) = client.invoke(&request).await {
            log::warn!("failed to mark a Telegram chat as unread: {error}");
            self.set_error(EngineError::MarkUnreadFailed);
        }
    }

    /// The panel is only "viewing" a chat while it is focused and scrolled to
    /// the bottom; anything else must keep the unread badge alive.
    fn is_actively_viewing(&self, chat_id: i64) -> bool {
        let chat_id = self.canonical_chat_id(chat_id);
        self.view.selected_chat == Some(chat_id)
            && self.viewing.chat_id == Some(chat_id)
            && self.viewing.focused
            && self.viewing.at_bottom
    }

    fn set_viewing(&mut self, chat_id: Option<i64>, focused: bool, at_bottom: bool) {
        let chat_id = chat_id.map(|chat_id| self.canonical_chat_id(chat_id));
        self.viewing = ViewingState {
            chat_id,
            focused,
            at_bottom,
        };
        self.maybe_mark_read();
    }

    fn maybe_mark_read(&mut self) {
        let ViewingState {
            chat_id: Some(chat_id),
            focused: true,
            at_bottom: true,
        } = self.viewing
        else {
            return;
        };
        let chat_id = self.canonical_chat_id(chat_id);
        if self.view.selected_chat != Some(chat_id) {
            return;
        }
        if let Some(topic_id) = self.view.selected_topic {
            let unread = self
                .view
                .topics
                .iter()
                .find(|topic| topic.id == topic_id)
                .map_or_else(
                    || self.chat_has_unread(chat_id),
                    |topic| topic.unread_count > 0 || topic.unread_mentions > 0,
                );
            if unread {
                self.mark_topic_read(chat_id, topic_id, true);
            }
            return;
        }
        // The General topic of a forum has its own read position. Clearing the
        // chat-wide counter would also hide unread replies in other topics.
        if self.chat_is_forum(chat_id) {
            // Wait for the topic list so the General share can be separated
            // from the other topics.
            if self.view.loading_topics {
                return;
            }
            if self.view.topics.is_empty() {
                // A forum without listed topics, or a failed topic request;
                // the chat-wide position is the only available information.
                if self.chat_has_unread(chat_id) {
                    self.mark_read(chat_id, None);
                }
                return;
            }
            if let Some(general) = self
                .view
                .topics
                .iter()
                .find(|topic| topic.id == GENERAL_TOPIC_ID)
            {
                if general.unread_count > 0 || general.unread_mentions > 0 {
                    self.mark_topic_read(chat_id, GENERAL_TOPIC_ID, false);
                }
                return;
            }
            // `messages.getForumTopics` may omit the General topic; the
            // remainder after the listed topics is its unread share.
            let other_unread: i32 = self
                .view
                .topics
                .iter()
                .map(|topic| topic.unread_count.max(0))
                .sum();
            let other_mentions: i32 = self
                .view
                .topics
                .iter()
                .map(|topic| topic.unread_mentions.max(0))
                .sum();
            let general_unread = self
                .all_chats
                .iter()
                .find(|chat| chat.id == chat_id)
                .is_some_and(|chat| {
                    chat.unread_count > other_unread || chat.unread_mentions > other_mentions
                });
            if general_unread {
                self.mark_general_topic_read(chat_id, other_unread, other_mentions);
            }
            return;
        }
        if self.chat_has_unread(chat_id) {
            self.mark_read(chat_id, None);
        }
    }

    /// Clears the General topic's share of a forum's unread counters while
    /// keeping the listed topics' badges intact.
    fn mark_general_topic_read(&mut self, chat_id: i64, other_unread: i32, other_mentions: i32) {
        let chat_id = self.canonical_chat_id(chat_id);
        let Some(newest) = self
            .view
            .history
            .iter()
            .filter(|message| message.chat_id == chat_id && message.id != 0)
            .map(|message| message.id)
            .max()
        else {
            return;
        };
        let changed = match self.all_chats.iter_mut().find(|chat| chat.id == chat_id) {
            Some(chat) => {
                let general_unread = chat.unread_count.saturating_sub(other_unread);
                let general_mentions = chat.unread_mentions.saturating_sub(other_mentions);
                chat.unread_count = chat.unread_count.saturating_sub(general_unread);
                chat.unread_mentions = chat.unread_mentions.saturating_sub(general_mentions);
                chat.read_inbox_message_id = chat.read_inbox_message_id.max(newest);
                general_unread > 0 || general_mentions > 0
            }
            None => false,
        };
        if changed {
            self.refresh_visible_chats();
            self.recompute_unread();
        }
        self.publish();
        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            return;
        };
        tokio::spawn(async move {
            let result = client
                .invoke(&tl::functions::channels::ReadHistory {
                    channel: peer_ref.into(),
                    max_id: newest,
                })
                .await
                .map(drop);
            if let Err(error) = result {
                log::debug!("failed to mark a Telegram topic as read: {error}");
            }
        });
    }

    fn chat_has_unread(&self, chat_id: i64) -> bool {
        self.all_chats
            .iter()
            .find(|chat| chat.id == chat_id)
            .is_some_and(|chat| chat.unread_count > 0 || chat.unread_mentions > 0)
    }

    /// Marks one forum topic as read. The chat-wide counters lose only this
    /// topic's share, so other topics keep their badges. `discussion` selects
    /// `messages.readDiscussion`; the General topic uses the channel read call
    /// instead.
    fn mark_topic_read(&mut self, chat_id: i64, topic_id: i32, discussion: bool) {
        let chat_id = self.canonical_chat_id(chat_id);
        let Some(newest) = self
            .view
            .history
            .iter()
            .filter(|message| message.chat_id == chat_id && message.id != 0)
            .map(|message| message.id)
            .max()
        else {
            return;
        };
        let topic_unread = self
            .view
            .topics
            .iter_mut()
            .find(|topic| topic.id == topic_id)
            .map(|topic| {
                let unread = (topic.unread_count, topic.unread_mentions);
                topic.unread_count = 0;
                topic.unread_mentions = 0;
                unread
            });
        let changed = match self.all_chats.iter_mut().find(|chat| chat.id == chat_id) {
            Some(chat) => {
                let (unread, mentions) =
                    topic_unread.unwrap_or((chat.unread_count, chat.unread_mentions));
                let changed = unread > 0 || mentions > 0;
                chat.unread_count = chat.unread_count.saturating_sub(unread);
                chat.unread_mentions = chat.unread_mentions.saturating_sub(mentions);
                chat.read_inbox_message_id = chat.read_inbox_message_id.max(newest);
                changed
            }
            None => false,
        };
        if changed {
            self.refresh_visible_chats();
            self.recompute_unread();
        }
        self.publish();
        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            return;
        };
        tokio::spawn(async move {
            let result = if discussion {
                client
                    .invoke(&tl::functions::messages::ReadDiscussion {
                        peer: peer_ref.into(),
                        msg_id: topic_id,
                        read_max_id: newest,
                    })
                    .await
                    .map(drop)
            } else {
                client
                    .invoke(&tl::functions::channels::ReadHistory {
                        channel: peer_ref.into(),
                        max_id: newest,
                    })
                    .await
                    .map(drop)
            };
            if let Err(error) = result {
                log::debug!("failed to mark a Telegram topic as read: {error}");
            }
        });
    }

    fn set_typing(&mut self, chat_id: i64, typing: bool) {
        let chat_id = self.canonical_chat_id(chat_id);
        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            return;
        };
        if typing {
            let now = Instant::now();
            if let Some(last) = self.typing_sent_at.get(&chat_id)
                && now.duration_since(*last) < TYPING_REFRESH_INTERVAL
            {
                return;
            }
            self.typing_sent_at.insert(chat_id, now);
        } else if self.typing_sent_at.remove(&chat_id).is_none() {
            return;
        }
        tokio::spawn(async move {
            let action = if typing {
                tl::enums::SendMessageAction::SendMessageTypingAction
            } else {
                tl::enums::SendMessageAction::SendMessageCancelAction
            };
            if let Err(error) = client.action(peer_ref).oneshot(action).await {
                log::debug!("failed to send a Telegram chat action: {error}");
            }
        });
    }

    fn conversation_sources(&self, chat_id: i64) -> Vec<PeerRef> {
        let chat_id = self.canonical_chat_id(chat_id);
        let mut sources = Vec::with_capacity(2);
        if let Some(peer_ref) = self.peer_refs.get(&chat_id).copied() {
            sources.push(peer_ref);
        }
        if let Some(source_chat_id) = self.migrated_from.get(&chat_id).copied()
            && let Some(peer_ref) = self.peer_refs.get(&source_chat_id).copied()
        {
            sources.push(peer_ref);
        }
        sources
    }

    fn search(&mut self, query: String, chat_id: Option<i64>, filter: SearchFilter) {
        let query = query.trim().to_owned();
        self.search_epoch = self.search_epoch.wrapping_add(1);
        let epoch = self.search_epoch;
        let chat_id = chat_id.map(|chat_id| self.canonical_chat_id(chat_id));
        self.view.searching = true;
        self.view.search_query = query.clone();
        self.view.search_filter = filter;
        self.view.search_scope = chat_id;
        self.view.search_results.clear();
        self.view.has_more_search = false;
        self.search_next_offset = None;
        self.publish();

        if query.is_empty() {
            self.view.searching = false;
            self.publish();
            return;
        }

        let lowercase = query.to_lowercase();
        let results: Vec<SearchHit> = self
            .all_chats
            .iter()
            .filter(|chat| chat.title.to_lowercase().contains(&lowercase))
            .map(|chat| SearchHit {
                chat_id: chat.id,
                chat_title: chat.title.clone(),
                message: None,
            })
            .collect();
        self.view.search_results = results;
        self.publish();

        if query.chars().count() < SEARCH_MIN_QUERY_CHARS {
            self.view.searching = false;
            self.publish();
            return;
        }

        let request = SearchRequest {
            query,
            chat_id,
            filter,
            offset_id: None,
        };
        self.spawn_search_page(epoch, request, false);
    }

    fn search_more(&mut self) {
        let Some(offset_id) = self.search_next_offset else {
            return;
        };
        if self.view.searching {
            return;
        }
        let request = SearchRequest {
            query: self.view.search_query.clone(),
            chat_id: self.view.search_scope,
            filter: self.view.search_filter,
            offset_id: Some(offset_id),
        };
        self.view.searching = true;
        self.publish();
        let epoch = self.search_epoch;
        self.spawn_search_page(epoch, request, true);
    }

    fn spawn_search_page(&mut self, epoch: u64, request: SearchRequest, append: bool) {
        let Some(client) = self.client.clone() else {
            self.view.searching = false;
            self.publish();
            return;
        };
        let peer_ref = request
            .chat_id
            .and_then(|chat_id| self.peer_refs.get(&chat_id).copied());
        let titles: HashMap<i64, String> = self
            .all_chats
            .iter()
            .map(|chat| (chat.id, chat.title.clone()))
            .collect();
        let migrated_to = self.migrated_to.clone();
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let result = fetch_search(&client, peer_ref, &request, &titles, &migrated_to).await;
            let account = |result: &Result<Vec<SearchHit>, EngineError>| {
                result
                    .as_ref()
                    .ok()
                    .and_then(|hits| search_next_offset(hits))
            };
            let next_offset = account(&result);
            if event_tx
                .send(EngineEvent::SearchLoaded {
                    epoch,
                    append,
                    next_offset,
                    result,
                })
                .is_err()
            {
                log::debug!("telegram event receiver has been dropped");
            }
        });
    }

    fn apply_search_loaded(
        &mut self,
        epoch: u64,
        _append: bool,
        next_offset: Option<i32>,
        result: Result<Vec<SearchHit>, EngineError>,
    ) {
        if epoch != self.search_epoch {
            return;
        }
        self.view.searching = false;
        match result {
            Ok(hits) => {
                // The first page already contains the locally filtered chat
                // titles, so both the initial and the appended page extend.
                self.view.search_results.extend(hits);
                self.search_next_offset = next_offset;
                self.view.has_more_search = next_offset.is_some();
                self.publish();
            }
            Err(error) => self.set_error(error),
        }
    }

    /// Schedules a media or thumbnail download, reusing an existing cache file.
    fn download(&mut self, chat_id: i64, message_id: i32, target: DownloadTarget) {
        let Some(index) = self.current_message_index(chat_id, message_id) else {
            return;
        };
        let Some(media) = self.view.history[index].media.as_ref() else {
            return;
        };
        if !target.is_eligible(media) {
            return;
        }
        let path = match target {
            DownloadTarget::Original => self.media_path(chat_id, message_id, media),
            DownloadTarget::Thumbnail => self.thumbnail_path(chat_id, message_id),
        };
        if path.is_file() {
            if let Some(media) = self.view.history[index].media.as_mut() {
                set_download_state(media, target, DownloadState::Downloaded);
                set_download_path(media, target, path);
            }
            self.cache_selected_history();
            self.publish();
            return;
        }

        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            return;
        };
        if let Some(media) = self.view.history[index].media.as_mut() {
            set_download_state(media, target, DownloadState::Downloading);
        }
        self.publish();

        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let result = match target {
                DownloadTarget::Original => {
                    download_media_file(&client, peer_ref, message_id, path).await
                }
                DownloadTarget::Thumbnail => {
                    download_thumbnail_file(&client, peer_ref, message_id, path).await
                }
            };
            let event = match target {
                DownloadTarget::Original => EngineEvent::MediaLoaded {
                    chat_id,
                    message_id,
                    result,
                },
                DownloadTarget::Thumbnail => EngineEvent::ThumbnailLoaded {
                    chat_id,
                    message_id,
                    result,
                },
            };
            if event_tx.send(event).is_err() {
                log::debug!("telegram event receiver has been dropped");
            }
        });
    }

    fn apply_media_loaded(
        &mut self,
        chat_id: i64,
        message_id: i32,
        result: Result<Option<PathBuf>, EngineError>,
    ) {
        match result {
            Ok(Some(path)) => {
                self.update_media_snapshot(chat_id, message_id, |media| {
                    media.download_state = DownloadState::Downloaded;
                    media.downloaded_path = Some(path.clone());
                });
                self.publish();
            }
            Ok(None) => {
                self.update_media_snapshot(chat_id, message_id, |media| {
                    media.download_state = DownloadState::NotDownloaded;
                });
                self.publish();
            }
            Err(error) => {
                self.update_media_snapshot(chat_id, message_id, |media| {
                    media.download_state = DownloadState::Failed;
                });
                self.set_error(error);
            }
        }
    }

    fn media_path(&self, chat_id: i64, message_id: i32, media: &MediaSnapshot) -> PathBuf {
        media_file_path(&self.media_dir, chat_id, message_id, media)
    }

    fn thumbnail_path(&self, chat_id: i64, message_id: i32) -> PathBuf {
        thumbnail_file_path(&self.thumbnail_dir, chat_id, message_id)
    }

    fn apply_thumbnail_loaded(
        &mut self,
        chat_id: i64,
        message_id: i32,
        result: Result<Option<PathBuf>, EngineError>,
    ) {
        match result {
            Ok(Some(path)) => {
                self.update_media_snapshot(chat_id, message_id, |media| {
                    media.thumbnail_state = DownloadState::Downloaded;
                    media.thumbnail_path = Some(path.clone());
                });
            }
            Ok(None) => {
                self.update_media_snapshot(chat_id, message_id, |media| {
                    media.thumbnail_state = DownloadState::NotDownloaded;
                });
            }
            Err(error) => {
                self.update_media_snapshot(chat_id, message_id, |media| {
                    media.thumbnail_state = DownloadState::Failed;
                });
                log::debug!(
                    "failed to download a Telegram thumbnail for {chat_id}/{message_id}: {error:?}"
                );
            }
        }
        self.publish();
    }

    fn update_media_snapshot(
        &mut self,
        chat_id: i64,
        message_id: i32,
        mut update: impl FnMut(&mut MediaSnapshot),
    ) {
        if let Some(message) = self.view.history.iter_mut().find(|message| {
            message.chat_id == chat_id && message.id == message_id && message.id != 0
        }) && let Some(media) = message.media.as_mut()
        {
            update(media);
        }
        let conversation_chat_id = self.canonical_chat_id(chat_id);
        if let Some(cached) = self.histories.get_mut(&conversation_chat_id)
            && let Some(message) = cached.messages.iter_mut().find(|message| {
                message.chat_id == chat_id && message.id == message_id && message.id != 0
            })
            && let Some(media) = message.media.as_mut()
        {
            update(media);
        }
        for cached in self.topic_histories.values_mut() {
            if let Some(message) = cached.messages.iter_mut().find(|message| {
                message.chat_id == chat_id && message.id == message_id && message.id != 0
            }) && let Some(media) = message.media.as_mut()
            {
                update(media);
            }
        }
    }

    fn current_message_index(&self, chat_id: i64, message_id: i32) -> Option<usize> {
        self.view.history.iter().position(|message| {
            message.chat_id == chat_id && message.id == message_id && message.id != 0
        })
    }

    async fn delete_messages(&mut self, chat_id: i64, message_ids: Vec<i32>, revoke: bool) {
        let chat_id = self.canonical_chat_id(chat_id);
        let mut message_ids: Vec<i32> = message_ids
            .into_iter()
            .filter(|message_id| *message_id != 0)
            .collect();
        message_ids.sort_unstable();
        message_ids.dedup();
        if message_ids.is_empty() {
            return;
        }
        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            return;
        };
        let is_channel = peer_ref.id.kind() == PeerKind::Channel;
        let result = if is_channel {
            if revoke {
                client
                    .invoke(&tl::functions::channels::DeleteMessages {
                        channel: peer_ref.into(),
                        id: message_ids.clone(),
                    })
                    .await
                    .map(drop)
            } else {
                // Telegram has no per-user deletion for channel messages; the
                // official clients hide them locally instead.
                Ok(())
            }
        } else {
            client
                .invoke(&tl::functions::messages::DeleteMessages {
                    revoke,
                    id: message_ids.clone(),
                })
                .await
                .map(drop)
        };
        match result {
            Ok(()) => {
                self.remove_messages_locally(chat_id, &message_ids);
                self.publish();
            }
            Err(error) => {
                log::warn!("failed to delete Telegram messages: {error}");
                self.set_error(EngineError::from_invocation_with(
                    &error,
                    EngineError::DeleteFailed,
                ));
            }
        }
    }

    /// Drops deleted messages from the displayed history and the per-chat cache.
    fn remove_messages_locally(&mut self, chat_id: i64, message_ids: &[i32]) {
        let chat_id = self.canonical_chat_id(chat_id);
        self.view
            .history
            .retain(|message| message.chat_id != chat_id || !message_ids.contains(&message.id));
        if let Some(cached) = self.histories.get_mut(&chat_id) {
            cached
                .messages
                .retain(|message| message.chat_id != chat_id || !message_ids.contains(&message.id));
        }
        for cached in self.topic_histories.values_mut() {
            cached
                .messages
                .retain(|message| message.chat_id != chat_id || !message_ids.contains(&message.id));
        }
        if self.view.selected_chat == Some(chat_id) {
            self.cache_selected_history();
        }
    }

    async fn forward_messages(
        &mut self,
        source_chat_id: i64,
        message_ids: Vec<i32>,
        target_chat_id: i64,
        drop_author: bool,
    ) {
        let target_chat_id = self.canonical_chat_id(target_chat_id);
        let Some(source) = self.peer_refs.get(&source_chat_id).copied() else {
            return;
        };
        let Some(target) = self.peer_refs.get(&target_chat_id).copied() else {
            return;
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        let result = if drop_author {
            let request = tl::functions::messages::ForwardMessages {
                silent: false,
                background: false,
                with_my_score: false,
                drop_author: true,
                drop_media_captions: false,
                noforwards: false,
                allow_paid_floodskip: false,
                from_ephemeral: false,
                from_peer: source.into(),
                random_id: message_ids.iter().map(|_| rand::random::<i64>()).collect(),
                id: message_ids.clone(),
                to_peer: target.into(),
                top_msg_id: None,
                reply_to: None,
                schedule_date: None,
                schedule_repeat_period: None,
                send_as: None,
                quick_reply_shortcut: None,
                effect: None,
                video_timestamp: None,
                allow_paid_stars: None,
                suggested_post: None,
            };
            client.invoke(&request).await.map(drop)
        } else {
            client
                .forward_messages(target, &message_ids, source)
                .await
                .map(drop)
        };
        if let Err(error) = result {
            log::warn!("failed to forward Telegram messages: {error}");
            self.set_error(EngineError::from_invocation_with(
                &error,
                EngineError::ForwardFailed,
            ));
        }
    }

    async fn set_pinned(&mut self, chat_id: i64, pinned: bool) {
        let chat_id = self.canonical_chat_id(chat_id);
        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            return;
        };
        let input_peer: tl::enums::InputPeer = peer_ref.into();
        let request = tl::functions::messages::ToggleDialogPin {
            pinned,
            peer: tl::enums::InputDialogPeer::Peer(tl::types::InputDialogPeer { peer: input_peer }),
        };
        match client.invoke(&request).await {
            Ok(_) => {
                if let Some(chat) = self.all_chats.iter_mut().find(|chat| chat.id == chat_id) {
                    chat.pinned = pinned;
                }
                self.refresh_visible_chats();
                self.publish();
            }
            Err(error) => {
                log::warn!("failed to pin a Telegram chat: {error}");
                self.set_error(EngineError::PinFailed);
            }
        }
    }

    async fn set_pinned_message(&mut self, chat_id: i64, message_id: Option<i32>) {
        let chat_id = self.canonical_chat_id(chat_id);
        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            return;
        };
        let result = if let Some(message_id) = message_id {
            client.pin_message(peer_ref, message_id).await
        } else {
            // Unpinning without an id clears every pinned message.
            let pinned = self
                .all_chats
                .iter()
                .find(|chat| chat.id == chat_id)
                .and_then(|chat| chat.pinned_message.as_ref())
                .map(|message| message.id);
            if let Some(message_id) = pinned {
                client.unpin_message(peer_ref, message_id).await
            } else {
                Ok(())
            }
        };
        match result {
            Ok(()) => {
                let pinned = message_id.and_then(|message_id| {
                    self.view
                        .history
                        .iter()
                        .find(|message| message.chat_id == chat_id && message.id == message_id)
                        .cloned()
                });
                if let Some(chat) = self.all_chats.iter_mut().find(|chat| chat.id == chat_id) {
                    chat.pinned_message = pinned.map(Box::new);
                }
                self.refresh_visible_chats();
                self.publish();
            }
            Err(error) => {
                log::warn!("failed to pin a Telegram message: {error}");
                self.set_error(EngineError::from_invocation_with(
                    &error,
                    EngineError::PinMessageFailed,
                ));
            }
        }
    }

    async fn set_reaction(&mut self, chat_id: i64, message_id: i32, emoji: Option<String>) {
        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            return;
        };
        let reaction = emoji.as_ref().map(|emoji| {
            vec![tl::enums::Reaction::Emoji(tl::types::ReactionEmoji {
                emoticon: emoji.clone(),
            })]
        });
        let request = tl::functions::messages::SendReaction {
            big: false,
            add_to_recent: true,
            peer: peer_ref.into(),
            msg_id: message_id,
            reaction,
        };
        match client.invoke(&request).await {
            Ok(_) => {
                self.apply_local_reaction(chat_id, message_id, emoji.as_deref());
                self.publish();
            }
            Err(error) => {
                log::warn!("failed to send a Telegram reaction: {error}");
                self.set_error(EngineError::from_invocation_with(
                    &error,
                    EngineError::ReactionFailed,
                ));
            }
        }
    }

    async fn vote_poll(&mut self, chat_id: i64, message_id: i32, options: Vec<Vec<u8>>) {
        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            return;
        };
        let request = tl::functions::messages::SendVote {
            peer: peer_ref.into(),
            msg_id: message_id,
            options: options.clone(),
        };
        match client.invoke(&request).await {
            Ok(updates) => {
                // The RPC result usually carries the authoritative poll counts;
                // fall back to local chosen flags when it does not.
                if self.apply_poll_updates(updates) {
                    self.cache_selected_history();
                } else {
                    self.apply_local_vote(chat_id, message_id, &options);
                }
                self.publish();
            }
            Err(error) => {
                log::warn!("failed to vote in a Telegram poll: {error}");
                self.set_error(EngineError::from_invocation_with(
                    &error,
                    EngineError::ReactionFailed,
                ));
            }
        }
    }

    /// Applies every `updateMessagePoll` in an RPC response. Returns whether any
    /// cached poll matched.
    fn apply_poll_updates(&mut self, updates: tl::enums::Updates) -> bool {
        let updates = match updates {
            tl::enums::Updates::Updates(updates) => updates.updates,
            tl::enums::Updates::Combined(updates) => updates.updates,
            tl::enums::Updates::UpdateShort(update) => vec![update.update],
            // Short-message variants describe a sent message, not a poll.
            _ => Vec::new(),
        };
        let mut applied = false;
        for update in updates {
            if let tl::enums::Update::MessagePoll(raw) = update {
                let tl::enums::PollResults::Results(results) = &raw.results;
                applied |= self.apply_poll_update(raw.poll_id, results);
            }
        }
        applied
    }

    /// Applies authoritative poll results to every cached copy of the poll.
    fn apply_poll_update(&mut self, poll_id: i64, results: &tl::types::PollResults) -> bool {
        let mut changed = false;
        let update = |message: &mut MessageSnapshot| -> bool {
            let Some(poll) = message.media.as_mut().and_then(|media| media.poll.as_mut()) else {
                return false;
            };
            if poll.id != poll_id {
                return false;
            }
            apply_poll_results(poll, results);
            true
        };
        for message in &mut self.view.history {
            changed |= update(message);
        }
        for entry in self.histories.values_mut() {
            for message in &mut entry.messages {
                changed |= update(message);
            }
        }
        for entry in self.topic_histories.values_mut() {
            for message in &mut entry.messages {
                changed |= update(message);
            }
        }
        changed
    }

    /// Updates the chosen flags locally so the poll reacts immediately; the
    /// server echoes `updateMessagePoll` with authoritative counts.
    fn apply_local_vote(&mut self, chat_id: i64, message_id: i32, options: &[Vec<u8>]) {
        let update = |message: &mut MessageSnapshot| {
            if message.chat_id != chat_id || message.id != message_id {
                return;
            }
            let Some(poll) = message.media.as_mut().and_then(|media| media.poll.as_mut()) else {
                return;
            };
            for option in &mut poll.options {
                option.chosen = options.iter().any(|chosen| chosen == &option.option);
            }
        };
        for message in &mut self.view.history {
            update(message);
        }
        let conversation_chat_id = self.canonical_chat_id(chat_id);
        if let Some(entry) = self.histories.get_mut(&conversation_chat_id) {
            for message in &mut entry.messages {
                update(message);
            }
        }
        for entry in self.topic_histories.values_mut() {
            for message in &mut entry.messages {
                update(message);
            }
        }
    }

    /// Applies a reaction optimistically. The server echoes
    /// `updateMessageReactions`, which reconciles counts shortly after.
    fn apply_local_reaction(&mut self, chat_id: i64, message_id: i32, emoji: Option<&str>) {
        let Some(message) = self
            .view
            .history
            .iter_mut()
            .find(|message| message.chat_id == chat_id && message.id == message_id)
        else {
            return;
        };
        for reaction in &mut message.reactions {
            if reaction.chosen {
                reaction.chosen = false;
                reaction.count = reaction.count.saturating_sub(1);
            }
        }
        if let Some(emoji) = emoji {
            match message
                .reactions
                .iter_mut()
                .find(|reaction| reaction.emoji == emoji)
            {
                Some(reaction) => {
                    reaction.chosen = true;
                    reaction.count = reaction.count.saturating_add(1);
                }
                None => message.reactions.push(ReactionSnapshot {
                    emoji: emoji.to_owned(),
                    count: 1,
                    chosen: true,
                }),
            }
        }
        message.reactions.retain(|reaction| reaction.count > 0);
        self.cache_selected_history();
    }

    async fn set_muted(&mut self, chat_id: i64, muted: bool) {
        let chat_id = self.canonical_chat_id(chat_id);
        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            return;
        };
        let request = tl::functions::account::UpdateNotifySettings {
            peer: tl::enums::InputNotifyPeer::Peer(tl::types::InputNotifyPeer {
                peer: peer_ref.into(),
            }),
            settings: tl::enums::InputPeerNotifySettings::Settings(
                tl::types::InputPeerNotifySettings {
                    show_previews: None,
                    silent: None,
                    mute_until: Some(if muted { i32::MAX } else { 0 }),
                    sound: None,
                    stories_muted: None,
                    stories_hide_sender: None,
                    stories_sound: None,
                },
            ),
        };
        match client.invoke(&request).await {
            Ok(_) => {
                if let Some(chat) = self.all_chats.iter_mut().find(|chat| chat.id == chat_id) {
                    chat.muted = muted;
                }
                self.refresh_visible_chats();
                self.recompute_unread();
                self.publish();
            }
            Err(error) => {
                log::warn!("failed to mute a Telegram chat: {error}");
                self.set_error(EngineError::from_invocation_with(
                    &error,
                    EngineError::MuteFailed,
                ));
            }
        }
    }

    async fn set_archived(&mut self, chat_id: i64, archived: bool) {
        let chat_id = self.canonical_chat_id(chat_id);
        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            return;
        };
        let request = tl::functions::folders::EditPeerFolders {
            folder_peers: vec![tl::enums::InputFolderPeer::Peer(
                tl::types::InputFolderPeer {
                    peer: peer_ref.into(),
                    folder_id: if archived { ARCHIVE_FOLDER_ID } else { 0 },
                },
            )],
        };
        match client.invoke(&request).await {
            Ok(_) => {
                if let Some(chat) = self.all_chats.iter_mut().find(|chat| chat.id == chat_id) {
                    chat.archived = archived;
                }
                self.refresh_visible_chats();
                self.publish();
            }
            Err(error) => {
                log::warn!("failed to archive a Telegram chat: {error}");
                self.set_error(EngineError::from_invocation_with(
                    &error,
                    EngineError::ArchiveFailed,
                ));
            }
        }
    }

    async fn clear_history(&mut self, chat_id: i64) {
        let chat_id = self.canonical_chat_id(chat_id);
        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            return;
        };
        let result = if peer_ref.id.kind() == PeerKind::Channel {
            client
                .invoke(&tl::functions::channels::DeleteHistory {
                    for_everyone: false,
                    channel: peer_ref.into(),
                    max_id: 0,
                })
                .await
                .map(drop)
        } else {
            client
                .invoke(&tl::functions::messages::DeleteHistory {
                    just_clear: true,
                    revoke: false,
                    peer: peer_ref.into(),
                    max_id: 0,
                    min_date: None,
                    max_date: None,
                })
                .await
                .map(drop)
        };
        match result {
            Ok(()) => {
                if let Some(cached) = self.histories.get_mut(&chat_id) {
                    cached.messages.clear();
                }
                self.topic_histories.retain(|(id, _), _| *id != chat_id);
                if self.view.selected_chat == Some(chat_id) {
                    self.view.history.clear();
                }
                if let Some(chat) = self.all_chats.iter_mut().find(|chat| chat.id == chat_id) {
                    chat.preview.clear();
                    chat.preview_media = None;
                    chat.pinned_message = None;
                }
                self.refresh_visible_chats();
                self.publish();
            }
            Err(error) => {
                log::warn!("failed to clear a Telegram history: {error}");
                self.set_error(EngineError::from_invocation_with(
                    &error,
                    EngineError::ClearHistoryFailed,
                ));
            }
        }
    }

    async fn leave_chat(&mut self, chat_id: i64) {
        let chat_id = self.canonical_chat_id(chat_id);
        let Some((peer_ref, client)) = self.chat_context(chat_id) else {
            return;
        };
        match client.delete_dialog(peer_ref).await {
            Ok(()) => {
                self.peer_refs.remove(&chat_id);
                self.folder_membership.remove(&chat_id);
                self.all_chats.retain(|chat| chat.id != chat_id);
                self.histories.remove(&chat_id);
                self.drafts.retain(|(id, _), _| *id != chat_id);
                self.draft_epochs.retain(|(id, _), _| *id != chat_id);
                self.saved_drafts.retain(|(id, _), _| *id != chat_id);
                self.topic_histories.retain(|(id, _), _| *id != chat_id);
                if self.view.selected_chat == Some(chat_id) {
                    self.view.selected_chat = None;
                    self.view.history.clear();
                    self.view.draft.clear();
                    self.view.topics.clear();
                    self.view.loading_topics = false;
                    self.view.selected_topic = None;
                    self.history_epoch = self.history_epoch.wrapping_add(1);
                }
                self.refresh_visible_chats();
                self.recompute_unread();
                self.publish();
            }
            Err(error) => {
                log::warn!("failed to leave a Telegram chat: {error}");
                self.set_error(EngineError::from_invocation_with(
                    &error,
                    EngineError::LeaveFailed,
                ));
            }
        }
    }

    async fn handle_event(&mut self, event: EngineEvent) {
        match event {
            EngineEvent::Disconnected { error, epoch } => {
                if epoch != self.connection_epoch {
                    return;
                }
                self.persist_session();
                if let Some(task) = self.runner_task.take() {
                    task.abort();
                }
                self.client = None;
                self.session = None;
                self.view.connection = ConnectionState::Offline;
                log::warn!("Telegram connection dropped: {error}");
                self.publish();
                self.schedule_reconnect();
            }
            EngineEvent::Reconnect => self.reconnect().await,
            EngineEvent::RefreshChats => {
                self.chats_refresh_scheduled = false;
                self.spawn_load_chats();
            }
            EngineEvent::DraftSync {
                chat_id,
                topic_id,
                epoch,
                text,
            } => self.apply_draft_sync(chat_id, topic_id, epoch, text).await,
            EngineEvent::TopicsLoaded { chat_id, result } => {
                self.apply_topics_loaded(chat_id, result)
            }
            EngineEvent::TopicHistoryLoaded {
                chat_id,
                topic_id,
                epoch,
                offset_id,
                result,
            } => self.apply_topic_history_loaded(chat_id, topic_id, epoch, offset_id, result),
            EngineEvent::FillGap {
                chat_id,
                after_id,
                before_id,
            } => self.load_gap(chat_id, after_id, before_id),
            EngineEvent::ChatsLoaded { result } => self.apply_chats_loaded(result),
            EngineEvent::HistoryLoaded {
                conversation_chat_id,
                epoch,
                request,
                result,
            } => self.apply_history_loaded(conversation_chat_id, epoch, request, result),
            EngineEvent::GapLoaded {
                chat_id,
                epoch,
                messages,
            } => self.apply_gap_loaded(chat_id, epoch, messages),
            EngineEvent::SearchLoaded {
                epoch,
                append,
                next_offset,
                result,
            } => self.apply_search_loaded(epoch, append, next_offset, result),
            EngineEvent::SendFinished {
                chat_id,
                local_id,
                result,
            } => self.apply_send_finished(chat_id, local_id, result),
            EngineEvent::TypingExpired {
                chat_id,
                until_unix,
            } => self.apply_typing_expired(chat_id, until_unix),
            EngineEvent::MediaLoaded {
                chat_id,
                message_id,
                result,
            } => self.apply_media_loaded(chat_id, message_id, result),
            EngineEvent::ThumbnailLoaded {
                chat_id,
                message_id,
                result,
            } => self.apply_thumbnail_loaded(chat_id, message_id, result),
            EngineEvent::Update(Update::NewMessage(message)) => {
                self.handle_incoming_message(message.into_inner()).await
            }
            EngineEvent::Update(Update::MessageEdited(message)) => {
                self.handle_edited_message(message.into_inner()).await
            }
            EngineEvent::Update(Update::Raw(raw)) => self.handle_raw_update(&raw.raw).await,
            EngineEvent::Update(Update::MessageDeleted(deletion)) => {
                let ids: HashSet<i32> = deletion.messages().iter().copied().collect();
                if let Some(chat_id) = deletion.channel_id().and_then(|channel_id| {
                    PeerId::channel(channel_id).and_then(PeerId::bot_api_dialog_id)
                }) {
                    let conversation_chat_id = self.canonical_chat_id(chat_id);
                    if let Some(cached) = self.histories.get_mut(&conversation_chat_id) {
                        cached.messages.retain(|message| {
                            message.chat_id != chat_id || !ids.contains(&message.id)
                        });
                    }
                    if self.view.selected_chat == Some(conversation_chat_id) {
                        self.view.history.retain(|message| {
                            message.chat_id != chat_id || !ids.contains(&message.id)
                        });
                        self.cache_selected_history();
                    }
                    self.publish();
                } else {
                    self.view.history.retain(|message| {
                        let is_channel = PeerId::from_bot_api_dialog_id(message.chat_id)
                            .is_some_and(|peer_id| peer_id.kind() == PeerKind::Channel);
                        is_channel || !ids.contains(&message.id)
                    });
                    self.cache_selected_history();
                    self.publish();
                }
            }
            EngineEvent::QrToken(result) => {
                if matches!(result, tl::enums::auth::LoginToken::Success(_)) {
                    self.stop_qr();
                }
                self.handle_qr_result(Ok(result)).await;
            }
            EngineEvent::QrError(error) => self.fail_qr(error),
            EngineEvent::Update(_) => {}
        }
    }

    async fn handle_incoming_message(&mut self, message: Message) {
        let Some(source_chat_id) = message.peer_id().bot_api_dialog_id() else {
            log::debug!("ignoring a Telegram message with an unresolvable peer");
            return;
        };
        let conversation_chat_id = self.canonical_chat_id(source_chat_id);
        let snapshot = message_to_snapshot(&message, source_chat_id);
        let outgoing = snapshot.outgoing;
        self.remember_sender(&snapshot);
        let selected = self.view.selected_chat == Some(conversation_chat_id);
        let topic_matches = match self.view.selected_topic {
            Some(topic_id) => snapshot.topic_id == Some(topic_id),
            None => snapshot
                .topic_id
                .is_none_or(|topic_id| topic_id == GENERAL_TOPIC_ID),
        };
        let actively_viewing = topic_matches && self.is_actively_viewing(conversation_chat_id);
        let mentioned = matches!(&message.raw, tl::enums::Message::Message(raw) if raw.mentioned);
        let message_topic_id = snapshot.topic_id.unwrap_or(GENERAL_TOPIC_ID);

        if let Some(chat) = self
            .all_chats
            .iter_mut()
            .find(|chat| chat.id == conversation_chat_id)
        {
            let (preview, preview_media) = chat_preview(&message);
            chat.preview = preview;
            chat.preview_media = preview_media;
            chat.timestamp_unix = snapshot.timestamp_unix;
            // A message from this peer ends its typing indicator.
            chat.typing = None;
            if !snapshot.outgoing {
                if !actively_viewing {
                    chat.unread_count = chat.unread_count.saturating_add(1);
                }
                if mentioned {
                    chat.unread_mentions = chat.unread_mentions.saturating_add(1);
                }
            }
        } else {
            // A chat we have not loaded yet; refresh the list once the burst of
            // messages settles instead of refetching on every message.
            self.schedule_chats_refresh();
        }

        if selected && !outgoing && !actively_viewing && !self.view.topics.is_empty() {
            if let Some(topic) = self
                .view
                .topics
                .iter_mut()
                .find(|topic| topic.id == message_topic_id)
            {
                topic.unread_count = topic.unread_count.saturating_add(1);
                if mentioned {
                    topic.unread_mentions = topic.unread_mentions.saturating_add(1);
                }
            }
        }

        if selected && snapshot.id != 0 && topic_matches {
            let newest_known = self
                .view
                .history
                .iter()
                .rev()
                .find(|message| message.chat_id == source_chat_id && message.id != 0)
                .map(|message| message.id);
            let incoming_id = snapshot.id;
            self.upsert_history_message(snapshot);
            self.sort_history();
            self.cache_selected_history();
            if let Some(newest_known) = newest_known {
                if incoming_id > newest_known.saturating_add(1) {
                    self.schedule_gap_fill(source_chat_id, newest_known, incoming_id);
                }
            }
        }
        self.refresh_visible_chats();
        self.recompute_unread();
        self.publish();

        if selected && topic_matches && !outgoing {
            self.maybe_mark_read();
        }
    }

    async fn handle_edited_message(&mut self, message: Message) {
        let Some(source_chat_id) = message.peer_id().bot_api_dialog_id() else {
            log::debug!("ignoring an edited Telegram message with an unresolvable peer");
            return;
        };
        let conversation_chat_id = self.canonical_chat_id(source_chat_id);
        let updated = message_to_snapshot(&message, source_chat_id);
        self.remember_sender(&updated);
        if let Some(chat) = self
            .all_chats
            .iter_mut()
            .find(|chat| chat.id == conversation_chat_id)
        {
            let (preview, preview_media) = chat_preview(&message);
            chat.preview = preview;
            chat.preview_media = preview_media;
        }
        let updated_for_cache = updated.clone();
        let mut found = false;
        for snapshot in &mut self.view.history {
            if snapshot.chat_id == updated.chat_id && snapshot.id == updated.id {
                *snapshot = updated;
                found = true;
                break;
            }
        }
        if let Some(cached) = self.histories.get_mut(&conversation_chat_id) {
            for snapshot in &mut cached.messages {
                if snapshot.chat_id == updated_for_cache.chat_id
                    && snapshot.id == updated_for_cache.id
                {
                    *snapshot = updated_for_cache.clone();
                    break;
                }
            }
        }
        for cached in self.topic_histories.values_mut() {
            for snapshot in &mut cached.messages {
                if snapshot.chat_id == updated_for_cache.chat_id
                    && snapshot.id == updated_for_cache.id
                {
                    *snapshot = updated_for_cache.clone();
                    break;
                }
            }
        }
        if found {
            self.cache_selected_history();
        }
        self.refresh_visible_chats();
        self.publish();
    }

    /// Applies read-state, typing, reaction, and pin updates from other devices
    /// to the visible snapshots. A peer that is not loaded is ignored until the
    /// next chat-list reload.
    async fn handle_raw_update(&mut self, update: &tl::enums::Update) {
        match update {
            tl::enums::Update::ReadHistoryInbox(raw) => {
                self.apply_unread_count(peer_bot_api_id(&raw.peer), raw.still_unread_count);
            }
            tl::enums::Update::ReadChannelInbox(raw) => {
                let chat_id = PeerId::channel(raw.channel_id).and_then(PeerId::bot_api_dialog_id);
                self.apply_unread_count(chat_id, raw.still_unread_count);
            }
            tl::enums::Update::ReadHistoryOutbox(raw) => {
                self.apply_outbox_read(peer_bot_api_id(&raw.peer), raw.max_id);
            }
            tl::enums::Update::ReadChannelOutbox(raw) => {
                let chat_id = PeerId::channel(raw.channel_id).and_then(PeerId::bot_api_dialog_id);
                self.apply_outbox_read(chat_id, raw.max_id);
            }
            tl::enums::Update::UserTyping(raw) => {
                let chat_id = PeerId::user(raw.user_id).and_then(PeerId::bot_api_dialog_id);
                if let Some(chat_id) = chat_id {
                    self.apply_remote_typing(chat_id, Some(raw.user_id), &raw.action);
                }
            }
            tl::enums::Update::ChatUserTyping(raw) => {
                let chat_id = PeerId::chat(raw.chat_id).and_then(PeerId::bot_api_dialog_id);
                if let Some(chat_id) = chat_id {
                    let user_id = peer_bot_api_id(&raw.from_id);
                    self.apply_remote_typing(chat_id, user_id, &raw.action);
                }
            }
            tl::enums::Update::ChannelUserTyping(raw) => {
                let chat_id = PeerId::channel(raw.channel_id).and_then(PeerId::bot_api_dialog_id);
                if let Some(chat_id) = chat_id {
                    let user_id = peer_bot_api_id(&raw.from_id);
                    self.apply_remote_typing(chat_id, user_id, &raw.action);
                }
            }
            tl::enums::Update::MessageReactions(raw) => {
                let Some(chat_id) = peer_bot_api_id(&raw.peer) else {
                    return;
                };
                let reactions = message_reactions(&raw.reactions);
                let mut changed = false;
                for message in &mut self.view.history {
                    if message.chat_id == chat_id && message.id == raw.msg_id {
                        message.reactions = reactions.clone();
                        changed = true;
                    }
                }
                let conversation_chat_id = self.canonical_chat_id(chat_id);
                if let Some(cached) = self.histories.get_mut(&conversation_chat_id) {
                    for message in &mut cached.messages {
                        if message.chat_id == chat_id && message.id == raw.msg_id {
                            message.reactions = reactions.clone();
                            changed = true;
                        }
                    }
                }
                for cached in self.topic_histories.values_mut() {
                    for message in &mut cached.messages {
                        if message.chat_id == chat_id && message.id == raw.msg_id {
                            message.reactions = reactions.clone();
                            changed = true;
                        }
                    }
                }
                if changed {
                    if self.view.selected_chat == Some(conversation_chat_id) {
                        self.cache_selected_history();
                    }
                    self.publish();
                }
            }
            tl::enums::Update::PinnedMessages(raw) => {
                let Some(chat_id) = peer_bot_api_id(&raw.peer) else {
                    return;
                };
                let conversation_chat_id = self.canonical_chat_id(chat_id);
                let pinned = if raw.pinned {
                    raw.messages.first().and_then(|message_id| {
                        self.view
                            .history
                            .iter()
                            .find(|message| message.id == *message_id)
                            .cloned()
                            .or_else(|| {
                                self.histories.get(&conversation_chat_id).and_then(|entry| {
                                    entry
                                        .messages
                                        .iter()
                                        .find(|message| message.id == *message_id)
                                        .cloned()
                                })
                            })
                    })
                } else {
                    None
                };
                if let Some(chat) = self
                    .all_chats
                    .iter_mut()
                    .find(|chat| chat.id == conversation_chat_id)
                {
                    chat.pinned_message = pinned.map(Box::new);
                }
                self.refresh_visible_chats();
                self.publish();
            }
            tl::enums::Update::UserStatus(raw) => {
                let Some(chat_id) = PeerId::user(raw.user_id).and_then(PeerId::bot_api_dialog_id)
                else {
                    return;
                };
                let presence = presence_from_status(&raw.status);
                let Some(chat) = self.all_chats.iter_mut().find(|chat| chat.id == chat_id) else {
                    return;
                };
                if chat.presence == presence {
                    return;
                }
                chat.presence = presence;
                self.refresh_visible_chats();
                self.publish();
            }
            tl::enums::Update::MessagePoll(raw) => {
                let tl::enums::PollResults::Results(results) = &raw.results;
                if self.apply_poll_update(raw.poll_id, results) {
                    self.cache_selected_history();
                    self.publish();
                }
            }
            _ => {}
        }
    }

    fn apply_unread_count(&mut self, chat_id: Option<i64>, unread_count: i32) {
        let Some(chat_id) = chat_id else {
            return;
        };
        let Some(chat) = self.all_chats.iter_mut().find(|chat| chat.id == chat_id) else {
            return;
        };
        let unread_count = unread_count.max(0);
        if chat.unread_count == unread_count {
            return;
        }
        chat.unread_count = unread_count;
        self.refresh_visible_chats();
        self.recompute_unread();
        self.publish();
    }

    fn apply_outbox_read(&mut self, chat_id: Option<i64>, max_id: i32) {
        let Some(chat_id) = chat_id else {
            return;
        };
        let Some(chat) = self.all_chats.iter_mut().find(|chat| chat.id == chat_id) else {
            return;
        };
        if chat.read_outbox_message_id >= max_id {
            return;
        }
        chat.read_outbox_message_id = max_id;
        self.refresh_visible_chats();
        self.publish();
    }
}

/// Fetches one history page and returns it oldest-first, together with whether
/// more pages may follow.
async fn fetch_history_page(
    client: &Client,
    peer_ref: PeerRef,
    chat_id: i64,
    before_id: Option<i32>,
) -> Result<(Vec<MessageSnapshot>, bool), EngineError> {
    let mut iterator = client.iter_messages(peer_ref).limit(HISTORY_PAGE_SIZE);
    if let Some(before_id) = before_id {
        iterator = iterator.offset_id(before_id);
    }

    let mut page = Vec::new();
    loop {
        match iterator.next().await {
            Ok(Some(message)) => page.push(message_to_snapshot(&message, chat_id)),
            Ok(None) => break,
            Err(error) => {
                log::warn!("failed to load Telegram messages: {error}");
                return Err(EngineError::from_invocation_with(
                    &error,
                    EngineError::LoadMessagesFailed,
                ));
            }
        }
    }

    // `iter_messages` yields newest-to-oldest, but the panel renders
    // oldest-first, so normalize the page before storing it.
    let has_more = page.len() == HISTORY_PAGE_SIZE;
    page.reverse();
    Ok((page, has_more))
}

/// Fetches a page of replies in a forum topic. `messages.getReplies` returns
/// raw message envelopes, so their ids are resolved through the client's peer
/// cache to get sender names and media.
async fn fetch_topic_page(
    client: &Client,
    peer_ref: PeerRef,
    chat_id: i64,
    topic_id: i32,
    offset_id: i32,
) -> Result<(Vec<MessageSnapshot>, bool), EngineError> {
    let request = tl::functions::messages::GetReplies {
        peer: peer_ref.into(),
        msg_id: topic_id,
        offset_id,
        offset_date: 0,
        add_offset: 0,
        limit: HISTORY_PAGE_SIZE as i32,
        max_id: 0,
        min_id: 0,
        hash: 0,
    };
    let response = client.invoke(&request).await.map_err(|error| {
        log::warn!("failed to load Telegram topic replies: {error}");
        EngineError::from_invocation_with(&error, EngineError::LoadMessagesFailed)
    })?;
    let raw_messages = match response {
        tl::enums::messages::Messages::ChannelMessages(messages) => messages.messages,
        tl::enums::messages::Messages::Messages(messages) => messages.messages,
        tl::enums::messages::Messages::Slice(messages) => messages.messages,
        tl::enums::messages::Messages::NotModified(_) => Vec::new(),
    };
    let ids: Vec<i32> = raw_messages
        .into_iter()
        .filter_map(|message| match message {
            tl::enums::Message::Message(message) => Some(message.id),
            tl::enums::Message::Service(message) => Some(message.id),
            tl::enums::Message::Empty(_) => None,
        })
        .collect();
    if ids.is_empty() {
        return Ok((Vec::new(), false));
    }
    // Pagination is based on how many replies the server returned, not on how
    // many of them resolved to full messages.
    let has_more = ids.len() >= HISTORY_PAGE_SIZE;
    let fetched = client
        .get_messages_by_id(peer_ref, &ids)
        .await
        .map_err(|error| {
            log::warn!("failed to resolve Telegram topic replies: {error}");
            EngineError::from_invocation_with(&error, EngineError::LoadMessagesFailed)
        })?;
    let mut page: Vec<MessageSnapshot> = fetched
        .into_iter()
        .flatten()
        .map(|message| message_to_snapshot(&message, chat_id))
        .collect();
    page.sort_by_key(|message| message.id);
    Ok((page, has_more))
}

/// Merges a fetched page into a conversation that may span two Telegram peers.
/// The fetched page wins over a cached copy of the same peer-local message.
fn merge_history(
    existing: &[MessageSnapshot],
    page: Vec<MessageSnapshot>,
    conversation_chat_id: i64,
    migrated_from_chat_id: Option<i64>,
) -> Vec<MessageSnapshot> {
    let mut merged = Vec::with_capacity(existing.len() + page.len());
    let mut seen = HashSet::new();
    for message in page.into_iter().chain(existing.iter().cloned()) {
        if message.id == 0 || seen.insert((message.chat_id, message.id)) {
            merged.push(message);
        }
    }
    sort_history_messages(&mut merged, conversation_chat_id, migrated_from_chat_id);
    merged
}

fn next_history_page(
    conversation_chat_id: i64,
    current_page: HistoryCursor,
    has_more: bool,
    oldest_message_id: Option<i32>,
    migrated_from_chat_id: Option<i64>,
) -> Option<HistoryCursor> {
    if has_more {
        return oldest_message_id.map(|before_message_id| HistoryCursor {
            source_chat_id: current_page.source_chat_id,
            before_message_id: Some(before_message_id),
        });
    }
    if current_page.source_chat_id != conversation_chat_id {
        return None;
    }
    migrated_from_chat_id.map(|source_chat_id| HistoryCursor {
        source_chat_id,
        before_message_id: None,
    })
}

fn sort_history_messages(
    messages: &mut [MessageSnapshot],
    conversation_chat_id: i64,
    migrated_from_chat_id: Option<i64>,
) {
    messages.sort_by_key(|message| {
        let source_order = if Some(message.chat_id) == migrated_from_chat_id {
            0
        } else if message.chat_id == conversation_chat_id {
            1
        } else {
            2
        };
        (message.id == 0, source_order, message.id)
    });
}

/// Drops duplicate server messages, keeping the most recent entry for each
/// peer-local id. Optimistic (id `0`) messages are always kept.
fn dedupe_history_messages(messages: &mut Vec<MessageSnapshot>) {
    let mut seen = HashSet::new();
    let mut deduped: Vec<MessageSnapshot> = Vec::with_capacity(messages.len());
    for message in messages.drain(..).rev() {
        if message.id == 0 || seen.insert((message.chat_id, message.id)) {
            deduped.push(message);
        }
    }
    deduped.reverse();
    *messages = deduped;
}

/// Replaces one optimistic message with the server's messages. Used both for
/// the visible history and for a background chat's cache, so a send that
/// finishes after the user switched chats cannot leak into the wrong chat.
fn replace_sent_messages(
    messages: &mut Vec<MessageSnapshot>,
    local_id: u64,
    sent: Vec<MessageSnapshot>,
    conversation_chat_id: i64,
    migrated_from_chat_id: Option<i64>,
) {
    messages.retain(|message| message.local_id != Some(local_id));
    for snapshot in sent {
        // A finished send must only contribute messages from the chat it was
        // sent to. This keeps a late event from leaking into another chat.
        if snapshot.chat_id != conversation_chat_id
            && Some(snapshot.chat_id) != migrated_from_chat_id
        {
            continue;
        }
        if let Some(existing) = messages
            .iter_mut()
            .find(|message| message.chat_id == snapshot.chat_id && message.id == snapshot.id)
        {
            *existing = snapshot;
        } else {
            messages.push(snapshot);
        }
    }
    dedupe_history_messages(messages);
    sort_history_messages(messages, conversation_chat_id, migrated_from_chat_id);
}

fn normalize_loaded_dialogs(
    entries: Vec<LoadedDialog>,
    folders: Vec<ChatFolderSnapshot>,
    folder_membership: HashMap<i64, Option<i32>>,
) -> LoadedChatList {
    let mut unique_entries = Vec::with_capacity(entries.len());
    let mut seen_chat_ids = HashSet::with_capacity(entries.len());
    for entry in entries {
        if seen_chat_ids.insert(entry.snapshot.id) {
            unique_entries.push(entry);
        }
    }

    let available_chat_ids: HashSet<i64> = unique_entries
        .iter()
        .map(|entry| entry.snapshot.id)
        .collect();
    let mut chats = Vec::with_capacity(unique_entries.len());
    let mut peer_refs = HashMap::with_capacity(unique_entries.len());
    let mut migrated_to = HashMap::new();
    let mut migrated_from = HashMap::new();
    for entry in unique_entries {
        let chat_id = entry.snapshot.id;
        peer_refs.insert(chat_id, entry.peer_ref);
        if let Some(target_chat_id) = entry
            .migrated_to_chat_id
            .filter(|target_chat_id| available_chat_ids.contains(target_chat_id))
        {
            migrated_to.insert(chat_id, target_chat_id);
            migrated_from.entry(target_chat_id).or_insert(chat_id);
        } else {
            chats.push(entry.snapshot);
        }
    }
    chats.sort_by(|left, right| {
        right
            .pinned
            .cmp(&left.pinned)
            .then_with(|| right.timestamp_unix.cmp(&left.timestamp_unix))
    });

    let folder_membership = folder_membership
        .into_iter()
        .filter(|(chat_id, _)| peer_refs.contains_key(chat_id))
        .collect();

    LoadedChatList {
        chats,
        peer_refs,
        migrated_to,
        migrated_from,
        folders,
        folder_membership,
    }
}

fn dialog_to_snapshot(dialog: &Dialog) -> Option<ChatSnapshot> {
    let id = dialog.peer_id().bot_api_dialog_id()?;
    let title = dialog.peer.name().unwrap_or("Unknown").to_owned();
    let last_message = dialog.last_message.as_ref();
    let (preview, preview_media) = last_message.map_or_else(|| (String::new(), None), chat_preview);
    let timestamp_unix = last_message.map_or_default(|message| message.date().as_second());

    let (unread_count, unread_mentions, pinned, muted, read_inbox, read_outbox, folder_id, draft) =
        match &dialog.raw {
            tl::enums::Dialog::Dialog(raw) => {
                let muted = match &raw.notify_settings {
                    tl::enums::PeerNotifySettings::Settings(settings) => {
                        is_muted(settings.mute_until)
                    }
                };
                let draft = match raw.draft.as_ref() {
                    Some(tl::enums::DraftMessage::Message(draft)) if !draft.message.is_empty() => {
                        Some(draft.message.clone())
                    }
                    _ => None,
                };
                (
                    raw.unread_count,
                    raw.unread_mentions_count,
                    raw.pinned,
                    muted,
                    raw.read_inbox_max_id,
                    raw.read_outbox_max_id,
                    raw.folder_id,
                    draft,
                )
            }
            // Dialog folders and community entries are not conversations, so
            // they are not listed.
            tl::enums::Dialog::Folder(_) | tl::enums::Dialog::Community(_) => return None,
        };

    let peer = dialog.peer();
    let mut snapshot = ChatSnapshot {
        id,
        title: title.clone(),
        username: peer.username().map(ToOwned::to_owned),
        kind: peer_chat_kind(peer),
        preview,
        is_forum: peer_is_forum(peer),
        preview_media,
        timestamp_unix,
        unread_count,
        unread_mentions,
        pinned,
        muted,
        archived: folder_id == Some(ARCHIVE_FOLDER_ID),
        can_send: peer_can_send(peer),
        slow_mode_seconds: 0,
        read_inbox_message_id: read_inbox,
        read_outbox_message_id: read_outbox,
        avatar_initials: avatar_initials(&title),
        draft,
        typing: None,
        pinned_message: None,
        presence: peer_presence(peer),
    };
    if snapshot.unread_mentions < 0 {
        snapshot.unread_mentions = 0;
    }
    Some(snapshot)
}

fn peer_chat_kind(peer: &Peer) -> ChatKind {
    match peer {
        Peer::User(_) => ChatKind::User,
        Peer::Channel(_) => ChatKind::Channel,
        Peer::Group(_) | Peer::Community(_) => ChatKind::Group,
    }
}

/// Whether the account may post in a chat. Channels are read-only unless the
/// account created or administers them. Basic groups also honor the default
/// banned rights, so a restricted member does not get a composer that cannot
/// send.
fn peer_can_send(peer: &Peer) -> bool {
    match peer {
        Peer::User(_) => true,
        Peer::Group(group) => match &group.raw {
            tl::enums::Chat::Chat(chat) => {
                !chat.left && !banned_from_sending(chat.default_banned_rights.as_ref())
            }
            tl::enums::Chat::Forbidden(_) => false,
            tl::enums::Chat::Channel(channel) => {
                !channel.left
                    && (!channel.broadcast || channel.creator || channel.admin_rights.is_some())
                    && !banned_from_sending(channel.banned_rights.as_ref())
            }
            tl::enums::Chat::ChannelForbidden(_) => false,
            _ => true,
        },
        Peer::Channel(channel) => {
            !channel.raw.left
                && (!channel.raw.broadcast
                    || channel.raw.creator
                    || channel.raw.admin_rights.is_some())
                && !banned_from_sending(channel.raw.banned_rights.as_ref())
        }
        Peer::Community(_) => false,
    }
}

/// Whether a banned-rights mask forbids sending regular messages.
fn banned_from_sending(rights: Option<&tl::enums::ChatBannedRights>) -> bool {
    matches!(
        rights,
        Some(tl::enums::ChatBannedRights::Rights(rights)) if rights.send_messages
    )
}

/// Whether a peer is a forum supergroup.
fn peer_is_forum(peer: &Peer) -> bool {
    match peer {
        Peer::Channel(channel) => channel.raw.forum,
        Peer::Group(group) => match &group.raw {
            tl::enums::Chat::Channel(channel) => channel.forum,
            _ => false,
        },
        Peer::User(_) | Peer::Community(_) => false,
    }
}

fn peer_presence(peer: &Peer) -> Presence {
    let Peer::User(user) = peer else {
        return Presence::Unknown;
    };
    presence_from_status(user.status())
}

fn presence_from_status(status: &tl::enums::UserStatus) -> Presence {
    match status {
        tl::enums::UserStatus::Online(_) => Presence::Online,
        tl::enums::UserStatus::Recently(_) => Presence::Recently,
        tl::enums::UserStatus::Offline(status) => Presence::LastSeen(i64::from(status.was_online)),
        tl::enums::UserStatus::LastWeek(_) => Presence::Recently,
        tl::enums::UserStatus::LastMonth(_) => Presence::Recently,
        tl::enums::UserStatus::Empty => Presence::Unknown,
    }
}

fn dialog_folder(dialog: &Dialog) -> Option<i32> {
    match &dialog.raw {
        tl::enums::Dialog::Dialog(raw) => raw.folder_id,
        _ => None,
    }
}

fn dialog_migration_target(dialog: &Dialog) -> Option<i64> {
    let Peer::Group(group) = dialog.peer() else {
        return None;
    };
    let tl::enums::Chat::Chat(chat) = &group.raw else {
        return None;
    };
    let channel_id = match chat.migrated_to.as_ref()? {
        tl::enums::InputChannel::Channel(channel) => channel.channel_id,
        tl::enums::InputChannel::FromMessage(channel) => channel.channel_id,
        tl::enums::InputChannel::Empty => return None,
    };
    PeerId::channel(channel_id).and_then(PeerId::bot_api_dialog_id)
}

/// Builds the chat-list preview for a message. Media-only messages carry their
/// kind so the panel can show a localized name instead of a blank line.
fn chat_preview(message: &Message) -> (String, Option<MediaKind>) {
    let text = message.text();
    if !text.is_empty() {
        return (text.replace('\n', " "), None);
    }
    let kind = message
        .media()
        .as_ref()
        .map(media_to_snapshot)
        .map(|m| m.kind);
    (String::new(), kind)
}

fn is_muted(mute_until: Option<i32>) -> bool {
    match mute_until {
        Some(until) if until == i32::MAX => true,
        Some(until) => until > current_unix_timestamp() as i32,
        None => false,
    }
}

fn message_to_snapshot(message: &Message, chat_id: i64) -> MessageSnapshot {
    let text = message.text().to_owned();
    let markdown = message
        .fmt_entities()
        .and_then(|entities| entities_to_markdown(&text, entities));
    let sender_name = message
        .sender()
        .and_then(Peer::name)
        .unwrap_or("")
        .to_owned();
    let sender_id = message
        .sender_id()
        .and_then(|sender_id| sender_id.bot_api_dialog_id());
    let (raw, service) = match &message.raw {
        tl::enums::Message::Message(raw) => (Some(raw), None),
        tl::enums::Message::Service(service) => (None, Some(service)),
        tl::enums::Message::Empty(_) => (None, None),
    };
    let reply_header = raw
        .and_then(|raw| raw.reply_to.as_ref())
        .or_else(|| service.and_then(|service| service.reply_to.as_ref()));
    let reply_to = reply_header.and_then(reply_header_snapshot).map(Box::new);
    let (topic_id, thread_root_id) = reply_header.map_or((None, None), |header| match header {
        tl::enums::MessageReplyHeader::Header(header) if header.forum_topic => {
            (header.reply_to_top_id.or(header.reply_to_msg_id), None)
        }
        tl::enums::MessageReplyHeader::Header(header) => (None, header.reply_to_top_id),
        tl::enums::MessageReplyHeader::MessageReplyStoryHeader(_) => (None, None),
    });
    let forwarded_from =
        raw.and_then(|raw| raw.fwd_from.as_ref())
            .and_then(|header| match header {
                tl::enums::MessageFwdHeader::Header(header) => header
                    .from_name
                    .clone()
                    .or_else(|| header.post_author.clone()),
            });
    let views = raw.and_then(|raw| raw.views);
    let grouped_id = message.grouped_id();
    let action = message
        .action()
        .and_then(|action| message_action(action, &sender_name));
    let reactions = raw
        .and_then(|raw| raw.reactions.as_ref())
        .or_else(|| service.and_then(|service| service.reactions.as_ref()))
        .map_or_default(message_reactions);

    MessageSnapshot {
        id: message.id(),
        chat_id,
        outgoing: message.outgoing(),
        sender_name,
        sender_id,
        timestamp_unix: message.date().as_second(),
        text,
        markdown,
        media: message.media().as_ref().map(media_to_snapshot),
        reply_to,
        forwarded_from,
        grouped_id,
        topic_id,
        thread_root_id,
        views,
        action,
        reactions,
        send_state: SendState::Sent,
        local_id: None,
        edited: message.edit_date().is_some(),
    }
}

fn reply_header_snapshot(header: &tl::enums::MessageReplyHeader) -> Option<ReplySnapshot> {
    let header = match header {
        tl::enums::MessageReplyHeader::Header(header) => header,
        tl::enums::MessageReplyHeader::MessageReplyStoryHeader(_) => return None,
    };
    // A message posted into a forum topic carries the topic id in its reply
    // header. That is routing metadata, not a visible reply to another message.
    // A direct post leaves `reply_to_top_id` empty and puts the topic id in
    // `reply_to_msg_id`; a reply to the topic itself repeats the same id in both.
    if header.forum_topic {
        let Some(top_id) = header.reply_to_top_id else {
            return None;
        };
        if header.reply_to_msg_id == Some(top_id) {
            return None;
        }
    }
    let message_id = header.reply_to_msg_id?;
    Some(ReplySnapshot {
        message_id,
        sender_name: String::new(),
        text: String::new(),
        media: None,
    })
}

fn message_reactions(reactions: &tl::enums::MessageReactions) -> Vec<ReactionSnapshot> {
    let tl::enums::MessageReactions::Reactions(reactions) = reactions;
    reactions
        .results
        .iter()
        .filter_map(|count| {
            let tl::enums::ReactionCount::Count(count) = count;
            let emoji = match &count.reaction {
                tl::enums::Reaction::Emoji(emoji) => emoji.emoticon.clone(),
                // Custom (premium) emoji need the sticker pack to render, which
                // this panel does not download.
                _ => return None,
            };
            Some(ReactionSnapshot {
                emoji,
                count: count.count,
                chosen: count.chosen_order.is_some(),
            })
        })
        .collect()
}

fn message_action(action: &tl::enums::MessageAction, actor: &str) -> Option<MessageAction> {
    let actor = actor.to_owned();
    Some(match action {
        tl::enums::MessageAction::ChatAddUser(raw) => MessageAction::MemberAdded {
            actor,
            member: raw.users.first().map_or_default(ToString::to_string),
        },
        tl::enums::MessageAction::ChatJoinedByLink(_)
        | tl::enums::MessageAction::ChatJoinedByRequest => MessageAction::MemberJoined { actor },
        tl::enums::MessageAction::ChatDeleteUser(_) => MessageAction::MemberLeft { actor },
        tl::enums::MessageAction::ChatEditTitle(raw) => MessageAction::TitleChanged {
            actor,
            title: raw.title.clone(),
        },
        tl::enums::MessageAction::ChatEditPhoto(_) => MessageAction::PhotoChanged { actor },
        tl::enums::MessageAction::PinMessage => MessageAction::PinnedMessage { actor },
        tl::enums::MessageAction::ChatCreate(raw) => MessageAction::GroupCreated {
            actor,
            title: raw.title.clone(),
        },
        tl::enums::MessageAction::ChannelCreate(raw) => MessageAction::ChannelCreated {
            title: raw.title.clone(),
        },
        tl::enums::MessageAction::PhoneCall(raw) => MessageAction::Call {
            outgoing: false,
            missed: matches!(
                raw.reason.as_ref(),
                Some(tl::enums::PhoneCallDiscardReason::Missed)
            ),
            duration_seconds: raw.duration,
        },
        _ => MessageAction::Other,
    })
}

/// Builds the chat-list preview for a message snapshot. Media-only messages
/// carry their kind so the panel can show a localized name instead of a blank.
fn snapshot_preview(message: &MessageSnapshot) -> (String, Option<MediaKind>) {
    let text = message.preview_text();
    (text, message.media.as_ref().map(|media| media.kind))
}

/// Which cached artifact a download request targets.
#[derive(Clone, Copy)]
enum DownloadTarget {
    /// The original media file.
    Original,
    /// The small preview thumbnail.
    Thumbnail,
}

impl DownloadTarget {
    /// Whether `media` can be downloaded for this target. A thumbnail that is
    /// already cached or in flight is skipped, but a cached path whose file the
    /// OS deleted is fetched again.
    fn is_eligible(self, media: &MediaSnapshot) -> bool {
        match self {
            Self::Original => media.is_downloadable(),
            Self::Thumbnail => {
                matches!(
                    media.kind,
                    MediaKind::Photo
                        | MediaKind::Sticker
                        | MediaKind::Video
                        | MediaKind::Animation
                        | MediaKind::VideoNote
                ) && media.thumbnail_state != DownloadState::Downloading
                    && media
                        .thumbnail_path
                        .as_deref()
                        .is_none_or(|path| !path.is_file())
            }
        }
    }
}

fn set_download_state(media: &mut MediaSnapshot, target: DownloadTarget, state: DownloadState) {
    match target {
        DownloadTarget::Original => media.download_state = state,
        DownloadTarget::Thumbnail => media.thumbnail_state = state,
    }
}

fn set_download_path(media: &mut MediaSnapshot, target: DownloadTarget, path: PathBuf) {
    match target {
        DownloadTarget::Original => media.downloaded_path = Some(path),
        DownloadTarget::Thumbnail => media.thumbnail_path = Some(path),
    }
}

/// Upload failures are reported as [`std::io::Error`] with the RPC error as the
/// source, so recover a flood wait instead of always reporting a generic
/// failure.
fn upload_failure(error: &std::io::Error) -> EngineError {
    error
        .get_ref()
        .and_then(|source| source.downcast_ref::<InvocationError>())
        .map_or(EngineError::SendFailed, |invocation| {
            EngineError::from_invocation_with(invocation, EngineError::SendFailed)
        })
}

/// Clears cached media paths whose files disappeared, for example after the
/// operating system cleaned the cache.
fn forget_missing_cached_media(messages: &mut [MessageSnapshot]) {
    for message in messages {
        if let Some(media) = message.media.as_mut() {
            media.forget_missing_cache_files();
        }
    }
}

/// Builds the media snapshot shown for an attachment that is being sent, before
/// the server echoes the real message back.
fn optimistic_media(attachment: &StagedAttachment) -> MediaSnapshot {
    MediaSnapshot {
        kind: attachment.kind.media_kind(),
        file_name: Some(attachment.file_name.clone()),
        size: Some(attachment.size),
        downloaded_path: Some(attachment.path.clone()),
        download_state: DownloadState::Downloaded,
        ..Default::default()
    }
}

fn media_to_snapshot(media: &Media) -> MediaSnapshot {
    let mut snapshot = MediaSnapshot::default();
    match media {
        Media::Photo(photo) => {
            snapshot.kind = MediaKind::Photo;
            snapshot.size = photo.size().map(|size| size as u64);
            if let Some((width, height)) = photo
                .thumbs()
                .iter()
                .filter_map(|thumb| match thumb {
                    PhotoSize::Size(size) => Some((size.width, size.height)),
                    PhotoSize::Cached(size) => Some((size.width, size.height)),
                    PhotoSize::Progressive(size) => Some((size.width, size.height)),
                    _ => None,
                })
                .max_by_key(|(width, height)| width.saturating_mul(*height))
            {
                snapshot.width = Some(width);
                snapshot.height = Some(height);
            }
        }
        Media::Document(document) => {
            snapshot.size = document.size().map(|size| size as u64);
            snapshot.mime_type = document.mime_type().map(ToOwned::to_owned);
            snapshot.file_name = document.name().map(ToOwned::to_owned);
            snapshot.duration_seconds = document.duration();
            if let Some((width, height)) = document.resolution() {
                snapshot.width = Some(width);
                snapshot.height = Some(height);
            }
            snapshot.kind = document_kind(document);
            snapshot.waveform = document_waveform(document);
        }
        Media::Sticker(sticker) => {
            snapshot.kind = MediaKind::Sticker;
            snapshot.size = sticker.document.size().map(|size| size as u64);
            snapshot.mime_type = sticker.document.mime_type().map(ToOwned::to_owned);
            snapshot.file_name = sticker.document.name().map(ToOwned::to_owned);
            snapshot.duration_seconds = sticker.document.duration();
            if let Some((width, height)) = sticker.document.resolution() {
                snapshot.width = Some(width);
                snapshot.height = Some(height);
            }
        }
        Media::WebPage(page) => {
            snapshot.kind = MediaKind::WebPage;
            snapshot.webpage = webpage_snapshot(page);
        }
        Media::Contact(contact) => {
            snapshot.kind = MediaKind::Contact;
            snapshot.contact = Some(ContactSnapshot {
                first_name: contact.raw.first_name.clone(),
                last_name: contact.raw.last_name.clone(),
                phone: contact.raw.phone_number.clone(),
            });
        }
        Media::Poll(poll) => {
            snapshot.kind = MediaKind::Poll;
            snapshot.poll = poll_snapshot(poll);
        }
        Media::Geo(geo) => {
            snapshot.kind = MediaKind::Geo;
            snapshot.geo = Some(geo_point_snapshot(&geo.raw, None, None));
        }
        Media::Venue(venue) => {
            snapshot.kind = MediaKind::Geo;
            snapshot.geo = Some(geo_snapshot(
                &venue.raw_venue.geo,
                Some(venue.raw_venue.title.clone()),
                Some(venue.raw_venue.address.clone()),
            ));
        }
        Media::GeoLive(live) => {
            snapshot.kind = MediaKind::Geo;
            snapshot.geo = Some(GeoSnapshot {
                live: true,
                ..geo_snapshot(&live.raw_geolive.geo, None, None)
            });
        }
        _ => snapshot.kind = MediaKind::Other,
    }
    snapshot
}

fn apply_poll_results(poll: &mut PollSnapshot, results: &tl::types::PollResults) {
    poll.total_voters = results.total_voters.unwrap_or(poll.total_voters);
    let Some(results) = results.results.as_ref() else {
        return;
    };
    for (index, result) in results.iter().enumerate() {
        let tl::enums::PollAnswerVoters::Voters(result) = result;
        let Some(option) = poll.options.get_mut(index) else {
            continue;
        };
        option.voters = result.voters.unwrap_or(0);
        option.chosen = result.chosen;
        option.correct = result.correct;
        // Results normally repeat the opaque option bytes; the poll media is
        // authoritative when they do not.
        if !result.option.is_empty() {
            option.option = result.option.clone();
        }
    }
}

fn poll_snapshot(poll: &grammers_client::media::Poll) -> Option<PollSnapshot> {
    let raw = &poll.raw;
    let results: Vec<&tl::types::PollAnswerVoters> = match poll.raw_results.results.as_ref() {
        Some(results) => results
            .iter()
            .map(|voters| match voters {
                tl::enums::PollAnswerVoters::Voters(voters) => voters,
            })
            .collect(),
        None => Vec::new(),
    };
    let options = raw
        .answers
        .iter()
        .enumerate()
        .map(|(index, answer)| {
            let result = results.get(index);
            let answer_option = match answer {
                tl::enums::PollAnswer::Answer(answer) => Some(answer.option.clone()),
                tl::enums::PollAnswer::InputPollAnswer(_) => None,
            };
            PollOptionSnapshot {
                text: text_with_entities(&answer.text()),
                voters: result.and_then(|result| result.voters).unwrap_or(0),
                chosen: result.is_some_and(|result| result.chosen),
                correct: result.is_some_and(|result| result.correct),
                option: answer_option
                    .filter(|option| !option.is_empty())
                    .or_else(|| result.map(|result| result.option.clone()))
                    .unwrap_or_default(),
            }
        })
        .collect();
    Some(PollSnapshot {
        id: raw.id,
        question: text_with_entities(&raw.question),
        options,
        closed: raw.closed,
        multiple_choice: raw.multiple_choice,
        quiz: raw.quiz,
        total_voters: poll.raw_results.total_voters.unwrap_or(0),
    })
}

fn text_with_entities(text: &tl::enums::TextWithEntities) -> String {
    match text {
        tl::enums::TextWithEntities::Entities(text) => text.text.clone(),
    }
}

fn geo_snapshot(
    point: &tl::enums::GeoPoint,
    title: Option<String>,
    address: Option<String>,
) -> GeoSnapshot {
    match point {
        tl::enums::GeoPoint::Point(point) => geo_point_snapshot(point, title, address),
        tl::enums::GeoPoint::Empty => GeoSnapshot {
            title,
            address,
            ..Default::default()
        },
    }
}

fn geo_point_snapshot(
    point: &tl::types::GeoPoint,
    title: Option<String>,
    address: Option<String>,
) -> GeoSnapshot {
    GeoSnapshot {
        latitude: point.lat,
        longitude: point.long,
        title,
        address,
        live: false,
    }
}

fn document_kind(document: &grammers_client::media::Document) -> MediaKind {
    if let Some(tl::enums::Document::Document(raw)) = document.raw.document.as_ref() {
        let mut animated = false;
        for attribute in &raw.attributes {
            match attribute {
                tl::enums::DocumentAttribute::Audio(audio) if audio.voice => {
                    return MediaKind::Voice;
                }
                tl::enums::DocumentAttribute::Audio(_) => return MediaKind::Audio,
                tl::enums::DocumentAttribute::Video(video) if video.round_message => {
                    return MediaKind::VideoNote;
                }
                tl::enums::DocumentAttribute::Video(_) => return MediaKind::Video,
                tl::enums::DocumentAttribute::Sticker(_) => return MediaKind::Sticker,
                tl::enums::DocumentAttribute::Animated => animated = true,
                _ => {}
            }
        }
        if animated {
            return MediaKind::Animation;
        }
        if raw.mime_type.starts_with("image/") {
            return MediaKind::Photo;
        }
    }
    MediaKind::Document
}

/// The packed voice waveform from a document's audio attribute, if any. The
/// panel renders it directly, so no decoding or analysis is needed.
fn document_waveform(document: &grammers_client::media::Document) -> Option<Vec<u8>> {
    if let Some(tl::enums::Document::Document(raw)) = document.raw.document.as_ref() {
        for attribute in &raw.attributes {
            if let tl::enums::DocumentAttribute::Audio(audio) = attribute
                && audio.voice
            {
                return audio.waveform.clone();
            }
        }
    }
    None
}

fn webpage_snapshot(page: &grammers_client::media::WebPage) -> Option<WebPageSnapshot> {
    match &page.raw.webpage {
        tl::enums::WebPage::Page(raw) => Some(WebPageSnapshot {
            url: raw.url.clone(),
            title: raw.title.clone(),
            description: raw.description.clone(),
        }),
        _ => None,
    }
}

/// Picks the cheapest usable thumbnail for an image-like attachment. Embedded
/// thumbnails are preferred because they need no network request; otherwise the
/// smallest server-side size is used.
fn select_thumbnail(media: &Media) -> Option<PhotoSize> {
    let thumbs = match media {
        Media::Photo(photo) => photo.thumbs(),
        Media::Document(document) => document.thumbs(),
        Media::Sticker(sticker) => sticker.document.thumbs(),
        _ => return None,
    };

    if let Some(thumbnail) = thumbs
        .iter()
        .filter(|thumbnail| matches!(thumbnail, PhotoSize::Cached(_) | PhotoSize::Stripped(_)))
        .max_by_key(|thumbnail| thumbnail.size())
    {
        return Some(thumbnail.clone());
    }

    thumbs
        .iter()
        .filter(|thumbnail| matches!(thumbnail, PhotoSize::Size(_)))
        .min_by_key(|thumbnail| thumbnail.size())
        .cloned()
}

/// Loads the full dialog list together with peer refs, group migrations,
/// custom folders, folder membership, and the archive.
async fn fetch_dialogs(client: &Client) -> Result<LoadedChatList, EngineError> {
    let mut dialogs = client.iter_dialogs();
    let mut entries = Vec::new();
    let mut folder_membership = HashMap::new();
    loop {
        match dialogs.next().await {
            Ok(Some(dialog)) => {
                let Some(snapshot) = dialog_to_snapshot(&dialog) else {
                    continue;
                };
                folder_membership.insert(snapshot.id, dialog_folder(&dialog));
                entries.push(LoadedDialog {
                    snapshot,
                    peer_ref: dialog.peer_ref(),
                    migrated_to_chat_id: dialog_migration_target(&dialog),
                });
            }
            Ok(None) => break,
            Err(error) => {
                log::warn!("failed to load Telegram chats: {error}");
                return Err(EngineError::from_invocation_with(
                    &error,
                    EngineError::LoadChatsFailed,
                ));
            }
        }
    }

    // Archived chats are excluded from the default dialog list, so they need a
    // separate request. A failure here must not hide the normal chat list.
    match fetch_archived_dialogs(client).await {
        Ok(archived) => {
            for entry in archived {
                folder_membership.entry(entry.snapshot.id).or_insert(None);
                entries.push(entry);
            }
        }
        Err(error) => log::warn!("failed to load archived Telegram chats: {error:?}"),
    }

    let folders = match fetch_folders(client).await {
        Ok(folders) => folders,
        Err(error) => {
            log::warn!("failed to load Telegram chat folders: {error:?}");
            Vec::new()
        }
    };

    Ok(normalize_loaded_dialogs(
        entries,
        folders,
        folder_membership,
    ))
}

/// Fetches the chat folders the account has configured.
async fn fetch_folders(client: &Client) -> Result<Vec<ChatFolderSnapshot>, EngineError> {
    let result = client
        .invoke(&tl::functions::messages::GetDialogFilters {})
        .await
        .map_err(|error| {
            log::warn!("failed to load Telegram chat folders: {error}");
            EngineError::from_invocation_with(&error, EngineError::LoadChatsFailed)
        })?;
    let tl::enums::messages::DialogFilters::Filters(result) = result;
    Ok(result
        .filters
        .into_iter()
        .filter_map(|filter| match filter {
            tl::enums::DialogFilter::Filter(filter) => Some(ChatFolderSnapshot {
                id: filter.id,
                title: text_with_entities(&filter.title),
            }),
            _ => None,
        })
        .collect())
}

/// Fetches the archive with raw `messages.getDialogs` requests, because the
/// high-level iterator only walks the default folder.
async fn fetch_archived_dialogs(client: &Client) -> Result<Vec<LoadedDialog>, EngineError> {
    let mut entries = Vec::new();
    let mut offset: Option<(i32, i32, tl::enums::InputPeer)> = None;
    for page in 0..ARCHIVE_PAGE_LIMIT {
        let request = tl::functions::messages::GetDialogs {
            exclude_pinned: page > 0,
            folder_id: Some(ARCHIVE_FOLDER_ID),
            offset_date: offset.as_ref().map_or(0, |(_, date, _)| *date),
            offset_id: offset.as_ref().map_or(0, |(id, _, _)| *id),
            offset_peer: offset
                .as_ref()
                .map_or(tl::enums::InputPeer::Empty, |(_, _, peer)| peer.clone()),
            limit: 100,
            hash: 0,
        };
        let (dialogs, messages, users, chats, count) = match client.invoke(&request).await {
            Ok(result) => match result {
                tl::enums::messages::Dialogs::Dialogs(dialogs) => (
                    dialogs.dialogs,
                    dialogs.messages,
                    dialogs.users,
                    dialogs.chats,
                    None,
                ),
                tl::enums::messages::Dialogs::Slice(dialogs) => (
                    dialogs.dialogs,
                    dialogs.messages,
                    dialogs.users,
                    dialogs.chats,
                    Some(dialogs.count),
                ),
                tl::enums::messages::Dialogs::NotModified(_) => break,
            },
            Err(error) => {
                return Err(EngineError::from_invocation_with(
                    &error,
                    EngineError::LoadChatsFailed,
                ));
            }
        };

        let mut next_offset = None;
        for dialog in dialogs {
            let tl::enums::Dialog::Dialog(raw) = &dialog else {
                continue;
            };
            let Some(peer_ref) = peer_ref_from_raw(&raw.peer, &users, &chats) else {
                continue;
            };
            let Some(snapshot) = archived_dialog_snapshot(raw, &messages, &users, &chats) else {
                continue;
            };
            next_offset = Some((
                raw.top_message,
                raw_message_date(&messages, &raw.peer, raw.top_message).unwrap_or_default() as i32,
                tl::enums::InputPeer::from(peer_ref),
            ));
            entries.push(LoadedDialog {
                snapshot,
                peer_ref,
                migrated_to_chat_id: None,
            });
        }

        let exhausted = count.is_none_or(|count| entries.len() >= count as usize);
        match next_offset {
            Some(next_offset) if !exhausted => offset = Some(next_offset),
            _ => break,
        }
    }
    Ok(entries)
}

/// Builds a snapshot for an archived dialog from raw `messages.getDialogs`
/// data, which has no `Dialog` wrapper.
fn archived_dialog_snapshot(
    raw: &tl::types::Dialog,
    messages: &[tl::enums::Message],
    users: &[tl::enums::User],
    chats: &[tl::enums::Chat],
) -> Option<ChatSnapshot> {
    let id = PeerId::from(&raw.peer).bot_api_dialog_id()?;
    let display = raw_peer_display(&raw.peer, users, chats)?;
    let title = display.title;
    let (preview, preview_media) =
        raw_message_preview(messages, &raw.peer, raw.top_message).unwrap_or_default();
    let timestamp_unix = raw_message_date(messages, &raw.peer, raw.top_message).unwrap_or_default();
    let muted = match &raw.notify_settings {
        tl::enums::PeerNotifySettings::Settings(settings) => is_muted(settings.mute_until),
    };
    let draft = match raw.draft.as_ref() {
        Some(tl::enums::DraftMessage::Message(draft)) if !draft.message.is_empty() => {
            Some(draft.message.clone())
        }
        _ => None,
    };
    Some(ChatSnapshot {
        id,
        title: title.clone(),
        username: display.username,
        kind: display.kind,
        preview,
        is_forum: display.is_forum,
        preview_media,
        timestamp_unix,
        unread_count: raw.unread_count.max(0),
        unread_mentions: raw.unread_mentions_count.max(0),
        pinned: raw.pinned,
        muted,
        archived: true,
        can_send: display.can_send,
        slow_mode_seconds: 0,
        read_inbox_message_id: raw.read_inbox_max_id,
        read_outbox_message_id: raw.read_outbox_max_id,
        avatar_initials: avatar_initials(&title),
        draft,
        typing: None,
        pinned_message: None,
        presence: Presence::Unknown,
    })
}

struct RawPeerDisplay {
    title: String,
    username: Option<String>,
    kind: ChatKind,
    can_send: bool,
    is_forum: bool,
}

fn raw_peer_display(
    peer: &tl::enums::Peer,
    users: &[tl::enums::User],
    chats: &[tl::enums::Chat],
) -> Option<RawPeerDisplay> {
    match peer {
        tl::enums::Peer::User(peer) => {
            let user = users.iter().find_map(|user| match user {
                tl::enums::User::User(user) if user.id == peer.user_id => Some(user),
                _ => None,
            })?;
            let name = format!(
                "{} {}",
                user.first_name.as_deref().unwrap_or_default(),
                user.last_name.as_deref().unwrap_or_default()
            );
            let name = name.trim().to_owned();
            Some(RawPeerDisplay {
                title: if name.is_empty() {
                    user.username
                        .clone()
                        .unwrap_or_else(|| "Unknown".to_owned())
                } else {
                    name
                },
                username: user.username.clone(),
                kind: ChatKind::User,
                can_send: true,
                is_forum: false,
            })
        }
        tl::enums::Peer::Chat(peer) => chats.iter().find_map(|chat| match chat {
            tl::enums::Chat::Chat(chat) if chat.id == peer.chat_id => Some(RawPeerDisplay {
                title: chat.title.clone(),
                username: None,
                kind: ChatKind::Group,
                can_send: !chat.left && !banned_from_sending(chat.default_banned_rights.as_ref()),
                is_forum: false,
            }),
            tl::enums::Chat::Forbidden(chat) if chat.id == peer.chat_id => Some(RawPeerDisplay {
                title: chat.title.clone(),
                username: None,
                kind: ChatKind::Group,
                can_send: false,
                is_forum: false,
            }),
            _ => None,
        }),
        tl::enums::Peer::Channel(peer) => {
            if let Some(chat) = chats.iter().find_map(|chat| match chat {
                tl::enums::Chat::Channel(chat) if chat.id == peer.channel_id => Some(chat),
                _ => None,
            }) {
                return Some(RawPeerDisplay {
                    title: chat.title.clone(),
                    username: chat.username.clone(),
                    kind: ChatKind::Channel,
                    can_send: !chat.left
                        && (!chat.broadcast || chat.creator || chat.admin_rights.is_some())
                        && !banned_from_sending(chat.banned_rights.as_ref()),
                    is_forum: chat.forum,
                });
            }
            chats.iter().find_map(|chat| match chat {
                tl::enums::Chat::ChannelForbidden(chat) if chat.id == peer.channel_id => {
                    Some(RawPeerDisplay {
                        title: chat.title.clone(),
                        username: None,
                        kind: ChatKind::Channel,
                        can_send: false,
                        is_forum: false,
                    })
                }
                _ => None,
            })
        }
    }
}

fn peer_ref_from_raw(
    peer: &tl::enums::Peer,
    users: &[tl::enums::User],
    chats: &[tl::enums::Chat],
) -> Option<PeerRef> {
    let id = PeerId::from(peer);
    let auth = match peer {
        tl::enums::Peer::User(peer) => users
            .iter()
            .find_map(|user| match user {
                tl::enums::User::User(user) if user.id == peer.user_id && !user.min => {
                    user.access_hash
                }
                _ => None,
            })
            .map_or_default(PeerAuth::from_hash),
        tl::enums::Peer::Channel(peer) => chats
            .iter()
            .find_map(|chat| match chat {
                tl::enums::Chat::Channel(chat) if chat.id == peer.channel_id => chat.access_hash,
                _ => None,
            })
            .map_or_default(PeerAuth::from_hash),
        tl::enums::Peer::Chat(_) => PeerAuth::default(),
    };
    Some(PeerRef { id, auth })
}

fn raw_message_preview(
    messages: &[tl::enums::Message],
    peer: &tl::enums::Peer,
    top_message: i32,
) -> Option<(String, Option<MediaKind>)> {
    let message = messages.iter().find(|message| {
        message.id() == top_message
            && raw_message_peer(message)
                .is_some_and(|message_peer| PeerId::from(message_peer) == PeerId::from(peer))
    })?;
    match message {
        tl::enums::Message::Message(message) => {
            let media = message.media.as_ref().map(raw_media_kind);
            if message.message.is_empty() {
                Some((String::new(), media))
            } else {
                Some((message.message.replace('\n', " "), media))
            }
        }
        _ => Some((String::new(), None)),
    }
}

fn raw_message_peer(message: &tl::enums::Message) -> Option<&tl::enums::Peer> {
    match message {
        tl::enums::Message::Message(message) => Some(&message.peer_id),
        tl::enums::Message::Service(message) => Some(&message.peer_id),
        tl::enums::Message::Empty(_) => None,
    }
}

fn raw_message_date(
    messages: &[tl::enums::Message],
    peer: &tl::enums::Peer,
    top_message: i32,
) -> Option<i64> {
    messages.iter().find_map(|message| match message {
        tl::enums::Message::Message(message)
            if message.id == top_message
                && PeerId::from(&message.peer_id) == PeerId::from(peer) =>
        {
            Some(i64::from(message.date))
        }
        tl::enums::Message::Service(message)
            if message.id == top_message
                && PeerId::from(&message.peer_id) == PeerId::from(peer) =>
        {
            Some(i64::from(message.date))
        }
        _ => None,
    })
}

fn raw_media_kind(media: &tl::enums::MessageMedia) -> MediaKind {
    match media {
        tl::enums::MessageMedia::Photo(_) => MediaKind::Photo,
        tl::enums::MessageMedia::Contact(_) => MediaKind::Contact,
        tl::enums::MessageMedia::Poll(_) => MediaKind::Poll,
        tl::enums::MessageMedia::Geo(_)
        | tl::enums::MessageMedia::Venue(_)
        | tl::enums::MessageMedia::GeoLive(_) => MediaKind::Geo,
        tl::enums::MessageMedia::WebPage(_) => MediaKind::WebPage,
        tl::enums::MessageMedia::Document(document) => document
            .document
            .as_ref()
            .map_or(MediaKind::Document, raw_document_kind),
        _ => MediaKind::Other,
    }
}

fn raw_document_kind(document: &tl::enums::Document) -> MediaKind {
    let tl::enums::Document::Document(document) = document else {
        return MediaKind::Document;
    };
    let mut animated = false;
    for attribute in &document.attributes {
        match attribute {
            tl::enums::DocumentAttribute::Audio(audio) if audio.voice => {
                return MediaKind::Voice;
            }
            tl::enums::DocumentAttribute::Audio(_) => return MediaKind::Audio,
            tl::enums::DocumentAttribute::Video(video) if video.round_message => {
                return MediaKind::VideoNote;
            }
            tl::enums::DocumentAttribute::Video(_) => return MediaKind::Video,
            tl::enums::DocumentAttribute::Sticker(_) => return MediaKind::Sticker,
            tl::enums::DocumentAttribute::Animated => animated = true,
            _ => {}
        }
    }
    if animated {
        MediaKind::Animation
    } else if document.mime_type.starts_with("image/") {
        MediaKind::Photo
    } else {
        MediaKind::Document
    }
}

fn peer_bot_api_id(peer: &tl::enums::Peer) -> Option<i64> {
    PeerId::from(peer).bot_api_dialog_id()
}

fn messages_filter(filter: SearchFilter) -> tl::enums::MessagesFilter {
    match filter {
        SearchFilter::All | SearchFilter::Text => {
            tl::enums::MessagesFilter::InputMessagesFilterEmpty
        }
        SearchFilter::Media => tl::enums::MessagesFilter::InputMessagesFilterPhotoVideo,
        SearchFilter::Links => tl::enums::MessagesFilter::InputMessagesFilterUrl,
        SearchFilter::Files => tl::enums::MessagesFilter::InputMessagesFilterDocument,
    }
}

/// `InputMessagesFilterEmpty` cannot express "text only", so that filter is
/// applied to the returned messages.
fn search_filter_matches(filter: SearchFilter, message: &Message) -> bool {
    match filter {
        SearchFilter::Text => !message.text().trim().is_empty(),
        _ => true,
    }
}

fn search_next_offset(hits: &[SearchHit]) -> Option<i32> {
    let oldest = hits
        .iter()
        .filter_map(|hit| hit.message.as_ref())
        .map(|message| message.id)
        .min()?;
    (hits.len() >= SEARCH_LIMIT).then_some(oldest)
}

/// Performs one page of a server-side message search.
async fn fetch_search(
    client: &Client,
    scoped_peer: Option<PeerRef>,
    request: &SearchRequest,
    titles: &HashMap<i64, String>,
    migrated_to: &HashMap<i64, i64>,
) -> Result<Vec<SearchHit>, EngineError> {
    let filter = messages_filter(request.filter);
    let messages = if let Some(peer_ref) = scoped_peer {
        let mut iterator = client
            .search_messages(peer_ref)
            .query(&request.query)
            .filter(filter)
            .limit(SEARCH_LIMIT);
        if let Some(offset_id) = request.offset_id {
            iterator = iterator.offset_id(offset_id);
        }
        collect_search_messages(SearchSource::Chat(iterator)).await?
    } else {
        let mut iterator = client
            .search_all_messages()
            .query(&request.query)
            .filter(filter)
            .limit(SEARCH_LIMIT);
        if let Some(offset_id) = request.offset_id {
            iterator = iterator.offset_id(offset_id);
        }
        collect_search_messages(SearchSource::Global(iterator)).await?
    };

    let mut found: Vec<Message> = messages
        .into_iter()
        .filter(|message| search_filter_matches(request.filter, message))
        .collect();
    found.sort_by_key(|message| std::cmp::Reverse(message.date().as_second()));

    Ok(found
        .into_iter()
        .filter_map(|message| {
            let source_chat_id = message.peer_id().bot_api_dialog_id()?;
            let chat_id = migrated_to
                .get(&source_chat_id)
                .copied()
                .unwrap_or(source_chat_id);
            let chat_title = titles
                .get(&chat_id)
                .cloned()
                .unwrap_or_else(|| chat_id.to_string());
            Some(SearchHit {
                chat_id,
                chat_title,
                message: Some(message_to_snapshot(&message, source_chat_id)),
            })
        })
        .collect())
}

/// Uploads the attachment and sends the message on the engine's behalf. Runs on
/// a background task, so a large upload never blocks incoming updates.
async fn perform_send(
    client: &Client,
    peer_ref: PeerRef,
    chat_id: i64,
    item: OutgoingItem,
    reply_to: Option<i32>,
    topic_id: Option<i32>,
) -> Result<Vec<MessageSnapshot>, EngineError> {
    // A topic post without an explicit reply is routed by replying to the
    // topic's top message.
    let reply_to = reply_to.or(topic_id);
    match item {
        OutgoingItem::Text(text) => {
            let text = text.trim_end().to_owned();
            if text.is_empty() {
                return Ok(Vec::new());
            }
            client
                .send_message(peer_ref, markdown_message(&text).reply_to(reply_to))
                .await
                .map(|message| vec![message_to_snapshot(&message, chat_id)])
                .map_err(|error| {
                    log::warn!("failed to send a Telegram message: {error}");
                    EngineError::from_invocation_with(&error, EngineError::SendFailed)
                })
        }
        OutgoingItem::Media {
            attachment,
            caption,
        } => {
            let uploaded = client
                .upload_file(&attachment.path)
                .await
                .map_err(|error| upload_failure(&error))?;
            let mut message = match caption.as_deref() {
                Some(caption) if !caption.is_empty() => markdown_message(caption),
                _ => InputMessage::new(),
            };
            message = message.reply_to(reply_to);
            message = match attachment.kind {
                AttachmentKind::Photo => message.photo(uploaded),
                AttachmentKind::Video | AttachmentKind::Document => message.document(uploaded),
            };
            client
                .send_message(peer_ref, message)
                .await
                .map(|message| vec![message_to_snapshot(&message, chat_id)])
                .map_err(|error| {
                    log::warn!("failed to send a Telegram attachment: {error}");
                    EngineError::from_invocation_with(&error, EngineError::SendFailed)
                })
        }
        OutgoingItem::Album { items } => {
            let mut medias = Vec::with_capacity(items.len());
            for item in &items {
                let uploaded = client
                    .upload_file(&item.attachment.path)
                    .await
                    .map_err(|error| upload_failure(&error))?;
                let mut input = match item.caption.as_deref() {
                    Some(caption) if !caption.is_empty() => markdown_caption(caption),
                    _ => InputMedia::new(),
                };
                input = match item.attachment.kind {
                    AttachmentKind::Photo => input.photo(uploaded),
                    AttachmentKind::Video | AttachmentKind::Document => input.document(uploaded),
                };
                medias.push(input);
            }
            // `send_album` derives the reply target from the first media item.
            if let Some(reply_to) = reply_to
                && let Some(first) = medias.first_mut()
            {
                *first = std::mem::take(first).reply_to(Some(reply_to));
            }
            client
                .send_album(peer_ref, medias)
                .await
                .map(|messages| {
                    messages
                        .into_iter()
                        .flatten()
                        .map(|message| message_to_snapshot(&message, chat_id))
                        .collect()
                })
                .map_err(|error| {
                    log::warn!("failed to send a Telegram album: {error}");
                    EngineError::from_invocation_with(&error, EngineError::SendFailed)
                })
        }
    }
}

/// Unifies grammers' two search iterators, which share a `next` method but no
/// common trait.
enum SearchSource {
    Chat(SearchIter),
    Global(GlobalSearchIter),
}

impl SearchSource {
    async fn next(&mut self) -> Result<Option<Message>, InvocationError> {
        match self {
            Self::Chat(iterator) => iterator.next().await,
            Self::Global(iterator) => iterator.next().await,
        }
    }
}

/// Drains a search iterator into a message list, mapping failures to
/// [`EngineError::SearchFailed`].
async fn collect_search_messages(mut source: SearchSource) -> Result<Vec<Message>, EngineError> {
    let mut found = Vec::with_capacity(SEARCH_LIMIT);
    loop {
        match source.next().await {
            Ok(Some(message)) => found.push(message),
            Ok(None) => break,
            Err(error) => {
                log::warn!("Telegram search failed: {error}");
                return Err(EngineError::from_invocation_with(
                    &error,
                    EngineError::SearchFailed,
                ));
            }
        }
    }
    Ok(found)
}

/// Fetches the messages between two known ids, oldest-first.
async fn fetch_gap(
    client: &Client,
    peer_ref: PeerRef,
    chat_id: i64,
    after_id: i32,
    before_id: i32,
) -> Vec<MessageSnapshot> {
    let mut iterator = client
        .iter_messages(peer_ref)
        .offset_id(before_id)
        .limit(HISTORY_PAGE_SIZE);
    let mut page = Vec::new();
    loop {
        match iterator.next().await {
            Ok(Some(message)) => {
                if message.id() <= after_id {
                    break;
                }
                page.push(message_to_snapshot(&message, chat_id));
            }
            Ok(None) => break,
            Err(error) => {
                log::warn!("failed to fill a Telegram history gap: {error}");
                break;
            }
        }
    }
    page.reverse();
    page
}

/// Whether a media download failed because Telegram expired the media's file
/// reference. grammers 0.11 does not refresh references itself (its
/// `DownloadIter` carries a `TODO handle FILEREF_UPGRADE_NEEDED`), so the
/// engine must refetch the message and try again.
///
/// Small files go through `Client::load`, which wraps the `InvocationError` in
/// an `io::Error` via `io::Error::other`; large files use the concurrent
/// downloader and return the `InvocationError` directly. Cover both shapes.
/// <https://core.telegram.org/api/file-references>
fn is_expired_file_reference(error: &InvocationError) -> bool {
    // Every `FILE_REFERENCE_*` error (expired, invalid, empty, or the indexed
    // `FILE_REFERENCE_%d_EXPIRED` form) means the same thing: refetch the source
    // message to mint a fresh reference. grammers' `RpcError::from` already
    // strips the index digit into `value`, so a single prefix check covers all
    // of them. The retry is bounded, so being permissive here is harmless.
    fn is_expired_rpc(rpc: &grammers_client::sender::RpcError) -> bool {
        rpc.name.starts_with("FILE_REFERENCE_")
    }

    match error {
        InvocationError::Rpc(rpc) => is_expired_rpc(rpc),
        InvocationError::Io(io) => io
            .get_ref()
            .and_then(|source| source.downcast_ref::<InvocationError>())
            .is_some_and(
                |invocation| matches!(invocation, InvocationError::Rpc(rpc) if is_expired_rpc(rpc)),
            ),
        _ => false,
    }
}

/// Removes a failed download's target. `Client::load` creates the file before
/// the first `upload.getFile` request, so a failed download can leave a zero or
/// partial file that `download()` would otherwise treat as a cache hit.
fn remove_partial_download(path: &Path) {
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        log::debug!("failed to remove a partial Telegram download: {error}");
    }
}

/// The `file_reference` bytes a media location carries, so a refetch that mints
/// a new reference can be told apart from one that returns the same one.
fn location_file_reference(location: Option<tl::enums::InputFileLocation>) -> Option<Vec<u8>> {
    match location? {
        tl::enums::InputFileLocation::InputPhotoFileLocation(location) => {
            Some(location.file_reference)
        }
        tl::enums::InputFileLocation::InputDocumentFileLocation(location) => {
            Some(location.file_reference)
        }
        _ => None,
    }
}

/// Whether the refetched location carries the reference that just failed.
/// Telegram Desktop stops at this point instead of repeating the request, and
/// so does the engine: a second attempt with identical bytes is rejected the
/// same way.
fn kept_the_same_reference(reference: Option<&[u8]>, rejected: Option<&[u8]>) -> bool {
    reference.is_some() && reference == rejected
}

/// Downloads a message's original media to `path`, creating parent directories.
async fn download_media_file(
    client: &Client,
    peer_ref: PeerRef,
    message_id: i32,
    path: PathBuf,
) -> Result<Option<PathBuf>, EngineError> {
    if let Some(parent) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            log::warn!("failed to create the Telegram media directory: {error}");
            return Err(EngineError::Other);
        }
    }
    let mut attempt = 0;
    let mut rejected_reference: Option<Vec<u8>> = None;
    loop {
        // `get_messages_by_id` always hits the network, so each attempt starts
        // with a freshly minted file reference for the media.
        let message = client
            .get_messages_by_id(peer_ref, &[message_id])
            .await
            .map_err(|error| {
                log::warn!("failed to fetch a Telegram message for download: {error}");
                EngineError::from_invocation(&error)
            })?
            .pop()
            .flatten();
        let Some(message) = message else {
            return Ok(None);
        };
        let reference = location_file_reference(
            message
                .media()
                .as_ref()
                .and_then(|media| media.to_raw_input_location()),
        );
        if kept_the_same_reference(reference.as_deref(), rejected_reference.as_deref()) {
            log::warn!(
                "Telegram kept rejecting the same file reference for message {message_id}; the media is unavailable"
            );
            return Err(EngineError::MediaUnavailable);
        }
        match message.download_media(&path).await {
            Ok(true) => {
                evict_cache_async(path.parent().and_then(Path::parent)).await;
                return Ok(Some(path));
            }
            Ok(false) => return Ok(None),
            Err(error) if is_expired_file_reference(&error) => {
                remove_partial_download(&path);
                if attempt >= MEDIA_FILE_REFERENCE_RETRIES {
                    log::warn!("failed to download Telegram media: {error}");
                    return Err(EngineError::MediaUnavailable);
                }
                log::info!(
                    "Telegram file reference expired for message {message_id}, refetching the message"
                );
                rejected_reference = reference;
                attempt += 1;
            }
            Err(error) => {
                log::warn!("failed to download Telegram media: {error}");
                remove_partial_download(&path);
                return Err(EngineError::from_invocation(&error));
            }
        }
    }
}

/// Fetches a thumbnail's bytes and writes them under the thumbnail cache root.
async fn download_thumbnail_file(
    client: &Client,
    peer_ref: PeerRef,
    message_id: i32,
    path: PathBuf,
) -> Result<Option<PathBuf>, EngineError> {
    let Some(bytes) = fetch_thumbnail_bytes(client, peer_ref, message_id).await? else {
        return Ok(None);
    };
    let cached = tokio::task::spawn_blocking(move || {
        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                log::warn!("failed to create the Telegram thumbnail directory: {error}");
                return Err(EngineError::Other);
            }
        }
        if let Err(error) = std::fs::write(&path, &bytes) {
            log::warn!("failed to cache a Telegram thumbnail: {error}");
            return Err(EngineError::Other);
        }
        evict_cache(path.parent().and_then(Path::parent));
        Ok(Some(path))
    })
    .await;
    match cached {
        Ok(result) => result,
        Err(error) => {
            log::warn!("the Telegram thumbnail cache task was cancelled: {error}");
            Err(EngineError::Other)
        }
    }
}

/// Downloads the bytes of a message's thumbnail, either from the embedded data
/// or with a single small file request.
async fn fetch_thumbnail_bytes(
    client: &Client,
    peer_ref: PeerRef,
    message_id: i32,
) -> Result<Option<Vec<u8>>, EngineError> {
    let mut attempt = 0;
    let mut rejected_reference: Option<Vec<u8>> = None;
    loop {
        // `get_messages_by_id` always hits the network, so each attempt starts
        // with a freshly minted file reference for the thumbnail.
        let message = client
            .get_messages_by_id(peer_ref, &[message_id])
            .await
            .map_err(|error| {
                log::warn!("failed to fetch a Telegram message for its thumbnail: {error}");
                EngineError::from_invocation(&error)
            })?
            .pop()
            .flatten();
        let Some(message) = message else {
            return Ok(None);
        };
        let Some(media) = message.media() else {
            return Ok(None);
        };
        let Some(thumbnail) = select_thumbnail(&media) else {
            return Ok(None);
        };
        let reference = location_file_reference(thumbnail.to_raw_input_location());
        if kept_the_same_reference(reference.as_deref(), rejected_reference.as_deref()) {
            log::warn!(
                "Telegram kept rejecting the same thumbnail file reference for message {message_id}; the thumbnail is unavailable"
            );
            return Err(EngineError::MediaUnavailable);
        }

        let mut download = client.iter_download(&thumbnail);
        let mut bytes = Vec::new();
        let result = loop {
            match download.next().await {
                Ok(Some(chunk)) => bytes.extend_from_slice(&chunk),
                Ok(None) => break Ok(()),
                Err(error) => break Err(error),
            }
        };
        match result {
            Ok(()) => return Ok((!bytes.is_empty()).then_some(bytes)),
            Err(error) if is_expired_file_reference(&error) => {
                if attempt >= MEDIA_FILE_REFERENCE_RETRIES {
                    log::warn!("failed to download a Telegram thumbnail: {error}");
                    return Err(EngineError::MediaUnavailable);
                }
                log::info!(
                    "Telegram thumbnail file reference expired for message {message_id}, refetching the message"
                );
                rejected_reference = reference;
                attempt += 1;
            }
            Err(error) => {
                log::warn!("failed to download a Telegram thumbnail: {error}");
                return Err(EngineError::from_invocation(&error));
            }
        }
    }
}

/// The file extension for downloadable media, derived from the document name
/// first, then the MIME type, then the media kind. Photos and stickers have no
/// file name, so the kind fallback keeps them openable instead of `.bin`.
fn media_extension(media: &MediaSnapshot) -> String {
    if let Some(extension) = media.file_name.as_deref().and_then(file_extension) {
        return extension;
    }
    if let Some(mime) = media.mime_type.as_deref() {
        if let Some(extension) = mime_extension(mime, media.kind) {
            return extension;
        }
    }
    match media.kind {
        MediaKind::Photo => "jpg".to_owned(),
        MediaKind::Sticker => "webp".to_owned(),
        MediaKind::Voice => "ogg".to_owned(),
        MediaKind::Video => "mp4".to_owned(),
        MediaKind::Audio => "mp3".to_owned(),
        _ => "bin".to_owned(),
    }
}

/// Telegram thumbnails are always JPEG, so the extension is fixed.
fn thumbnail_extension() -> String {
    "jpg".to_owned()
}

/// Builds the sharded on-disk path for a downloaded attachment.
fn media_file_path(root: &Path, chat_id: i64, message_id: i32, media: &MediaSnapshot) -> PathBuf {
    root.join(chat_id.to_string())
        .join(format!("{message_id}.{}", media_extension(media)))
}

/// Builds the sharded on-disk path for a cached thumbnail.
fn thumbnail_file_path(root: &Path, chat_id: i64, message_id: i32) -> PathBuf {
    root.join(chat_id.to_string())
        .join(format!("{message_id}.{}", thumbnail_extension()))
}

fn file_extension(name: &str) -> Option<String> {
    sanitize_extension(Path::new(name).extension()?.to_str()?)
}

fn mime_extension(mime: &str, kind: MediaKind) -> Option<String> {
    // Only the subtype between the first two slashes is used, so parameters and
    // a trailing slash cannot introduce more path segments.
    let subtype = mime.split(';').next()?.split('/').nth(1)?.trim();
    let mapped = match subtype {
        "jpeg" => "jpg",
        "svg+xml" => "svg",
        "quicktime" => "mov",
        "x-matroska" => "mkv",
        "x-7z-compressed" => "7z",
        "x-rar-compressed" => "rar",
        "x-tar" => "tar",
        "x-gzip" => "gz",
        "x-tgsticker" => "tgs",
        "mpeg" if kind == MediaKind::Video => "mpg",
        "mpeg" => "mp3",
        "plain" => "txt",
        other => other,
    };
    sanitize_extension(mapped)
}

/// Reduces an extension to ASCII alphanumerics, so a peer-controlled file name
/// or MIME type can never introduce a path separator, a `..` segment, or a
/// second extension. Overlong values are rejected and fall back to the kind.
fn sanitize_extension(raw: &str) -> Option<String> {
    const MAX_EXTENSION_LENGTH: usize = 8;
    let sanitized: String = raw
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect();
    (!sanitized.is_empty() && sanitized.len() <= MAX_EXTENSION_LENGTH)
        .then(|| sanitized.to_ascii_lowercase())
}

/// Removes cache files older than [`MEDIA_CACHE_MAX_AGE`], then the oldest
/// files until the cache is under [`MEDIA_CACHE_MAX_BYTES`]. A missing root is
/// not an error. The session lives elsewhere and is never touched.
fn evict_cache(root: Option<&Path>) {
    evict_cache_with(root, MEDIA_CACHE_MAX_BYTES, MEDIA_CACHE_MAX_AGE);
}

async fn evict_cache_async(root: Option<&Path>) {
    let root = root.map(Path::to_path_buf);
    if let Err(error) = tokio::task::spawn_blocking(move || evict_cache(root.as_deref())).await {
        log::debug!("the Telegram cache eviction task was cancelled: {error}");
    }
}

fn evict_cache_with(root: Option<&Path>, max_bytes: u64, max_age: Duration) {
    let Some(root) = root else {
        return;
    };
    let now = SystemTime::now();
    let mut entries = collect_cache_files(root);
    for (path, modified) in &entries {
        let expired = modified
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > max_age);
        if expired {
            if let Err(error) = std::fs::remove_file(path) {
                log::debug!("failed to evict a Telegram cache file: {error}");
            }
        }
    }
    entries.retain(|(path, _)| path.is_file());
    let mut total: u64 = entries
        .iter()
        .filter_map(|(path, _)| path.metadata().ok())
        .map(|metadata| metadata.len())
        .sum();
    if total <= max_bytes {
        return;
    }
    entries.sort_by_key(|(_, modified)| *modified);
    for (path, _) in entries {
        if total <= max_bytes {
            break;
        }
        let size = path.metadata().map_or(0, |metadata| metadata.len());
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
}

fn collect_cache_files(root: &Path) -> Vec<(PathBuf, Option<SystemTime>)> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        let Ok(read_dir) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                let modified = entry
                    .metadata()
                    .ok()
                    .and_then(|metadata| metadata.modified().ok());
                files.push((path, modified));
            }
        }
    }
    files
}

/// Asks Telegram to mint a QR login token for this connection. Each call both
/// creates a token and reports whether a previously scanned token has been
/// confirmed by the user's phone.
async fn export_qr_token(
    client: &Client,
    credentials: &TelegramCredentials,
) -> Result<tl::enums::auth::LoginToken, EngineError> {
    let request = tl::functions::auth::ExportLoginToken {
        api_id: credentials.api_id,
        api_hash: credentials.api_hash.clone(),
        except_ids: Vec::new(),
    };
    client.invoke(&request).await.map_err(|error| {
        log::warn!("Telegram QR login failed: {error}");
        EngineError::from_invocation(&error)
    })
}

/// The `tg://login?token=...` URL encoded into the QR code.
fn qr_login_url(token: &[u8]) -> String {
    let encoded = base64::engine::general_purpose::URL_SAFE.encode(token);
    format!("tg://login?token={encoded}")
}

/// Normalizes a user-entered phone number to E.164 form (`+` followed by the
/// country code and subscriber number, no separators).
///
/// Telegram silently accepts malformed numbers and delivers the login code to
/// whatever destination the digits resolve to, so callers must reject input
/// that is not unmistakably international. Returns `None` when the input is not
/// a plausible E.164 number.
fn normalize_phone(raw: &str) -> Option<String> {
    let mut normalized = String::with_capacity(raw.len());
    for character in raw.trim().chars() {
        match character {
            '0'..='9' => normalized.push(character),
            '+' if normalized.is_empty() => normalized.push('+'),
            ' ' | '-' | '(' | ')' | '.' | '\t' => {}
            _ => return None,
        }
    }
    if !normalized.starts_with('+') {
        return None;
    }
    let digits = normalized.len() - 1;
    if !(7..=15).contains(&digits) {
        return None;
    }
    Some(normalized)
}

fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_default(|duration| duration.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::{
        DownloadState, DownloadTarget, EngineConfig, EngineState, HistoryCursor, LoadedDialog,
        MediaKind, MediaSnapshot, MessageSnapshot, SendState, ViewModel, apply_poll_results,
        avatar_initials, current_unix_timestamp, evict_cache_with, is_expired_file_reference,
        kept_the_same_reference, location_file_reference, media_extension, media_file_path,
        merge_history, next_history_page, normalize_loaded_dialogs, normalize_phone,
        remove_partial_download, replace_sent_messages, reply_header_snapshot, thumbnail_extension,
        thumbnail_file_path, upload_failure,
    };
    use crate::error::EngineError;
    use crate::model::{PollOptionSnapshot, PollSnapshot};
    use grammers_client::InvocationError;
    use grammers_client::sender::RpcError;
    use grammers_client::session::types::PeerId;
    use grammers_client::tl;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    fn message(id: i32) -> MessageSnapshot {
        message_in(1, id)
    }

    fn message_in(chat_id: i64, id: i32) -> MessageSnapshot {
        MessageSnapshot {
            id,
            chat_id,
            outgoing: false,
            sender_name: String::new(),
            sender_id: None,
            timestamp_unix: 0,
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

    fn pending(local_id: u64) -> MessageSnapshot {
        MessageSnapshot {
            id: 0,
            local_id: Some(local_id),
            send_state: SendState::Sending,
            outgoing: true,
            ..message(0)
        }
    }

    #[test]
    fn merge_history_keeps_messages_ordered() {
        let existing = vec![message(1), message(2)];
        let page = vec![message(3), message(4)];
        let ids: Vec<i32> = merge_history(&existing, page, 1, None)
            .into_iter()
            .map(|message| message.id)
            .collect();
        assert_eq!(ids, vec![1, 2, 3, 4]);
    }

    #[test]
    fn merge_history_prefers_page_and_keeps_pending() {
        let existing = vec![message(2), message(3), pending(7)];
        let page = vec![message(3), message(4), message(5)];
        let merged = merge_history(&existing, page, 1, None);
        let ids: Vec<i32> = merged.iter().map(|message| message.id).collect();
        assert_eq!(ids, vec![2, 3, 4, 5, 0]);
        assert_eq!(merged.last().and_then(|message| message.local_id), Some(7));
    }

    #[test]
    fn merge_history_handles_empty_page() {
        let existing = vec![message(1), pending(9)];
        let ids: Vec<i32> = merge_history(&existing, Vec::new(), 1, None)
            .into_iter()
            .map(|message| message.id)
            .collect();
        assert_eq!(ids, vec![1, 0]);
    }

    #[test]
    fn replace_sent_messages_replaces_the_optimistic_entry() {
        let mut history = vec![message(1), pending(42)];
        replace_sent_messages(&mut history, 42, vec![message(2)], 1, None);
        let ids: Vec<i32> = history.iter().map(|message| message.id).collect();
        assert_eq!(ids, vec![1, 2]);
        assert!(history.iter().all(|message| message.local_id.is_none()));
    }

    #[test]
    fn replace_sent_messages_never_mixes_chats() {
        let mut selected = vec![message_in(2, 5)];
        replace_sent_messages(&mut selected, 42, vec![message_in(1, 9)], 2, None);
        assert!(selected.iter().all(|message| message.chat_id == 2));
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn merge_history_deduplicates_an_overlapping_older_page() {
        let existing = vec![message(3), message(4)];
        let page = vec![message(2), message(3), message(4)];
        let ids: Vec<i32> = merge_history(&existing, page, 1, None)
            .into_iter()
            .map(|message| message.id)
            .collect();
        assert_eq!(ids, vec![2, 3, 4]);
    }

    #[test]
    fn merge_history_keeps_equal_ids_from_both_sides_of_a_migration() {
        let old_chat_id = -10;
        let current_chat_id = -1_000_000_000_020;
        let existing = vec![message_in(current_chat_id, 7)];
        let page = vec![message_in(old_chat_id, 7)];
        let merged = merge_history(&existing, page, current_chat_id, Some(old_chat_id));
        let identities: Vec<(i64, i32)> = merged
            .into_iter()
            .map(|message| (message.chat_id, message.id))
            .collect();
        assert_eq!(identities, vec![(old_chat_id, 7), (current_chat_id, 7)]);
    }

    #[test]
    fn history_pagination_crosses_the_migration_boundary() {
        let current_chat_id = -1_000_000_000_020;
        let old_chat_id = -10;
        let current_page = HistoryCursor {
            source_chat_id: current_chat_id,
            before_message_id: Some(100),
        };

        assert_eq!(
            next_history_page(
                current_chat_id,
                current_page,
                true,
                Some(60),
                Some(old_chat_id)
            ),
            Some(HistoryCursor {
                source_chat_id: current_chat_id,
                before_message_id: Some(60),
            })
        );
        assert_eq!(
            next_history_page(
                current_chat_id,
                current_page,
                false,
                Some(60),
                Some(old_chat_id)
            ),
            Some(HistoryCursor {
                source_chat_id: old_chat_id,
                before_message_id: None,
            })
        );
        assert_eq!(
            next_history_page(
                current_chat_id,
                HistoryCursor {
                    source_chat_id: old_chat_id,
                    before_message_id: Some(40),
                },
                false,
                Some(1),
                Some(old_chat_id),
            ),
            None
        );
    }

    #[test]
    fn migrated_dialog_is_hidden_only_when_its_target_is_available() {
        let old_chat_id = -10;
        let current_chat_id = -1_000_000_000_020;
        let entries = vec![
            loaded_dialog(old_chat_id, "Group", Some(current_chat_id)),
            loaded_dialog(current_chat_id, "Group", None),
            loaded_dialog(current_chat_id, "Duplicate", None),
        ];

        let loaded =
            normalize_loaded_dialogs(entries, Vec::new(), std::collections::HashMap::new());

        assert_eq!(loaded.chats.len(), 1);
        assert_eq!(loaded.chats[0].id, current_chat_id);
        assert_eq!(loaded.peer_refs.len(), 2);
        assert_eq!(loaded.migrated_to.get(&old_chat_id), Some(&current_chat_id));
        assert_eq!(
            loaded.migrated_from.get(&current_chat_id),
            Some(&old_chat_id)
        );
    }

    #[test]
    fn migrated_dialog_remains_visible_without_its_target() {
        let old_chat_id = -10;
        let current_chat_id = -1_000_000_000_020;
        let loaded = normalize_loaded_dialogs(
            vec![loaded_dialog(old_chat_id, "Group", Some(current_chat_id))],
            Vec::new(),
            std::collections::HashMap::new(),
        );

        assert_eq!(loaded.chats.len(), 1);
        assert_eq!(loaded.chats[0].id, old_chat_id);
        assert!(loaded.migrated_to.is_empty());
    }

    fn loaded_dialog(chat_id: i64, title: &str, migrated_to_chat_id: Option<i64>) -> LoadedDialog {
        let peer_id = PeerId::from_bot_api_dialog_id(chat_id).expect("valid test chat id");
        LoadedDialog {
            snapshot: super::ChatSnapshot {
                id: chat_id,
                title: title.to_owned(),
                ..Default::default()
            },
            peer_ref: peer_id.to_ambient_ref(),
            migrated_to_chat_id,
        }
    }

    #[test]
    fn unix_timestamp_is_positive() {
        assert!(current_unix_timestamp() > 0);
    }

    #[test]
    fn avatar_initials_helper_is_reachable() {
        assert_eq!(avatar_initials("Telegram"), "T");
    }

    #[test]
    fn phone_is_normalized_to_e164() {
        assert_eq!(
            normalize_phone(" +1 (415) 555-1234 "),
            Some("+14155551234".to_owned())
        );
        assert_eq!(
            normalize_phone("+86-138-0013-8000"),
            Some("+8613800138000".to_owned())
        );
    }

    #[test]
    fn phone_without_country_code_is_rejected() {
        assert_eq!(normalize_phone("13800138000"), None);
        assert_eq!(normalize_phone("+123"), None);
        assert_eq!(normalize_phone("call me"), None);
        assert_eq!(normalize_phone("+1234567890123456"), None);
    }

    #[test]
    fn extension_prefers_the_file_name() {
        let media = MediaSnapshot {
            kind: MediaKind::Document,
            file_name: Some("report.PDF".to_owned()),
            mime_type: Some("application/octet-stream".to_owned()),
            ..Default::default()
        };
        assert_eq!(media_extension(&media), "pdf");
    }

    #[test]
    fn extension_falls_back_to_mime_then_kind() {
        let photo = MediaSnapshot {
            kind: MediaKind::Photo,
            ..Default::default()
        };
        assert_eq!(media_extension(&photo), "jpg");

        let voice = MediaSnapshot {
            kind: MediaKind::Voice,
            mime_type: Some("audio/ogg; codecs=opus".to_owned()),
            ..Default::default()
        };
        assert_eq!(media_extension(&voice), "ogg");

        let video = MediaSnapshot {
            kind: MediaKind::Video,
            mime_type: Some("video/mpeg".to_owned()),
            ..Default::default()
        };
        assert_eq!(media_extension(&video), "mpg");
    }

    #[test]
    fn extension_never_escapes_the_cache_directory() {
        let media = |mime: &str| MediaSnapshot {
            kind: MediaKind::Document,
            mime_type: Some(mime.to_owned()),
            ..Default::default()
        };

        assert_eq!(media_extension(&media("application/pdf")), "pdf");
        assert_eq!(media_extension(&media("image/png; charset=utf-8")), "png");
        assert_eq!(media_extension(&media("application/..")), "bin");

        for mime in [
            "application/../../../../tmp/evil",
            "application/..%2F..%2Fevil",
            "application/..\\..\\evil",
            "application/a/b/c",
        ] {
            let path = media_file_path(Path::new("/cache/media"), 1, 2, &media(mime));
            assert_eq!(
                path.parent(),
                Some(Path::new("/cache/media/1")),
                "{mime} escaped the cache: {path:?}"
            );
        }
    }

    #[test]
    fn thumbnail_extension_is_jpeg() {
        assert_eq!(thumbnail_extension(), "jpg");
    }

    #[test]
    fn media_paths_are_sharded_by_chat() {
        let media = MediaSnapshot {
            kind: MediaKind::Photo,
            ..Default::default()
        };
        assert_eq!(
            media_file_path(Path::new("/cache/media"), 42, 7, &media),
            PathBuf::from("/cache/media/42/7.jpg")
        );
        assert_eq!(
            thumbnail_file_path(Path::new("/cache/thumbnails"), 42, 7),
            PathBuf::from("/cache/thumbnails/42/7.jpg")
        );
    }

    fn temporary_cache(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "zzz-telegram-cache-{name}-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        path
    }

    #[test]
    fn cached_media_is_reused_without_a_connection() {
        let root = temporary_cache("hit");
        let config = EngineConfig {
            session_dir: root.join("session"),
            cache_dir: root.join("cache"),
            credentials: None,
            proxy_url: None,
        };
        let (snapshot_tx, _snapshot_rx) =
            tokio::sync::watch::channel(Arc::new(ViewModel::default()));
        let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut engine = EngineState::new(config, snapshot_tx, event_tx);

        let media = MediaSnapshot {
            kind: MediaKind::Document,
            file_name: Some("report.pdf".to_owned()),
            ..Default::default()
        };
        let path = engine.media_path(1, 7, &media);
        std::fs::create_dir_all(path.parent().expect("cache parent")).expect("create cache dir");
        std::fs::write(&path, b"cached").expect("write cache file");

        let mut snapshot = message(7);
        snapshot.media = Some(media);
        engine.view.history.push(snapshot);

        engine.download(1, 7, DownloadTarget::Original);

        let stored = engine.view.history[0].media.as_ref().expect("media");
        assert_eq!(stored.download_state, DownloadState::Downloaded);
        assert_eq!(stored.downloaded_path.as_deref(), Some(path.as_path()));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn evict_cache_ignores_a_missing_directory() {
        let missing = temporary_cache("missing");
        evict_cache_with(Some(&missing), 0, Duration::ZERO);
        assert!(!missing.exists());
    }

    #[test]
    fn evict_cache_keeps_fresh_files_under_the_cap() {
        let root = temporary_cache("keep");
        let media = root.join("42");
        std::fs::create_dir_all(&media).expect("create cache dir");
        let file = media.join("7.jpg");
        std::fs::write(&file, b"data").expect("write cache file");
        evict_cache_with(Some(&root), 1024, Duration::from_secs(3600));
        assert!(file.is_file());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn evict_cache_removes_files_over_the_size_cap() {
        let root = temporary_cache("size");
        let media = root.join("42");
        std::fs::create_dir_all(&media).expect("create cache dir");
        let file = media.join("7.jpg");
        std::fs::write(&file, b"data").expect("write cache file");
        evict_cache_with(Some(&root), 0, Duration::from_secs(3600));
        assert!(!file.is_file());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn evict_cache_removes_files_older_than_the_age_cap() {
        let root = temporary_cache("age");
        let media = root.join("42");
        std::fs::create_dir_all(&media).expect("create cache dir");
        let file = media.join("7.jpg");
        std::fs::write(&file, b"data").expect("write cache file");
        std::thread::sleep(Duration::from_millis(10));
        evict_cache_with(Some(&root), u64::MAX, Duration::ZERO);
        assert!(!file.is_file());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn upload_flood_wait_is_recovered_from_the_io_error() {
        // `upload_file` reports an `io::Error` with the invocation error as its
        // source, so a rate limit during upload must still start the countdown
        // instead of degrading to a generic send failure.
        let invocation = InvocationError::Rpc(RpcError {
            code: 420,
            name: "FLOOD_WAIT".to_owned(),
            value: Some(9),
            caused_by: None,
        });
        assert_eq!(
            upload_failure(&std::io::Error::other(invocation)),
            EngineError::FloodWait { seconds: 9 }
        );
    }

    #[test]
    fn upload_failure_without_an_rpc_error_is_a_send_failure() {
        let plain = std::io::Error::other("disk full");
        assert_eq!(upload_failure(&plain), EngineError::SendFailed);
    }

    fn file_reference_rpc(name: &str) -> InvocationError {
        InvocationError::Rpc(RpcError {
            code: 400,
            name: name.to_owned(),
            value: None,
            caused_by: None,
        })
    }

    #[test]
    fn expired_file_reference_is_detected_for_both_error_shapes() {
        // Large files return the RPC error directly; small files go through
        // `Client::load`, which wraps it in an `io::Error` via `io::Error::other`.
        for name in [
            "FILE_REFERENCE_EXPIRED",
            "FILE_REFERENCE_INVALID",
            "FILE_REFERENCE_EMPTY",
            "FILE_REFERENCE_2_EXPIRED",
        ] {
            let direct = file_reference_rpc(name);
            assert!(
                is_expired_file_reference(&direct),
                "{name} should be treated as an expired file reference"
            );
            let wrapped = InvocationError::Io(std::io::Error::other(direct));
            assert!(
                is_expired_file_reference(&wrapped),
                "wrapped {name} should be treated as an expired file reference"
            );
        }
        assert!(!is_expired_file_reference(&file_reference_rpc(
            "PEER_ID_INVALID"
        )));
        assert!(!is_expired_file_reference(&InvocationError::Io(
            std::io::Error::other("connection reset")
        )));
    }

    fn photo_file_location(reference: &[u8]) -> tl::enums::InputFileLocation {
        tl::types::InputPhotoFileLocation {
            id: 1,
            access_hash: 2,
            file_reference: reference.to_vec(),
            thumb_size: "w".to_owned(),
        }
        .into()
    }

    #[test]
    fn location_file_reference_reads_photos_and_documents() {
        assert_eq!(
            location_file_reference(Some(photo_file_location(b"photo"))),
            Some(b"photo".to_vec())
        );
        let document = tl::types::InputDocumentFileLocation {
            id: 3,
            access_hash: 4,
            file_reference: b"document".to_vec(),
            thumb_size: String::new(),
        };
        assert_eq!(
            location_file_reference(Some(document.into())),
            Some(b"document".to_vec())
        );
        // A location without a reference cannot show whether a refetch helped,
        // so it keeps the retry instead of reporting the media as unavailable.
        assert_eq!(location_file_reference(None), None);
        assert_eq!(
            location_file_reference(Some(tl::enums::InputFileLocation::InputTakeoutFileLocation)),
            None
        );
    }

    #[test]
    fn only_a_changed_reference_is_retried() {
        // The same bytes the failed request used cannot succeed on a retry, so
        // the download stops and reports the media as unavailable.
        assert!(kept_the_same_reference(Some(b"stale"), Some(b"stale")));
        assert!(!kept_the_same_reference(Some(b"fresh"), Some(b"stale")));
        // Without a known rejected reference, or without a reference at all,
        // the retry budget decides.
        assert!(!kept_the_same_reference(Some(b"fresh"), None));
        assert!(!kept_the_same_reference(None, Some(b"stale")));
        assert!(!kept_the_same_reference(None, None));
    }

    #[test]
    fn partial_downloads_are_removed_without_reporting_a_missing_file() {
        let root = temporary_cache("partial-download");
        let path = root.join("7.jpg");
        std::fs::create_dir_all(&root).expect("create cache dir");
        std::fs::write(&path, b"partial").expect("write partial file");
        remove_partial_download(&path);
        assert!(!path.is_file());
        // A second removal (the file is already gone) must not panic or log an error.
        remove_partial_download(&path);
        std::fs::remove_dir_all(&root).ok();
    }

    fn forum_reply_header(
        reply_to_msg_id: Option<i32>,
        reply_to_top_id: Option<i32>,
    ) -> grammers_client::tl::enums::MessageReplyHeader {
        grammers_client::tl::enums::MessageReplyHeader::Header(
            grammers_client::tl::types::MessageReplyHeader {
                reply_to_scheduled: false,
                forum_topic: true,
                quote: false,
                reply_to_ephemeral: false,
                reply_to_msg_id,
                reply_to_peer_id: None,
                reply_from: None,
                reply_media: None,
                reply_to_top_id,
                quote_text: None,
                quote_entities: None,
                quote_offset: None,
                todo_item_id: None,
                poll_option: None,
                quote_rich: None,
            },
        )
    }

    #[test]
    fn topic_routing_is_not_rendered_as_a_reply_preview() {
        assert!(reply_header_snapshot(&forum_reply_header(Some(1), Some(1))).is_none());
        // A direct post routes through the topic id and has no top id.
        assert!(reply_header_snapshot(&forum_reply_header(Some(1), None)).is_none());
        let reply = reply_header_snapshot(&forum_reply_header(Some(42), Some(1)))
            .expect("a reply inside the topic");
        assert_eq!(reply.message_id, 42);
    }

    fn poll_with_options() -> PollSnapshot {
        PollSnapshot {
            id: 9,
            question: "Pick one".to_owned(),
            options: vec![
                PollOptionSnapshot {
                    text: "A".to_owned(),
                    option: vec![1, 2],
                    ..Default::default()
                },
                PollOptionSnapshot {
                    text: "B".to_owned(),
                    option: vec![3, 4],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    fn poll_results(results: Option<Vec<tl::enums::PollAnswerVoters>>) -> tl::types::PollResults {
        tl::types::PollResults {
            min: false,
            has_unread_votes: false,
            can_view_stats: false,
            results,
            total_voters: Some(4),
            recent_voters: None,
            solution: None,
            solution_entities: None,
            solution_media: None,
        }
    }

    #[test]
    fn poll_results_without_answers_keep_the_vote_option_bytes() {
        // Anonymous polls omit per-answer results, but `sendVote` still needs
        // the opaque option bytes from the poll media.
        let mut poll = poll_with_options();
        apply_poll_results(&mut poll, &poll_results(None));
        assert_eq!(poll.total_voters, 4);
        assert_eq!(poll.options[0].option, vec![1, 2]);
        assert_eq!(poll.options[1].option, vec![3, 4]);
    }

    #[test]
    fn poll_results_update_chosen_and_correct_flags() {
        let mut poll = poll_with_options();
        let results = poll_results(Some(vec![tl::enums::PollAnswerVoters::Voters(
            tl::types::PollAnswerVoters {
                chosen: true,
                correct: true,
                option: vec![1, 2],
                voters: Some(3),
                recent_voters: None,
            },
        )]));
        apply_poll_results(&mut poll, &results);
        assert!(poll.options[0].chosen);
        assert!(poll.options[0].correct);
        assert_eq!(poll.options[0].voters, 3);
        assert!(!poll.options[1].chosen);
        assert_eq!(poll.options[1].option, vec![3, 4]);
    }
}
