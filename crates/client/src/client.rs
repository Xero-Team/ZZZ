#[cfg(any(test, feature = "test-support"))]
pub mod test;

pub mod platform_info;
mod proxy;
pub mod user;
pub mod zzz_urls;

use anyhow::{Context as _, Result, anyhow};
use async_tungstenite::tungstenite::{
    client::IntoClientRequest,
    error::Error as WebsocketError,
    http::{HeaderValue, StatusCode},
};
use clock::SystemClock;
use credentials_provider::CredentialsProvider;
use futures::{
    FutureExt, SinkExt, Stream, StreamExt, TryFutureExt as _, TryStreamExt, future::BoxFuture,
};
use gpui::{App, AsyncApp, Entity, Global, Task, WeakEntity, actions};
use http_client::{HttpClient, HttpClientWithUrl, http, read_proxy_from_env};
use parking_lot::{Mutex, RwLock};
use postage::watch;
use proxy::connect_proxy_stream;
use rand::prelude::*;
use release_channel::{AppVersion, ReleaseChannel};
use rpc::proto::{AnyTypedEnvelope, EnvelopedMessage, PeerId, RequestMessage};
use serde::Deserialize;
#[cfg(test)]
use settings::SettingsContent;
use settings::{RegisterSetting, Settings};
use std::{
    any::TypeId,
    future::Future,
    marker::PhantomData,
    path::PathBuf,
    sync::{
        Arc, LazyLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use std::{cmp, pin::Pin};
use thiserror::Error;
use tokio::net::TcpStream;
use url::Url;
use util::{ConnectionResult, ResultExt};

pub use rpc::*;
pub use user::*;

static ZZZ_SERVER_URL: LazyLock<Option<String>> =
    LazyLock::new(|| std::env::var("ZZZ_SERVER_URL").ok());
static ZZZ_RPC_URL: LazyLock<Option<String>> = LazyLock::new(|| std::env::var("ZZZ_RPC_URL").ok());

pub static ZZZ_APP_PATH: LazyLock<Option<PathBuf>> =
    LazyLock::new(|| std::env::var("ZZZ_APP_PATH").ok().map(PathBuf::from));

pub static ZZZ_ALWAYS_ACTIVE: LazyLock<bool> =
    LazyLock::new(|| std::env::var("ZZZ_ALWAYS_ACTIVE").is_ok_and(|e| !e.is_empty()));

pub const INITIAL_RECONNECTION_DELAY: Duration = Duration::from_millis(500);
pub const MAX_RECONNECTION_DELAY: Duration = Duration::from_secs(30);
pub const CONNECTION_TIMEOUT: Duration = Duration::from_secs(20);

actions!(
    client,
    [
        /// Reconnects to the collaboration server.
        Reconnect
    ]
);

#[derive(Deserialize, RegisterSetting)]
pub struct ClientSettings {
    pub server_url: String,
    /// Overrides the key used to store credentials in the system keychain.
    /// Defaults to `server_url` when unset.
    ///
    /// Useful when running multiple ZZZ instances side by side without them
    /// overwriting each other's keychain entries.
    ///
    /// Note: changing this after signing in will require signing in again, as
    /// existing credentials are stored under the old key.
    pub credentials_url: Option<String>,
}

impl ClientSettings {
    /// The local-first default deliberately does not point at any hosted service.
    /// A remote server is enabled only when the user explicitly configures one.
    pub fn remote_server_enabled(&self) -> bool {
        let value = self.server_url.trim_end_matches('/');
        !value.is_empty()
            && !value.starts_with("http://127.0.0.1")
            && !value.starts_with("http://localhost")
            && !value.starts_with("http://[::1]")
            && !value.starts_with("https://127.0.0.1")
            && !value.starts_with("https://localhost")
            && !value.starts_with("https://[::1]")
    }
}

impl Settings for ClientSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        if let Some(server_url) = &*ZZZ_SERVER_URL {
            return Self {
                server_url: server_url.clone(),
                credentials_url: content.credentials_url.clone(),
            };
        }
        Self {
            server_url: content
                .server_url
                .clone()
                .unwrap_or_else(|| "http://127.0.0.1:7331".to_owned()),
            credentials_url: content.credentials_url.clone(),
        }
    }
}

#[derive(Deserialize, Default, RegisterSetting)]
pub struct ProxySettings {
    pub proxy: Option<String>,
}

impl ProxySettings {
    pub fn proxy_url(&self) -> Option<Url> {
        self.proxy
            .as_deref()
            .map(str::trim)
            .filter(|input| !input.is_empty())
            .and_then(|input| {
                input
                    .parse::<Url>()
                    .inspect_err(|e| log::error!("Error parsing proxy settings: {}", e))
                    .ok()
            })
            .or_else(read_proxy_from_env)
    }
}

impl Settings for ProxySettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        Self {
            proxy: content
                .proxy
                .as_deref()
                .map(str::trim)
                .filter(|proxy| !proxy.is_empty())
                .map(ToOwned::to_owned),
        }
    }
}

pub fn init(client: &Arc<Client>, cx: &mut App) {
    let client = Arc::downgrade(client);
    cx.on_action({
        let client = client;
        move |_: &Reconnect, cx| {
            if let Some(client) = client.upgrade() {
                cx.spawn(async move |cx| {
                    client.reconnect(cx);
                })
                .detach();
            }
        }
    });
}

struct GlobalClient(Arc<Client>);

impl Global for GlobalClient {}

pub struct Client {
    id: AtomicU64,
    peer: Arc<Peer>,
    http: Arc<HttpClientWithUrl>,
    credentials_provider: ClientCredentialsProvider,
    state: RwLock<ClientState>,
    handler_set: Mutex<ProtoMessageHandlerSet>,

    #[allow(clippy::type_complexity)]
    #[cfg(any(test, feature = "test-support"))]
    authenticate:
        RwLock<Option<Box<dyn 'static + Send + Sync + Fn(&AsyncApp) -> Task<Result<Credentials>>>>>,

    #[allow(clippy::type_complexity)]
    #[cfg(any(test, feature = "test-support"))]
    establish_connection: RwLock<
        Option<
            Box<
                dyn 'static
                    + Send
                    + Sync
                    + Fn(
                        &Credentials,
                        &AsyncApp,
                    ) -> Task<Result<Connection, EstablishConnectionError>>,
            >,
        >,
    >,

    #[cfg(any(test, feature = "test-support"))]
    rpc_url: RwLock<Option<Url>>,
}

