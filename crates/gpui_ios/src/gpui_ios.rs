//! Native iOS platform support for GPUI.
//!
//! This crate connects GPUI to UIKit, CoreText, and Metal through the shared
//! Apple renderer. It owns iOS application lifecycle, windowing, native
//! text input, safe-area and keyboard insets, and raw touch delivery.

// UIKit callbacks are single-threaded Objective-C entry points: a message send,
// a retain, or a class lookup on the same thread cannot race, so grouping the
// FFI operations of one callback in a single block keeps it readable.
#![cfg_attr(
    target_os = "ios",
    expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "UIKit delegate entry points issue several independent FFI calls on the main thread"
    )
)]

#[cfg(target_os = "ios")]
pub mod ios;

#[cfg(target_os = "ios")]
pub use ios::{IosPlatform, current_platform};

/// The foreground color used by the iOS status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusBarContentStyle {
    /// Light status-bar content for dark backgrounds.
    Light,
    /// Dark status-bar content for light backgrounds.
    #[default]
    Dark,
}
