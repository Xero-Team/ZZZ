#[cfg(target_os = "macos")]
use futures::executor::block_on;

#[cfg(target_os = "macos")]
mod mac;

#[cfg(target_os = "windows")]
mod win;

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub(crate) mod linux;

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct Window {
    pub id: u32,
    pub title: String,

    #[cfg(target_os = "windows")]
    pub raw_handle: windows::Win32::Foundation::HWND,

    #[cfg(target_os = "macos")]
    pub raw_handle: cidre::cg::WindowId,

    #[cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "x11"))]
    pub raw_handle: xcb::x::Window,
}

#[derive(Debug, Clone)]
pub struct Display {
    pub id: u32,
    pub title: String,

    #[cfg(target_os = "windows")]
    pub raw_handle: windows::Win32::Graphics::Gdi::HMONITOR,

    #[cfg(target_os = "macos")]
    pub raw_handle: cidre::cg::DirectDisplayId,

    #[cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "x11"))]
    pub raw_handle: xcb::x::Window,
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "windows"))]
    pub width: u16,
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "windows"))]
    pub height: u16,
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    pub x_offset: i16,
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    pub y_offset: i16,
}

#[derive(Debug, Clone)]
pub enum Target {
    Window(Window),
    Display(Display),
}

// SAFETY: HWND and HMONITOR are opaque copyable handles; moving these wrappers does not transfer
// ownership of the underlying Windows objects.
#[cfg(target_os = "windows")]
unsafe impl Send for Display {}
#[cfg(target_os = "windows")]
unsafe impl Sync for Display {}
#[cfg(target_os = "windows")]
unsafe impl Send for Window {}
#[cfg(target_os = "windows")]
unsafe impl Sync for Window {}

/// Returns a list of targets that can be captured
pub fn get_all_targets() -> Result<Vec<Target>> {
    #[cfg(target_os = "macos")]
    return Ok(mac::get_all_targets());

    #[cfg(target_os = "windows")]
    return win::get_all_targets();

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    return linux::get_all_targets();
}

#[cfg(target_os = "macos")]
pub fn get_scale_factor(target: &Target) -> f64 {
    mac::get_scale_factor(target)
}

pub fn get_main_display() -> Result<Display> {
    #[cfg(target_os = "macos")]
    return Ok(mac::get_main_display());

    #[cfg(target_os = "windows")]
    return win::get_main_display();

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    return linux::get_main_display();
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub fn get_target_dimensions(target: &Target) -> (u64, u64) {
    #[cfg(target_os = "macos")]
    return mac::get_target_dimensions(target);

    #[cfg(target_os = "windows")]
    return win::get_target_dimensions(target);
}
