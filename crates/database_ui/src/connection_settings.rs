//! Durable, non-sensitive connection configuration.
//!
//! `database.connections` in settings.json is the single durable owner of
//! saved connections. `ConnectionManager` keeps a runtime copy and owns live
//! connections and query cancellation; it is resynced whenever settings change.
//! Secrets are never part of this file (D-09).

use std::path::{Path, PathBuf};

use database_core::{ConnectionConfig, ConnectionId, DataSource};
use fs::Fs;
use futures::channel::oneshot;
use gpui::App;
use settings::{RegisterSetting, Settings, SettingsContent, update_settings_file_with_completion};

/// Saved connections as read from settings.
#[derive(Debug, Default, Clone, RegisterSetting)]
pub struct DatabaseSettings {
    pub connections: Vec<ConnectionConfig>,
}

impl Settings for DatabaseSettings {
    fn from_settings(content: &SettingsContent) -> Self {
        let connections = content.database.as_ref().map_or_default(|database| {
            database
                .connections
                .iter()
                .filter_map(connection_from_content)
                .collect()
        });
        Self { connections }
    }
}

/// Convert one settings entry into a runtime config. Entries without a usable
/// target are skipped rather than failing the whole settings load.
fn connection_from_content(
    content: &settings::DatabaseConnectionContent,
) -> Option<ConnectionConfig> {
    let source = match (&content.path, &content.host, content.port) {
        (Some(path), _, _) => DataSource::File {
            path: PathBuf::from(path),
        },
        (None, Some(host), Some(port)) => DataSource::Server {
            host: host.clone(),
            port,
        },
        _ => {
            log::warn!(
                "database connection `{}` has neither a path nor a host:port; ignoring it",
                content.id
            );
            return None;
        }
    };
    Some(ConnectionConfig {
        id: ConnectionId(content.id.clone()),
        display_name: content.name.clone(),
        driver_id: content.driver.clone(),
        source,
        host: content.host.clone(),
        port: content.port,
        user: content.user.clone(),
        database: content.database.clone(),
        options: Default::default(),
    })
}

/// Convert a runtime config into the persisted representation.
pub fn connection_to_content(config: &ConnectionConfig) -> settings::DatabaseConnectionContent {
    let (path, host, port) = match &config.source {
        DataSource::File { path } => (Some(path.display().to_string()), None, None),
        DataSource::Server { host, port } => (None, Some(host.clone()), Some(*port)),
        DataSource::InMemory { .. } => (None, None, None),
    };
    settings::DatabaseConnectionContent {
        id: config.id.0.clone(),
        name: config.display_name.clone(),
        driver: config.driver_id.clone(),
        path,
        host,
        port,
        user: config.user.clone(),
        database: config.database.clone(),
    }
}

/// Persist a connection into `database.connections` in settings.json. This is
/// the only durable write path; the runtime manager is synced from settings when
/// the returned receiver completes (settings reload included).
pub(crate) fn persist_connection(
    config: &ConnectionConfig,
    cx: &App,
) -> oneshot::Receiver<anyhow::Result<()>> {
    let content = connection_to_content(config);
    let fs = <dyn Fs>::global(cx);
    update_settings_file_with_completion(fs, cx, move |settings, _cx| {
        let database = settings.database.get_or_insert_default();
        match database
            .connections
            .iter_mut()
            .find(|existing| existing.id == content.id)
        {
            Some(existing) => *existing = content,
            None => database.connections.push(content),
        }
    })
}

/// Generate an id that is stable for the lifetime of the connection and never
/// derived from the display name, so renaming cannot orphan the keychain entry.
pub fn next_connection_id(existing: &[ConnectionConfig]) -> ConnectionId {
    let mut index = 1;
    loop {
        let candidate = ConnectionId(format!("connection-{index}"));
        if !existing.iter().any(|config| config.id == candidate) {
            return candidate;
        }
        index += 1;
    }
}

/// Build a config for a newly opened database file. Reuses an existing saved
/// connection for the same path so the same file is never listed twice.
pub fn config_for_file_path(
    existing: &[ConnectionConfig],
    path: &Path,
) -> (ConnectionConfig, bool) {
    if let Some(config) = existing.iter().find(|config| {
        matches!(&config.source, DataSource::File { path: existing_path } if existing_path == path)
    }) {
        return (config.clone(), false);
    }
    let display_name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let config = ConnectionConfig {
        id: next_connection_id(existing),
        display_name,
        driver_id: database_core::SQLITE_DRIVER_ID.to_owned(),
        source: DataSource::File {
            path: path.to_path_buf(),
        },
        host: None,
        port: None,
        user: None,
        database: None,
        options: Default::default(),
    };
    (config, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use database_core::SQLITE_DRIVER_ID;

    fn file_config(id: &str, path: &str, name: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: ConnectionId(id.into()),
            display_name: name.into(),
            driver_id: SQLITE_DRIVER_ID.into(),
            source: DataSource::File {
                path: PathBuf::from(path),
            },
            host: None,
            port: None,
            user: None,
            database: None,
            options: Default::default(),
        }
    }

    #[test]
    fn settings_round_trip_keeps_ids_and_drops_no_fields() {
        let config = file_config("connection-1", "/tmp/a.db", "a.db");
        let content = connection_to_content(&config);
        assert_eq!(content.id, "connection-1");
        assert_eq!(content.path.as_deref(), Some("/tmp/a.db"));
        assert_eq!(connection_from_content(&content), Some(config));
    }

    #[test]
    fn entries_without_a_target_are_skipped() {
        let content = settings::DatabaseConnectionContent {
            id: "connection-2".into(),
            name: "nowhere".into(),
            driver: SQLITE_DRIVER_ID.into(),
            path: None,
            host: None,
            port: None,
            user: None,
            database: None,
        };
        assert_eq!(connection_from_content(&content), None);
    }

    #[test]
    fn reopened_path_reuses_its_saved_connection() {
        let existing = vec![file_config("connection-3", "/tmp/reused.db", "reused.db")];
        let (config, is_new) = config_for_file_path(&existing, Path::new("/tmp/reused.db"));
        assert!(!is_new);
        assert_eq!(config.id, ConnectionId("connection-3".into()));
    }

    #[test]
    fn new_connection_ids_avoid_existing_ones() {
        let existing = vec![
            file_config("connection-1", "/tmp/a.db", "a.db"),
            file_config("connection-3", "/tmp/c.db", "c.db"),
        ];
        assert_eq!(
            next_connection_id(&existing),
            ConnectionId("connection-2".into())
        );
        assert_eq!(
            next_connection_id(&existing[..1]),
            ConnectionId("connection-2".into())
        );
    }
}
