use settings::{RegisterSetting, Settings, SettingsContent, WhichKeySettingsContent};

#[derive(Debug, Clone, Copy, RegisterSetting)]
pub struct WhichKeySettings {
    pub enabled: bool,
    pub delay_ms: u64,
}

impl Settings for WhichKeySettings {
    fn from_settings(content: &SettingsContent) -> Self {
        let which_key: &WhichKeySettingsContent = content.which_key.as_ref().unwrap();

        Self {
            enabled: which_key.enabled.unwrap(),
            delay_ms: which_key.delay_ms.unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_key_settings_reads_enabled_and_delay_values() {
        let mut content = SettingsContent::default();
        content.which_key = Some(WhichKeySettingsContent {
            enabled: Some(true),
            delay_ms: Some(250),
        });

        let settings = WhichKeySettings::from_settings(&content);

        assert!(settings.enabled);
        assert_eq!(settings.delay_ms, 250);
    }
}
