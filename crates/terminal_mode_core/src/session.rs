use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ::settings::Settings as _;
use anyhow::{Context as _, anyhow};
use collections::{HashMap, HashSet};
use gpui::{
    AsyncApp, Context, Entity, EntityId, EventEmitter, Subscription, Task, WeakEntity,
};
use project::Project;
use remote::{RemoteClient, RemoteConnectionOptions};
use serde::{Deserialize, Serialize};
use terminal::Terminal;
use util::ResultExt as _;
use vfs::VfsManager;
use worktree::Worktree;

use crate::metrics::{ServerMetrics, parse_metrics, sampling_command};
use crate::settings::TerminalModeSettings;

/// Key-value store key prefix for terminal-mode session persistence.
pub const TERMINAL_MODE_KEY: &str = "TerminalMode";

/// Initial delay before the first automatic reconnect attempt.
const BASE_RECONNECT_BACKOFF_MS: u64 = 1_000;

/// Maximum number of consecutive automatic reconnect attempts per session.
const MAX_RECONNECT_ATTEMPTS: u32 = 5;

/// How long to wait for a metrics sample command to finish before reading the
/// monitor terminal's output. The command itself sleeps for
/// [`crate::metrics::SAMPLE_INTERVAL_SECS`], so this covers that plus the
/// remote round-trip.
const METRICS_SETTLE_MS: u64 = 900;

/// Maximum number of trailing terminal lines scanned for a metrics sample.
const METRICS_SCAN_LINES: usize = 512;

/// Reconnects a terminal-mode session using its stored connection options.
///
/// `background` suppresses the interactive connection modal; it is set for
/// automatic reconnection attempts.
#[derive(Clone, PartialEq, Deserialize, gpui::Action)]
#[action(namespace = terminal_mode, no_json)]
pub struct ReconnectSession {
    pub session_id: usize,
    /// Whether the reconnect was triggered by the backoff timer.
    #[serde(default)]
    pub background: bool,
}

/// Returns whether the session saved at `session_index` was the active one.
pub fn serialized_session_is_active(
    saved_session_id: Option<usize>,
    session_index: usize,
    saved_active_session: Option<usize>,
) -> bool {
    saved_active_session == Some(saved_session_id.unwrap_or(session_index))
}

/// A small stable identifier for a session within a single [`SessionStore`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(pub usize);

/// The filesystem root a session's file tree is currently browsing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionRoot {
    /// The remote user's home directory (mounted at connect time).
    #[default]
    Home,
    /// The remote filesystem root (`/` on Unix, `C:\` on Windows).
    Filesystem,
}

impl SessionRoot {
    pub fn label_key(self) -> (&'static str, &'static str) {
        match self {
            Self::Home => ("terminal_mode.root.home", "Home"),
            Self::Filesystem => ("terminal_mode.root.filesystem", "Filesystem"),
        }
    }
}

/// The lifecycle state of a session's connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionStatus {
    /// A connection attempt is in flight and no session exists yet.
    Connecting,
    /// The connection is established and the remote project is usable.
    Connected,
    /// A reconnect is in flight for a previously connected session.
    Reconnecting,
    /// The connection dropped or failed and is not being retried.
    Failed,
}

/// A connection attempt that has not yet produced a [`SshSession`].
#[derive(Clone, Debug)]
pub struct PendingConnection {
    pub id: SessionId,
    pub connection_options: RemoteConnectionOptions,
}

/// How a connection attempt should surface progress and prompts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionMode {
    /// User-initiated: show the remote connection modal, then dismiss it once
    /// the attempt finishes.
    Interactive,
    /// Background: connect silently, without any modal. Used when restoring
    /// saved sessions and for automatic reconnection.
    Background,
}

/// Error produced when the user cancels an interactive connection attempt.
///
/// Callers use [`is_connection_cancelled`] to avoid reporting a deliberate
/// cancellation as a failure.
#[derive(Clone, Copy, Debug)]
pub struct ConnectionCancelled;

