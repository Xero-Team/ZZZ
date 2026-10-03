use std::cell::RefCell;
use std::rc::Rc;

use crate::WgpuContext;
use gpui::{DevicePixels, Size};

pub struct WgpuSurfaceConfig {
    pub size: Size<DevicePixels>,
    pub transparent: bool,
    /// Preferred presentation mode. When `Some`, the renderer will use this
    /// mode if supported by the surface, falling back to `Fifo`.
    /// When `None`, defaults to `Fifo` (VSync).
    ///
    /// Mobile platforms may prefer `Mailbox` (triple-buffering) to avoid
    /// blocking in `get_current_texture()` during lifecycle transitions.
    pub preferred_present_mode: Option<wgpu::PresentMode>,
}

/// Shared GPU context reference, used to coordinate device recovery across multiple windows.
pub type GpuContext = Rc<RefCell<Option<WgpuContext>>>;
