use gpui::Pixels;
use settings::{RegisterSetting, Settings};
use ui::px;
use workspace::dock::DockPosition;

/// Settings for the read-only Fossil panel.
///
/// Deliberately small: the panel offers no staging, no commit editor and no
/// remotes, so there is nothing here to configure about them.
#[derive(Debug, Clone, PartialEq, RegisterSetting)]
pub struct FossilPanelSettings {
    pub button: bool,
    pub dock: DockPosition,
    pub default_width: Pixels,
    pub starts_open: bool,
}

impl Settings for FossilPanelSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let fossil_panel = content
            .fossil_panel
            .clone()
            .expect("value should be present");
        Self {
            button: fossil_panel.button.expect("button should be present"),
            dock: fossil_panel.dock.expect("dock should be present").into(),
            default_width: px(fossil_panel
                .default_width
                .expect("default_width should be present")),
            starts_open: fossil_panel
                .starts_open
                .expect("starts_open should be present"),
        }
    }
}
