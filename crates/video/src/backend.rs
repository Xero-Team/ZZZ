#[cfg(any(target_os = "linux", target_os = "windows"))]
mod ffmpeg;

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub use ffmpeg::FfmpegDecoder as VideoDecoder;

#[cfg(target_os = "macos")]
mod avfoundation;

#[cfg(target_os = "macos")]
pub use avfoundation::AvFoundationDecoder as VideoDecoder;

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
compile_error!("the `video` crate supports Linux, Windows, and macOS only");