#[derive(Error, Debug)]
pub enum EstablishConnectionError {
    #[error("upgrade required")]
    UpgradeRequired,
    #[error("unauthorized")]
    Unauthorized,
    #[error("{0}")]
    Other(#[from] anyhow::Error),
    #[error("{0}")]
    InvalidHeaderValue(#[from] async_tungstenite::tungstenite::http::header::InvalidHeaderValue),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Websocket(#[from] async_tungstenite::tungstenite::http::Error),
}

impl From<WebsocketError> for EstablishConnectionError {
    fn from(error: WebsocketError) -> Self {
        if let WebsocketError::Http(response) = &error {
            match response.status() {
                StatusCode::UNAUTHORIZED => return EstablishConnectionError::Unauthorized,
                StatusCode::UPGRADE_REQUIRED => return EstablishConnectionError::UpgradeRequired,
                _ => {}
            }
        }
        EstablishConnectionError::Other(error.into())
    }
}

impl EstablishConnectionError {
    pub fn other(error: impl Into<anyhow::Error> + Send + Sync) -> Self {
        Self::Other(error.into())
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Status {
    SignedOut,
    UpgradeRequired,
    Authenticating,
    Authenticated,
    AuthenticationError,
    Connecting,
    ConnectionError,
    Connected {
        peer_id: PeerId,
        connection_id: ConnectionId,
    },
    ConnectionLost,
    Reauthenticating,
    Reauthenticated,
    Reconnecting,
    ReconnectionError {
        next_reconnection: Instant,
    },
}

impl Status {
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected { .. })
    }

    pub fn was_connected(&self) -> bool {
        matches!(
            self,
            Self::ConnectionLost
                | Self::Reauthenticating
                | Self::Reauthenticated
                | Self::Reconnecting
        )
    }

    /// Returns whether the client is currently connected or was connected at some point.
    pub fn is_or_was_connected(&self) -> bool {
        self.is_connected() || self.was_connected()
    }

    pub fn is_signing_in(&self) -> bool {
        matches!(
            self,
            Self::Authenticating | Self::Reauthenticating | Self::Connecting | Self::Reconnecting
        )
    }

    pub fn is_signed_out(&self) -> bool {
        matches!(self, Self::SignedOut | Self::UpgradeRequired)
    }
}

struct ClientState {
    credentials: Option<Credentials>,
    status: (watch::Sender<Status>, watch::Receiver<Status>),
    _reconnect_task: Option<Task<()>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Credentials {
    pub user_id: u64,
    pub access_token: String,
}

impl Credentials {
    pub fn authorization_header(&self) -> String {
        format!("{} {}", self.user_id, self.access_token)
    }
}

pub struct ClientCredentialsProvider {
    provider: Arc<dyn CredentialsProvider>,
}

impl ClientCredentialsProvider {
    pub fn new(cx: &App) -> Self {
        Self {
            provider: zzz_credentials_provider::global(cx),
        }
    }

    fn server_url(&self, cx: &AsyncApp) -> Result<String> {
        Ok(cx.update(|cx| ClientSettings::get_global(cx).server_url.clone()))
    }

    /// Returns the key used for credential storage in the system keychain.
    fn credentials_url(&self, cx: &AsyncApp) -> Result<String> {
        let from_settings = cx.update(|cx| ClientSettings::get_global(cx).credentials_url.clone());
        Ok(from_settings.unwrap_or(self.server_url(cx)?))
    }

    /// Reads the credentials from the provider.
    fn read_credentials<'a>(
        &'a self,
        cx: &'a AsyncApp,
    ) -> Pin<Box<dyn Future<Output = Option<Credentials>> + 'a>> {
        async move {
            let credentials_url = self.credentials_url(cx).ok()?;
            let (user_id, access_token) = self
                .provider
                .read_credentials(&credentials_url, cx)
                .await
                .log_err()
                .flatten()?;

            Some(Credentials {
                user_id: user_id.parse().ok()?,
                access_token: String::from_utf8(access_token).ok()?,
            })
        }
        .boxed_local()
    }
}

impl Default for ClientState {
    fn default() -> Self {
        Self {
            credentials: None,
            status: watch::channel_with(Status::SignedOut),
            _reconnect_task: None,
        }
    }
}

pub enum Subscription {
    Entity {
        client: Weak<Client>,
        id: (TypeId, u64),
    },
    Message {
        client: Weak<Client>,
        id: TypeId,
    },
}

impl Drop for Subscription {
    fn drop(&mut self) {
        match self {
            Subscription::Entity { client, id } => {
                if let Some(client) = client.upgrade() {
                    let mut state = client.handler_set.lock();
                    let _ = state.entities_by_type_and_remote_id.remove(id);
                }
            }
            Subscription::Message { client, id } => {
                if let Some(client) = client.upgrade() {
                    let mut state = client.handler_set.lock();
                    let _ = state.entity_types_by_message_type.remove(id);
                    let _ = state.message_handlers.remove(id);
                }
            }
        }
    }
}

pub struct PendingEntitySubscription<T: 'static> {
    client: Arc<Client>,
    remote_id: u64,
    _entity_type: PhantomData<T>,
    consumed: bool,
}

impl<T: 'static> PendingEntitySubscription<T> {
    pub fn set_entity(mut self, entity: &Entity<T>, cx: &AsyncApp) -> Subscription {
        self.consumed = true;
        let mut handlers = self.client.handler_set.lock();
        let id = (TypeId::of::<T>(), self.remote_id);
        let Some(EntityMessageSubscriber::Pending(messages)) =
            handlers.entities_by_type_and_remote_id.remove(&id)
        else {
            unreachable!()
        };

        handlers.entities_by_type_and_remote_id.insert(
            id,
            EntityMessageSubscriber::Entity {
                handle: entity.downgrade().into(),
            },
        );
        drop(handlers);
        for message in messages {
            let client_id = self.client.id();
            let type_name = message.payload_type_name();
            let sender_id = message.original_sender_id();
            log::debug!(
                "handling queued rpc message. client_id:{}, sender_id:{:?}, type:{}",
                client_id,
                sender_id,
                type_name
            );
            self.client.handle_message(message, cx);
        }
        Subscription::Entity {
            client: Arc::downgrade(&self.client),
            id,
        }
    }
}