impl std::fmt::Display for ConnectionCancelled {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("connection cancelled")
    }
}

impl std::error::Error for ConnectionCancelled {}

/// Returns `true` when `error` was produced by the user cancelling a
/// connection attempt rather than by a real failure.
pub fn is_connection_cancelled(error: &anyhow::Error) -> bool {
    error.downcast_ref::<ConnectionCancelled>().is_some()
}

/// One SSH connection and its remote project.
///
/// A session owns a dedicated [`Project`] built over a [`RemoteClient`], so its
/// terminal, file tree and preview all resolve against the same remote host.
pub struct SshSession {
    pub id: SessionId,
    pub connection_options: RemoteConnectionOptions,
    pub client: Entity<RemoteClient>,
    pub project: Entity<Project>,
    /// Worktree rooted at the remote user's `$HOME`, once resolved.
    pub home_worktree: Option<Entity<Worktree>>,
    /// Worktree rooted at the remote filesystem root, mounted on demand.
    pub filesystem_worktree: Option<Entity<Worktree>>,
    /// Current connection lifecycle state.
    pub status: SessionStatus,
    /// Hidden terminal used to sample server metrics, created lazily the first
    /// time monitoring runs for this session.
    pub monitor_terminal: Option<Entity<Terminal>>,
    /// Most recent resource sample, if one has been parsed.
    pub metrics: Option<ServerMetrics>,
    /// Keeps the session's disconnect handler alive for exactly as long as the
    /// session exists, so it is dropped when the session is removed.
    project_subscription: Option<Subscription>,
}

impl SshSession {
    /// Builds a freshly connected session. The project subscription is attached
    /// by [`SessionStore::register_session`].
    pub fn new(
        id: SessionId,
        connection_options: RemoteConnectionOptions,
        client: Entity<RemoteClient>,
        project: Entity<Project>,
        home_worktree: Option<Entity<Worktree>>,
    ) -> Self {
        Self {
            id,
            connection_options,
            client,
            project,
            home_worktree,
            filesystem_worktree: None,
            status: SessionStatus::Connected,
            monitor_terminal: None,
            metrics: None,
            project_subscription: None,
        }
    }
}

/// Events emitted by the [`SessionStore`] when its observable state changes.
#[derive(Clone, Debug)]
pub enum Event {
    SessionActivated(SessionId),
    SessionsChanged,
    /// A new resource sample was parsed for the given session.
    MetricsUpdated(SessionId),
}

#[derive(Serialize, Deserialize)]
pub struct SerializedSession {
    #[serde(default)]
    pub id: Option<usize>,
    pub connection_options: RemoteConnectionOptions,
    /// Number of terminal tabs that were open for this session.
    #[serde(default = "default_terminal_count")]
    pub terminal_count: usize,
}

fn default_terminal_count() -> usize {
    1
}

#[derive(Serialize, Deserialize, Default)]
pub struct SerializedTerminalMode {
    #[serde(default)]
    pub sessions: Vec<SerializedSession>,
    #[serde(default)]
    pub active_session: Option<usize>,
    #[serde(default)]
    pub terminal_mode_active: bool,
}

/// Owns all terminal-mode sessions for a single window and the state that is
/// shared across them (active session, cross-host transfer engine).
pub struct SessionStore {
    serialization_key: Option<String>,
    pub sessions: Vec<SshSession>,
    /// Connections that are being established but have no session yet.
    pub pending_connections: Vec<PendingConnection>,
    pub active_session: Option<SessionId>,
    /// Relay used for cross-host copies (R6); populated in a later phase.
    pub transfer_manager: VfsManager,
    /// Whether FinalShell-style terminal mode is currently active.
    pub terminal_mode_active: bool,
    /// Whether input is mirrored to every session terminal.
    pub broadcast_enabled: bool,
    /// Whether the server monitor section is shown in the session file panel.
    pub monitor_visible: bool,
    /// Whether the background metrics sampling loop is running.
    monitor_running: bool,
    restore_started: bool,
    pub next_session_id: usize,
    /// Consecutive automatic reconnect attempts, keyed by session id.
    reconnect_attempts: HashMap<SessionId, u32>,
    /// Sessions with a reconnect timer already scheduled, to avoid duplicates.
    scheduled_reconnects: HashSet<SessionId>,
}

