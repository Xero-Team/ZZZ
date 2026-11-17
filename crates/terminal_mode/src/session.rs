//! Workspace-facing half of terminal mode.
//!
//! The session state machine and the transfer engines live in
//! `terminal_mode_core`. This module owns everything that needs a window: the
//! per-workspace store registry, connection establishment, terminal tabs, panel
//! restoration and persistence.

use std::path::PathBuf;

use ::settings::Settings as _;
use anyhow::{Context as _, anyhow};
use collections::HashMap;
use db::kvp::KeyValueStore;
use gpui::{
    App, AppContext as _, Context, Entity, EntityId, Subscription, Task, WeakEntity, Window,
};
use project::Project;
use remote::RemoteConnectionOptions;
use terminal::Terminal;
use terminal_view::TerminalView;
use util::ResultExt as _;
use workspace::dock::DockPosition;
use workspace::{Workspace, remote_project_from_client};

use terminal_mode_core::{
    ConnectionCancelled, ConnectionMode, Event, SerializedTerminalMode, SessionId, SessionStatus,
    SessionStore, SshSession, TERMINAL_MODE_KEY, TerminalModeSettings, remote_home_directory,
    serialized_session_is_active,
};

/// Per-window registry so both terminal-mode panels can reach the same
/// [`SessionStore`] without threading a handle through panel loading.
#[derive(Default)]
struct SessionStoreRegistry {
    stores: HashMap<EntityId, Entity<SessionStore>>,
    /// Keeps the store's workspace and event subscriptions alive for the
    /// window's lifetime.
    subscriptions: HashMap<EntityId, Vec<Subscription>>,
    /// Whether the bottom dock was open before terminal mode hid it, keyed by
    /// the workspace whose mode hid it.
    bottom_dock_hidden: HashMap<EntityId, bool>,
}

impl gpui::Global for SessionStoreRegistry {}

impl SessionStoreRegistry {
    fn key(workspace: &Workspace) -> EntityId {
        workspace.weak_handle().entity_id()
    }
}

