use std::{
    str::FromStr,
    sync::atomic::{AtomicBool, Ordering},
};

use cpal::DeviceId;
use gpui::App;
use settings::{RegisterSetting, Settings, SettingsStore};
use util::ResultExt;

#[derive(Clone, Debug, RegisterSetting)]
pub struct AudioSettings {
    /// Automatically increase or decrease your microphone's volume. This affects how
    /// loud you sound to others.
    ///
    /// Recommended: off (default)
    /// Microphones are too quite in zzz, until everyone is on experimental
    /// audio and has auto speaker volume on this will make you very loud
    /// compared to other speakers.
    pub auto_microphone_volume: bool,
    /// Select specific output audio device.
    pub output_audio_device: Option<DeviceId>,
    /// Select specific input audio device.
    pub input_audio_device: Option<DeviceId>,
    /// Volume fader for the audio preview, before applying the perceptual gain curve.
    pub preview_volume: f32,
}

/// Configuration of audio in ZZZ
impl Settings for AudioSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let audio = content.audio.as_ref();
        AudioSettings {
            auto_microphone_volume: audio
                .and_then(|audio| audio.auto_microphone_volume)
                .unwrap_or(false),
            output_audio_device: audio
                .and_then(|audio| audio.output_audio_device.as_ref())
                .and_then(|device| parse_device_id(device.0.as_deref())),
            input_audio_device: audio
                .and_then(|audio| audio.input_audio_device.as_ref())
                .and_then(|device| parse_device_id(device.0.as_deref())),
            preview_volume: audio
                .and_then(|audio| audio.preview_volume)
                .unwrap_or(1.0)
                .clamp(0.0, 1.0),
        }
    }
}

/// Parses a device id stored in the settings file.
///
/// An unparsable id is logged and treated as "no device selected", so a stale
/// or hand-edited setting falls back to the default device instead of failing.
fn parse_device_id(id: Option<&str>) -> Option<DeviceId> {
    let id = id?;
    DeviceId::from_str(id).log_err()
}

/// See docs on [LIVE_SETTINGS]
pub struct LiveSettings {
    pub auto_microphone_volume: AtomicBool,
}

impl LiveSettings {
    pub(crate) fn initialize(&self, cx: &mut App) {
        cx.observe_global::<SettingsStore>(move |cx| {
            LIVE_SETTINGS.auto_microphone_volume.store(
                AudioSettings::get_global(cx).auto_microphone_volume,
                Ordering::Relaxed,
            );
        })
        .detach();

        let init_settings = AudioSettings::get_global(cx);
        LIVE_SETTINGS
            .auto_microphone_volume
            .store(init_settings.auto_microphone_volume, Ordering::Relaxed);
    }
}

/// Allows access to settings from the audio thread. Updated by
/// observer of SettingsStore. Needed because audio playback and recording are
/// real time and must each run in a dedicated OS thread, therefore we can not
/// use the background executor.
pub static LIVE_SETTINGS: LiveSettings = LiveSettings {
    auto_microphone_volume: AtomicBool::new(false),
};

#[cfg(test)]
mod tests {
    use super::*;
    use settings::{AudioSettingsContent, SettingsContent};

    #[test]
    fn unparsable_device_ids_fall_back_to_no_device() {
        assert_eq!(parse_device_id(None), None);
        assert_eq!(parse_device_id(Some("not-a-device-id")), None);
    }

    #[test]
    fn parsable_device_ids_are_kept() {
        // Derive the string from the host of the current platform: `FromStr`
        // rejects host names that the running platform does not support, so a
        // hard-coded backend name would make this test platform-specific.
        let device_id = cpal::DeviceId::new(cpal::default_host().id(), "device-0");
        assert_eq!(
            parse_device_id(Some(device_id.to_string().as_str())),
            Some(device_id)
        );
    }

    fn settings_with_preview_volume(preview_volume: Option<f32>) -> SettingsContent {
        SettingsContent {
            audio: Some(AudioSettingsContent {
                preview_volume,
                ..AudioSettingsContent::default()
            }),
            ..SettingsContent::default()
        }
    }

    #[test]
    fn preview_volume_defaults_to_full_volume() {
        assert_eq!(
            AudioSettings::from_settings(&SettingsContent::default()).preview_volume,
            1.0
        );
    }

    #[test]
    fn preview_volume_is_read_and_clamped() {
        assert_eq!(
            AudioSettings::from_settings(&settings_with_preview_volume(Some(0.35))).preview_volume,
            0.35
        );
        assert_eq!(
            AudioSettings::from_settings(&settings_with_preview_volume(Some(-0.5))).preview_volume,
            0.0
        );
        assert_eq!(
            AudioSettings::from_settings(&settings_with_preview_volume(Some(1.5))).preview_volume,
            1.0
        );
    }
}
