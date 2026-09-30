//! The Telegram engine: a grammers client driven on a dedicated runtime thread.
//!
//! The UI sends [`Command`]s through an `mpsc` channel and subscribes to
//! immutable [`ViewModel`] snapshots. All networking, file, and media work
//! happens on the engine thread.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use base64::Engine as _;
use grammers_client::client::{LoginToken, PasswordToken, UpdatesConfiguration};
use grammers_client::media::{InputMedia, Media, PhotoSize};
use grammers_client::message::{InputMessage, Message};
use grammers_client::peer::{Dialog, Peer};
use grammers_client::sender::{ConnectionParams, SenderPool};
use grammers_client::session::SessionData;
use grammers_client::session::types::{PeerId, PeerRef};
use grammers_client::tl;
use grammers_client::update::Update;
use grammers_client::{Client, SignInError};
use tokio::sync::{mpsc, watch};

use crate::compose::{
    AlbumItem, AttachmentKind, OutgoingItem, StagedAttachment, markdown_caption, markdown_message,
};
use crate::credentials::TelegramCredentials;
use crate::error::EngineError;
use crate::markdown::entities_to_markdown;
use crate::model::{
    AuthState, ChatSnapshot, ConnectionState, DownloadState, MediaKind, MediaSnapshot,
    MessageSnapshot, SearchHit, SendState, ViewModel, WebPageSnapshot, avatar_initials,
};
use crate::proxy::socks5_url;
use crate::session::{SessionStore, TelegramSession};

/// Number of messages fetched per history page.
const HISTORY_PAGE_SIZE: usize = 40;
/// Number of messages returned by a server-side search.
const SEARCH_LIMIT: usize = 50;
/// Upper bound for the on-disk media cache. Older entries are evicted first.
const MEDIA_CACHE_MAX_BYTES: u64 = 512 * 1024 * 1024;
/// Maximum age for on-disk media before it is evicted.
const MEDIA_CACHE_MAX_AGE: Duration = Duration::from_hours(30 * 24);

/// Requests sent from the UI thread to the engine thread.
#[derive(Clone)]
pub enum Command {
    /// Lazily start the engine and connect if credentials exist.
    Start,
    RefreshChats,
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
    CancelLogin,
    Logout,
    SelectChat {
        chat_id: i64,
    },
    LoadOlder {
        chat_id: i64,
    },
    /// Sends a planned sequence of text and media messages.
    SendOutgoing {
        chat_id: i64,
        items: Vec<OutgoingItem>,
    },
    RetrySend {
        chat_id: i64,
        local_id: u64,
    },
    SetDraft {
        chat_id: i64,
        text: String,
    },
    MarkRead {
        chat_id: i64,
    },
    Search {
        query: String,
        chat_id: Option<i64>,
    },
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
    DeleteMessage {
        chat_id: i64,
        message_id: i32,
    },
    ForwardMessages {
        source_chat_id: i64,
        message_ids: Vec<i32>,
        target_chat_id: i64,
    },
    SetPinned {
        chat_id: i64,
        pinned: bool,
    },
    DismissError,
    Shutdown,
}

