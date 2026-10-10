use std::path::{Path, PathBuf};
use std::sync::Arc;

use ::settings::Settings as _;
use anyhow::{Context as _, anyhow};
use collections::HashMap;
use db::kvp::KeyValueStore;
use gpui::{
    AppContext as _, AsyncWindowContext, Context, Entity, EntityId, EventEmitter, Global,
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
use worktree::Worktree;

use crate::TerminalModeSettings;

/// Key-value store key prefix for terminal-mode session persistence.
const TERMINAL_MODE_KEY: &str = "TerminalMode";

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
    pub active_session: Option<SessionId>,
    /// Relay used for cross-host copies (R6); populated in a later phase.
    pub transfer_manager: VfsManager,
    subscriptions: Vec<Subscription>,
    /// Whether FinalShell-style terminal mode is currently active.
    pub terminal_mode_active: bool,
    restore_started: bool,
    next_session_id: usize,
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
            active_session: None,
            transfer_manager: VfsManager::default(),
            subscriptions: Vec::new(),
            terminal_mode_active: false,
            restore_started: false,
            next_session_id: 0,
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

    /// Adds a fully-connected session and notifies observers.
    pub fn register_session(&mut self, session: SshSession, cx: &mut Context<Self>) {
        self.sessions.push(session);
        cx.emit(Event::SessionsChanged);
        self.serialize(cx);
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

    /// Persists the session list (connection options + titles) to the KVP store.
    fn serialize(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.serialization_key.clone() else {
            return;
        };
        let serialized = SerializedTerminalMode {
            sessions: self
                .sessions
                .iter()
                .map(|session| SerializedSession {
                    id: Some(session.id.0),
                    connection_options: session.connection_options.clone(),
                })
                .collect(),
            active_session: self.active_session.map(|id| id.0),
            terminal_mode_active: self.terminal_mode_active,
        };

        let kvp = KeyValueStore::global(cx);
        cx.background_spawn(async move {
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
                workspace.update_in(cx, |workspace, window, cx| {
                    workspace.open_panel::<crate::SshSessionPanel>(window, cx);
                    workspace.open_panel::<crate::SessionFilePanel>(window, cx);
                    workspace
                        .close_panel::<terminal_view::terminal_panel::TerminalPanel>(window, cx);
                })?;
            }

            if auto_connect {
                let mut restored_active_session = None;
                for (index, session) in serialized.sessions.into_iter().enumerate() {
                    let is_active =
                        serialized_session_is_active(session.id, index, serialized.active_session);
                    let connect_task = store.update_in(cx, |store, window, cx| {
                        let id = if let Some(id) = session.id {
                            store.next_session_id = store.next_session_id.max(id.saturating_add(1));
                            SessionId(id)
                        } else {
                            store.next_session_id()
                        };
                        store.connect_with_id(id, session.connection_options, window, cx)
                    })?;
                    if let Some(id) = connect_task.await.log_err() {
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
        self.connect_with_id(id, connection_options, window, cx)
    }

    fn connect_with_id(
        &mut self,
        id: SessionId,
        connection_options: RemoteConnectionOptions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<anyhow::Result<SessionId>> {
        self.next_session_id = self.next_session_id.max(id.0.saturating_add(1));
        let Some(workspace) = self.workspace.upgrade() else {
            return gpui::Task::ready(Err(anyhow!("cannot connect without a workspace")));
        };

        let client_task = remote_connection::connect_with_modal(
            &workspace,
            connection_options.clone(),
            window,
            cx,
        );

        cx.spawn_in(window, async move |store, cx| {
            let client = client_task
                .await
                .context("remote connection task failed")?
                .context("remote connection was cancelled")?;

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
                    },
                    cx,
                );
                store.set_active_session(id, cx);
            })?;

            let terminal_task = workspace.update_in(cx, |workspace, window, cx| {
                spawn_session_terminal(workspace, project_for_terminal, window, cx)
            })?;
            cx.update(|_window, cx| terminal_task.detach_and_log_err(cx))?;

            Ok(id)
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
    }

    /// Opens a replacement connection and only disconnects `id` after the
    /// replacement is fully established.
    pub fn reconnect(
        &mut self,
        id: SessionId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<anyhow::Result<SessionId>> {
        let Some(connection_options) = self
            .session(id)
            .map(|session| session.connection_options.clone())
        else {
            return Task::ready(Err(anyhow!("no such session")));
        };
        let connect_task = self.connect(connection_options, window, cx);
        cx.spawn_in(window, async move |store, cx| {
            let replacement_id = connect_task.await?;
            store.update_in(cx, |store, window, cx| {
                store.disconnect(id, window, cx);
            })?;
            Ok(replacement_id)
        })
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
    cx.spawn_in(window, async move |workspace, cx| {
        let terminal = project
            .update(cx, |project, cx| project.create_terminal_shell(None, cx))
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
