use ::settings::{RegisterSetting, Settings};
use gpui::{Pixels, px};

/// Settings for the FinalShell-style terminal mode.
#[derive(Debug, Clone, PartialEq, RegisterSetting)]
pub struct TerminalModeSettings {
    pub session_list_default_width: Pixels,
    pub file_panel_default_width: Pixels,
    pub preview_enabled: bool,
    pub auto_connect_last_session: bool,
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
        }
    }
}
