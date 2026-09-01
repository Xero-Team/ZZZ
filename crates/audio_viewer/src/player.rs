use std::{io::Cursor, sync::Arc, time::Duration};

use anyhow::Context as _;
use audio::{Audio, PlaybackHandle};
use gpui::App;
use rodio::{Decoder, Source};

pub(crate) const SUPPORTED_EXTENSIONS: &[&str] =
    &["mp3", "wav", "flac", "ogg", "oga", "m4a", "aac"];

pub(crate) fn is_supported_audio_extension(extension: &str) -> bool {
    SUPPORTED_EXTENSIONS
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

pub(crate) fn format_label(extension: &str) -> &'static str {
    match extension.to_ascii_lowercase().as_str() {
        "mp3" => "MP3",
        "wav" => "WAV",
        "flac" => "FLAC",
        "ogg" | "oga" => "OGG",
        "m4a" => "M4A",
        "aac" => "AAC",
        _ => "Audio",
    }
}

pub(crate) struct AudioMetadata {
    pub sample_rate: u32,
    pub channels: u16,
    pub duration: Option<Duration>,
    pub file_size: u64,
    pub format_label: &'static str,
}

pub(crate) fn open_decoder(
    bytes: Arc<[u8]>,
    format_hint: &str,
) -> anyhow::Result<Decoder<Cursor<Arc<[u8]>>>> {
    let byte_len = bytes.len() as u64;
    Decoder::builder()
        .with_data(Cursor::new(bytes))
        .with_byte_len(byte_len)
        .with_seekable(true)
        .with_hint(format_hint)
        .build()
        .map_err(|error| anyhow::anyhow!(error))
        .context("decoding audio")
}

pub(crate) fn decode_metadata(
    bytes: Arc<[u8]>,
    format_hint: &str,
) -> anyhow::Result<AudioMetadata> {
    let file_size = bytes.len() as u64;
    let decoder = open_decoder(bytes, format_hint)?;
    Ok(AudioMetadata {
        sample_rate: decoder.sample_rate().get(),
        channels: decoder.channels().get(),
        duration: decoder.total_duration(),
        file_size,
        format_label: format_label(format_hint),
    })
}

pub(crate) fn play_bytes(
    bytes: Arc<[u8]>,
    format_hint: &str,
    start: Duration,
    volume: f32,
    cx: &mut App,
) -> anyhow::Result<PlaybackHandle> {
    let mut decoder = open_decoder(bytes, format_hint)?;
    if start > Duration::ZERO
        && let Err(error) = decoder.try_seek(start)
    {
        log::warn!("seeking audio decoder: {error}");
    }

    let handle = Audio::play_source(decoder, cx)?;
    handle.set_volume(volume);
    if start > Duration::ZERO
        && let Err(error) = handle.try_seek(start)
    {
        log::warn!("seeking audio preview: {error}");
    }
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::is_supported_audio_extension;

    #[test]
    fn mp3_is_supported() {
        assert!(is_supported_audio_extension("mp3"));
    }

    #[test]
    fn wav_is_supported_case_insensitive() {
        assert!(is_supported_audio_extension("WAV"));
        assert!(is_supported_audio_extension("wav"));
        assert!(is_supported_audio_extension("Wav"));
    }

    #[test]
    fn flac_ogg_m4a_aac_are_supported() {
        assert!(is_supported_audio_extension("flac"));
        assert!(is_supported_audio_extension("ogg"));
        assert!(is_supported_audio_extension("oga"));
        assert!(is_supported_audio_extension("m4a"));
        assert!(is_supported_audio_extension("aac"));
    }

    #[test]
    fn opus_wma_wv_mka_are_not_supported() {
        assert!(!is_supported_audio_extension("opus"));
        assert!(!is_supported_audio_extension("wma"));
        assert!(!is_supported_audio_extension("wv"));
        assert!(!is_supported_audio_extension("mka"));
    }
}
