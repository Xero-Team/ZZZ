use std::collections::BTreeSet;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use strum::EnumString;

use crate::KnownOrUnknown;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct ExtensionApiManifest {
    pub name: String,
    pub version: Arc<str>,
    pub description: Option<String>,
    pub authors: Vec<String>,
    pub repository: String,
    pub schema_version: Option<i32>,
    pub wasm_api_version: Option<String>,
    #[serde(default, deserialize_with = "deserialize_extension_provides")]
    pub provides: BTreeSet<ExtensionProvides>,
}

fn deserialize_extension_provides<'de, D>(
    deserializer: D,
) -> Result<BTreeSet<ExtensionProvides>, D::Error>
where
    D: Deserializer<'de>,
{
    let provides = Vec::<KnownOrUnknown<ExtensionProvides, String>>::deserialize(deserializer)?;
    Ok(provides
        .into_iter()
        .filter_map(|provides| match provides {
            KnownOrUnknown::Known(provides) => Some(provides),
            KnownOrUnknown::Unknown(_) => None,
        })
        .collect())
}

#[derive(
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    EnumString,
    strum::Display,
    strum::EnumIter,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum ExtensionProvides {
    Themes,
    IconThemes,
    Languages,
    Grammars,
    LanguageServers,
    ContextServers,
    /// Deprecated
    AgentServers,
    SlashCommands,
    /// Deprecated
    IndexedDocsProviders,
    /// Deprecated
    Snippets,
    DebugAdapters,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct ExtensionMetadata {
    pub id: Arc<str>,
    #[serde(flatten)]
    pub manifest: ExtensionApiManifest,
    pub published_at: DateTime<Utc>,
    pub download_count: u64,
}

#[derive(Serialize, Deserialize)]
pub struct GetExtensionsResponse {
    pub data: Vec<ExtensionMetadata>,
}
