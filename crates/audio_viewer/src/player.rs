use std::{fs::File, io::BufReader, path::Path, sync::Arc, time::Duration};

use anyhow::Context as _;
use audio::{Audio, PlaybackHandle};
use gpui::{App, Image, ImageFormat};
use lofty::{
    file::{AudioFile, TaggedFileExt},
    picture::{MimeType, Picture, PictureType},
    prelude::Accessor,
};
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
    pub bitrate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
}

pub(crate) struct LoadedAudioInfo {
    pub metadata: AudioMetadata,
    pub cover: Option<Arc<Image>>,
}

pub(crate) fn open_decoder(
    path: &Path,
    format_hint: &str,
) -> anyhow::Result<Decoder<BufReader<File>>> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let byte_len = file.metadata()?.len();
    Decoder::builder()
        .with_data(BufReader::new(file))
        .with_byte_len(byte_len)
        .with_seekable(true)
        .with_hint(format_hint)
        .build()
        .map_err(|error| anyhow::anyhow!(error))
        .context("decoding audio")
}

pub(crate) fn read_audio(path: &Path, format_hint: &str) -> anyhow::Result<LoadedAudioInfo> {
    let file_size = std::fs::metadata(path)
        .with_context(|| format!("reading {}", path.display()))?
        .len();
    match read_with_lofty(path, format_hint, file_size) {
        Ok(info) => Ok(info),
        Err(error) => {
            log::warn!("reading audio tags with lofty: {error:#}");
            read_with_decoder(path, format_hint, file_size)
        }
    }
}

fn read_with_lofty(
    path: &Path,
    format_hint: &str,
    file_size: u64,
) -> anyhow::Result<LoadedAudioInfo> {
    let tagged_file = lofty::read_from_path(path).map_err(|error| anyhow::anyhow!(error))?;
    let properties = tagged_file.properties();
    let duration = properties.duration();
    let mut metadata = AudioMetadata {
        sample_rate: properties.sample_rate().unwrap_or(0),
        channels: properties.channels().unwrap_or(0) as u16,
        duration: (!duration.is_zero()).then_some(duration),
        file_size,
        format_label: format_label(format_hint),
        bitrate: properties
            .overall_bitrate()
            .or_else(|| properties.audio_bitrate())
            .filter(|bitrate| *bitrate > 0),
        bit_depth: properties.bit_depth().filter(|depth| *depth > 0),
        title: None,
        artist: None,
        album: None,
    };

    if let Some(tag) = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag())
    {
        metadata.title = nonempty_tag(tag.title());
        metadata.artist = nonempty_tag(tag.artist());
        metadata.album = nonempty_tag(tag.album());
    }

    if metadata.sample_rate == 0 || metadata.channels == 0 {
        let decoder = open_decoder(path, format_hint)?;
        if metadata.sample_rate == 0 {
            metadata.sample_rate = decoder.sample_rate().get();
        }
        if metadata.channels == 0 {
            metadata.channels = decoder.channels().get();
        }
        if metadata.duration.is_none() {
            metadata.duration = decoder.total_duration();
        }
    }

    Ok(LoadedAudioInfo {
        cover: extract_cover(tagged_file.tags()),
        metadata,
    })
}

fn read_with_decoder(
    path: &Path,
    format_hint: &str,
    file_size: u64,
) -> anyhow::Result<LoadedAudioInfo> {
    let decoder = open_decoder(path, format_hint)?;
    Ok(LoadedAudioInfo {
        metadata: AudioMetadata {
            sample_rate: decoder.sample_rate().get(),
            channels: decoder.channels().get(),
            duration: decoder.total_duration(),
            file_size,
            format_label: format_label(format_hint),
            bitrate: None,
            bit_depth: None,
            title: None,
            artist: None,
            album: None,
        },
        cover: None,
    })
}

