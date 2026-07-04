//! Contains the [`VimModeSetting`] and [`HelixModeSetting`] used to enable/disable Vim and Helix modes.
//!
//! This is in its own crate as we want other crates to be able to enable or
//! disable Vim/Helix modes without having to depend on the `vim` crate in its
//! entirety.

use gpui::App;
use settings::{RegisterSetting, Settings, SettingsContent};

#[derive(RegisterSetting)]
pub struct VimModeSetting(pub bool);

impl Settings for VimModeSetting {
    fn from_settings(content: &SettingsContent) -> Self {
        Self(content.vim_mode.unwrap())
    }
}

impl VimModeSetting {
    pub fn is_enabled(cx: &App) -> bool {
        Self::try_get(cx)
            .map(|vim_mode| vim_mode.0)
            .unwrap_or(false)
    }
}

#[derive(RegisterSetting)]
pub struct HelixModeSetting(pub bool);

impl HelixModeSetting {
    pub fn is_enabled(cx: &App) -> bool {
        Self::try_get(cx)
            .map(|helix_mode| helix_mode.0)
            .unwrap_or(false)
    }
}

impl Settings for HelixModeSetting {
    fn from_settings(content: &SettingsContent) -> Self {
        Self(content.helix_mode.unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vim_mode_setting_reads_boolean_from_settings_content() {
        let mut content = SettingsContent::default();
        content.vim_mode = Some(true);

        let setting = VimModeSetting::from_settings(&content);

        assert!(setting.0);
    }

    #[test]
    fn helix_mode_setting_reads_boolean_from_settings_content() {
        let mut content = SettingsContent::default();
        content.helix_mode = Some(false);

        let setting = HelixModeSetting::from_settings(&content);

        assert!(!setting.0);
    }
}
