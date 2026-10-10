mod file_panel;
mod session;
mod session_panel;
mod settings;
mod transfer;

#[cfg(test)]
mod tests;

pub use file_panel::SessionFilePanel;
pub use session::{Event as SessionEvent, SessionId, SessionStore, SshSession};
pub use session_panel::SshSessionPanel;
pub use settings::TerminalModeSettings;

use ::settings::Settings as _;
use gpui::{Action, App, actions};
use serde::Deserialize;
use workspace::Workspace;

actions!(
    terminal_mode,
    [
        /// Toggles the FinalShell-style terminal mode.
        ToggleTerminalMode,
        /// Toggles focus on the SSH session list.
        ToggleSessionPanel,
        /// Toggles focus on the session file panel.
        ToggleFilePanel,
    ]
);

/// Closes a terminal-mode session and its terminal tabs.
#[derive(Clone, PartialEq, Deserialize, Action)]
#[action(namespace = terminal_mode, no_json)]
pub struct DisconnectSession {
    pub session_id: usize,
}

/// Reconnects a terminal-mode session using its stored connection options.
#[derive(Clone, PartialEq, Deserialize, Action)]
#[action(namespace = terminal_mode, no_json)]
pub struct ReconnectSession {
    pub session_id: usize,
}

pub fn init(cx: &mut App) {
    TerminalModeSettings::register(cx);

    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(|workspace, _: &ToggleTerminalMode, window, cx| {
            let store = SessionStore::for_workspace(workspace, cx);
            let active = store.update(cx, |store, cx| {
                let active = !store.terminal_mode_active;
                store.set_terminal_mode_active(active, cx);
                active
            });
            if active {
                workspace.open_panel::<SshSessionPanel>(window, cx);
                workspace.open_panel::<SessionFilePanel>(window, cx);
                workspace.close_panel::<terminal_view::terminal_panel::TerminalPanel>(window, cx);
            } else {
                workspace.close_panel::<SshSessionPanel>(window, cx);
                workspace.close_panel::<SessionFilePanel>(window, cx);
                workspace.open_panel::<terminal_view::terminal_panel::TerminalPanel>(window, cx);
            }
        });
        workspace.register_action(|workspace, _: &ToggleSessionPanel, window, cx| {
            workspace.toggle_panel_focus::<SshSessionPanel>(window, cx);
        });
        workspace.register_action(|workspace, _: &ToggleFilePanel, window, cx| {
            workspace.toggle_panel_focus::<SessionFilePanel>(window, cx);
        });
        workspace.register_action(|workspace, action: &DisconnectSession, window, cx| {
            let store = SessionStore::for_workspace(workspace, cx);
            store.update(cx, |store, cx| {
                store.disconnect(SessionId(action.session_id), window, cx);
            });
        });
        workspace.register_action(|workspace, action: &ReconnectSession, window, cx| {
            let store = SessionStore::for_workspace(workspace, cx);
            store
                .update(cx, |store, cx| {
                    store.reconnect(SessionId(action.session_id), window, cx)
                })
                .detach_and_log_err(cx);
        });
    })
    .detach();
}
