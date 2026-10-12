mod broadcast;
mod file_panel;
mod local_transfer;
mod metrics;
mod monitor;
mod session;
mod session_panel;
mod settings;
mod transfer;

#[cfg(test)]
mod tests;

pub use broadcast::BroadcastModal;
pub use file_panel::SessionFilePanel;
pub use session::{
    Event as SessionEvent, PendingConnection, SessionId, SessionRoot, SessionStatus, SessionStore,
    SshSession,
};
pub use session_panel::SshSessionPanel;
pub use settings::TerminalModeSettings;

use ::settings::Settings as _;
use gpui::{Action, App, Context, Entity, Window, actions};
use i18n::tr;
use serde::Deserialize;
use std::sync::Arc;
use util::ResultExt as _;
use workspace::{Toast, Workspace, dock::DockPosition, notifications::NotificationId};

use crate::session::{ConnectionMode, is_connection_cancelled};

actions!(
    terminal_mode,
    [
        /// Toggles the FinalShell-style terminal mode.
        ToggleTerminalMode,
        /// Toggles focus on the SSH session list.
        ToggleSessionPanel,
        /// Toggles focus on the session file panel.
        ToggleFilePanel,
        /// Toggles the server resource monitor for the active session.
        ToggleMonitorPanel,
        /// Selects the next connection in the SSH session list.
        SelectNextConnection,
        /// Selects the previous connection in the SSH session list.
        SelectPreviousConnection,
        /// Connects to or focuses the selected connection.
        ConfirmConnection,
    ]
);

/// Closes a terminal-mode session and its terminal tabs.
#[derive(Clone, PartialEq, Deserialize, Action)]
#[action(namespace = terminal_mode, no_json)]
pub struct DisconnectSession {
    pub session_id: usize,
}

/// Reconnects a terminal-mode session using its stored connection options.
///
/// `background` suppresses the interactive connection modal; it is set for
/// automatic reconnection attempts.
#[derive(Clone, PartialEq, Deserialize, Action)]
#[action(namespace = terminal_mode, no_json)]
pub struct ReconnectSession {
    pub session_id: usize,
    pub background: bool,
}

pub fn init(cx: &mut App) {
    TerminalModeSettings::register(cx);

    workspace::dock::set_panel_button_visibility_filter(
        Some(Arc::new(TerminalModePanelButtonVisibility)),
        cx,
    );

    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(|workspace, _: &ToggleTerminalMode, window, cx| {
            let store = SessionStore::for_workspace(workspace, cx);
            let active = store.update(cx, |store, cx| {
                let active = !store.terminal_mode_active;
                store.set_terminal_mode_active(active, cx);
                active
            });
            if active {
                enter_terminal_mode(workspace, &store, window, cx);
            } else {
                leave_terminal_mode(workspace, &store, window, cx);
            }
        });
        workspace.register_action(|workspace, _: &ToggleSessionPanel, window, cx| {
            ensure_terminal_mode_active(workspace, window, cx);
            workspace.toggle_panel_focus::<SshSessionPanel>(window, cx);
        });
        workspace.register_action(|workspace, _: &ToggleFilePanel, window, cx| {
            ensure_terminal_mode_active(workspace, window, cx);
            workspace.toggle_panel_focus::<SessionFilePanel>(window, cx);
        });
        workspace.register_action(|workspace, _: &ToggleMonitorPanel, window, cx| {
            ensure_terminal_mode_active(workspace, window, cx);
            let store = SessionStore::for_workspace(workspace, cx);
            store.update(cx, |store, cx| {
                let visible = !store.monitor_visible;
                store.set_monitor_visible(visible, cx);
            });
        });
        workspace.register_action(|workspace, action: &DisconnectSession, window, cx| {
            let store = SessionStore::for_workspace(workspace, cx);
            store.update(cx, |store, cx| {
                store.disconnect(SessionId(action.session_id), window, cx);
            });
        });
        workspace.register_action(|workspace, action: &ReconnectSession, window, cx| {
            let store = SessionStore::for_workspace(workspace, cx);
            let mode = if action.background {
                ConnectionMode::Background
            } else {
                ConnectionMode::Interactive
            };
            let task = store.update(cx, |store, cx| {
                store.reconnect(SessionId(action.session_id), mode, window, cx)
            });
            cx.spawn_in(window, async move |workspace, cx| {
                if let Err(error) = task.await
                    && !is_connection_cancelled(&error)
                {
                    workspace
                        .update_in(cx, |workspace, _window, cx| {
                            workspace.show_toast(
                                Toast::new(
                                    NotificationId::unique::<SshSessionPanel>(),
                                    tr(
                                        cx,
                                        "terminal_mode.reconnect.failed",
                                        "Reconnect failed: {}",
                                    )
                                    .replacen(
                                        "{}",
                                        &format!("{error:#}"),
                                        1,
                                    ),
                                ),
                                cx,
                            );
                        })
                        .log_err();
                }
                anyhow::Ok(())
            })
            .detach_and_log_err(cx);
        });
    })
    .detach();
}