impl fmt::Debug for Command {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start => formatter.write_str("Start"),
            Self::RefreshChats => formatter.write_str("RefreshChats"),
            Self::StartQrLogin => formatter.write_str("StartQrLogin"),
            Self::SubmitPhone { phone } => formatter
                .debug_struct("SubmitPhone")
                .field("phone", &redact_phone(phone))
                .finish(),
            Self::SubmitCode { .. } => formatter.write_str("SubmitCode(<redacted>)"),
            Self::SubmitPassword { .. } => formatter.write_str("SubmitPassword(<redacted>)"),
            Self::CancelLogin => formatter.write_str("CancelLogin"),
            Self::Logout => formatter.write_str("Logout"),
            Self::SelectChat { chat_id } => formatter
                .debug_struct("SelectChat")
                .field("chat_id", chat_id)
                .finish(),
            Self::LoadOlder { chat_id } => formatter
                .debug_struct("LoadOlder")
                .field("chat_id", chat_id)
                .finish(),
            Self::SendOutgoing { chat_id, items } => formatter
                .debug_struct("SendOutgoing")
                .field("chat_id", chat_id)
                .field("items", &items.len())
                .finish(),
            Self::RetrySend { chat_id, local_id } => formatter
                .debug_struct("RetrySend")
                .field("chat_id", chat_id)
                .field("local_id", local_id)
                .finish(),
            Self::SetDraft { chat_id, .. } => formatter
                .debug_struct("SetDraft")
                .field("chat_id", chat_id)
                .field("text", &"<redacted>")
                .finish(),
            Self::MarkRead { chat_id } => formatter
                .debug_struct("MarkRead")
                .field("chat_id", chat_id)
                .finish(),
            Self::Search { chat_id, .. } => formatter
                .debug_struct("Search")
                .field("chat_id", chat_id)
                .field("query", &"<redacted>")
                .finish(),
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
            Self::DeleteMessage {
                chat_id,
                message_id,
            } => formatter
                .debug_struct("DeleteMessage")
                .field("chat_id", chat_id)
                .field("message_id", message_id)
                .finish(),
            Self::ForwardMessages {
                source_chat_id,
                message_ids,
                target_chat_id,
            } => formatter
                .debug_struct("ForwardMessages")
                .field("source_chat_id", source_chat_id)
                .field("message_count", &message_ids.len())
                .field("target_chat_id", target_chat_id)
                .finish(),
            Self::SetPinned { chat_id, pinned } => formatter
                .debug_struct("SetPinned")
                .field("chat_id", chat_id)
                .field("pinned", pinned)
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
        let mut engine = self.engine.lock().unwrap_or_else(|error| error.into_inner());
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
        let mut engine = self.engine.lock().unwrap_or_else(|error| error.into_inner());
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
        result: Result<Vec<(ChatSnapshot, PeerRef)>, EngineError>,
    },
    /// A history page finished loading.
    HistoryLoaded {
        chat_id: i64,
        epoch: u64,
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
        result: Result<Vec<SearchHit>, EngineError>,
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

/// The in-memory history for one chat. Kept across chat switches so selecting a
/// chat again renders instantly before its tail is refreshed.
#[derive(Clone, Default)]
struct CachedHistory {
    messages: Vec<MessageSnapshot>,
    has_more: bool,
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
    qr_task: Option<tokio::task::JoinHandle<()>>,
    peer_refs: HashMap<i64, PeerRef>,
    drafts: HashMap<i64, String>,
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
    pending_outgoing: HashMap<u64, OutgoingItem>,
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
            qr_task: None,
            peer_refs: HashMap::new(),
            drafts: HashMap::new(),
            histories: HashMap::new(),
            history_epoch: 0,
            search_epoch: 0,
            connection_epoch: 0,
            chats_refresh_scheduled: false,
            reconnect_scheduled: false,
            reconnect_attempts: 0,
            pending_outgoing: HashMap::new(),
        }
    }

    fn publish(&self) {
        if self.snapshot_tx.send(Arc::new(self.view.clone())).is_err() {
            log::debug!("telegram snapshot receiver has been dropped");
        }
    }

    fn set_error(&mut self, error: EngineError) {
        log::warn!("Telegram engine error: {error:?}");
        self.view.flood_wait_seconds = match &error {
            EngineError::FloodWait { seconds } => Some(*seconds),
            _ => None,
        };
        self.view.error = Some(error);
        self.publish();
    }

    fn clear_error(&mut self) {
        self.view.error = None;
        self.view.flood_wait_seconds = None;
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
            Command::StartQrLogin => self.start_qr_login().await,
            Command::SubmitPhone { phone } => self.submit_phone(phone).await,
            Command::SubmitCode { code } => self.submit_code(code).await,
            Command::SubmitPassword { password } => self.submit_password(password).await,
            Command::CancelLogin => {
                self.stop_qr();
                self.login_token = None;
                self.password_token = None;
                self.view.auth = AuthState::SignedOut;
                self.clear_error();
                self.publish();
            }
            Command::Logout => self.logout().await,
            Command::SelectChat { chat_id } => self.select_chat(chat_id),
            Command::LoadOlder { chat_id } => self.load_older(chat_id),
            Command::SendOutgoing { chat_id, items } => self.send_outgoing(chat_id, items).await,
            Command::RetrySend { chat_id, local_id } => self.retry_send(chat_id, local_id).await,
            Command::SetDraft { chat_id, text } => {
                self.drafts.insert(chat_id, text.clone());
                if self.view.selected_chat == Some(chat_id) {
                    self.view.draft = text;
                    self.publish();
                }
            }
            Command::MarkRead { chat_id } => self.mark_read(chat_id),
            Command::Search { query, chat_id } => self.search(query, chat_id),
            Command::ClearSearch => {
                self.search_epoch = self.search_epoch.wrapping_add(1);
                self.view.searching = false;
                self.view.search_results.clear();
                self.publish();
            }
            Command::DownloadMedia {
                chat_id,
                message_id,
            } => self.download_media(chat_id, message_id),
            Command::DownloadThumbnail {
                chat_id,
                message_id,
            } => self.download_thumbnail(chat_id, message_id),
            Command::DeleteMessage {
                chat_id,
                message_id,
            } => self.delete_message(chat_id, message_id).await,
            Command::ForwardMessages {
                source_chat_id,
                message_ids,
                target_chat_id,
            } => {
                self.forward_messages(source_chat_id, message_ids, target_chat_id)
                    .await
            }
            Command::SetPinned { chat_id, pinned } => self.set_pinned(chat_id, pinned).await,
            Command::DismissError => {
                self.clear_error();
                self.publish();
            }
            Command::Shutdown => {}
        }
    }

    async fn ensure_connected(&mut self) {
        if self.client.is_some() {
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
        tokio::spawn(runner.run());

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
                self.view.connection = ConnectionState::Offline;
                log::warn!("failed to connect to Telegram: {error}");
                self.set_error(EngineError::NotConnected);
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
                        let _ = event_tx.send(EngineEvent::QrError(error));
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
        for _ in 0..3 {
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
        self.drafts.clear();
        self.histories.clear();
        self.pending_outgoing.clear();
        self.history_epoch = self.history_epoch.wrapping_add(1);
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
            let _ = event_tx.send(EngineEvent::ChatsLoaded { result });
        });
    }

    fn apply_chats_loaded(&mut self, result: Result<Vec<(ChatSnapshot, PeerRef)>, EngineError>) {
        self.view.loading_chats = false;
        match result {
            Ok(entries) => {
                let mut chats = Vec::with_capacity(entries.len());
                let mut peer_refs = HashMap::new();
                for (chat, peer_ref) in entries {
                    peer_refs.insert(chat.id, peer_ref);
                    chats.push(chat);
                }
                chats.sort_by(|left, right| {
                    right
                        .pinned
                        .cmp(&left.pinned)
                        .then_with(|| right.timestamp_unix.cmp(&left.timestamp_unix))
                });
                self.peer_refs.extend(peer_refs);
                self.view.chats = chats;
                self.recompute_unread();
                self.publish();
            }
            Err(error) => self.set_error(error),
        }
    }

    /// Sums unread counts for the dock badge. Muted chats are excluded, matching
    /// the badge behavior of Telegram's own clients.
    fn recompute_unread(&mut self) {
        self.view.unread_total = self
            .view
            .chats
            .iter()
            .filter(|chat| !chat.muted)
            .map(|chat| chat.unread_count.max(0) as u32)
            .sum();
    }

    fn select_chat(&mut self, chat_id: i64) {
        let Some(peer_ref) = self.peer_refs.get(&chat_id).copied() else {
            self.set_error(EngineError::ChatUnavailableOffline);
            return;
        };
        if self.view.selected_chat == Some(chat_id) {
            return;
        }
        self.history_epoch = self.history_epoch.wrapping_add(1);
        let epoch = self.history_epoch;
        self.view.selected_chat = Some(chat_id);
        self.view.draft = self.drafts.get(&chat_id).cloned().unwrap_or_default();
        self.view.search_results.clear();
        self.view.searching = false;
        match self.histories.get(&chat_id) {
            Some(cached) if cached.loaded => {
                self.view.history = cached.messages.clone();
                self.view.has_more_history = cached.has_more;
            }
            _ => {
                self.view.history.clear();
                self.view.has_more_history = true;
            }
        }
        self.publish();

        self.spawn_history_page(peer_ref, chat_id, epoch, None);
        self.mark_read(chat_id);
    }

    /// Schedules a history page fetch, returning immediately so the select
    /// loop is not blocked by a slow (or large) download.
    fn spawn_history_page(
        &mut self,
        peer_ref: PeerRef,
        chat_id: i64,
        epoch: u64,
        before_id: Option<i32>,
    ) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.view.loading_history = true;
        self.publish();
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let result = fetch_history_page(&client, peer_ref, chat_id, before_id).await;
            let _ = event_tx.send(EngineEvent::HistoryLoaded {
                chat_id,
                epoch,
                result,
            });
        });
    }

    fn apply_history_loaded(
        &mut self,
        chat_id: i64,
        epoch: u64,
        result: Result<(Vec<MessageSnapshot>, bool), EngineError>,
    ) {
        if epoch != self.history_epoch || self.view.selected_chat != Some(chat_id) {
            return;
        }
        self.view.loading_history = false;
        match result {
            Ok((page, has_more)) => {
                if !has_more {
                    self.view.has_more_history = false;
                }
                self.view.history = merge_tail(&self.view.history, page);
                self.cache_selected_history(chat_id);
                self.publish();
            }
            Err(error) => self.set_error(error),
        }
    }

    /// Loads older messages and prepends them. History is stored oldest-first,
    /// so the first non-optimistic message is the oldest one.
    fn load_older(&mut self, chat_id: i64) {
        if self.view.selected_chat != Some(chat_id)
            || !self.view.has_more_history
            || self.view.loading_history
        {
            return;
        }
        let Some(peer_ref) = self.peer_refs.get(&chat_id).copied() else {
            return;
        };
        let Some(oldest) = self
            .view
            .history
            .iter()
            .find(|message| message.id != 0)
            .map(|message| message.id)
        else {
            return;
        };
        let epoch = self.history_epoch;
        self.spawn_history_page(peer_ref, chat_id, epoch, Some(oldest));
    }

    /// Fetches the messages between two known ids for the selected chat and
    /// merges them into history, closing an out-of-order gap.
    fn load_gap(&mut self, chat_id: i64, after_id: i32, before_id: i32) {
        if self.view.selected_chat != Some(chat_id) {
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
            let _ = event_tx.send(EngineEvent::GapLoaded {
                chat_id,
                epoch,
                messages,
            });
        });
    }

    fn apply_gap_loaded(&mut self, chat_id: i64, epoch: u64, messages: Vec<MessageSnapshot>) {
        if epoch != self.history_epoch || self.view.selected_chat != Some(chat_id) {
            return;
        }
        for snapshot in messages {
            self.upsert_history_message(snapshot);
        }
        self.sort_history();
        self.cache_selected_history(chat_id);
        self.publish();
    }

    /// Stores the displayed history in the per-chat cache.
    fn cache_selected_history(&mut self, chat_id: i64) {
        let entry = self.histories.entry(chat_id).or_default();
        entry.messages = self.view.history.clone();
        entry.has_more = self.view.has_more_history;
        entry.loaded = true;
    }

    /// Sorts history oldest-first, keeping optimistic (id `0`) sends at the end.
    fn sort_history(&mut self) {
        self.view.history.sort_by_key(|message| {
            if message.id == 0 {
                i32::MAX
            } else {
                message.id
            }
        });
    }

    fn schedule_gap_fill(&mut self, chat_id: i64, after_id: i32, before_id: i32) {
        if self
            .event_tx
            .send(EngineEvent::FillGap {
                chat_id,
                after_id,
                before_id,
            })
            .is_err()
        {
            log::debug!("telegram event receiver has been dropped");
        }
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
            let _ = event_tx.send(EngineEvent::RefreshChats);
        });
    }

    /// Schedules a reconnection attempt with exponential backoff.
    fn schedule_reconnect(&mut self) {
        if self.reconnect_scheduled || !self.view.configured || !self.view.auth.is_signed_in() {
            return;
        }
        self.reconnect_scheduled = true;
        let exponent = self.reconnect_attempts.min(5);
        let delay = Duration::from_secs(1u64 << exponent).min(Duration::from_secs(30));
        self.reconnect_attempts = self.reconnect_attempts.saturating_add(1);
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let _ = event_tx.send(EngineEvent::Reconnect);
        });
    }

    async fn reconnect(&mut self) {
        self.reconnect_scheduled = false;
        if self.client.is_some() || !self.view.configured || !self.view.auth.is_signed_in() {
            return;
        }
        self.ensure_connected().await;
    }

    /// Sends a planned sequence of messages, clearing the stored draft once.
    async fn send_outgoing(&mut self, chat_id: i64, items: Vec<OutgoingItem>) {
        if items.is_empty() {
            return;
        }
        self.view.draft.clear();
        self.drafts.remove(&chat_id);
        self.publish();
        for item in items {
            self.send_outgoing_item(chat_id, item).await;
        }
    }

    async fn send_outgoing_item(&mut self, chat_id: i64, item: OutgoingItem) {
        match item {
            OutgoingItem::Text(text) => {
                let text = text.trim_end().to_owned();
                if !text.is_empty() {
                    self.send_text_item(chat_id, text).await;
                }
            }
            OutgoingItem::Media {
                attachment,
                caption,
            } => {
                self.send_media_item(chat_id, attachment, caption).await;
            }
            OutgoingItem::Album { items } => {
                self.send_album_item(chat_id, items).await;
            }
        }
    }

    async fn send_text_item(&mut self, chat_id: i64, text: String) {
        let Some(peer_ref) = self.peer_refs.get(&chat_id).copied() else {
            self.set_error(EngineError::ChatUnavailable);
            return;
        };
        let Some(client) = self.client.clone() else {
            self.set_error(EngineError::NotConnected);
            return;
        };

        let local_id = rand::random::<u64>();
        self.push_optimistic(chat_id, local_id, text.clone(), Some(text.clone()), None);
        self.pending_outgoing
            .insert(local_id, OutgoingItem::Text(text.clone()));

        match client.send_message(peer_ref, markdown_message(&text)).await {
            Ok(message) => self.finish_send(chat_id, local_id, &message),
            Err(error) => self.fail_send(chat_id, local_id, error),
        }
    }

    async fn send_media_item(
        &mut self,
        chat_id: i64,
        attachment: StagedAttachment,
        caption: Option<String>,
    ) {
        let Some(peer_ref) = self.peer_refs.get(&chat_id).copied() else {
            self.set_error(EngineError::ChatUnavailable);
            return;
        };
        let Some(client) = self.client.clone() else {
            self.set_error(EngineError::NotConnected);
            return;
        };

        let local_id = rand::random::<u64>();
        let media = MediaSnapshot {
            kind: attachment.kind.media_kind(),
            file_name: Some(attachment.file_name.clone()),
            size: Some(attachment.size),
            downloaded_path: Some(attachment.path.clone()),
            download_state: DownloadState::Downloaded,
            ..Default::default()
        };
        self.push_optimistic(
            chat_id,
            local_id,
            caption.clone().unwrap_or_default(),
            caption.clone(),
            Some(media),
        );
        self.pending_outgoing.insert(
            local_id,
            OutgoingItem::Media {
                attachment: attachment.clone(),
                caption: caption.clone(),
            },
        );

        let uploaded = match client.upload_file(&attachment.path).await {
            Ok(uploaded) => uploaded,
            Err(error) => {
                self.fail_send(chat_id, local_id, error);
                return;
            }
        };

        let mut message = match caption.as_deref() {
            Some(caption) if !caption.is_empty() => markdown_message(caption),
            _ => InputMessage::new(),
        };
        message = match attachment.kind {
            AttachmentKind::Photo => message.photo(uploaded),
            AttachmentKind::Video | AttachmentKind::Document => message.document(uploaded),
        };

        match client.send_message(peer_ref, message).await {
            Ok(message) => self.finish_send(chat_id, local_id, &message),
            Err(error) => self.fail_send(chat_id, local_id, error),
        }
    }

    /// Sends a group of previewable attachments as a single Telegram album.
    async fn send_album_item(&mut self, chat_id: i64, items: Vec<AlbumItem>) {
        let Some(peer_ref) = self.peer_refs.get(&chat_id).copied() else {
            self.set_error(EngineError::ChatUnavailable);
            return;
        };
        let Some(client) = self.client.clone() else {
            self.set_error(EngineError::NotConnected);
            return;
        };
        let Some(first) = items.first() else {
            return;
        };

        let local_id = rand::random::<u64>();
        let media = MediaSnapshot {
            kind: first.attachment.kind.media_kind(),
            file_name: Some(first.attachment.file_name.clone()),
            size: Some(first.attachment.size),
            downloaded_path: Some(first.attachment.path.clone()),
            download_state: DownloadState::Downloaded,
            ..Default::default()
        };
        self.push_optimistic(
            chat_id,
            local_id,
            first.caption.clone().unwrap_or_default(),
            first.caption.clone(),
            Some(media),
        );
        self.pending_outgoing.insert(
            local_id,
            OutgoingItem::Album {
                items: items.clone(),
            },
        );

        let mut medias = Vec::with_capacity(items.len());
        for item in &items {
            let uploaded = match client.upload_file(&item.attachment.path).await {
                Ok(uploaded) => uploaded,
                Err(error) => {
                    self.fail_send(chat_id, local_id, error);
                    return;
                }
            };
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

        match client.send_album(peer_ref, medias).await {
            Ok(messages) => self.finish_album_send(chat_id, local_id, messages),
            Err(error) => self.fail_send(chat_id, local_id, error),
        }
    }

    /// Replaces the album's optimistic entry with the server messages.
    fn finish_album_send(&mut self, chat_id: i64, local_id: u64, messages: Vec<Option<Message>>) {
        self.pending_outgoing.remove(&local_id);
        if let Some(index) = self.message_index(local_id) {
            self.view.history.remove(index);
        }
        let mut last = None;
        for message in messages.into_iter().flatten() {
            self.upsert_history_message(message_to_snapshot(&message, chat_id));
            last = Some(message);
        }
        self.dedupe_history();
        self.sort_history();
        self.cache_selected_history(chat_id);
        if let Some(last) = last.as_ref() {
            self.update_chat_preview(chat_id, last);
        }
        self.persist_session();
        self.publish();
    }

    /// Pushes an optimistic history entry for a message being sent.
    fn push_optimistic(
        &mut self,
        chat_id: i64,
        local_id: u64,
        text: String,
        markdown: Option<String>,
        media: Option<MediaSnapshot>,
    ) {
        self.view.history.push(MessageSnapshot {
            id: 0,
            chat_id,
            outgoing: true,
            sender_name: String::new(),
            timestamp_unix: current_unix_timestamp(),
            text,
            markdown,
            media,
            send_state: SendState::Sending,
            local_id: Some(local_id),
            edited: false,
        });
        self.publish();
    }

    fn finish_send(&mut self, chat_id: i64, local_id: u64, message: &Message) {
        self.pending_outgoing.remove(&local_id);
        if let Some(index) = self.message_index(local_id) {
            self.view.history[index] = message_to_snapshot(message, chat_id);
            self.view.history[index].local_id = None;
        }
        self.dedupe_history();
        self.sort_history();
        self.cache_selected_history(chat_id);
        self.update_chat_preview(chat_id, message);
        self.persist_session();
        self.publish();
    }

    fn fail_send(&mut self, chat_id: i64, local_id: u64, error: impl std::fmt::Display) {
        if let Some(index) = self.message_index(local_id) {
            self.view.history[index].send_state = SendState::Failed;
        }
        self.cache_selected_history(chat_id);
        log::warn!("failed to send the message: {error}");
        self.set_error(EngineError::SendFailed);
    }

    async fn retry_send(&mut self, chat_id: i64, local_id: u64) {
        let Some(index) = self.message_index(local_id) else {
            return;
        };
        let item = self
            .pending_outgoing
            .remove(&local_id)
            .unwrap_or_else(|| OutgoingItem::Text(self.view.history[index].text.clone()));
        self.view.history.remove(index);
        self.cache_selected_history(chat_id);
        self.publish();
        self.send_outgoing_item(chat_id, item).await;
    }

    fn message_index(&self, local_id: u64) -> Option<usize> {
        self.view
            .history
            .iter()
            .position(|message| message.local_id == Some(local_id))
    }

    /// Inserts a server message, replacing any existing entry with the same id.
    /// This keeps the outgoing-message echo from appending a duplicate.
    fn upsert_history_message(&mut self, snapshot: MessageSnapshot) {
        if let Some(existing) = self
            .view
            .history
            .iter_mut()
            .find(|message| message.id == snapshot.id)
        {
            *existing = snapshot;
        } else {
            self.view.history.push(snapshot);
        }
    }

    /// Drops duplicate server messages, keeping the most recent entry for each
    /// id. Optimistic (id `0`) messages are always kept.
    fn dedupe_history(&mut self) {
        let mut seen = HashSet::new();
        let mut deduped: Vec<MessageSnapshot> = Vec::with_capacity(self.view.history.len());
        for message in self.view.history.drain(..).rev() {
            if message.id == 0 || seen.insert(message.id) {
                deduped.push(message);
            }
        }
        deduped.reverse();
        self.view.history = deduped;
    }

    fn update_chat_preview(&mut self, chat_id: i64, message: &Message) {
        let (preview, preview_media) = chat_preview(message);
        if let Some(chat) = self.view.chats.iter_mut().find(|chat| chat.id == chat_id) {
            chat.preview = preview;
            chat.preview_media = preview_media;
            chat.timestamp_unix = message.date().timestamp();
        }
    }

    /// Updates read state synchronously and reports it to the server on a
    /// background task, so incoming updates are never blocked by a read call.
    fn mark_read(&mut self, chat_id: i64) {
        let Some(peer_ref) = self.peer_refs.get(&chat_id).copied() else {
            return;
        };
        let changed = match self.view.chats.iter_mut().find(|chat| chat.id == chat_id) {
            Some(chat) => {
                let changed = chat.unread_count != 0;
                chat.unread_count = 0;
                changed
            }
            None => false,
        };
        if changed {
            self.recompute_unread();
            self.publish();
        }
        let Some(client) = self.client.clone() else {
            return;
        };
        tokio::spawn(async move {
            if let Err(error) = client.mark_as_read(peer_ref).await {
                log::debug!("failed to mark a Telegram chat as read: {error}");
            }
        });
    }

    fn search(&mut self, query: String, chat_id: Option<i64>) {
        let query = query.trim().to_owned();
        self.search_epoch = self.search_epoch.wrapping_add(1);
        let epoch = self.search_epoch;
        self.view.searching = true;
        self.view.search_results.clear();
        self.publish();

        if query.is_empty() {
            self.view.searching = false;
            self.publish();
            return;
        }

        let mut results = Vec::new();
        for chat in &self.view.chats {
            if chat.title.to_lowercase().contains(&query.to_lowercase()) {
                results.push(SearchHit {
                    chat_id: chat.id,
                    chat_title: chat.title.clone(),
                    message: None,
                });
            }
        }
        self.view.search_results = results;
        self.publish();

        let Some(client) = self.client.clone() else {
            self.view.searching = false;
            self.publish();
            return;
        };
        let scoped_peer = chat_id.and_then(|id| self.peer_refs.get(&id).copied());
        let titles: HashMap<i64, String> = self
            .view
            .chats
            .iter()
            .map(|chat| (chat.id, chat.title.clone()))
            .collect();
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let result = fetch_search(&client, scoped_peer, &query, &titles).await;
            let _ = event_tx.send(EngineEvent::SearchLoaded { epoch, result });
        });
    }

    fn apply_search_loaded(&mut self, epoch: u64, result: Result<Vec<SearchHit>, EngineError>) {
        if epoch != self.search_epoch {
            return;
        }
        self.view.searching = false;
        match result {
            Ok(hits) => {
                self.view.search_results.extend(hits);
                self.publish();
            }
            Err(error) => self.set_error(error),
        }
    }

    fn download_media(&mut self, chat_id: i64, message_id: i32) {
        let Some(index) = self.current_message_index(chat_id, message_id) else {
            return;
        };
        let Some(media) = self.view.history[index].media.as_ref() else {
            return;
        };
        if !media.is_downloadable() {
            return;
        }
        let path = self.media_path(chat_id, message_id, media);
        if path.is_file() {
            if let Some(media) = self.view.history[index].media.as_mut() {
                media.download_state = DownloadState::Downloaded;
                media.downloaded_path = Some(path);
            }
            self.publish();
            return;
        }

        let Some(peer_ref) = self.peer_refs.get(&chat_id).copied() else {
            return;
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        if let Some(media) = self.view.history[index].media.as_mut() {
            media.download_state = DownloadState::Downloading;
        }
        self.publish();

        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let result = download_media_file(&client, peer_ref, message_id, path).await;
            let _ = event_tx.send(EngineEvent::MediaLoaded {
                chat_id,
                message_id,
                result,
            });
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
            }
            Ok(None) => {
                self.update_media_snapshot(chat_id, message_id, |media| {
                    media.download_state = DownloadState::NotDownloaded;
                });
            }
            Err(error) => {
                self.update_media_snapshot(chat_id, message_id, |media| {
                    media.download_state = DownloadState::Failed;
                });
                self.view.flood_wait_seconds = match &error {
                    EngineError::FloodWait { seconds } => Some(*seconds),
                    _ => None,
                };
                self.view.error = Some(error);
            }
        }
        self.publish();
    }

    fn media_path(&self, chat_id: i64, message_id: i32, media: &MediaSnapshot) -> PathBuf {
        media_file_path(&self.media_dir, chat_id, message_id, media)
    }

    fn thumbnail_path(&self, chat_id: i64, message_id: i32, media: &MediaSnapshot) -> PathBuf {
        thumbnail_file_path(&self.thumbnail_dir, chat_id, message_id, media)
    }

    /// Schedules a small preview fetch for an image-like attachment. The bytes
    /// are cached under the thumbnail cache root and re-fetched on a miss.
    fn download_thumbnail(&mut self, chat_id: i64, message_id: i32) {
        let Some(index) = self.current_message_index(chat_id, message_id) else {
            return;
        };
        let Some(media) = self.view.history[index].media.as_ref() else {
            return;
        };
        if !matches!(
            media.kind,
            MediaKind::Photo | MediaKind::Sticker | MediaKind::Video
        ) {
            return;
        }
        if media.thumbnail_path.is_some() || media.thumbnail_state == DownloadState::Downloading {
            return;
        }

        let path = self.thumbnail_path(chat_id, message_id, media);
        if path.is_file() {
            if let Some(media) = self.view.history[index].media.as_mut() {
                media.thumbnail_path = Some(path);
                media.thumbnail_state = DownloadState::Downloaded;
            }
            self.publish();
            return;
        }

        let Some(peer_ref) = self.peer_refs.get(&chat_id).copied() else {
            return;
        };
        let Some(client) = self.client.clone() else {
            return;
        };

        if let Some(media) = self.view.history[index].media.as_mut() {
            media.thumbnail_state = DownloadState::Downloading;
        }
        self.publish();

        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            let result = download_thumbnail_file(&client, peer_ref, message_id, path).await;
            let _ = event_tx.send(EngineEvent::ThumbnailLoaded {
                chat_id,
                message_id,
                result,
            });
        });
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
        if let Some(cached) = self.histories.get_mut(&chat_id)
            && let Some(message) = cached.messages.iter_mut().find(|message| {
                message.chat_id == chat_id && message.id == message_id && message.id != 0
            })
            && let Some(media) = message.media.as_mut()
        {
            update(media);
        }
    }

    fn current_message_index(&self, chat_id: i64, message_id: i32) -> Option<usize> {
        self.view.history.iter().position(|message| {
            message.chat_id == chat_id && message.id == message_id && message.id != 0
        })
    }

    async fn delete_message(&mut self, chat_id: i64, message_id: i32) {
        let Some(peer_ref) = self.peer_refs.get(&chat_id).copied() else {
            return;
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        match client.delete_messages(peer_ref, &[message_id]).await {
            Ok(_) => {
                self.view.history.retain(|message| message.id != message_id);
                self.cache_selected_history(chat_id);
                self.publish();
            }
            Err(error) => {
                log::warn!("failed to delete a Telegram message: {error}");
                self.set_error(EngineError::DeleteFailed);
            }
        }
    }

    async fn forward_messages(
        &mut self,
        source_chat_id: i64,
        message_ids: Vec<i32>,
        target_chat_id: i64,
    ) {
        let Some(source) = self.peer_refs.get(&source_chat_id).copied() else {
            return;
        };
        let Some(target) = self.peer_refs.get(&target_chat_id).copied() else {
            return;
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        match client.forward_messages(target, &message_ids, source).await {
            Ok(_) => {}
            Err(error) => {
                log::warn!("failed to forward Telegram messages: {error}");
                self.set_error(EngineError::ForwardFailed);
            }
        }
    }

    async fn set_pinned(&mut self, chat_id: i64, pinned: bool) {
        let Some(peer_ref) = self.peer_refs.get(&chat_id).copied() else {
            return;
        };
        let Some(client) = self.client.clone() else {
            return;
        };
        let input_peer: tl::enums::InputPeer = peer_ref.into();
        let request = tl::functions::messages::ToggleDialogPin {
            pinned,
            peer: tl::enums::InputDialogPeer::Peer(tl::types::InputDialogPeer { peer: input_peer }),
        };
        match client.invoke(&request).await {
            Ok(_) => {
                if let Some(chat) = self.view.chats.iter_mut().find(|chat| chat.id == chat_id) {
                    chat.pinned = pinned;
                }
                self.view.chats.sort_by(|left, right| {
                    right
                        .pinned
                        .cmp(&left.pinned)
                        .then_with(|| right.timestamp_unix.cmp(&left.timestamp_unix))
                });
                self.publish();
            }
            Err(error) => {
                log::warn!("failed to pin a Telegram chat: {error}");
                self.set_error(EngineError::PinFailed);
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
            EngineEvent::FillGap {
                chat_id,
                after_id,
                before_id,
            } => self.load_gap(chat_id, after_id, before_id),
            EngineEvent::ChatsLoaded { result } => self.apply_chats_loaded(result),
            EngineEvent::HistoryLoaded {
                chat_id,
                epoch,
                result,
            } => self.apply_history_loaded(chat_id, epoch, result),
            EngineEvent::GapLoaded {
                chat_id,
                epoch,
                messages,
            } => self.apply_gap_loaded(chat_id, epoch, messages),
            EngineEvent::SearchLoaded { epoch, result } => self.apply_search_loaded(epoch, result),
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
                if let Some(chat_id) = deletion.channel_id().and_then(|channel_id| {
                    PeerId::channel(channel_id).and_then(PeerId::bot_api_dialog_id)
                }) {
                    let ids: HashSet<i32> = deletion.messages().iter().copied().collect();
                    if let Some(cached) = self.histories.get_mut(&chat_id) {
                        cached.messages.retain(|message| !ids.contains(&message.id));
                    }
                    if self.view.selected_chat == Some(chat_id) {
                        self.view
                            .history
                            .retain(|message| !ids.contains(&message.id));
                    }
                    self.publish();
                } else {
                    let ids: HashSet<i32> = deletion.messages().iter().copied().collect();
                    self.view
                        .history
                        .retain(|message| !ids.contains(&message.id));
                    if let Some(chat_id) = self.view.selected_chat {
                        self.cache_selected_history(chat_id);
                    }
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
        let Some(chat_id) = message.peer_id().bot_api_dialog_id() else {
            log::debug!("ignoring a Telegram message with an unresolvable peer");
            return;
        };
        let snapshot = message_to_snapshot(&message, chat_id);
        let selected = self.view.selected_chat == Some(chat_id);

        if let Some(chat) = self.view.chats.iter_mut().find(|chat| chat.id == chat_id) {
            let (preview, preview_media) = chat_preview(&message);
            chat.preview = preview;
            chat.preview_media = preview_media;
            chat.timestamp_unix = snapshot.timestamp_unix;
            if !snapshot.outgoing && !selected {
                chat.unread_count = chat.unread_count.saturating_add(1);
            }
        } else {
            // A chat we have not loaded yet; refresh the list once the burst of
            // messages settles instead of refetching on every message.
            self.schedule_chats_refresh();
        }

        if selected && snapshot.id != 0 {
            let newest_known = self
                .view
                .history
                .iter()
                .rev()
                .find(|message| message.id != 0)
                .map(|message| message.id);
            let incoming_id = snapshot.id;
            self.upsert_history_message(snapshot);
            self.sort_history();
            self.cache_selected_history(chat_id);
            if let Some(newest_known) = newest_known {
                if incoming_id > newest_known.saturating_add(1) {
                    self.schedule_gap_fill(chat_id, newest_known, incoming_id);
                }
            }
        }
        self.recompute_unread();
        self.publish();

        if selected && !message.outgoing() {
            self.mark_read(chat_id);
        }
    }

    async fn handle_edited_message(&mut self, message: Message) {
        let Some(chat_id) = message.peer_id().bot_api_dialog_id() else {
            log::debug!("ignoring an edited Telegram message with an unresolvable peer");
            return;
        };
        if let Some(chat) = self.view.chats.iter_mut().find(|chat| chat.id == chat_id) {
            let (preview, preview_media) = chat_preview(&message);
            chat.preview = preview;
            chat.preview_media = preview_media;
        }
        if self.view.selected_chat == Some(chat_id) {
            let updated = message_to_snapshot(&message, chat_id);
            let mut found = false;
            for snapshot in &mut self.view.history {
                if snapshot.id == updated.id {
                    *snapshot = updated;
                    found = true;
                    break;
                }
            }
            if found {
                self.cache_selected_history(chat_id);
            }
        }
        self.publish();
    }

    /// Applies read-state updates from other devices to the chat list. A peer
    /// that is not in the loaded chat list is ignored until the next reload.
    async fn handle_raw_update(&mut self, update: &tl::enums::Update) {
        let (chat_id, unread_count) = match update {
            tl::enums::Update::ReadHistoryInbox(raw) => (
                PeerId::from(&raw.peer).bot_api_dialog_id(),
                raw.still_unread_count,
            ),
            tl::enums::Update::ReadChannelInbox(raw) => (
                PeerId::channel(raw.channel_id).and_then(PeerId::bot_api_dialog_id),
                raw.still_unread_count,
            ),
            _ => return,
        };
        let Some(chat_id) = chat_id else {
            return;
        };
        let unread_count = unread_count.max(0);
        let Some(chat) = self.view.chats.iter_mut().find(|chat| chat.id == chat_id) else {
            return;
        };
        if chat.unread_count == unread_count {
            return;
        }
        chat.unread_count = unread_count;
        self.recompute_unread();
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

/// Merges a freshly fetched newest page into the current history, keeping older
/// loaded messages and in-flight optimistic sends. The page wins over an
/// existing entry with the same id, and the result is oldest-first with
/// optimistic (id `0`) sends last.
fn merge_tail(existing: &[MessageSnapshot], page: Vec<MessageSnapshot>) -> Vec<MessageSnapshot> {
    let mut merged = Vec::with_capacity(existing.len() + page.len());
    let mut seen = HashSet::new();
    for message in page.iter().chain(existing.iter()) {
        if message.id == 0 || seen.insert(message.id) {
            merged.push(message.clone());
        }
    }
    merged.sort_by_key(|message| {
        if message.id == 0 {
            i32::MAX
        } else {
            message.id
        }
    });
    merged
}

fn dialog_to_snapshot(dialog: &Dialog) -> Option<ChatSnapshot> {
    let id = dialog.peer_id().bot_api_dialog_id()?;
    let title = dialog.peer.name().unwrap_or("Unknown").to_owned();
    let last_message = dialog.last_message.as_ref();
    let (preview, preview_media) = last_message.map_or_else(|| (String::new(), None), chat_preview);
    let timestamp_unix = last_message
        .map(|message| message.date().timestamp())
        .unwrap_or_default();

    let (unread_count, pinned, muted) = match &dialog.raw {
        tl::enums::Dialog::Dialog(raw) => {
            let muted = match &raw.notify_settings {
                tl::enums::PeerNotifySettings::Settings(settings) => is_muted(settings.mute_until),
            };
            (raw.unread_count, raw.pinned, muted)
        }
        // Dialog folders are not conversations, so they are not listed.
        tl::enums::Dialog::Folder(_) => return None,
    };

    Some(ChatSnapshot {
        id,
        title: title.clone(),
        preview,
        preview_media,
        timestamp_unix,
        unread_count,
        pinned,
        muted,
        avatar_initials: avatar_initials(&title),
    })
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

    MessageSnapshot {
        id: message.id(),
        chat_id,
        outgoing: message.outgoing(),
        sender_name,
        timestamp_unix: message.date().timestamp(),
        text,
        markdown,
        media: message.media().as_ref().map(media_to_snapshot),
        send_state: SendState::Sent,
        local_id: None,
        edited: message.edit_date().is_some(),
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
        }
        Media::WebPage(page) => {
            snapshot.kind = MediaKind::WebPage;
            snapshot.webpage = webpage_snapshot(page);
        }
        Media::Contact(_) => snapshot.kind = MediaKind::Contact,
        Media::Poll(_) => snapshot.kind = MediaKind::Poll,
        Media::Geo(_) | Media::Venue(_) | Media::GeoLive(_) => snapshot.kind = MediaKind::Geo,
        _ => snapshot.kind = MediaKind::Other,
    }
    snapshot
}

fn document_kind(document: &grammers_client::media::Document) -> MediaKind {
    if let Some(tl::enums::Document::Document(raw)) = document.raw.document.as_ref() {
        for attribute in &raw.attributes {
            match attribute {
                tl::enums::DocumentAttribute::Audio(audio) if audio.voice => {
                    return MediaKind::Voice;
                }
                tl::enums::DocumentAttribute::Audio(_) => return MediaKind::Audio,
                tl::enums::DocumentAttribute::Video(_) => return MediaKind::Video,
                tl::enums::DocumentAttribute::Sticker(_) => return MediaKind::Sticker,
                _ => {}
            }
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

/// Loads the full dialog list as chat snapshots paired with their peer refs.
async fn fetch_dialogs(client: &Client) -> Result<Vec<(ChatSnapshot, PeerRef)>, EngineError> {
    let mut dialogs = client.iter_dialogs();
    let mut chats = Vec::new();
    loop {
        match dialogs.next().await {
            Ok(Some(dialog)) => {
                if let Some(snapshot) = dialog_to_snapshot(&dialog) {
                    chats.push((snapshot, dialog.peer_ref()));
                }
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
    Ok(chats)
}

/// Runs a server-side message search, resolving chat titles from `titles`.
async fn fetch_search(
    client: &Client,
    scoped_peer: Option<PeerRef>,
    query: &str,
    titles: &HashMap<i64, String>,
) -> Result<Vec<SearchHit>, EngineError> {
    let mut found: Vec<Message> = Vec::new();
    if let Some(peer_ref) = scoped_peer {
        let mut iterator = client
            .search_messages(peer_ref)
            .query(query)
            .limit(SEARCH_LIMIT);
        loop {
            match iterator.next().await {
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
    } else {
        let mut iterator = client
            .search_all_messages()
            .query(query)
            .limit(SEARCH_LIMIT);
        loop {
            match iterator.next().await {
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
    }

    Ok(found
        .into_iter()
        .filter_map(|message| {
            let chat_id = message.peer_id().bot_api_dialog_id()?;
            let chat_title = titles
                .get(&chat_id)
                .cloned()
                .unwrap_or_else(|| chat_id.to_string());
            Some(SearchHit {
                chat_id,
                chat_title,
                message: Some(message_to_snapshot(&message, chat_id)),
            })
        })
        .collect())
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
    match message.download_media(&path).await {
        Ok(true) => {
            evict_cache_async(path.parent().and_then(Path::parent)).await;
            Ok(Some(path))
        }
        Ok(false) => Ok(None),
        Err(error) => {
            log::warn!("failed to download Telegram media: {error}");
            Err(EngineError::Other)
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

    let mut download = client.iter_download(&thumbnail);
    let mut bytes = Vec::new();
    loop {
        match download.next().await {
            Ok(Some(chunk)) => bytes.extend_from_slice(&chunk),
            Ok(None) => break,
            Err(error) => {
                log::warn!("failed to download a Telegram thumbnail: {error}");
                return Err(EngineError::from_invocation(&error));
            }
        }
    }
    Ok((!bytes.is_empty()).then_some(bytes))
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
fn thumbnail_extension(_media: &MediaSnapshot) -> String {
    "jpg".to_owned()
}

/// Builds the sharded on-disk path for a downloaded attachment.
fn media_file_path(root: &Path, chat_id: i64, message_id: i32, media: &MediaSnapshot) -> PathBuf {
    root.join(chat_id.to_string())
        .join(format!("{message_id}.{}", media_extension(media)))
}

/// Builds the sharded on-disk path for a cached thumbnail.
fn thumbnail_file_path(
    root: &Path,
    chat_id: i64,
    message_id: i32,
    media: &MediaSnapshot,
) -> PathBuf {
    root.join(chat_id.to_string())
        .join(format!("{message_id}.{}", thumbnail_extension(media)))
}

fn file_extension(name: &str) -> Option<String> {
    let extension = Path::new(name).extension()?.to_str()?;
    let sanitized: String = extension
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect();
    (!sanitized.is_empty()).then(|| sanitized.to_ascii_lowercase())
}

fn mime_extension(mime: &str, kind: MediaKind) -> Option<String> {
    let subtype = mime.split('/').nth(1)?.split(';').next()?.trim();
    let mapped = match subtype {
        "jpeg" => "jpg",
        "svg+xml" => "svg",
        "quicktime" => "mov",
        "x-matroska" => "mkv",
        "mpeg" if kind == MediaKind::Video => "mpg",
        "mpeg" => "mp3",
        "plain" => "txt",
        other => other,
    };
    (!mapped.is_empty()).then(|| mapped.to_owned())
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
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{
        DownloadState, EngineConfig, EngineState, MediaKind, MediaSnapshot, MessageSnapshot,
        SendState, ViewModel, avatar_initials, current_unix_timestamp, evict_cache_with,
        media_extension, media_file_path, merge_tail, normalize_phone, thumbnail_extension,
        thumbnail_file_path,
    };
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    fn message(id: i32) -> MessageSnapshot {
        MessageSnapshot {
            id,
            chat_id: 1,
            outgoing: false,
            sender_name: String::new(),
            timestamp_unix: 0,
            text: String::new(),
            markdown: None,
            media: None,
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
    fn merge_tail_keeps_older_messages_ordered() {
        let existing = vec![message(1), message(2)];
        let page = vec![message(3), message(4)];
        let ids: Vec<i32> = merge_tail(&existing, page)
            .into_iter()
            .map(|message| message.id)
            .collect();
        assert_eq!(ids, vec![1, 2, 3, 4]);
    }

    #[test]
    fn merge_tail_prefers_page_and_keeps_pending() {
        let existing = vec![message(2), message(3), pending(7)];
        let page = vec![message(3), message(4), message(5)];
        let merged = merge_tail(&existing, page);
        let ids: Vec<i32> = merged.iter().map(|message| message.id).collect();
        assert_eq!(ids, vec![2, 3, 4, 5, 0]);
        assert_eq!(merged.last().and_then(|message| message.local_id), Some(7));
    }

    #[test]
    fn merge_tail_handles_empty_page() {
        let existing = vec![message(1), pending(9)];
        let ids: Vec<i32> = merge_tail(&existing, Vec::new())
            .into_iter()
            .map(|message| message.id)
            .collect();
        assert_eq!(ids, vec![1, 0]);
    }

    #[test]
    fn merge_tail_deduplicates_an_overlapping_older_page() {
        let existing = vec![message(3), message(4)];
        let page = vec![message(2), message(3), message(4)];
        let ids: Vec<i32> = merge_tail(&existing, page)
            .into_iter()
            .map(|message| message.id)
            .collect();
        assert_eq!(ids, vec![2, 3, 4]);
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
    fn thumbnail_extension_is_jpeg() {
        let media = MediaSnapshot {
            kind: MediaKind::Photo,
            ..Default::default()
        };
        assert_eq!(thumbnail_extension(&media), "jpg");
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
            thumbnail_file_path(Path::new("/cache/thumbnails"), 42, 7, &media),
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

        engine.download_media(1, 7);

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
}
