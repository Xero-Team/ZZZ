mod db;
mod history;
mod legacy_thread;
pub mod outline;
mod pattern_extraction;
mod thread_store;
mod tool_permissions;
mod tool_protocol;
mod tools;

pub use db::*;
pub use history::*;
pub use pattern_extraction::*;
pub use shell_command_parser::extract_commands;
pub use thread_store::*;
pub use tool_permissions::*;
pub use tool_protocol::*;
pub use tools::*;

use chrono::{DateTime, Utc};
use project::AgentId;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

pub static ZED_AGENT_ID: LazyLock<AgentId> = LazyLock::new(|| AgentId::new("zed-agent"));

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectSnapshot {
    pub worktree_snapshots: Vec<project::project_snapshot::ProjectWorktreeSnapshot>,
    pub timestamp: DateTime<Utc>,
}
