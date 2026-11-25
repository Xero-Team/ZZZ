#![cfg(any(target_os = "macos", target_os = "ios"))]
#![expect(
    clippy::multiple_unsafe_ops_per_block,
    reason = "Metal and dispatch FFI call sequences are inherently multi-op"
)]
//! Shared Apple rendering, task dispatch, and system services for GPUI.
//!
//! This crate renders GPUI scenes directly with Metal on Apple platforms. It
//! owns GPU resources and text rendering while leaving application lifecycle,
//! windowing, and input to each platform backend.

mod dispatcher;
pub mod display_link;
mod metal_atlas;
pub mod metal_renderer;

#[cfg(feature = "font-kit")]
mod open_type;
#[cfg(feature = "font-kit")]
mod text_system;

use objc2::MainThreadMarker;

pub use dispatcher::AppleDispatcher;
pub use display_link::MetalFrameSource;
#[cfg(feature = "font-kit")]
pub use text_system::AppleTextSystem;

/// Returns a marker for the main thread, or `None` on any other thread.
///
/// The marker is created once per thread so callers get a cheap, stable answer
/// instead of paying for `NSThread` each time.
#[inline]
pub fn main_thread_marker() -> Option<MainThreadMarker> {
    thread_local! {
        static MAIN_THREAD_MARKER: Option<MainThreadMarker> = MainThreadMarker::new();
    }
    MAIN_THREAD_MARKER.with(|marker| *marker)
}

#[cfg(test)]
mod tests {
    use super::main_thread_marker;

    #[test]
    fn main_thread_marker_rejects_background_threads() {
        std::thread::spawn(|| {
            assert_eq!(main_thread_marker(), None);
            assert_eq!(main_thread_marker(), None);
        })
        .join()
        .expect("background thread panicked");
    }
}
