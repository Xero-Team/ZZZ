//! Transport and session bookkeeping for the FinalShell-style terminal mode.
//!
//! This crate owns everything that is not a widget: the SSH session state
//! machine, the cross-host and local transfer engines, and the server metrics
//! parser. The `terminal_mode` crate keeps the panels and wires the workspace
//! to these types.

#[cfg(test)]
mod tests;

mod local_transfer;
mod metrics;
mod session;
mod settings;
mod transfer;

pub use local_transfer::{
    LOCAL_MOUNT_ID, download_from_session, ensure_local_snapshot, local_vfs_path, upload_to_session,
};
pub use metrics::{
    DiskUsage, LoadAverage, MemoryUsage, NetworkInterface, ServerMetrics, format_bytes,
    format_rate, format_uptime, parse_metrics, sampling_command,
};
pub use session::{
    ConnectionCancelled, ConnectionMode, Event, PendingConnection, ReconnectSession,
    SerializedSession, SerializedTerminalMode, SessionId, SessionRoot, SessionStatus, SessionStore,
    SshSession, TERMINAL_MODE_KEY, is_connection_cancelled, remote_home_directory,
    serialized_session_is_active,
};
pub use settings::TerminalModeSettings;
pub use transfer::{
    copy_between_sessions, ensure_session_snapshot, mount_id_for_session,
    mount_id_for_session_root, move_between_sessions,
};
