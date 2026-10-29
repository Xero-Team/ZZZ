use collections::HashMap;
use wayland_client::{Proxy, protocol::wl_surface};

#[derive(Debug, Hash, PartialEq, Eq)]
pub(crate) enum SerialKind {
    DataDevice,
    InputMethod,
    MouseEnter,
    MousePress,
    KeyPress,
}

#[derive(Debug)]
struct SerialData {
    serial: u32,
}

impl SerialData {
    fn new(value: u32) -> Self {
        Self { serial: value }
    }
}

/// The user interaction an `xdg_activation_v1` token request must reference.
#[derive(Clone, Debug)]
pub(crate) struct ActivationContext {
    pub(crate) serial: u32,
    pub(crate) requesting_surface: wl_surface::WlSurface,
}

#[derive(Debug)]
struct Press {
    serial: u32,
    focused_surface: Option<wl_surface::WlSurface>,
}

#[derive(Debug)]
/// Helper for tracking of different serial kinds.
pub(crate) struct SerialTracker {
    serials: HashMap<SerialKind, SerialData>,
    latest_press: Option<Press>,
}

impl SerialTracker {
    pub fn new() -> Self {
        Self {
            serials: HashMap::default(),
            latest_press: None,
        }
    }

    pub fn update(&mut self, kind: SerialKind, value: u32) {
        self.update_with_focused_surface(kind, value, None);
    }

    /// Like [`Self::update`], but for key and pointer presses also records the surface
    /// that had focus, which activation token requests must name alongside the serial.
    pub fn update_with_focused_surface(
        &mut self,
        kind: SerialKind,
        value: u32,
        focused_surface: Option<wl_surface::WlSurface>,
    ) {
        if matches!(&kind, SerialKind::KeyPress | SerialKind::MousePress) {
            self.latest_press = Some(Press {
                serial: value,
                focused_surface,
            });
        }

        self.serials.insert(kind, SerialData::new(value));
    }

    /// Returns the latest press if a still-alive surface had focus when it arrived.
    pub fn activation_context(&self) -> Option<ActivationContext> {
        let press = self.latest_press.as_ref()?;
        let surface = press
            .focused_surface
            .as_ref()
            .filter(|surface| surface.is_alive())?;
        Some(ActivationContext {
            serial: press.serial,
            requesting_surface: surface.clone(),
        })
    }

    /// Returns the latest tracked serial of the provided [`SerialKind`]
    ///
    /// Will return 0 if not tracked.
    pub fn get(&self, kind: SerialKind) -> u32 {
        self.serials
            .get(&kind)
            .map_or(0, |serial_data| serial_data.serial)
    }

    /// Returns the most recent serial across all tracked kinds.
    ///
    /// Wayland compositor serial numbers are monotonically increasing, so the
    /// highest value is always the most recently received one. This is the
    /// correct serial to use for [`set_selection`] when the triggering event
    /// may have been a mouse press rather than a key press: using 0 (the
    /// default when a kind has never been seen) causes compositors to silently
    /// reject the request.
    ///
    /// Returns 0 only if no serial of any kind has been received yet.
    pub fn get_latest(&self) -> u32 {
        self.serials
            .values()
            .map(|serial_data| serial_data.serial)
            .max()
            .unwrap_or(0)
    }
}
