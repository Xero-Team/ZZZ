mod db;
mod edit_agent;
mod legacy_thread;
pub mod outline;
mod pattern_extraction;
mod templates;
#[cfg(test)]
mod tests;
mod thread;
mod thread_store;
mod tool_permissions;
mod tools;

pub use db::*;
pub use pattern_extraction::*;
pub use shell_command_parser::extract_commands;
pub use templates::*;
pub use thread::*;
pub use thread_store::*;
pub use tool_permissions::*;
pub use tools::*;

use chrono::{DateTime, Utc};
use gpui::SharedString;
use project::AgentId;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

pub static ZED_AGENT_ID: LazyLock<AgentId> = LazyLock::new(|| AgentId::new("zed-agent"));

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectSnapshot {
    pub worktree_snapshots: Vec<project::project_snapshot::ProjectWorktreeSnapshot>,
    pub timestamp: DateTime<Utc>,
}

pub struct RulesLoadingError {
    pub message: SharedString,
}
