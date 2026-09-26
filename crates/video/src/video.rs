//! Cross-platform video decoding for ZZZ.
//!
//! On Linux and Windows the decoder is backed by FFmpeg, on macOS by
//! AVFoundation. Both backends decode into the same CPU [`VideoFrame`] type so
//! the viewer can render through GPUI's cross-platform image path.

mod backend;
mod frame;
mod metadata;
mod rotation;

pub use backend::VideoDecoder;
pub use frame::VideoFrame;
pub use metadata::VideoMetadata;
pub use rotation::Rotation;

/// File extensions that the video viewer will attempt to open.
pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mov", "mkv", "webm", "avi", "wmv", "mpg", "mpeg", "hevc", "265",
];

/// Returns whether `extension` names a video container this build can attempt.
pub fn is_supported_video_extension(extension: &str) -> bool {
    SUPPORTED_EXTENSIONS
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

/// A short human readable label for a video container extension.
pub fn format_label(extension: &str) -> &'static str {
    match extension.to_ascii_lowercase().as_str() {
        "mp4" => "MP4",
        "m4v" => "M4V",
        "mov" => "MOV",
        "mkv" => "MKV",
        "webm" => "WebM",
        "avi" => "AVI",
        "wmv" => "WMV",
        "mpg" | "mpeg" => "MPEG",
        "hevc" | "265" => "HEVC",
        _ => "Video",
    }
}