/// Returns the store for `workspace`, creating it on first use.
pub fn for_workspace(
    workspace: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> Entity<SessionStore> {
    let workspace_id = SessionStoreRegistry::key(workspace);
    if let Some(existing) = cx
        .try_global::<SessionStoreRegistry>()
        .and_then(|registry| registry.stores.get(&workspace_id))
        .cloned()
    {
        return existing;
    }

    let serialization_key = serialization_key(workspace);
    let store = cx.new(|_| SessionStore::new(serialization_key));
    if !cx.has_global::<SessionStoreRegistry>() {
        cx.set_global(SessionStoreRegistry::default());
    }
    cx.global_mut::<SessionStoreRegistry>()
        .stores
        .insert(workspace_id, store.clone());

    let workspace_entity = cx.entity();
    let store_handle = store.downgrade();
    let workspace_subscription = cx.subscribe(
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

    let release_subscription = cx.on_release(move |_, cx| {
        if !cx.has_global::<SessionStoreRegistry>() {
            return;
        }
        let registry = cx.global_mut::<SessionStoreRegistry>();
        registry.stores.remove(&workspace_id);
        registry.subscriptions.remove(&workspace_id);
        registry.bottom_dock_hidden.remove(&workspace_id);
    });

    // Persistence is driven by the store's own events, so every mutation the
    // core performs is captured without threading counts through each call.
    let persist_subscription = cx.subscribe(&store, {
        let store = store.clone();
        move |workspace, _store, event: &Event, cx| {
            if matches!(event, Event::SessionsChanged | Event::SessionActivated(_)) {
                persist(workspace, &store, cx);
            }
        }
    });

    cx.global_mut::<SessionStoreRegistry>()
        .subscriptions
        .insert(
            workspace_id,
            vec![
                workspace_subscription,
                release_subscription,
                persist_subscription,
            ],
        );

    store
}

/// Returns the store for `workspace` without creating one, for read-only
/// queries from contexts that only have `&Workspace` and `&App`.
pub fn for_workspace_read(workspace: &Workspace, cx: &App) -> Option<Entity<SessionStore>> {
    let workspace_id = SessionStoreRegistry::key(workspace);
    cx.try_global::<SessionStoreRegistry>()?
        .stores
        .get(&workspace_id)
        .cloned()
}

fn serialization_key(workspace: &Workspace) -> Option<String> {
    workspace
        .database_id()
        .map(|id| i64::from(id).to_string())
        .or_else(|| workspace.session_id())
        .map(|id| format!("{TERMINAL_MODE_KEY}-{id:?}"))
}

/// Records whether `workspace`'s bottom dock was hidden by terminal mode.
pub fn set_bottom_dock_hidden(workspace: &Workspace, hidden: bool, cx: &mut App) {
    let workspace_id = SessionStoreRegistry::key(workspace);
    if !cx.has_global::<SessionStoreRegistry>() {
        cx.set_global(SessionStoreRegistry::default());
    }
    cx.global_mut::<SessionStoreRegistry>()
        .bottom_dock_hidden
        .insert(workspace_id, hidden);
}

/// Returns whether `workspace`'s bottom dock was hidden by terminal mode.
pub fn bottom_dock_hidden(workspace: &Workspace, cx: &App) -> bool {
    cx.try_global::<SessionStoreRegistry>()
        .and_then(|registry| {
            registry
                .bottom_dock_hidden
                .get(&SessionStoreRegistry::key(workspace))
                .copied()
        })
        .unwrap_or(false)
}

/// Persists the session list (connection options + terminal counts) to the KVP
/// store.
///
/// The snapshot is built in a deferred task because the terminal counts come
/// from the workspace's panes, and persistence is triggered from within
/// workspace updates (for example the terminal-mode toggle action), where
/// reading the workspace synchronously would re-enter an already-leased entity
/// and panic.
pub fn persist(
    workspace: &mut Workspace,
    store: &Entity<SessionStore>,
    cx: &mut Context<Workspace>,
) {
    let store = store.clone();
    let workspace_handle = workspace.weak_handle();
    cx.defer(move |cx| persist_now(&workspace_handle, &store, cx));
}

/// Writes the current session list to the KVP store.
///
/// Split from [`persist`] so it runs after the workspace update that triggered
/// it has finished, which is required because measuring terminal counts reads
/// the workspace's panes.
fn persist_now(workspace: &WeakEntity<Workspace>, store: &Entity<SessionStore>, cx: &mut App) {
    let Some(key) = store.read(cx).serialization_key().cloned() else {
        return;
    };
    let Some(workspace) = workspace.upgrade() else {
        return;
    };
    let project_counts = workspace.read_with(cx, |workspace, cx| {
        let mut counts: HashMap<EntityId, usize> = HashMap::default();
        for pane in workspace.panes() {
            for terminal_view in pane.read(cx).items_of_type::<TerminalView>() {
                let project_id = terminal_view.read(cx).project().entity_id();
                *counts.entry(project_id).or_default() += 1;
            }
        }
        counts
    });
    let serialized = store.read_with(cx, |store, _| {
        let counts: HashMap<SessionId, usize> = store
            .sessions
            .iter()
            .map(|session| {
                let count = project_counts
                    .get(&session.project.entity_id())
                    .copied()
                    .unwrap_or_default();
                (session.id, count.max(1))
            })
            .collect();
        store.snapshot(&counts)
    });
    let value = match serde_json::to_string(&serialized) {
        Ok(value) => value,
        Err(error) => {
            log::error!("failed to serialize terminal mode state: {error}");
            return;
        }
    };
    let kvp = KeyValueStore::global(cx);
    cx.background_spawn(async move { kvp.write_kvp(key, value).await })
        .detach_and_log_err(cx);
}

/// Establishes a new remote connection, builds its project and mounts a
/// worktree rooted at the remote `$HOME`.
pub fn connect(
    store: &Entity<SessionStore>,
    connection_options: RemoteConnectionOptions,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Task<anyhow::Result<SessionId>> {
    let id = store.update(cx, |store, _| store.next_session_id());
    connect_with_id(
        store,
        id,
        connection_options,
        ConnectionMode::Interactive,
        window,
        cx,
    )
}

/// Connects `id` and registers the resulting session, reporting progress
/// according to `mode`.
pub fn connect_with_id(
    store: &Entity<SessionStore>,
    id: SessionId,
    connection_options: RemoteConnectionOptions,
    mode: ConnectionMode,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Task<anyhow::Result<SessionId>> {
    let workspace_entity = cx.entity();
    store.update(cx, |store, cx| {
        store.begin_connection(id, connection_options.clone(), cx)
    });

    let client_task = match mode {
        ConnectionMode::Interactive => remote_connection::connect_with_modal(
            &workspace_entity,
            connection_options.clone(),
            window,
            cx,
        ),
        ConnectionMode::Background => {
            remote_connection::connect_in_background(connection_options.clone(), cx)
        }
    };

    let store_handle = store.downgrade();
    cx.spawn_in(window, async move |_workspace, cx| {
        let outcome = async {
            let Some(client) = client_task.await.context("remote connection task failed")? else {
                return Err(anyhow::Error::new(ConnectionCancelled));
            };

            let project = cx.update(|_window, cx| {
                let app_state = workspace_entity.read(cx).app_state().clone();
                remote_project_from_client(client.clone(), &app_state, cx)
            })?;

            let home_directory = remote_home_directory(&project, cx).await?;
            let (home_worktree, _) = project
                .update(cx, |project, cx| {
                    project.find_or_create_worktree(home_directory, true, cx)
                })
                .await?;

            let project_for_terminal = project.clone();
            store_handle.update(cx, |store, cx| {
                store.register_session(
                    SshSession::new(id, connection_options, client, project, Some(home_worktree)),
                    cx,
                );
                store.set_active_session(id, cx);
            })?;

            let terminal_task = workspace_entity.update_in(cx, |workspace, window, cx| {
                spawn_session_terminal(workspace, project_for_terminal, window, cx)
            })?;
            cx.update(|_window, cx| terminal_task.detach_and_log_err(cx))?;

            anyhow::Ok(id)
        }
        .await;

        if mode == ConnectionMode::Interactive {
            remote_connection::dismiss_connection_modal(&workspace_entity, cx);
        }

        store_handle.update(cx, |store, cx| {
            store.finish_connection(id, cx);
            if outcome.is_err()
                && let Some(session) = store.sessions.iter_mut().find(|session| session.id == id)
            {
                session.status = SessionStatus::Failed;
            }
        })?;

        outcome
    })
}

/// Drops a session and closes every center terminal bound to its project.
pub fn disconnect(
    store: &Entity<SessionStore>,
    id: SessionId,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(session) = store.update(cx, |store, cx| store.remove_session(id, cx)) else {
        return;
    };
    let project_id = session.project.entity_id();
    // Deferred so this does not re-enter the workspace while it is leased by
    // the action handler that dispatched the disconnect.
    cx.defer_in(window, move |workspace, window, cx| {
        for pane in workspace.panes() {
            let terminal_item_ids: Vec<EntityId> = pane
                .read(cx)
                .items_of_type::<TerminalView>()
                .filter(|terminal_view| terminal_view.read(cx).project().entity_id() == project_id)
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
    store: &Entity<SessionStore>,
    id: SessionId,
    mode: ConnectionMode,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Task<anyhow::Result<SessionId>> {
    let Some(connection_options) = store
        .read(cx)
        .session(id)
        .map(|session| session.connection_options.clone())
    else {
        return Task::ready(Err(anyhow!("no such session")));
    };
    store.update(cx, |store, cx| {
        if let Some(session) = store.sessions.iter_mut().find(|session| session.id == id) {
            session.status = SessionStatus::Reconnecting;
        }
        cx.emit(Event::SessionsChanged);
    });
    let replacement_id = store.update(cx, |store, _| store.next_session_id());
    let connect_task = connect_with_id(store, replacement_id, connection_options, mode, window, cx);
    let store_handle = store.downgrade();
    cx.spawn_in(window, async move |workspace, cx| {
        match connect_task.await {
            Ok(replacement_id) => {
                workspace.update_in(cx, |_workspace, window, cx| {
                    if let Some(store) = store_handle.upgrade() {
                        disconnect(&store, id, window, cx);
                    }
                })?;
                Ok(replacement_id)
            }
            Err(error) => {
                store_handle.update(cx, |store, cx| {
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
        }
    })
}

/// Restores the persisted terminal-mode state: re-opens the panels when the
/// mode was active, and reconnects saved sessions when
/// `auto_connect_last_session` is enabled.
pub fn restore(
    workspace: &mut Workspace,
    store: &Entity<SessionStore>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if !store.update(cx, |store, _| store.begin_restore()) {
        return;
    }
    let Some(key) = store.read(cx).serialization_key().cloned() else {
        return;
    };
    let auto_connect = TerminalModeSettings::get_global(cx).auto_connect_last_session;
    let workspace_handle = workspace.weak_handle();
    let store_handle = store.downgrade();
    let kvp = KeyValueStore::global(cx);
    cx.spawn_in(window, async move |_workspace, cx| {
        let saved = cx
            .background_spawn(async move { kvp.read_kvp(&key) })
            .await
            .log_err()
            .flatten();
        let Some(saved) = saved else {
            return anyhow::Ok(());
        };
        let serialized = match serde_json::from_str::<SerializedTerminalMode>(&saved) {
            Ok(serialized) => serialized,
            Err(error) => {
                log::warn!("failed to restore terminal mode state: {error}");
                return anyhow::Ok(());
            }
        };

        if serialized.terminal_mode_active {
            // The persisted layout mode and the persisted terminal mode are
            // independent, so a saved Zen layout would otherwise stay active
            // while the session panels are on screen. Reset the layout mode
            // before marking terminal mode active: the mode observer ignores
            // this switch, because terminal mode does not own anything yet.
            workspace_handle.update_in(cx, |workspace, window, cx| {
                workspace_modes::switch_to_editor(workspace, window, cx);
            })?;
            store_handle.update(cx, |store, cx| {
                store.set_terminal_mode_active(true, cx);
            })?;
            let bottom_dock_was_open =
                workspace_handle.update_in(cx, |workspace, window, cx| {
                    let bottom_dock = workspace.dock_at_position(DockPosition::Bottom).clone();
                    let was_open = bottom_dock.read(cx).is_open();
                    if was_open {
                        bottom_dock.update(cx, |dock, cx| dock.set_open(false, window, cx));
                    }
                    workspace.open_panel::<crate::SshSessionPanel>(window, cx);
                    workspace.open_panel::<crate::SessionFilePanel>(window, cx);
                    was_open
                })?;
            workspace_handle.update(cx, |workspace, cx| {
                set_bottom_dock_hidden(workspace, bottom_dock_was_open, cx);
            })?;
        }

        if auto_connect {
            let mut restored_active_session = None;
            for (index, session) in serialized.sessions.into_iter().enumerate() {
                let is_active =
                    serialized_session_is_active(session.id, index, serialized.active_session);
                let terminal_count = session.terminal_count;
                let Some(store) = store_handle.upgrade() else {
                    return anyhow::Ok(());
                };
                let connect_task = workspace_handle.update_in(cx, |_workspace, window, cx| {
                    let id = match session.id {
                        Some(id) => {
                            store.update(cx, |store, _| {
                                store.next_session_id =
                                    store.next_session_id.max(id.saturating_add(1));
                            });
                            SessionId(id)
                        }
                        None => store.update(cx, |store, _| store.next_session_id()),
                    };
                    connect_with_id(
                        &store,
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
                        let project = store.read_with(cx, |store, _| {
                            store.session(id).map(|session| session.project.clone())
                        });
                        if let Some(project) = project {
                            workspace_handle.update_in(cx, |workspace, window, cx| {
                                for _ in 0..extra {
                                    spawn_session_terminal(workspace, project.clone(), window, cx)
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
                store_handle.update(cx, |store, cx| store.set_active_session(id, cx))?;
            }
        }
        anyhow::Ok(())
    })
    .detach_and_log_err(cx);
}

/// Returns the terminal views in `workspace` bound to `project_id`.
fn terminals_for_project(
    workspace: &Workspace,
    project_id: EntityId,
    cx: &App,
) -> Vec<Entity<TerminalView>> {
    let mut terminals = Vec::new();
    for pane in workspace.panes() {
        for terminal_view in pane.read(cx).items_of_type::<TerminalView>() {
            if terminal_view.read(cx).project().entity_id() == project_id {
                terminals.push(terminal_view);
            }
        }
    }
    terminals
}

/// Writes `text` to every terminal bound to `id`.
pub fn send_to_session(
    workspace: &Workspace,
    store: &Entity<SessionStore>,
    id: SessionId,
    text: &str,
    cx: &mut App,
) {
    let input = text.as_bytes().to_vec();
    let Some(project_id) = store.read_with(cx, |store, _| {
        store.session(id).map(|session| session.project.entity_id())
    }) else {
        return;
    };
    for terminal_view in terminals_for_project(workspace, project_id, cx) {
        terminal_view.update(cx, |view, cx| {
            view.terminal()
                .update(cx, |terminal, _| terminal.input(input.clone()));
        });
    }
}

/// Writes `text` to every session's terminals.
pub fn send_to_all_sessions(
    workspace: &Workspace,
    store: &Entity<SessionStore>,
    text: &str,
    cx: &mut App,
) {
    let ids: Vec<SessionId> = store.read_with(cx, |store, _| {
        store.sessions.iter().map(|session| session.id).collect()
    });
    for id in ids {
        send_to_session(workspace, store, id, text, cx);
    }
}

/// Sends a quick command (with a trailing newline) to the active session,
/// or to every session when broadcast is enabled.
pub fn run_quick_command(
    workspace: &Workspace,
    store: &Entity<SessionStore>,
    command: &str,
    cx: &mut App,
) {
    let input = format!("{command}\n");
    let (broadcast_enabled, active_session) = store.read_with(cx, |store, _| {
        (store.broadcast_enabled, store.active_session)
    });
    if broadcast_enabled {
        send_to_all_sessions(workspace, store, &input, cx);
    } else if let Some(id) = active_session {
        send_to_session(workspace, store, id, &input, cx);
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
