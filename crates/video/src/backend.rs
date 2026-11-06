#[cfg(any(target_os = "linux", target_os = "windows", target_os = "freebsd"))]
mod ffmpeg;

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "freebsd"))]
pub use ffmpeg::FfmpegDecoder as VideoDecoder;

#[cfg(target_os = "macos")]
mod avfoundation;

#[cfg(target_os = "macos")]
pub use avfoundation::AvFoundationDecoder as VideoDecoder;

#[cfg(not(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "macos",
    target_os = "freebsd"
)))]
compile_error!("the `video` crate supports Linux, Windows, macOS, and FreeBSD only");
