mod extension;
pub mod internal_api;
mod known_or_unknown;
pub mod websocket_protocol;

use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub use crate::extension::*;
pub use crate::known_or_unknown::*;

pub const ZED_SYSTEM_ID_HEADER_NAME: &str = "x-zed-system-id";

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Serialize, Deserialize)]
pub struct OrganizationId(pub Arc<str>);

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SubmitAgentThreadFeedbackBody {
    pub organization_id: Option<OrganizationId>,
    pub agent: String,
    pub session_id: String,
    pub parent_session_id: Option<String>,
    pub rating: String,
    pub thread: serde_json::Value,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SubmitAgentThreadFeedbackCommentsBody {
    pub organization_id: Option<OrganizationId>,
    pub agent: String,
    pub session_id: String,
    pub comments: String,
    pub thread: serde_json::Value,
}
