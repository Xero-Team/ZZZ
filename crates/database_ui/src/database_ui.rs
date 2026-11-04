//! GPUI wiring for the relational database explorer.

use std::sync::Arc;

use database_core::{ConnectionManager, SqliteDriver};
use gpui::{App, AppContext as _, Context, Global, actions};
use workspace::Workspace;

use crate::connection_panel::ConnectionPanel;
use crate::credentials::KeychainCredentials;

mod connection_panel;
mod credentials;
mod result_set_view;

actions!(database, [OpenDatabase, ToggleDatabasePanel]);

/// Application-global database state. The manager is the sole owner of
/// connection lifecycle (invariant 6); the credentials helper is the only path
/// to the keychain (D-09).
pub struct DatabaseState {
    manager: Arc<ConnectionManager>,
    credentials: Arc<KeychainCredentials>,
}

impl Global for DatabaseState {}

impl DatabaseState {
    pub fn manager(cx: &App) -> Arc<ConnectionManager> {
        cx.global::<DatabaseState>().manager.clone()
    }

    pub fn credentials(cx: &App) -> Arc<KeychainCredentials> {
        cx.global::<DatabaseState>().credentials.clone()
    }
}

/// Register the database explorer UI with the application. Called from
/// `crates/zzz/src/main.rs` alongside the other `init(cx)` registrations.
pub fn init(cx: &mut App) {
    let manager = Arc::new(ConnectionManager::new());
    manager.register_driver(Arc::new(SqliteDriver));
    let credentials = Arc::new(KeychainCredentials::new(cx));
    cx.set_global(DatabaseState {
        manager,
        credentials,
    });

    cx.observe_new(
        |workspace: &mut Workspace, window, cx: &mut Context<Workspace>| {
            workspace.register_action(|workspace, _: &OpenDatabase, window, cx| {
                ConnectionPanel::open_database_file(workspace, window, cx);
            });
            workspace.register_action(|workspace, _: &ToggleDatabasePanel, window, cx| {
                workspace.toggle_panel_focus::<ConnectionPanel>(window, cx);
            });
            let Some(window) = window else {
                return;
            };
            let panel = cx.new(|cx| ConnectionPanel::new(cx));
            workspace.add_panel(panel, window, cx);
        },
    )
    .detach();
}
