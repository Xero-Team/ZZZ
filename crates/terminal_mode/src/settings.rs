use ::settings::{RegisterSetting, Settings};
use gpui::{Pixels, px};

/// Settings for the FinalShell-style terminal mode.
#[derive(Debug, Clone, PartialEq, RegisterSetting)]
pub struct TerminalModeSettings {
    pub session_list_default_width: Pixels,
    pub file_panel_default_width: Pixels,
    pub preview_enabled: bool,
    pub auto_connect_last_session: bool,
    pub auto_reconnect: bool,
    pub reconnect_max_backoff_ms: u64,
    pub monitor_enabled: bool,
    pub monitor_refresh_interval_ms: u64,
    pub quick_commands: Vec<String>,
    pub session_starting_command: Option<String>,
    pub session_working_directory: Option<String>,
}

impl Settings for TerminalModeSettings {
    fn from_settings(content: &::settings::SettingsContent) -> Self {
        let terminal_mode = content
            .terminal_mode
            .as_ref()
            .expect("terminal_mode should be present");
        Self {
            session_list_default_width: px(terminal_mode
                .session_list_default_width
                .expect("session_list_default_width should be present")),
            file_panel_default_width: px(terminal_mode
                .file_panel_default_width
                .expect("file_panel_default_width should be present")),
            preview_enabled: terminal_mode
                .preview_enabled
                .expect("preview_enabled should be present"),
            auto_connect_last_session: terminal_mode
                .auto_connect_last_session
                .expect("auto_connect_last_session should be present"),
            auto_reconnect: terminal_mode
                .auto_reconnect
                .expect("auto_reconnect should be present"),
            reconnect_max_backoff_ms: terminal_mode
                .reconnect_max_backoff_ms
                .expect("reconnect_max_backoff_ms should be present"),
            monitor_enabled: terminal_mode
                .monitor_enabled
                .expect("monitor_enabled should be present"),
            monitor_refresh_interval_ms: terminal_mode
                .monitor_refresh_interval_ms
                .expect("monitor_refresh_interval_ms should be present"),
            quick_commands: terminal_mode.quick_commands.clone().unwrap_or_default(),
            session_starting_command: terminal_mode.session_starting_command.clone(),
            session_working_directory: terminal_mode.session_working_directory.clone(),
        }
    }
}
