use std::time::Duration;

use crate::Rotation;

/// Desktop metadata about a video file, gathered when it is opened.
#[derive(Clone, Debug)]
pub struct VideoMetadata {
    /// Total duration, when the container reports one.
    pub duration: Option<Duration>,
    /// Width of the video track in display orientation, in pixels.
    pub width: u32,
    /// Height of the video track in display orientation, in pixels.
    pub height: u32,
    /// Rotation applied to decoded frames for display.
    pub rotation: Rotation,
    /// Average frame rate, when the container reports one.
    pub frame_rate: Option<f64>,
    /// Codec of the video track, e.g. `H.264`.
    pub video_codec: String,
    /// Codec of the audio track, when one is present.
    pub audio_codec: Option<String>,
    /// Whether the file contains an audio track.
    pub has_audio: bool,
    /// Size of the file on disk in bytes.
    pub file_size: u64,
    /// Short label derived from the file extension.
    pub format_label: &'static str,
}

impl VideoMetadata {
    /// Format the resolution as `1920 × 1080`.
    pub fn resolution_label(&self) -> String {
        format!("{} × {}", self.width, self.height)
    }
}