impl<T: 'static> Drop for PendingEntitySubscription<T> {
    fn drop(&mut self) {
        if !self.consumed {
            let mut state = self.client.handler_set.lock();
            if let Some(EntityMessageSubscriber::Pending(messages)) = state
                .entities_by_type_and_remote_id
                .remove(&(TypeId::of::<T>(), self.remote_id))
            {
                for message in messages {
                    log::info!("unhandled message {}", message.payload_type_name());
                }
            }
        }
    }
}

impl Client {
    pub fn new(
        _clock: Arc<dyn SystemClock>,
        http: Arc<HttpClientWithUrl>,
        cx: &mut App,
    ) -> Arc<Self> {
        Arc::new(Self {
            id: AtomicU64::new(0),
            peer: Peer::new(0),
            http,
            credentials_provider: ClientCredentialsProvider::new(cx),
            state: Default::default(),
            handler_set: Default::default(),

            #[cfg(any(test, feature = "test-support"))]
            authenticate: Default::default(),
            #[cfg(any(test, feature = "test-support"))]
            establish_connection: Default::default(),
            #[cfg(any(test, feature = "test-support"))]
            rpc_url: RwLock::default(),
        })
    }

    pub fn production(cx: &mut App) -> Arc<Self> {
        let clock = Arc::new(clock::RealSystemClock);
        let http = Arc::new(HttpClientWithUrl::new_url(
            cx.http_client(),
            &ClientSettings::get_global(cx).server_url,
            cx.http_client().proxy().cloned(),
        ));
        Self::new(clock, http, cx)
    }

    pub fn id(&self) -> u64 {
        self.id.load(Ordering::SeqCst)
    }

    pub fn http_client(&self) -> Arc<HttpClientWithUrl> {
        self.http.clone()
    }

    pub fn credentials_provider(&self) -> Arc<dyn CredentialsProvider> {
        self.credentials_provider.provider.clone()
    }

