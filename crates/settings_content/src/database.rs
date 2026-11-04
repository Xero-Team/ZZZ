use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use settings_macros::MergeFrom;

/// Configuration for saved database connections.
#[derive(Debug, PartialEq, Clone, Default, Serialize, Deserialize, JsonSchema, MergeFrom)]
pub struct DatabaseSettingsContent {
    /// Saved connections. Secrets are never stored here; they live in the
    /// platform keychain under the connection's stable id.
    #[serde(default)]
    pub connections: Vec<DatabaseConnectionContent>,
}

/// One saved database connection. Only non-sensitive fields are persisted.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DatabaseConnectionContent {
    /// Stable identifier, also used as the keychain entry name. Renaming the
    /// display name must not change it.
    pub id: String,
    /// Display name shown in the database panel.
    pub name: String,
    /// Driver id, for example `sqlite`.
    pub driver: String,
    /// Database file for file-backed drivers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Server host for network drivers. There is no default host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    /// Login name for network drivers. The password lives in the keychain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
}