impl EventEmitter<Event> for SessionStore {}

impl SessionStore {
    pub fn new(serialization_key: Option<String>) -> Self {
        Self {
            serialization_key,
            sessions: Vec::new(),
            pending_connections: Vec::new(),
            active_session: None,
            transfer_manager: VfsManager::default(),
            terminal_mode_active: false,
            broadcast_enabled: false,
            monitor_visible: true,
            monitor_running: false,
            restore_started: false,
            next_session_id: 0,
            reconnect_attempts: HashMap::default(),
            scheduled_reconnects: HashSet::default(),
        }
    }

    /// The key-value store key this store persists under, if the window is
    /// stable enough to have one.
    pub fn serialization_key(&self) -> Option<&String> {
        self.serialization_key.as_ref()
    }

    /// Reserves a fresh, never-reused session identifier.
    pub fn next_session_id(&mut self) -> SessionId {
        let id = SessionId(self.next_session_id);
        self.next_session_id += 1;
        id
    }

    pub fn session(&self, id: SessionId) -> Option<&SshSession> {
        self.sessions.iter().find(|session| session.id == id)
    }

    pub fn session_for_project(&self, project_id: EntityId) -> Option<SessionId> {
        self.sessions
            .iter()
            .find(|session| session.project.entity_id() == project_id)
            .map(|session| session.id)
    }

    /// Returns the worktree backing `root` for `id`, if it has been mounted.
    pub fn worktree_for_root(&self, id: SessionId, root: SessionRoot) -> Option<Entity<Worktree>> {
        let session = self.sessions.iter().find(|session| session.id == id)?;
        match root {
            SessionRoot::Home => session.home_worktree.clone(),
            SessionRoot::Filesystem => session.filesystem_worktree.clone(),
        }
    }

    /// Mounts the remote filesystem root as a visible worktree, idempotently.
    pub fn ensure_filesystem_worktree(
        &mut self,
        id: SessionId,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Entity<Worktree>>> {
        let Some(session) = self.sessions.iter().find(|session| session.id == id) else {
            return Task::ready(Err(anyhow!("no such session")));
        };
        if let Some(worktree) = session.filesystem_worktree.clone() {
            return Task::ready(Ok(worktree));
        }
        let project = session.project.clone();
        let path_style = project.read(cx).path_style(cx);
        let root = if path_style.is_windows() {
            PathBuf::from("C:\\")
        } else {
            PathBuf::from("/")
        };
        cx.spawn(async move |store, cx| {
            let (worktree, _) = project
                .update(cx, |project, cx| {
                    project.find_or_create_worktree(root, true, cx)
                })
                .await?;
            store.update(cx, |store, _| {
                if let Some(session) = store.sessions.iter_mut().find(|session| session.id == id) {
                    session.filesystem_worktree = Some(worktree.clone());
                }
            })?;
            Ok(worktree)
        })
    }

    /// Records a connection attempt before it starts, so the session list can
    /// show it as pending.
    pub fn begin_connection(
        &mut self,
        id: SessionId,
        connection_options: RemoteConnectionOptions,
        cx: &mut Context<Self>,
    ) {
        self.next_session_id = self.next_session_id.max(id.0.saturating_add(1));
        self.pending_connections.push(PendingConnection {
            id,
            connection_options,
        });
        cx.emit(Event::SessionsChanged);
    }

    /// Drops the pending entry for `id` once its attempt has settled.
    pub fn finish_connection(&mut self, id: SessionId, cx: &mut Context<Self>) {
        self.pending_connections.retain(|pending| pending.id != id);
        cx.emit(Event::SessionsChanged);
    }

    /// Adds a fully-connected session and notifies observers.
    pub fn register_session(&mut self, mut session: SshSession, cx: &mut Context<Self>) {
        let id = session.id;
        self.reconnect_attempts.remove(&id);
        self.scheduled_reconnects.remove(&id);
        self.pending_connections.retain(|pending| pending.id != id);

        let project = session.project.clone();
        let subscription = cx.subscribe(&project, move |store, _project, event, cx| {
            if matches!(event, project::Event::DisconnectedFromRemote { .. }) {
                store.handle_session_disconnected(id, cx);
            }
        });
        session.project_subscription = Some(subscription);

        self.sessions.push(session);
        cx.emit(Event::SessionsChanged);
        self.start_monitor(cx);
    }

    /// Marks a session as failed after an unexpected disconnect and, when
    /// automatic reconnect is enabled, schedules a retry with backoff.
    fn handle_session_disconnected(&mut self, id: SessionId, cx: &mut Context<Self>) {
        let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) else {
            return;
        };
        if session.status == SessionStatus::Reconnecting {
            return;
        }
        session.status = SessionStatus::Failed;
        cx.emit(Event::SessionsChanged);
        if TerminalModeSettings::get_global(cx).auto_reconnect {
            self.schedule_auto_reconnect(id, cx);
        }
    }