    pub fn set_id(&self, id: u64) -> &Self {
        self.id.store(id, Ordering::SeqCst);
        self
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn teardown(&self) {
        let mut state = self.state.write();
        state._reconnect_task.take();
        self.handler_set.lock().clear();
        self.peer.teardown();
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn override_authenticate<F>(&self, authenticate: F) -> &Self
    where
        F: 'static + Send + Sync + Fn(&AsyncApp) -> Task<Result<Credentials>>,
    {
        *self.authenticate.write() = Some(Box::new(authenticate));
        self
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn override_establish_connection<F>(&self, connect: F) -> &Self
    where
        F: 'static
            + Send
            + Sync
            + Fn(&Credentials, &AsyncApp) -> Task<Result<Connection, EstablishConnectionError>>,
    {
        *self.establish_connection.write() = Some(Box::new(connect));
        self
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn override_rpc_url(&self, url: Url) -> &Self {
        *self.rpc_url.write() = Some(url);
        self
    }

    pub fn global(cx: &App) -> Arc<Self> {
        cx.global::<GlobalClient>().0.clone()
    }
    pub fn set_global(client: Arc<Client>, cx: &mut App) {
        cx.set_global(GlobalClient(client))
    }

    pub fn user_id(&self) -> Option<u64> {
        self.state
            .read()
            .credentials
            .as_ref()
            .map(|credentials| credentials.user_id)
    }

    pub fn peer_id(&self) -> Option<PeerId> {
        if let Status::Connected { peer_id, .. } = &*self.status().borrow() {
            Some(*peer_id)
        } else {
            None
        }
    }

    pub fn status(&self) -> watch::Receiver<Status> {
        self.state.read().status.1.clone()
    }

    fn set_status(self: &Arc<Self>, status: Status, cx: &AsyncApp) {
        log::info!("set status on client {}: {:?}", self.id(), status);
        let mut state = self.state.write();
        *state.status.0.borrow_mut() = status;

        match status {
            Status::Connected { .. } => {
                state._reconnect_task = None;
            }
            Status::ConnectionLost => {
                let client = self.clone();
                state._reconnect_task = Some(cx.spawn(async move |cx| {
                    #[cfg(any(test, feature = "test-support"))]
                    let mut rng = StdRng::seed_from_u64(0);
                    #[cfg(not(any(test, feature = "test-support")))]
                    let mut rng = StdRng::from_rng(&mut rand::rng());

                    let mut delay = INITIAL_RECONNECTION_DELAY;
                    loop {
                        match client.connect(true, cx).await {
                            ConnectionResult::Timeout => {
                                log::error!("client connect attempt timed out")
                            }
                            ConnectionResult::ConnectionReset => {
                                log::error!("client connect attempt reset")
                            }
                            ConnectionResult::Result(r) => {
                                if let Err(error) = r {
                                    log::error!("failed to connect: {error}");
                                } else {
                                    break;
                                }
                            }
                        }

                        if matches!(
                            *client.status().borrow(),
                            Status::AuthenticationError | Status::ConnectionError
                        ) {
                            client.set_status(
                                Status::ReconnectionError {
                                    next_reconnection: Instant::now() + delay,
                                },
                                cx,
                            );
                            let jitter = Duration::from_millis(
                                rng.random_range(0..delay.as_millis() as u64),
                            );
                            cx.background_executor().timer(delay + jitter).await;
                            delay = cmp::min(delay * 2, MAX_RECONNECTION_DELAY);
                        } else {
                            break;
                        }
                    }
                }));
            }
            Status::SignedOut | Status::UpgradeRequired => {
                state._reconnect_task.take();
            }
            _ => {}
        }
    }

    pub fn subscribe_to_entity<T>(
        self: &Arc<Self>,
        remote_id: u64,
    ) -> Result<PendingEntitySubscription<T>>
    where
        T: 'static,
    {
        let id = (TypeId::of::<T>(), remote_id);

        let mut state = self.handler_set.lock();
        anyhow::ensure!(
            !state.entities_by_type_and_remote_id.contains_key(&id),
            "already subscribed to entity"
        );

        state
            .entities_by_type_and_remote_id
            .insert(id, EntityMessageSubscriber::Pending(Default::default()));

        Ok(PendingEntitySubscription {
            client: self.clone(),
            remote_id,
            consumed: false,
            _entity_type: PhantomData,
        })
    }

    #[track_caller]
    pub fn add_message_handler<M, E, H, F>(
        self: &Arc<Self>,
        entity: WeakEntity<E>,
        handler: H,
    ) -> Subscription
    where
        M: EnvelopedMessage,
        E: 'static,
        H: 'static + Sync + Fn(Entity<E>, TypedEnvelope<M>, AsyncApp) -> F + Send + Sync,
        F: 'static + Future<Output = Result<()>>,
    {
        self.add_message_handler_impl(entity, move |entity, message, _, cx| {
            handler(entity, message, cx)
        })
    }

    fn add_message_handler_impl<M, E, H, F>(
        self: &Arc<Self>,
        entity: WeakEntity<E>,
        handler: H,
    ) -> Subscription
    where
        M: EnvelopedMessage,
        E: 'static,
        H: 'static
            + Sync
            + Fn(Entity<E>, TypedEnvelope<M>, AnyProtoClient, AsyncApp) -> F
            + Send
            + Sync,
        F: 'static + Future<Output = Result<()>>,
    {
        let message_type_id = TypeId::of::<M>();
        let mut state = self.handler_set.lock();
        state
            .entities_by_message_type
            .insert(message_type_id, entity.into());

        let prev_handler = state.message_handlers.insert(
            message_type_id,
            Arc::new(move |subscriber, envelope, client, cx| {
                let subscriber = subscriber.downcast::<E>().unwrap();
                let envelope = envelope.into_any().downcast::<TypedEnvelope<M>>().unwrap();
                handler(subscriber, *envelope, client, cx).boxed_local()
            }),
        );
        if prev_handler.is_some() {
            let location = std::panic::Location::caller();
            panic!(
                "{}:{} registered handler for the same message {} twice",
                location.file(),
                location.line(),
                std::any::type_name::<M>()
            );
        }

        Subscription::Message {
            client: Arc::downgrade(self),
            id: message_type_id,
        }
    }

    pub fn add_request_handler<M, E, H, F>(
        self: &Arc<Self>,
        entity: WeakEntity<E>,
        handler: H,
    ) -> Subscription
    where
        M: RequestMessage,
        E: 'static,
        H: 'static + Sync + Fn(Entity<E>, TypedEnvelope<M>, AsyncApp) -> F + Send + Sync,
        F: 'static + Future<Output = Result<M::Response>>,
    {
        self.add_message_handler_impl(entity, move |handle, envelope, this, cx| {
            Self::respond_to_request(envelope.receipt(), handler(handle, envelope, cx), this)
        })
    }

    async fn respond_to_request<T: RequestMessage, F: Future<Output = Result<T::Response>>>(
        receipt: Receipt<T>,
        response: F,
        client: AnyProtoClient,
    ) -> Result<()> {
        match response.await {
            Ok(response) => {
                client.send_response(receipt.message_id, response)?;
                Ok(())
            }
            Err(error) => {
                client.send_response(receipt.message_id, error.to_proto())?;
                Err(error)
            }
        }
    }

    pub async fn sign_in(
        self: &Arc<Self>,
        try_provider: bool,
        cx: &AsyncApp,
    ) -> Result<Credentials> {
        let is_reauthenticating = if self.status().borrow().is_signed_out() {
            self.set_status(Status::Authenticating, cx);
            false
        } else {
            self.set_status(Status::Reauthenticating, cx);
            true
        };

        let mut credentials = self.state.read().credentials.clone();

        if credentials.is_none()
            && try_provider
            && let Some(stored_credentials) = self.credentials_provider.read_credentials(cx).await
        {
            credentials = Some(stored_credentials);
        }

        if credentials.is_none() {
            let mut status_rx = self.status();
            let _ = status_rx.next().await;
            futures::select_biased! {
                authenticate = self.authenticate(cx).fuse() => {
                    match authenticate {
                        Ok(creds) => credentials = Some(creds),
                        Err(err) => {
                            self.set_status(Status::AuthenticationError, cx);
                            return Err(err);
                        }
                    }
                }
                _ = status_rx.next().fuse() => {
                    return Err(anyhow!("authentication canceled"));
                }
            }
        }

        let credentials = credentials.unwrap();
        self.set_id(credentials.user_id);
        self.state.write().credentials = Some(credentials.clone());
        self.set_status(
            if is_reauthenticating {
                Status::Reauthenticated
            } else {
                Status::Authenticated
            },
            cx,
        );

        Ok(credentials)
    }

    pub async fn connect(
        self: &Arc<Self>,
        try_provider: bool,
        cx: &AsyncApp,
    ) -> ConnectionResult<()> {
        let remote_enabled = cx.update(|cx| ClientSettings::get_global(cx).remote_server_enabled());
        if !remote_enabled && !cfg!(test) {
            return ConnectionResult::Result(Ok(()));
        }
        let was_disconnected = match *self.status().borrow() {
            Status::SignedOut | Status::Authenticated => true,
            Status::ConnectionError
            | Status::ConnectionLost
            | Status::Authenticating
            | Status::AuthenticationError
            | Status::Reauthenticating
            | Status::Reauthenticated
            | Status::ReconnectionError { .. } => false,
            Status::Connected { .. } | Status::Connecting | Status::Reconnecting => {
                return ConnectionResult::Result(Ok(()));
            }
            Status::UpgradeRequired => {
                return ConnectionResult::Result(
                    Err(EstablishConnectionError::UpgradeRequired)
                        .context("client auth and connect"),
                );
            }
        };
        let credentials = match self.sign_in(try_provider, cx).await {
            Ok(credentials) => credentials,
            Err(err) => return ConnectionResult::Result(Err(err)),
        };

        if was_disconnected {
            self.set_status(Status::Connecting, cx);
        } else {
            self.set_status(Status::Reconnecting, cx);
        }

        self.connect_with_credentials(credentials, cx).await
    }

    async fn connect_with_credentials(
        self: &Arc<Self>,
        credentials: Credentials,
        cx: &AsyncApp,
    ) -> ConnectionResult<()> {
        let mut timeout =
            futures::FutureExt::fuse(cx.background_executor().timer(CONNECTION_TIMEOUT));
        futures::select_biased! {
            connection = self.establish_connection(&credentials, cx).fuse() => {
                match connection {
                    Ok(conn) => {
                        futures::select_biased! {
                            result = self.set_connection(conn, cx).fuse() => {
                                match result.context("client auth and connect") {
                                    Ok(()) => ConnectionResult::Result(Ok(())),
                                    Err(err) => {
                                        self.set_status(Status::ConnectionError, cx);
                                        ConnectionResult::Result(Err(err))
                                    },
                                }
                            },
                            _ = timeout => {
                                self.set_status(Status::ConnectionError, cx);
                                ConnectionResult::Timeout
                            }
                        }
                    }
                    Err(EstablishConnectionError::Unauthorized) => {
                        self.state.write().credentials = None;
                        self.set_status(Status::ConnectionError, cx);
                        ConnectionResult::Result(Err(EstablishConnectionError::Unauthorized).context("client auth and connect"))
                    }
                    Err(EstablishConnectionError::UpgradeRequired) => {
                        self.set_status(Status::UpgradeRequired, cx);
                        ConnectionResult::Result(Err(EstablishConnectionError::UpgradeRequired).context("client auth and connect"))
                    }
                    Err(error) => {
                        self.set_status(Status::ConnectionError, cx);
                        ConnectionResult::Result(Err(error).context("client auth and connect"))
                    }
                }
            }
            _ = &mut timeout => {
                self.set_status(Status::ConnectionError, cx);
                ConnectionResult::Timeout
            }
        }
    }

    async fn set_connection(self: &Arc<Self>, conn: Connection, cx: &AsyncApp) -> Result<()> {
        let executor = cx.background_executor();
        log::debug!("add connection to peer");
        let (connection_id, handle_io, mut incoming) = self.peer.add_connection(conn, {
            let executor = executor.clone();
            move |duration| executor.timer(duration)
        });
        let handle_io = executor.spawn(handle_io);

        let peer_id = async {
            log::debug!("waiting for server hello");
            let message = incoming.next().await.context("no hello message received")?;
            log::debug!("got server hello");
            let hello_message_type_name = message.payload_type_name().to_owned();
            let hello = message
                .into_any()
                .downcast::<TypedEnvelope<proto::Hello>>()
                .map_err(|_| {
                    anyhow!(
                        "invalid hello message received: {:?}",
                        hello_message_type_name
                    )
                })?;
            let peer_id = hello.payload.peer_id.context("invalid peer id")?;
            Ok(peer_id)
        };

        let peer_id = match peer_id.await {
            Ok(peer_id) => peer_id,
            Err(error) => {
                self.peer.disconnect(connection_id);
                return Err(error);
            }
        };

        log::debug!(
            "set status to connected (connection id: {:?}, peer id: {:?})",
            connection_id,
            peer_id
        );
        self.set_status(
            Status::Connected {
                peer_id,
                connection_id,
            },
            cx,
        );

        cx.spawn({
            let this = self.clone();
            async move |cx| {
                while let Some(message) = incoming.next().await {
                    this.handle_message(message, cx);
                    // Don't starve the main thread when receiving lots of messages at once.
                    smol::future::yield_now().await;
                }
            }
        })
        .detach();

        cx.spawn({
            let this = self.clone();
            async move |cx| match handle_io.await {
                Ok(()) => {
                    if *this.status().borrow()
                        == (Status::Connected {
                            connection_id,
                            peer_id,
                        })
                    {
                        this.set_status(Status::SignedOut, cx);
                    }
                }
                Err(err) => {
                    log::error!("connection error: {:?}", err);
                    this.set_status(Status::ConnectionLost, cx);
                }
            }
        })
        .detach();

        Ok(())
    }

    fn authenticate(self: &Arc<Self>, cx: &AsyncApp) -> Task<Result<Credentials>> {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(callback) = self.authenticate.read().as_ref() {
            return callback(cx);
        }

        let _ = cx;
        Task::ready(Err(anyhow!(
            "no stored credentials for the configured remote server"
        )))
    }

    fn establish_connection(
        self: &Arc<Self>,
        credentials: &Credentials,
        cx: &AsyncApp,
    ) -> Task<Result<Connection, EstablishConnectionError>> {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(callback) = self.establish_connection.read().as_ref() {
            return callback(credentials, cx);
        }

        self.establish_websocket_connection(credentials, cx)
    }

    fn rpc_url(
        &self,
        http: Arc<HttpClientWithUrl>,
        release_channel: Option<ReleaseChannel>,
    ) -> impl Future<Output = Result<url::Url>> + use<> {
        #[cfg(any(test, feature = "test-support"))]
        let url_override = self.rpc_url.read().clone();

        async move {
            #[cfg(any(test, feature = "test-support"))]
            if let Some(url) = url_override {
                return Ok(url);
            }

            if let Some(url) = &*ZZZ_RPC_URL {
                return Url::parse(url).context("invalid rpc url");
            }

            let mut url = http.build_url("/rpc");
            if let Some(preview_param) =
                release_channel.and_then(|channel| channel.release_query_param())
            {
                url += "?";
                url += preview_param;
            }

            let response = http.get(&url, Default::default(), false).await?;
            anyhow::ensure!(
                response.status().is_redirection(),
                "unexpected /rpc response status {}",
                response.status()
            );
            let collab_url = response
                .headers()
                .get("Location")
                .context("missing location header in /rpc response")?
                .to_str()
                .map_err(EstablishConnectionError::other)?
                .to_owned();
            Url::parse(&collab_url).with_context(|| format!("parsing collab rpc url {collab_url}"))
        }
    }

    fn establish_websocket_connection(
        self: &Arc<Self>,
        credentials: &Credentials,
        cx: &AsyncApp,
    ) -> Task<Result<Connection, EstablishConnectionError>> {
        let release_channel = cx.update(|cx| ReleaseChannel::try_global(cx));
        let app_version = cx.update(|cx| AppVersion::global(cx).to_string());

        let http = self.http.clone();
        let proxy = http.proxy().cloned();
        let user_agent = http.user_agent().cloned();
        let credentials = credentials.clone();
        let rpc_url = self.rpc_url(http, release_channel);
        cx.spawn(async move |cx| {
            use HttpOrHttps::*;

            #[derive(Debug)]
            enum HttpOrHttps {
                Http,
                Https,
            }

            let mut rpc_url = rpc_url.await?;
            let url_scheme = match rpc_url.scheme() {
                "https" => Https,
                "http" => Http,
                _ => Err(anyhow!("invalid rpc url: {}", rpc_url))?,
            };

            let stream = gpui_tokio::Tokio::spawn_result(cx, {
                let rpc_url = rpc_url.clone();
                async move {
                    let rpc_host = rpc_url
                        .host_str()
                        .zip(rpc_url.port_or_known_default())
                        .context("missing host in rpc url")?;
                    Ok(match proxy {
                        Some(proxy) => connect_proxy_stream(&proxy, rpc_host).await?,
                        None => Box::new(TcpStream::connect(rpc_host).await?),
                    })
                }
            })
            .await?;

            log::info!("connected to rpc endpoint {}", rpc_url);

            rpc_url
                .set_scheme(match url_scheme {
                    Https => "wss",
                    Http => "ws",
                })
                .unwrap();

            // We call `into_client_request` to let `tungstenite` construct the WebSocket request
            // for us from the RPC URL.
            //
            // Among other things, it will generate and set a `Sec-WebSocket-Key` header for us.
            let mut request = IntoClientRequest::into_client_request(rpc_url.as_str())?;

            // We then modify the request to add our desired headers.
            let request_headers = request.headers_mut();
            request_headers.insert(
                http::header::AUTHORIZATION,
                HeaderValue::from_str(&credentials.authorization_header())?,
            );
            request_headers.insert(
                "x-zzz-protocol-version",
                HeaderValue::from_str(&rpc::PROTOCOL_VERSION.to_string())?,
            );
            request_headers.insert("x-zzz-app-version", HeaderValue::from_str(&app_version)?);
            request_headers.insert(
                "x-zzz-release-channel",
                HeaderValue::from_str(release_channel.map(|r| r.dev_name()).unwrap_or("unknown"))?,
            );
            if let Some(user_agent) = user_agent {
                request_headers.insert(http::header::USER_AGENT, user_agent);
            }

            let (stream, _) = async_tungstenite::tokio::client_async_tls_with_connector_and_config(
                request,
                stream,
                Some(Arc::new(http_client_tls::tls_config()?).into()),
                None,
            )
            .await?;

            Ok(Connection::new(
                stream
                    .map_err(|error| anyhow!(error))
                    .sink_map_err(|error| anyhow!(error)),
            ))
        })
    }

    pub fn disconnect(self: &Arc<Self>, cx: &AsyncApp) {
        self.peer.teardown();
        self.set_status(Status::SignedOut, cx);
    }

    pub fn reconnect(self: &Arc<Self>, cx: &AsyncApp) {
        self.peer.teardown();
        self.set_status(Status::ConnectionLost, cx);
    }

    fn connection_id(&self) -> Result<ConnectionId> {
        if let Status::Connected { connection_id, .. } = *self.status().borrow() {
            Ok(connection_id)
        } else {
            anyhow::bail!("not connected");
        }
    }

    pub fn send<T: EnvelopedMessage>(&self, message: T) -> Result<()> {
        log::debug!("rpc send. client_id:{}, name:{}", self.id(), T::NAME);
        self.peer.send(self.connection_id()?, message)
    }

    pub fn request<T: RequestMessage>(
        &self,
        request: T,
    ) -> impl Future<Output = Result<T::Response>> + use<T> {
        self.request_envelope(request)
            .map_ok(|envelope| envelope.payload)
    }

    pub fn request_stream<T: RequestMessage>(
        &self,
        request: T,
    ) -> impl Future<Output = Result<impl Stream<Item = Result<T::Response>>>> {
        let client_id = self.id.load(Ordering::SeqCst);
        log::debug!(
            "rpc request start. client_id:{}. name:{}",
            client_id,
            T::NAME
        );
        let response = self
            .connection_id()
            .map(|conn_id| self.peer.request_stream(conn_id, request));
        async move {
            let response = response?.await;
            log::debug!(
                "rpc request finish. client_id:{}. name:{}",
                client_id,
                T::NAME
            );
            response
        }
    }

    pub fn request_envelope<T: RequestMessage>(
        &self,
        request: T,
    ) -> impl Future<Output = Result<TypedEnvelope<T::Response>>> + use<T> {
        let client_id = self.id();
        log::debug!(
            "rpc request start. client_id:{}. name:{}",
            client_id,
            T::NAME
        );
        let response = self
            .connection_id()
            .map(|conn_id| self.peer.request_envelope(conn_id, request));
        async move {
            let response = response?.await;
            log::debug!(
                "rpc request finish. client_id:{}. name:{}",
                client_id,
                T::NAME
            );
            response
        }
    }

    pub fn request_dynamic(
        &self,
        envelope: proto::Envelope,
        request_type: &'static str,
    ) -> impl Future<Output = Result<proto::Envelope>> + use<> {
        let client_id = self.id();
        log::debug!(
            "rpc request start. client_id:{}. name:{}",
            client_id,
            request_type
        );
        let response = self
            .connection_id()
            .map(|conn_id| self.peer.request_dynamic(conn_id, envelope, request_type));
        async move {
            let response = response?.await;
            log::debug!(
                "rpc request finish. client_id:{}. name:{}",
                client_id,
                request_type
            );
            Ok(response?.0)
        }
    }

    fn handle_message(self: &Arc<Client>, message: Box<dyn AnyTypedEnvelope>, cx: &AsyncApp) {
        let sender_id = message.sender_id();
        let request_id = message.message_id();
        let type_name = message.payload_type_name();
        let original_sender_id = message.original_sender_id();

        if let Some(future) = ProtoMessageHandlerSet::handle_message(
            &self.handler_set,
            message,
            self.clone().into(),
            cx.clone(),
        ) {
            let client_id = self.id();
            log::debug!(
                "rpc message received. client_id:{}, sender_id:{:?}, type:{}",
                client_id,
                original_sender_id,
                type_name
            );
            cx.spawn(async move |_| match future.await {
                Ok(()) => {
                    log::debug!("rpc message handled. client_id:{client_id}, sender_id:{original_sender_id:?}, type:{type_name}");
                }
                Err(error) => {
                    log::error!("error handling message. client_id:{client_id}, sender_id:{original_sender_id:?}, type:{type_name}, error:{error:#}");
                }
            })
            .detach();
        } else {
            log::info!("unhandled message {}", type_name);
            self.peer
                .respond_with_unhandled_message(sender_id.into(), request_id, type_name)
                .log_err();
        }
    }
}

impl ProtoClient for Client {
    fn request(
        &self,
        envelope: proto::Envelope,
        request_type: &'static str,
    ) -> BoxFuture<'static, Result<proto::Envelope>> {
        self.request_dynamic(envelope, request_type).boxed()
    }

    fn request_stream(
        &self,
        envelope: proto::Envelope,
        _request_type: &'static str,
    ) -> BoxFuture<'static, Result<futures::stream::BoxStream<'static, Result<proto::Envelope>>>>
    {
        let peer = self.peer.clone();
        let connection_id = self.connection_id();
        async move {
            let connection_id = connection_id?;
            let stream = peer
                .request_stream_dynamic(connection_id, envelope, "stream request")
                .await?;
            Ok(stream.boxed())
        }
        .boxed()
    }

    fn send(&self, envelope: proto::Envelope, message_type: &'static str) -> Result<()> {
        log::debug!("rpc send. client_id:{}, name:{}", self.id(), message_type);
        let connection_id = self.connection_id()?;
        self.peer.send_dynamic(connection_id, envelope)
    }

    fn send_response(&self, envelope: proto::Envelope, message_type: &'static str) -> Result<()> {
        log::debug!(
            "rpc respond. client_id:{}, name:{}",
            self.id(),
            message_type
        );
        let connection_id = self.connection_id()?;
        self.peer.send_dynamic(connection_id, envelope)
    }

    fn message_handler_set(&self) -> &Mutex<ProtoMessageHandlerSet> {
        &self.handler_set
    }

    fn has_wsl_interop(&self) -> bool {
        false
    }
}

/// Prefix for the zzz:// URL scheme.
pub const ZZZ_URL_SCHEME: &str = "zzz";

/// A parsed ZZZ link that can be handled internally by the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZZZLink {}

/// Parses the given link into a ZZZ link.
///
/// Returns [`None`] for links that should be opened in the browser.
pub fn parse_zzz_link(_link: &str, _cx: &App) -> Option<ZZZLink> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::FakeServer;

    use clock::FakeSystemClock;
    use gpui::{AppContext as _, BackgroundExecutor, TestAppContext};
    use http_client::FakeHttpClient;
    use parking_lot::Mutex;
    use proto::TypedEnvelope;
    use settings::SettingsStore;
    use std::future;

    #[test]
    fn test_proxy_settings_trims_and_ignores_empty_proxy() {
        let mut content = SettingsContent::default();
        content.proxy = Some("   ".to_owned());
        assert_eq!(ProxySettings::from_settings(&content).proxy, None);

        content.proxy = Some("http://127.0.0.1:10809".to_owned());
        assert_eq!(
            ProxySettings::from_settings(&content).proxy.as_deref(),
            Some("http://127.0.0.1:10809")
        );
    }

    #[test]
    fn default_server_is_local_and_remote_requires_explicit_configuration() {
        let local = ClientSettings {
            server_url: "http://127.0.0.1:7331".into(),
            credentials_url: None,
        };
        assert!(!local.remote_server_enabled());

        let remote = ClientSettings {
            server_url: "https://example.invalid".into(),
            credentials_url: None,
        };
        assert!(remote.remote_server_enabled());
    }

    #[gpui::test(iterations = 10)]
    async fn test_reconnection(cx: &mut TestAppContext) {
        init_test(cx);
        let user_id = 5;
        let client = cx.update(|cx| {
            Client::new(
                Arc::new(FakeSystemClock::new()),
                FakeHttpClient::with_404_response(),
                cx,
            )
        });
        let server = FakeServer::for_client(user_id, &client, cx).await;
        let mut status = client.status();
        assert!(matches!(
            status.next().await,
            Some(Status::Connected { .. })
        ));
        assert_eq!(server.auth_count(), 1);

        server.forbid_connections();
        server.disconnect();
        while !matches!(status.next().await, Some(Status::ReconnectionError { .. })) {}

        server.allow_connections();
        cx.executor().advance_clock(Duration::from_secs(10));
        while !matches!(status.next().await, Some(Status::Connected { .. })) {}
        assert_eq!(server.auth_count(), 1); // Client reused the cached credentials when reconnecting

        server.forbid_connections();
        server.disconnect();
        while !matches!(status.next().await, Some(Status::ReconnectionError { .. })) {}

        // Clear cached credentials after authentication fails
        server.roll_access_token();
        server.allow_connections();
        cx.executor().run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(10));
        while !matches!(status.next().await, Some(Status::Connected { .. })) {}
        assert_eq!(server.auth_count(), 2); // Client re-authenticated due to an invalid token
    }

    #[gpui::test(iterations = 10)]
    async fn test_connection_timeout(executor: BackgroundExecutor, cx: &mut TestAppContext) {
        init_test(cx);
        let user_id = 5;
        let client = cx.update(|cx| {
            Client::new(
                Arc::new(FakeSystemClock::new()),
                FakeHttpClient::with_404_response(),
                cx,
            )
        });
        let mut status = client.status();

        // Time out when client tries to connect.
        client.override_authenticate(move |cx| {
            cx.background_spawn(async move {
                Ok(Credentials {
                    user_id,
                    access_token: "0".into(),
                })
            })
        });
        client.override_establish_connection(|_, cx| {
            cx.background_spawn(async move {
                future::pending::<()>().await;
                unreachable!()
            })
        });
        let auth_and_connect = cx.spawn({
            let client = client.clone();
            |cx| async move { client.connect(false, &cx).await }
        });
        executor.run_until_parked();
        assert!(matches!(status.next().await, Some(Status::Connecting)));

        executor.advance_clock(CONNECTION_TIMEOUT);
        assert!(matches!(status.next().await, Some(Status::ConnectionError)));
        auth_and_connect.await.into_response().unwrap_err();

        // Allow the connection to be established.
        let server = FakeServer::for_client(user_id, &client, cx).await;
        assert!(matches!(
            status.next().await,
            Some(Status::Connected { .. })
        ));

        // Disconnect client.
        server.forbid_connections();
        server.disconnect();
        while !matches!(status.next().await, Some(Status::ReconnectionError { .. })) {}

        // Time out when re-establishing the connection.
        server.allow_connections();
        client.override_establish_connection(|_, cx| {
            cx.background_spawn(async move {
                future::pending::<()>().await;
                unreachable!()
            })
        });
        executor.advance_clock(2 * INITIAL_RECONNECTION_DELAY);
        assert!(matches!(status.next().await, Some(Status::Reconnecting)));

        executor.advance_clock(CONNECTION_TIMEOUT);
        assert!(matches!(
            status.next().await,
            Some(Status::ReconnectionError { .. })
        ));
    }

    #[gpui::test(iterations = 10)]
    async fn test_sign_in_reuses_cached_credentials(cx: &mut TestAppContext) {
        init_test(cx);
        let auth_count = Arc::new(Mutex::new(0));
        let client = cx.update(|cx| {
            Client::new(
                Arc::new(FakeSystemClock::new()),
                FakeHttpClient::with_404_response(),
                cx,
            )
        });
        client.override_authenticate({
            let auth_count = auth_count.clone();
            move |cx| {
                let auth_count = auth_count.clone();
                cx.background_spawn(async move {
                    *auth_count.lock() += 1;
                    Ok(Credentials {
                        user_id: 1,
                        access_token: auth_count.lock().to_string(),
                    })
                })
            }
        });

        let credentials = client.sign_in(false, &cx.to_async()).await.unwrap();
        assert_eq!(*auth_count.lock(), 1);
        assert_eq!(credentials.access_token, "1");

        // Cached credentials are reused without triggering authentication again.
        let credentials = client.sign_in(false, &cx.to_async()).await.unwrap();
        assert_eq!(*auth_count.lock(), 1);
        assert_eq!(credentials.access_token, "1");
    }

    #[gpui::test(iterations = 10)]
    async fn test_authenticating_more_than_once(
        cx: &mut TestAppContext,
        executor: BackgroundExecutor,
    ) {
        init_test(cx);
        let auth_count = Arc::new(Mutex::new(0));
        let dropped_auth_count = Arc::new(Mutex::new(0));
        let client = cx.update(|cx| {
            Client::new(
                Arc::new(FakeSystemClock::new()),
                FakeHttpClient::with_404_response(),
                cx,
            )
        });
        client.override_authenticate({
            let auth_count = auth_count.clone();
            let dropped_auth_count = dropped_auth_count.clone();
            move |cx| {
                let auth_count = auth_count.clone();
                let dropped_auth_count = dropped_auth_count.clone();
                cx.background_spawn(async move {
                    *auth_count.lock() += 1;
                    let _drop = util::defer(move || *dropped_auth_count.lock() += 1);
                    future::pending::<()>().await;
                    unreachable!()
                })
            }
        });

        let _authenticate = cx.spawn({
            let client = client.clone();
            move |cx| async move { client.connect(false, &cx).await }
        });
        executor.run_until_parked();
        assert_eq!(*auth_count.lock(), 1);
        assert_eq!(*dropped_auth_count.lock(), 0);

        let _authenticate = cx.spawn(|cx| async move { client.connect(false, &cx).await });
        executor.run_until_parked();
        assert_eq!(*auth_count.lock(), 2);
        assert_eq!(*dropped_auth_count.lock(), 1);
    }

    #[gpui::test]
    async fn test_subscribing_to_entity(cx: &mut TestAppContext) {
        init_test(cx);
        let user_id = 5;
        let client = cx.update(|cx| {
            Client::new(
                Arc::new(FakeSystemClock::new()),
                FakeHttpClient::with_404_response(),
                cx,
            )
        });
        let server = FakeServer::for_client(user_id, &client, cx).await;

        let (done_tx1, done_rx1) = async_channel::unbounded();
        let (done_tx2, done_rx2) = async_channel::unbounded();
        AnyProtoClient::from(client.clone()).add_entity_message_handler(
            move |entity: Entity<TestEntity>, _: TypedEnvelope<proto::UpdateProject>, cx| {
                match entity.read_with(&cx, |entity, _| entity.id) {
                    1 => done_tx1.try_send(()).unwrap(),
                    2 => done_tx2.try_send(()).unwrap(),
                    _ => unreachable!(),
                }
                async { Ok(()) }
            },
        );
        let entity1 = cx.new(|_| TestEntity {
            id: 1,
            subscription: None,
        });
        let entity2 = cx.new(|_| TestEntity {
            id: 2,
            subscription: None,
        });
        let entity3 = cx.new(|_| TestEntity {
            id: 3,
            subscription: None,
        });

        let _subscription1 = client
            .subscribe_to_entity(1)
            .unwrap()
            .set_entity(&entity1, &cx.to_async());
        let _subscription2 = client
            .subscribe_to_entity(2)
            .unwrap()
            .set_entity(&entity2, &cx.to_async());
        // Ensure dropping a subscription for the same entity type still allows receiving of
        // messages for other entity IDs of the same type.
        let subscription3 = client
            .subscribe_to_entity(3)
            .unwrap()
            .set_entity(&entity3, &cx.to_async());
        drop(subscription3);

        server.send(proto::UpdateProject {
            project_id: 1,
            worktrees: Vec::new(),
        });
        server.send(proto::UpdateProject {
            project_id: 2,
            worktrees: Vec::new(),
        });
        done_rx1.recv().await.unwrap();
        done_rx2.recv().await.unwrap();
    }

    #[gpui::test]
    async fn test_subscribing_after_dropping_subscription(cx: &mut TestAppContext) {
        init_test(cx);
        let user_id = 5;
        let client = cx.update(|cx| {
            Client::new(
                Arc::new(FakeSystemClock::new()),
                FakeHttpClient::with_404_response(),
                cx,
            )
        });
        let server = FakeServer::for_client(user_id, &client, cx).await;

        let entity = cx.new(|_| TestEntity::default());
        let (done_tx1, _done_rx1) = async_channel::unbounded();
        let (done_tx2, done_rx2) = async_channel::unbounded();
        let subscription1 = client.add_message_handler(
            entity.downgrade(),
            move |_, _: TypedEnvelope<proto::Ping>, _| {
                done_tx1.try_send(()).unwrap();
                async { Ok(()) }
            },
        );
        drop(subscription1);
        let _subscription2 = client.add_message_handler(
            entity.downgrade(),
            move |_, _: TypedEnvelope<proto::Ping>, _| {
                done_tx2.try_send(()).unwrap();
                async { Ok(()) }
            },
        );
        server.send(proto::Ping {});
        done_rx2.recv().await.unwrap();
    }

    #[gpui::test]
    async fn test_dropping_subscription_in_handler(cx: &mut TestAppContext) {
        init_test(cx);
        let user_id = 5;
        let client = cx.update(|cx| {
            Client::new(
                Arc::new(FakeSystemClock::new()),
                FakeHttpClient::with_404_response(),
                cx,
            )
        });
        let server = FakeServer::for_client(user_id, &client, cx).await;

        let entity = cx.new(|_| TestEntity::default());
        let (done_tx, done_rx) = async_channel::unbounded();
        let subscription = client.add_message_handler(
            entity.clone().downgrade(),
            move |entity: Entity<TestEntity>, _: TypedEnvelope<proto::Ping>, mut cx| {
                entity
                    .update(&mut cx, |entity, _| entity.subscription.take())
                    .unwrap();
                done_tx.try_send(()).unwrap();
                async { Ok(()) }
            },
        );
        entity.update(cx, |entity, _| {
            entity.subscription = Some(subscription);
        });
        server.send(proto::Ping {});
        done_rx.recv().await.unwrap();
    }

    #[derive(Default)]
    struct TestEntity {
        id: usize,
        subscription: Option<Subscription>,
    }

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        });
    }
}
