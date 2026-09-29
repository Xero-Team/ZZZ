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
}

impl Settings for TelegramPanelSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let panel = content
            .telegram_panel
            .clone()
            .expect("telegram_panel settings must be present in default.json");
        Self {
            button: panel.button.unwrap(),
            dock: panel.dock.unwrap().into(),
            default_width: px(panel.default_width.unwrap()),
            default_height: px(panel.default_height.unwrap()),
            flexible: panel.flexible.unwrap(),
            starts_open: panel.starts_open.unwrap(),
            show_unread_badge: panel.show_unread_badge.unwrap(),
            max_content_width: if panel.limit_content_width.unwrap() {
                Some(px(panel.max_content_width.unwrap()))
            } else {
                None
            },
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
            .expect("telegram settings must be present in default.json");
        Self {
            proxy: telegram
                .proxy
                .map(|proxy| proxy.trim().to_owned())
                .filter(|proxy| !proxy.is_empty()),
        }
    }
}