    /// Schedules a single automatic reconnect attempt after an exponential
    /// backoff, up to [`MAX_RECONNECT_ATTEMPTS`].
    pub fn schedule_auto_reconnect(&mut self, id: SessionId, cx: &mut Context<Self>) {
        if !self.scheduled_reconnects.insert(id) {
            return;
        }
        let attempt = self.reconnect_attempts.entry(id).or_insert(0);
        *attempt += 1;
        if *attempt > MAX_RECONNECT_ATTEMPTS {
            self.scheduled_reconnects.remove(&id);
            return;
        }

        let max_backoff = TerminalModeSettings::get_global(cx).reconnect_max_backoff_ms;
        let shift = (*attempt - 1).min(6);
        let delay = BASE_RECONNECT_BACKOFF_MS
            .saturating_mul(1u64 << shift)
            .min(max_backoff);

        cx.spawn(async move |store, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(delay))
                .await;
            let Some(store) = store.upgrade() else {
                return;
            };
            cx.update(|cx| {
                let should_reconnect = store.update(cx, |store, cx| {
                    store.scheduled_reconnects.remove(&id);
                    cx.notify();
                    store
                        .sessions
                        .iter()
                        .any(|session| session.id == id && session.status == SessionStatus::Failed)
                });
                if should_reconnect {
                    cx.dispatch_action(&ReconnectSession {
                        session_id: id.0,
                        background: true,
                    });
                }
            });
        })
        .detach();
    }

    /// Marks `id` as the active session and notifies observers.
    pub fn set_active_session(&mut self, id: SessionId, cx: &mut Context<Self>) {
        if self.active_session == Some(id) {
            return;
        }
        self.active_session = Some(id);
        cx.emit(Event::SessionActivated(id));
    }

    /// Removes a session, promoting the most recent remaining session to active
    /// when the removed session was active.
    pub fn remove_session(&mut self, id: SessionId, cx: &mut Context<Self>) -> Option<SshSession> {
        let index = self.sessions.iter().position(|session| session.id == id)?;
        let session = self.sessions.remove(index);
        self.reconnect_attempts.remove(&id);
        self.scheduled_reconnects.remove(&id);
        if self.active_session == Some(id) {
            self.active_session = self.sessions.last().map(|session| session.id);
            if let Some(active_session) = self.active_session {
                cx.emit(Event::SessionActivated(active_session));
            }
        }
        cx.emit(Event::SessionsChanged);
        Some(session)
    }

    /// Builds the persisted snapshot of the session list.
    ///
    /// `terminal_counts` maps each session to the number of terminal tabs the
    /// workspace currently shows for it; the UI layer, which owns the panes,
    /// measures them.
    pub fn snapshot(
        &self,
        terminal_counts: &HashMap<SessionId, usize>,
    ) -> SerializedTerminalMode {
        SerializedTerminalMode {
            sessions: self
                .sessions
                .iter()
                .map(|session| SerializedSession {
                    id: Some(session.id.0),
                    connection_options: session.connection_options.clone(),
                    terminal_count: terminal_counts
                        .get(&session.id)
                        .copied()
                        .unwrap_or(1)
                        .max(1),
                })
                .collect(),
            active_session: self.active_session.map(|id| id.0),
            terminal_mode_active: self.terminal_mode_active,
        }
    }

    /// Claims the one-shot restore slot.
    ///
    /// Returns `false` once a restore has started or once any session exists,
    /// so the persisted state is only replayed on an empty store.
    pub fn begin_restore(&mut self) -> bool {
        if self.restore_started || !self.sessions.is_empty() {
            return false;
        }
        self.restore_started = true;
        true
    }

    /// Records whether terminal mode is active.
    pub fn set_terminal_mode_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.terminal_mode_active == active {
            return;
        }
        self.terminal_mode_active = active;
        if active {
            self.start_monitor(cx);
        }
        cx.emit(Event::SessionsChanged);
    }

    /// Toggles whether input is mirrored to every session terminal.
    pub fn set_broadcast_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.broadcast_enabled == enabled {
            return;
        }
        self.broadcast_enabled = enabled;
        cx.emit(Event::SessionsChanged);
        cx.notify();
    }

    /// Returns the most recent resource sample for `id`, if any.
    pub fn metrics(&self, id: SessionId) -> Option<&ServerMetrics> {
        self.sessions
            .iter()
            .find(|session| session.id == id)?
            .metrics
            .as_ref()
    }

    /// Shows or hides the server monitor section, starting sampling when it
    /// becomes visible.
    pub fn set_monitor_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.monitor_visible == visible {
            return;
        }
        self.monitor_visible = visible;
        if visible {
            self.start_monitor(cx);
        }
        cx.emit(Event::SessionsChanged);
        cx.notify();
    }

    /// Records a fresh sample for `id` and notifies observers.
    fn set_metrics(
        &mut self,
        id: SessionId,
        metrics: Option<ServerMetrics>,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) else {
            return;
        };
        if session.metrics == metrics {
            return;
        }
        session.metrics = metrics;
        cx.emit(Event::MetricsUpdated(id));
    }

    /// Starts the background metrics loop, if it is not already running and both
    /// terminal mode and monitoring are enabled.
    ///
    /// The loop stops on its own once terminal mode is left or monitoring is
    /// disabled in settings, so it never outlives the feature that needs it.
    pub fn start_monitor(&mut self, cx: &mut Context<Self>) {
        if self.monitor_running || !self.terminal_mode_active {
            return;
        }
        if !TerminalModeSettings::get_global(cx).monitor_enabled {
            return;
        }
        self.monitor_running = true;
        cx.spawn(async move |store, cx| {
            loop {
                let should_run = store
                    .update(cx, |store, cx| {
                        store.terminal_mode_active
                            && TerminalModeSettings::get_global(cx).monitor_enabled
                    })
                    .unwrap_or(false);
                if !should_run {
                    break;
                }

                let session_ids: Vec<SessionId> = store
                    .update(cx, |store, _| {
                        store
                            .sessions
                            .iter()
                            .filter(|session| session.status == SessionStatus::Connected)
                            .map(|session| session.id)
                            .collect()
                    })
                    .unwrap_or_default();
                for id in session_ids {
                    sample_session_metrics(&store, id, cx).await.log_err();
                }

                let refresh_interval = store
                    .update(cx, |_, cx| {
                        TerminalModeSettings::get_global(cx).monitor_refresh_interval_ms
                    })
                    .unwrap_or(2_000);
                cx.background_executor()
                    .timer(Duration::from_millis(refresh_interval))
                    .await;
            }
            store
                .update(cx, |store, _| store.monitor_running = false)
                .log_err();
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    /// Returns the session's hidden monitor terminal, creating it on first use.
    fn ensure_monitor_terminal(
        &mut self,
        id: SessionId,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<Entity<Terminal>>> {
        let Some(session) = self.sessions.iter().find(|session| session.id == id) else {
            return Task::ready(Err(anyhow!("no such session")));
        };
        if let Some(terminal) = session.monitor_terminal.clone() {
            return Task::ready(Ok(terminal));
        }
        let project = session.project.clone();
        cx.spawn(async move |store, cx| {
            let terminal = project
                .update(cx, |project, cx| project.create_terminal_shell(None, cx))
                .await?;
            store.update(cx, |store, _| {
                if let Some(session) = store.sessions.iter_mut().find(|session| session.id == id) {
                    session.monitor_terminal = Some(terminal.clone());
                }
            })?;
            Ok(terminal)
        })
    }
}

/// Runs one metrics sample for `id`: writes the sampling command into the
/// session's hidden terminal, waits for it to finish, then parses the output.
async fn sample_session_metrics(
    store: &WeakEntity<SessionStore>,
    id: SessionId,
    cx: &mut AsyncApp,
) -> anyhow::Result<()> {
    let terminal = store
        .update(cx, |store, cx| store.ensure_monitor_terminal(id, cx))?
        .await?;
    terminal.update(cx, |terminal, _| {
        terminal.input(sampling_command().into_bytes());
    });
    cx.background_executor()
        .timer(Duration::from_millis(METRICS_SETTLE_MS))
        .await;
    let lines = terminal.read_with(cx, |terminal, _| {
        terminal.last_n_non_empty_lines(METRICS_SCAN_LINES)
    });
    let metrics = parse_metrics(&lines);
    store.update(cx, |store, cx| store.set_metrics(id, metrics, cx))?;
    Ok(())
}

/// Resolves the remote user's home directory by reading the environment of the
/// remote filesystem root over the project's RPC connection.
pub async fn remote_home_directory(
    project: &Entity<Project>,
    cx: &mut AsyncApp,
) -> anyhow::Result<PathBuf> {
    let path_style = project.read_with(cx, |project, cx| project.path_style(cx));
    let root: Arc<std::path::Path> = if path_style.is_windows() {
        Arc::from(PathBuf::from("C:\\"))
    } else {
        Arc::from(PathBuf::from("/"))
    };

    let environment_task = project.update(cx, |project, cx| {
        let environment = project.environment().clone();
        environment.update(cx, |environment, cx| {
            environment.directory_environment(root, cx)
        })
    });

    environment_task
        .await
        .and_then(|environment| {
            environment
                .get("HOME")
                .or_else(|| environment.get("USERPROFILE"))
                .map(PathBuf::from)
        })
        .context("the remote environment did not contain a home directory")
}

#[cfg(test)]
mod serialization_tests {
    use super::*;

    #[test]
    fn serialized_terminal_mode_round_trips() {
        let options = RemoteConnectionOptions::Ssh(remote::SshConnectionOptions {
            host: "example.com".into(),
            username: Some("alice".into()),
            port: Some(2222),
            ..Default::default()
        });
        let serialized = SerializedTerminalMode {
            sessions: vec![SerializedSession {
                id: Some(0),
                connection_options: options.clone(),
                terminal_count: 2,
            }],
            active_session: Some(0),
            terminal_mode_active: true,
        };

        let json = serde_json::to_string(&serialized).expect("serialize terminal mode");
        let restored: SerializedTerminalMode =
            serde_json::from_str(&json).expect("deserialize terminal mode");

        assert_eq!(restored.sessions.len(), 1);
        assert_eq!(restored.active_session, Some(0));
        assert!(restored.terminal_mode_active);
        assert_eq!(restored.sessions[0].connection_options, options);
        assert_eq!(restored.sessions[0].terminal_count, 2);
    }
}
