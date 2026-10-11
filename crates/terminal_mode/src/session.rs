use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ::settings::Settings as _;
use anyhow::{Context as _, anyhow};
use collections::{HashMap, HashSet};
use db::kvp::KeyValueStore;
use gpui::{
    App, AppContext as _, AsyncWindowContext, Context, Entity, EntityId, EventEmitter, Global,
    Subscription, Task, WeakEntity, Window,
};
use project::Project;
use remote::{RemoteClient, RemoteConnectionOptions};
use serde::{Deserialize, Serialize};
use terminal::Terminal;
use terminal_view::TerminalView;
use util::ResultExt as _;
use vfs::VfsManager;
use workspace::Workspace;
use workspace::dock::DockPosition;
use worktree::Worktree;

use crate::{ReconnectSession, TerminalModeSettings};

/// Key-value store key prefix for terminal-mode session persistence.
const TERMINAL_MODE_KEY: &str = "TerminalMode";

/// Initial delay before the first automatic reconnect attempt.
const BASE_RECONNECT_BACKOFF_MS: u64 = 1_000;

/// Maximum number of consecutive automatic reconnect attempts per session.
const MAX_RECONNECT_ATTEMPTS: u32 = 5;

fn serialized_session_is_active(
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
pub(crate) enum ConnectionMode {
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
pub(crate) struct ConnectionCancelled;

impl std::fmt::Display for ConnectionCancelled {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("connection cancelled")
    }
}

impl std::error::Error for ConnectionCancelled {}

/// Returns `true` when `error` was produced by the user cancelling a
/// connection attempt rather than by a real failure.
pub(crate) fn is_connection_cancelled(error: &anyhow::Error) -> bool {
    error.downcast_ref::<ConnectionCancelled>().is_some()
}

/// One SSH connection, its remote project, and the views bound to it.
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
    /// Keeps the session's disconnect handler alive for exactly as long as the
    /// session exists, so it is dropped when the session is removed.
    project_subscription: Option<Subscription>,
}

/// Events emitted by the [`SessionStore`] when its observable state changes.
#[derive(Clone, Debug)]
pub enum Event {
    SessionActivated(SessionId),
    SessionsChanged,
}

#[derive(Serialize, Deserialize)]
struct SerializedSession {
    #[serde(default)]
    id: Option<usize>,
    connection_options: RemoteConnectionOptions,
    /// Number of terminal tabs that were open for this session.
    #[serde(default = "default_terminal_count")]
    terminal_count: usize,
}

fn default_terminal_count() -> usize {
    1
}

#[derive(Serialize, Deserialize, Default)]
struct SerializedTerminalMode {
    #[serde(default)]
    sessions: Vec<SerializedSession>,
    #[serde(default)]
    active_session: Option<usize>,
    #[serde(default)]
    terminal_mode_active: bool,
}

/// Owns all terminal-mode sessions for a single window and the state that is
/// shared across them (active session, cross-host transfer engine).
pub struct SessionStore {
    pub workspace: WeakEntity<Workspace>,
    serialization_key: Option<String>,
    pub sessions: Vec<SshSession>,
    /// Connections that are being established but have no session yet.
    pub pending_connections: Vec<PendingConnection>,
    pub active_session: Option<SessionId>,
    /// Relay used for cross-host copies (R6); populated in a later phase.
    pub transfer_manager: VfsManager,
    subscriptions: Vec<Subscription>,
    /// Whether FinalShell-style terminal mode is currently active.
    pub terminal_mode_active: bool,
    /// Whether the bottom dock was open when terminal mode hid it, so it can be
    /// restored when terminal mode is left.
    pub bottom_dock_hidden: bool,
    /// Whether input is mirrored to every session terminal.
    pub broadcast_enabled: bool,
    restore_started: bool,
    next_session_id: usize,
    /// Consecutive automatic reconnect attempts, keyed by session id.
    reconnect_attempts: HashMap<SessionId, u32>,
    /// Sessions with a reconnect timer already scheduled, to avoid duplicates.
    scheduled_reconnects: HashSet<SessionId>,
}

impl EventEmitter<Event> for SessionStore {}

/// Per-window registry so both terminal-mode panels can reach the same
/// [`SessionStore`] without threading a handle through panel loading.
#[derive(Default)]
struct SessionStoreRegistry {
    stores: HashMap<EntityId, Entity<SessionStore>>,
}

impl Global for SessionStoreRegistry {}

