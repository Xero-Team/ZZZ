use gpui::{Pixels, px};
use settings::{RegisterSetting, Settings};
use workspace::dock::DockPosition;

/// Settings for the Telegram panel.
#[derive(Debug, Clone, PartialEq, RegisterSetting)]
pub struct TelegramPanelSettings {
    pub button: bool,
    pub dock: DockPosition,
    pub default_width: Pixels,
    pub default_height: Pixels,
    pub flexible: bool,
    pub starts_open: bool,
    pub show_unread_badge: bool,
    pub max_content_width: Option<Pixels>,
    /// Minimum number of lines the composer shows before it grows.
    pub composer_min_lines: usize,
}

impl Settings for TelegramPanelSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let panel = content
            .telegram_panel
            .clone()
            .expect("telegram_panel should be present");
        Self {
            button: panel.button.expect("button should be present"),
            dock: panel.dock.expect("dock should be present").into(),
            default_width: px(panel
                .default_width
                .expect("default_width should be present")),
            default_height: px(panel
                .default_height
                .expect("default_height should be present")),
            flexible: panel.flexible.expect("flexible should be present"),
            starts_open: panel.starts_open.expect("starts_open should be present"),
            show_unread_badge: panel
                .show_unread_badge
                .expect("show_unread_badge should be present"),
            max_content_width: if panel
                .limit_content_width
                .expect("limit_content_width should be present")
            {
                Some(px(panel
                    .max_content_width
                    .expect("max_content_width should be present")))
            } else {
                None
            },
            composer_min_lines: panel
                .composer_min_lines
                .expect("composer_min_lines should be present")
                as usize,
        }
    }
}

/// Settings for the Telegram connection.
#[derive(Debug, Clone, PartialEq, RegisterSetting)]
pub struct TelegramSettings {
    /// A dedicated SOCKS5 proxy for Telegram.
    pub proxy: Option<String>,
}

impl Settings for TelegramSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let telegram = content
            .telegram
            .clone()
            .expect("telegram should be present");
        Self {
            proxy: telegram
                .proxy
                .map(|proxy| proxy.trim().to_owned())
                .filter(|proxy| !proxy.is_empty()),
        }
    }
}
