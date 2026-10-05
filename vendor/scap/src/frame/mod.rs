#[cfg(any(target_os = "macos", target_os = "windows"))]
mod audio;
mod video;

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use audio::*;
pub use video::*;

pub enum Frame {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    Audio(AudioFrame),
    Video(VideoFrame),
}