/// Returns whether terminal mode is active for `workspace`.
pub fn is_terminal_mode_active(workspace: &Workspace, cx: &App) -> bool {
    SessionStore::for_workspace_read(workspace, cx)
        .is_some_and(|store| store.read(cx).terminal_mode_active)
}

/// Restricts dock buttons to the panel set that matches the current mode: only
/// the terminal-mode panels are offered while terminal mode is active, and they
/// are hidden otherwise.
struct TerminalModePanelButtonVisibility;

impl workspace::dock::PanelButtonVisibilityFilter for TerminalModePanelButtonVisibility {
    fn is_panel_button_visible(
        &self,
        workspace: &Workspace,
        panel_key: &'static str,
        cx: &App,
    ) -> bool {
        let is_terminal_mode_panel = panel_key == session_panel::SSH_SESSION_PANEL_KEY
            || panel_key == file_panel::SESSION_FILE_PANEL_KEY;
        if is_terminal_mode_active(workspace, cx) {
            is_terminal_mode_panel
        } else {
            !is_terminal_mode_panel
        }
    }
}

/// Enters terminal mode when it is not already active, so that focusing an
/// individual terminal-mode panel also reveals the rest of the mode.
fn ensure_terminal_mode_active(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let store = SessionStore::for_workspace(workspace, cx);
    if store.read(cx).terminal_mode_active {
        return;
    }
    store.update(cx, |store, cx| store.set_terminal_mode_active(true, cx));
    enter_terminal_mode(workspace, &store, window, cx);
}

/// Opens the terminal-mode panels and closes the bottom dock so the window is
/// focused on remote sessions. The bottom dock's previous visibility is
/// restored when terminal mode is left.
fn enter_terminal_mode(
    workspace: &mut Workspace,
    store: &Entity<SessionStore>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let bottom_dock = workspace.dock_at_position(DockPosition::Bottom).clone();
    let bottom_dock_was_open = bottom_dock.read(cx).is_open();
    if bottom_dock_was_open {
        bottom_dock.update(cx, |dock, cx| dock.set_open(false, window, cx));
    }

    workspace.open_panel::<SshSessionPanel>(window, cx);
    workspace.open_panel::<SessionFilePanel>(window, cx);

    store.update(cx, |store, _| {
        store.bottom_dock_hidden = bottom_dock_was_open
    });
}

/// Closes the terminal-mode panels and restores the dock state that
/// [`enter_terminal_mode`] changed.
fn leave_terminal_mode(
    workspace: &mut Workspace,
    store: &Entity<SessionStore>,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    workspace.close_panel::<SshSessionPanel>(window, cx);
    workspace.close_panel::<SessionFilePanel>(window, cx);

    let restore_bottom_dock = store.update(cx, |store, _| {
        let hidden = store.bottom_dock_hidden;
        store.bottom_dock_hidden = false;
        hidden
    });
    if restore_bottom_dock {
        let bottom_dock = workspace.dock_at_position(DockPosition::Bottom).clone();
        bottom_dock.update(cx, |dock, cx| dock.set_open(true, window, cx));
    }
}