impl SessionStore {
    pub fn new(workspace: WeakEntity<Workspace>, serialization_key: Option<String>) -> Self {
        Self {
            workspace,
            serialization_key,
            sessions: Vec::new(),
            pending_connections: Vec::new(),
            active_session: None,
            transfer_manager: VfsManager::default(),
            subscriptions: Vec::new(),
            terminal_mode_active: false,
            bottom_dock_hidden: false,
            broadcast_enabled: false,
            restore_started: false,
            next_session_id: 0,
            reconnect_attempts: HashMap::default(),
            scheduled_reconnects: HashSet::default(),
        }
    }

    /// Returns the store for `workspace`, creating it on first use.
    pub fn for_workspace(workspace: &mut Workspace, cx: &mut Context<Workspace>) -> Entity<Self> {
        let workspace_id = cx.entity().entity_id();
        if let Some(existing) = cx
            .try_global::<SessionStoreRegistry>()
            .and_then(|registry| registry.stores.get(&workspace_id))
            .cloned()
        {
            return existing;
        }

        let workspace_handle = workspace.weak_handle();
        let serialization_key = Self::serialization_key(workspace);
        let store = cx.new(|_| SessionStore::new(workspace_handle, serialization_key));
        if !cx.has_global::<SessionStoreRegistry>() {
            cx.set_global(SessionStoreRegistry::default());
        }
        cx.global_mut::<SessionStoreRegistry>()
            .stores
            .insert(workspace_id, store.clone());

        let workspace_entity = cx.entity();
        let store_handle = store.downgrade();
        let subscription = cx.subscribe(
            &workspace_entity,
            move |workspace, _, event: &workspace::Event, cx| {
                if !matches!(event, workspace::Event::ActiveItemChanged) {
                    return;
                }
                let Some(active_item) = workspace.active_item(cx) else {
                    return;
                };
                let Some(terminal_view) = active_item.downcast::<TerminalView>() else {
                    return;
                };
                let project_id = terminal_view.read(cx).project().entity_id();
                let Some(store_handle) = store_handle.upgrade() else {
                    return;
                };
                store_handle.update(cx, |store, cx| {
                    if let Some(id) = store.session_for_project(project_id) {
                        store.set_active_session(id, cx);
                    }
                });
            },
        );
        store.update(cx, |store, _| store.subscriptions.push(subscription));
        let workspace_id = workspace_entity.entity_id();
        let release_subscription = cx.on_release(move |_, cx| {
            cx.global_mut::<SessionStoreRegistry>()
                .stores
                .remove(&workspace_id);
        });
        store.update(cx, |store, _| {
            store.subscriptions.push(release_subscription)
        });
        store
    }