fn nonempty_tag(value: Option<std::borrow::Cow<'_, str>>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn extract_cover<'a>(tags: impl IntoIterator<Item = &'a lofty::tag::Tag>) -> Option<Arc<Image>> {
    let mut fallback = None;
    for tag in tags {
        for picture in tag.pictures() {
            if picture.pic_type() == PictureType::CoverFront {
                if let Some(image) = cover_from_picture(picture) {
                    return Some(image);
                }
            } else if fallback.is_none() {
                fallback = cover_from_picture(picture);
            }
        }
    }
    fallback
}

fn cover_from_picture(picture: &Picture) -> Option<Arc<Image>> {
    let data = picture.data();
    if data.is_empty() {
        return None;
    }
    let format = picture
        .mime_type()
        .and_then(image_format_from_mime)
        .or_else(|| image_format_from_bytes(data))?;
    Some(Arc::new(Image::from_bytes(format, data.to_vec())))
}

fn image_format_from_mime(mime: &MimeType) -> Option<ImageFormat> {
    match mime.as_str() {
        "image/jpeg" | "image/jpg" => Some(ImageFormat::Jpeg),
        "image/png" => Some(ImageFormat::Png),
        "image/gif" => Some(ImageFormat::Gif),
        "image/webp" => Some(ImageFormat::Webp),
        "image/bmp" => Some(ImageFormat::Bmp),
        "image/tiff" => Some(ImageFormat::Tiff),
        _ => None,
    }
}

fn image_format_from_bytes(data: &[u8]) -> Option<ImageFormat> {
    match image::guess_format(data).ok()? {
        image::ImageFormat::Png => Some(ImageFormat::Png),
        image::ImageFormat::Jpeg => Some(ImageFormat::Jpeg),
        image::ImageFormat::WebP => Some(ImageFormat::Webp),
        image::ImageFormat::Gif => Some(ImageFormat::Gif),
        image::ImageFormat::Bmp => Some(ImageFormat::Bmp),
        image::ImageFormat::Tiff => Some(ImageFormat::Tiff),
        image::ImageFormat::Ico => Some(ImageFormat::Ico),
        _ => None,
    }
}

pub(crate) fn play_path(
    path: &Path,
    format_hint: &str,
    start: Duration,
    volume: f32,
    cx: &mut App,
) -> anyhow::Result<PlaybackHandle> {
    let mut decoder = open_decoder(path, format_hint)?;
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
    use super::{
        extract_cover, is_supported_audio_extension, nonempty_tag, read_audio, read_with_lofty,
    };
    use lofty::{
        config::WriteOptions,
        file::{AudioFile, TaggedFileExt},
        picture::{MimeType, Picture, PictureType},
        prelude::Accessor,
        tag::{Tag, TagType},
    };
    use std::{borrow::Cow, fs, io::Write, path::Path};

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

    #[test]
    fn nonempty_tag_trims_and_rejects_blank() {
        assert_eq!(
            nonempty_tag(Some(Cow::Borrowed("  Title  "))).as_deref(),
            Some("Title")
        );
        assert_eq!(nonempty_tag(Some(Cow::Borrowed("   "))), None);
        assert_eq!(nonempty_tag(None), None);
    }

    #[test]
    fn read_audio_streams_tagged_wav_without_loading_all_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("track.wav");
        write_silent_wav(&path, 8000, 256);
        write_wav_tags(&path, "Preview Title", "Preview Artist", "Preview Album");

        let info = read_audio(&path, "wav").unwrap();
        assert_eq!(info.metadata.title.as_deref(), Some("Preview Title"));
        assert_eq!(info.metadata.artist.as_deref(), Some("Preview Artist"));
        assert_eq!(info.metadata.album.as_deref(), Some("Preview Album"));
        assert_eq!(info.metadata.sample_rate, 8000);
        assert_eq!(info.metadata.channels, 1);
        assert_eq!(info.metadata.bit_depth, Some(16));
        assert!(info.metadata.file_size > 0);
    }

    #[test]
    fn extract_cover_prefers_front_cover() {
        let mut tag = Tag::new(TagType::Id3v2);
        tag.push_picture(
            Picture::unchecked(b"other".to_vec())
                .pic_type(PictureType::Other)
                .mime_type(MimeType::Png)
                .build(),
        );
        tag.push_picture(
            Picture::unchecked(TINY_PNG.to_vec())
                .pic_type(PictureType::CoverFront)
                .mime_type(MimeType::Png)
                .build(),
        );
        let cover = extract_cover([&tag]);
        assert!(cover.is_some());
    }

    #[test]
    fn lofty_read_falls_back_when_tags_are_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.wav");
        write_silent_wav(&path, 16000, 64);
        let info = read_with_lofty(&path, "wav", fs::metadata(&path).unwrap().len()).unwrap();
        assert_eq!(info.metadata.sample_rate, 16000);
        assert_eq!(info.metadata.channels, 1);
        assert_eq!(info.metadata.title, None);
        assert!(info.cover.is_none());
    }

    fn write_silent_wav(path: &Path, sample_rate: u32, sample_count: u32) {
        let data_size = sample_count * 2;
        let file_size = 36 + data_size;
        let mut bytes = Vec::with_capacity((44 + data_size) as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&file_size.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_size.to_le_bytes());
        bytes.extend(std::iter::repeat_n(0u8, data_size as usize));
        let mut file = fs::File::create(path).unwrap();
        file.write_all(&bytes).unwrap();
    }

    fn write_wav_tags(path: &Path, title: &str, artist: &str, album: &str) {
        let mut tagged = lofty::read_from_path(path).unwrap();
        let mut tag = tagged
            .primary_tag()
            .cloned()
            .unwrap_or_else(|| Tag::new(TagType::RiffInfo));
        tag.set_title(title.to_string());
        tag.set_artist(artist.to_string());
        tag.set_album(album.to_string());
        tagged.insert_tag(tag);
        tagged.save_to_path(path, WriteOptions::default()).unwrap();
    }

    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
}
