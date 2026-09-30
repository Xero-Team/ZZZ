use settings::{RegisterSetting, Settings, SettingsContent, WhichKeySettingsContent};

#[derive(Debug, Clone, Copy, RegisterSetting)]
pub struct WhichKeySettings {
    pub enabled: bool,
    pub delay_ms: u64,
}

impl Settings for WhichKeySettings {
    fn from_settings(content: &SettingsContent) -> Self {
        let which_key: &WhichKeySettingsContent = content
            .which_key
            .as_ref()
            .expect("value should have the expected type");

        Self {
            enabled: which_key.enabled.expect("enabled should be present"),
            delay_ms: which_key.delay_ms.expect("delay_ms should be present"),
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
