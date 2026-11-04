//! GPUI wiring for the relational database explorer.

use std::sync::Arc;

use database_core::{ConnectionManager, SqliteDriver};
use gpui::{App, AppContext as _, Context, Global, actions};
use settings::{Settings as _, SettingsStore};
use workspace::Workspace;

pub use database_core::{ExtensionDriver, ExtensionDriverConfig};

use crate::connection_panel::ConnectionPanel;
use crate::connection_settings::DatabaseSettings;
use crate::credentials::KeychainCredentials;

mod connection_dialog;
mod connection_panel;
mod connection_settings;
mod credentials;
mod result_grid;
mod result_set_view;

actions!(
    database,
    [
        OpenDatabase,
        AddDatabaseConnection,
        ToggleDatabasePanel,
        RunStatement,
        RunSelection,
        CancelQuery,
        StageCellEdit,
        CancelCellEdit
    ]
);

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

    /// Apply saved connections from settings to the runtime registry. Live
    /// connections whose configuration did not change are preserved; changed
    /// or removed ones are reset and their queries cancelled.
    pub fn sync_from_settings(cx: &App) {
        let manager = Self::manager(cx);
        let settings = DatabaseSettings::get_global(cx);
        manager.sync_configs(settings.connections.clone());
    }
}

/// Register a driver provided by an installed extension. Registration is
/// keyed by the namespaced id, so a reload replaces the previous driver.
pub fn register_extension_driver(config: ExtensionDriverConfig, cx: &App) {
    DatabaseState::manager(cx).register_driver(Arc::new(ExtensionDriver::new(config)));
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
    DatabaseState::sync_from_settings(cx);
    cx.observe_global::<SettingsStore>(|cx: &mut App| {
        DatabaseState::sync_from_settings(cx);
    })
    .detach();

    cx.observe_new(
        |workspace: &mut Workspace, window, cx: &mut Context<Workspace>| {
            let workspace_handle = cx.entity().downgrade();
            workspace.register_action(|workspace, _: &OpenDatabase, window, cx| {
                ConnectionPanel::open_database_file(workspace, window, cx);
            });
            workspace.register_action(|workspace, _: &AddDatabaseConnection, window, cx| {
                ConnectionPanel::add_connection(workspace, window, cx);
            });
            workspace.register_action(|workspace, _: &ToggleDatabasePanel, window, cx| {
                workspace.toggle_panel_focus::<ConnectionPanel>(window, cx);
            });
            let Some(window) = window else {
                return;
            };
            let panel = cx.new(|cx| ConnectionPanel::new(workspace_handle, cx));
            workspace.add_panel(panel, window, cx);
        },
    )
    .detach();
}