    /// Returns the store for `workspace` without creating one, for read-only
    /// queries from contexts that only have `&Workspace` and `&App`.
    pub fn for_workspace_read(workspace: &Workspace, cx: &App) -> Option<Entity<Self>> {
        let workspace_id = workspace.weak_handle().entity_id();
        cx.try_global::<SessionStoreRegistry>()?
            .stores
            .get(&workspace_id)
            .cloned()
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
        self.serialize(cx);
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
    fn schedule_auto_reconnect(&mut self, id: SessionId, cx: &mut Context<Self>) {
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
        self.serialize(cx);
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
        self.serialize(cx);
        Some(session)
    }

    fn serialization_key(workspace: &Workspace) -> Option<String> {
        workspace
            .database_id()
            .map(|id| i64::from(id).to_string())
            .or_else(|| workspace.session_id())
            .map(|id| format!("{TERMINAL_MODE_KEY}-{id:?}"))
    }

    /// Persists the session list (connection options + terminal counts) to the
    /// KVP store.
    ///
    /// The snapshot is built in a deferred task because the terminal counts come
    /// from the workspace's panes, and `serialize` is invoked from within
    /// workspace updates (for example the terminal-mode toggle action), where
    /// reading the workspace synchronously would re-enter an already-leased
    /// entity and panic.
    fn serialize(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.serialization_key.clone() else {
            return;
        };
        let kvp = KeyValueStore::global(cx);
        cx.spawn(async move |store, cx| {
            let serialized = store.update(cx, |store, cx| {
                let session_terminal_counts: HashMap<SessionId, usize> = store
                    .sessions
                    .iter()
                    .map(|session| {
                        (
                            session.id,
                            store.session_terminals(session.id, cx).len().max(1),
                        )
                    })
                    .collect();
                SerializedTerminalMode {
                    sessions: store
                        .sessions
                        .iter()
                        .map(|session| SerializedSession {
                            id: Some(session.id.0),
                            connection_options: session.connection_options.clone(),
                            terminal_count: session_terminal_counts
                                .get(&session.id)
                                .copied()
                                .unwrap_or(1),
                        })
                        .collect(),
                    active_session: store.active_session.map(|id| id.0),
                    terminal_mode_active: store.terminal_mode_active,
                }
            })?;
            let value = serde_json::to_string(&serialized)?;
            kvp.write_kvp(key, value).await?;
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    /// Records whether terminal mode is active and persists the change.
    pub fn set_terminal_mode_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.terminal_mode_active == active {
            return;
        }
        self.terminal_mode_active = active;
        self.serialize(cx);
    }

    /// Restores the persisted terminal-mode state: re-opens the panels when the
    /// mode was active, and reconnects saved sessions when
    /// `auto_connect_last_session` is enabled.
    pub fn restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restore_started || !self.sessions.is_empty() {
            return;
        }
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        let Some(key) = self.serialization_key.clone() else {
            return;
        };
        self.restore_started = true;
        let auto_connect = TerminalModeSettings::get_global(cx).auto_connect_last_session;
        let kvp = KeyValueStore::global(cx);
        cx.spawn_in(window, async move |store, cx| {
            let saved = cx
                .background_spawn(async move { kvp.read_kvp(&key) })
                .await
                .log_err()
                .flatten();
            let Some(saved) = saved else {
                return anyhow::Ok(());
            };
            let Ok(serialized) = serde_json::from_str::<SerializedTerminalMode>(&saved) else {
                return anyhow::Ok(());
            };

            if serialized.terminal_mode_active {
                store.update(cx, |store, _| store.terminal_mode_active = true)?;
                let bottom_dock_was_open = workspace.update_in(cx, |workspace, window, cx| {
                    let bottom_dock = workspace.dock_at_position(DockPosition::Bottom).clone();
                    let was_open = bottom_dock.read(cx).is_open();
                    if was_open {
                        bottom_dock.update(cx, |dock, cx| dock.set_open(false, window, cx));
                    }
                    workspace.open_panel::<crate::SshSessionPanel>(window, cx);
                    workspace.open_panel::<crate::SessionFilePanel>(window, cx);
                    was_open
                })?;
                store.update(cx, |store, _| {
                    store.bottom_dock_hidden = bottom_dock_was_open
                })?;
            }

            if auto_connect {
                let mut restored_active_session = None;
                for (index, session) in serialized.sessions.into_iter().enumerate() {
                    let is_active =
                        serialized_session_is_active(session.id, index, serialized.active_session);
                    let terminal_count = session.terminal_count;
                    let connect_task = store.update_in(cx, |store, window, cx| {
                        let id = if let Some(id) = session.id {
                            store.next_session_id = store.next_session_id.max(id.saturating_add(1));
                            SessionId(id)
                        } else {
                            store.next_session_id()
                        };
                        store.connect_with_id(
                            id,
                            session.connection_options,
                            ConnectionMode::Background,
                            window,
                            cx,
                        )
                    })?;
                    if let Some(id) = connect_task.await.log_err() {
                        let extra = terminal_count.saturating_sub(1);
                        if extra > 0 {
                            let project = store.update(cx, |store, _| {
                                store.session(id).map(|session| session.project.clone())
                            })?;
                            if let Some(project) = project {
                                workspace.update_in(cx, |workspace, window, cx| {
                                    for _ in 0..extra {
                                        spawn_session_terminal(
                                            workspace,
                                            project.clone(),
                                            window,
                                            cx,
                                        )
                                        .detach_and_log_err(cx);
                                    }
                                })?;
                            }
                        }
                        if is_active {
                            restored_active_session = Some(id);
                        }
                    }
                }
                if let Some(id) = restored_active_session {
                    store.update(cx, |store, cx| store.set_active_session(id, cx))?;
                }
            }
            anyhow::Ok(())
        })
        .detach_and_log_err(cx);
    }

    /// Establishes a new remote connection, builds its project and mounts a
    /// worktree rooted at the remote `$HOME`.
    pub fn connect(
        &mut self,
        connection_options: RemoteConnectionOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<anyhow::Result<SessionId>> {
        let id = self.next_session_id();
        self.connect_with_id(
            id,
            connection_options,
            ConnectionMode::Interactive,
            window,
            cx,
        )
    }

    fn connect_with_id(
        &mut self,
        id: SessionId,
        connection_options: RemoteConnectionOptions,
        mode: ConnectionMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<anyhow::Result<SessionId>> {
        self.next_session_id = self.next_session_id.max(id.0.saturating_add(1));
        let Some(workspace) = self.workspace.upgrade() else {
            return gpui::Task::ready(Err(anyhow!("cannot connect without a workspace")));
        };

        self.pending_connections.push(PendingConnection {
            id,
            connection_options: connection_options.clone(),
        });
        cx.emit(Event::SessionsChanged);

        let client_task = match mode {
            ConnectionMode::Interactive => remote_connection::connect_with_modal(
                &workspace,
                connection_options.clone(),
                window,
                cx,
            ),
            ConnectionMode::Background => {
                remote_connection::connect_in_background(connection_options.clone(), cx)
            }
        };

        cx.spawn_in(window, async move |store, cx| {
            let outcome = async {
                let Some(client) = client_task.await.context("remote connection task failed")?
                else {
                    return Err(anyhow::Error::new(ConnectionCancelled));
                };

                let project = cx.update(|_window, cx| {
                    let app_state = workspace.read(cx).app_state().clone();
                    workspace::remote_project_from_client(client.clone(), &app_state, cx)
                })?;

                let home_directory = remote_home_directory(&project, cx).await?;
                let (home_worktree, _) = project
                    .update(cx, |project, cx| {
                        project.find_or_create_worktree(home_directory, true, cx)
                    })
                    .await?;

                let project_for_terminal = project.clone();
                store.update(cx, |store, cx| {
                    store.register_session(
                        SshSession {
                            id,
                            connection_options,
                            client,
                            project,
                            home_worktree: Some(home_worktree),
                            filesystem_worktree: None,
                            status: SessionStatus::Connected,
                            project_subscription: None,
                        },
                        cx,
                    );
                    store.set_active_session(id, cx);
                })?;

                let terminal_task = workspace.update_in(cx, |workspace, window, cx| {
                    spawn_session_terminal(workspace, project_for_terminal, window, cx)
                })?;
                cx.update(|_window, cx| terminal_task.detach_and_log_err(cx))?;

                anyhow::Ok(id)
            }
            .await;

            if mode == ConnectionMode::Interactive {
                remote_connection::dismiss_connection_modal(&workspace, cx);
            }

            store.update(cx, |store, cx| {
                store.pending_connections.retain(|pending| pending.id != id);
                if outcome.is_err()
                    && let Some(session) =
                        store.sessions.iter_mut().find(|session| session.id == id)
                {
                    session.status = SessionStatus::Failed;
                }
                cx.emit(Event::SessionsChanged);
            })?;

            outcome
        })
    }
}

impl SessionStore {
    /// Drops a session and closes every center terminal bound to its project.
    pub fn disconnect(&mut self, id: SessionId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.remove_session(id, cx) else {
            return;
        };
        let project_id = session.project.entity_id();
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        // Deferred so this does not re-enter the workspace while it is leased by
        // the action handler that dispatched the disconnect.
        cx.defer_in(window, move |_store, window, cx| {
            workspace.update(cx, |workspace, cx| {
                for pane in workspace.panes() {
                    let terminal_item_ids: Vec<EntityId> = pane
                        .read(cx)
                        .items_of_type::<TerminalView>()
                        .filter(|terminal_view| {
                            terminal_view.read(cx).project().entity_id() == project_id
                        })
                        .map(|terminal_view| terminal_view.entity_id())
                        .collect();
                    for item_id in terminal_item_ids {
                        pane.update(cx, |pane, cx| {
                            pane.remove_item(item_id, true, false, window, cx);
                        });
                    }
                }
            });
        });
    }

    /// Opens a replacement connection and only disconnects `id` after the
    /// replacement is fully established.
    pub(crate) fn reconnect(
        &mut self,
        id: SessionId,
        mode: ConnectionMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<SessionId>> {
        let Some(connection_options) = self
            .session(id)
            .map(|session| session.connection_options.clone())
        else {
            return Task::ready(Err(anyhow!("no such session")));
        };
        if let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) {
            session.status = SessionStatus::Reconnecting;
        }
        cx.emit(Event::SessionsChanged);
        let replacement_id = self.next_session_id();
        let connect_task =
            self.connect_with_id(replacement_id, connection_options, mode, window, cx);
        cx.spawn_in(window, async move |store, cx| match connect_task.await {
            Ok(replacement_id) => {
                store.update_in(cx, |store, window, cx| {
                    store.disconnect(id, window, cx);
                })?;
                Ok(replacement_id)
            }
            Err(error) => {
                store.update(cx, |store, cx| {
                    if let Some(session) =
                        store.sessions.iter_mut().find(|session| session.id == id)
                    {
                        session.status = SessionStatus::Failed;
                    }
                    cx.emit(Event::SessionsChanged);
                    if TerminalModeSettings::get_global(cx).auto_reconnect {
                        store.schedule_auto_reconnect(id, cx);
                    }
                })?;
                Err(error)
            }
        })
    }
}

impl SessionStore {
    /// Returns the terminal views bound to `id`'s remote project.
    pub fn session_terminals(&self, id: SessionId, cx: &App) -> Vec<Entity<TerminalView>> {
        let Some(session) = self.session(id) else {
            return Vec::new();
        };
        let project_id = session.project.entity_id();
        let Some(workspace) = self.workspace.upgrade() else {
            return Vec::new();
        };
        let mut terminals = Vec::new();
        for pane in workspace.read(cx).panes() {
            for terminal_view in pane.read(cx).items_of_type::<TerminalView>() {
                if terminal_view.read(cx).project().entity_id() == project_id {
                    terminals.push(terminal_view);
                }
            }
        }
        terminals
    }

    /// Writes `text` to every terminal bound to `id`.
    pub fn send_to_session(&self, id: SessionId, text: &str, cx: &mut App) {
        let input = text.as_bytes().to_vec();
        for terminal_view in self.session_terminals(id, cx) {
            terminal_view.update(cx, |view, cx| {
                view.terminal()
                    .update(cx, |terminal, _| terminal.input(input.clone()));
            });
        }
    }

    /// Writes `text` to every session's terminals.
    pub fn send_to_all_sessions(&self, text: &str, cx: &mut App) {
        for session in &self.sessions {
            self.send_to_session(session.id, text, cx);
        }
    }

    /// Sends a quick command (with a trailing newline) to the active session,
    /// or to every session when broadcast is enabled.
    pub fn run_quick_command(&self, command: &str, cx: &mut App) {
        let input = format!("{command}\n");
        if self.broadcast_enabled {
            self.send_to_all_sessions(&input, cx);
        } else if let Some(id) = self.active_session {
            self.send_to_session(id, &input, cx);
        }
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
}

/// Spawns a terminal bound to `project` and adds it to the active center pane.
///
/// Mirrors `TerminalPanel::add_center_terminal`, but takes the project
/// explicitly so a session terminal resolves against its own remote host.
pub fn spawn_session_terminal(
    workspace: &mut Workspace,
    project: Entity<Project>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Task<anyhow::Result<WeakEntity<Terminal>>> {
    let workspace_handle = workspace.weak_handle();
    let workspace_id = workspace.database_id();
    let settings = TerminalModeSettings::get_global(cx);
    let starting_command = settings.session_starting_command.clone();
    let working_directory = settings
        .session_working_directory
        .clone()
        .map(PathBuf::from);
    cx.spawn_in(window, async move |workspace, cx| {
        let terminal = project
            .update(cx, |project, cx| {
                project.create_terminal_shell(working_directory, cx)
            })
            .await?;
        workspace.update_in(cx, |workspace, window, cx| {
            let terminal_view = cx.new(|cx| {
                TerminalView::new(
                    terminal.clone(),
                    workspace_handle,
                    workspace_id,
                    project.downgrade(),
                    window,
                    cx,
                )
            });
            workspace.add_item_to_active_pane(Box::new(terminal_view), None, true, window, cx);
            if let Some(command) = starting_command.as_ref() {
                terminal.update(cx, |terminal, _| {
                    terminal.input(format!("{command}\n").into_bytes());
                });
            }
        })?;
        Ok(terminal.downgrade())
    })
}

/// Resolves the remote user's home directory by reading the environment of the
/// remote filesystem root over the project's RPC connection.
async fn remote_home_directory(
    project: &Entity<Project>,
    cx: &mut AsyncWindowContext,
) -> anyhow::Result<PathBuf> {
    let path_style = project.read_with(cx, |project, cx| project.path_style(cx));
    let root: Arc<Path> = if path_style.is_windows() {
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

    #[test]
    fn active_session_persists_across_session_id_gaps() {
        assert_eq!(serialized_session_is_active(Some(8), 2, Some(8)), true);
        assert_eq!(serialized_session_is_active(Some(8), 2, Some(3)), false);
    }

    #[test]
    fn old_saved_sessions_fall_back_to_their_position() {
        assert!(serialized_session_is_active(None, 2, Some(2)));
        assert!(!serialized_session_is_active(None, 2, Some(8)));
    }
}
