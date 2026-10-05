//! Adapted from gpui-ce `c6b17e616a35271183ab49f0da1890ee81953a99`
//! `crates/gpui_wgpu/src/wgpu_renderer/surface_tests.rs` (Apache-2.0).

use std::{ffi::c_void, ptr::NonNull, rc::Rc};

use gpui::{DevicePixels, Scene, Size};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_compositor, wl_registry, wl_surface},
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

use super::{WgpuRenderer, WgpuSurfaceConfig};

#[derive(Default)]
struct WindowState {
    configured: bool,
}

delegate_noop!(WindowState: ignore wl_compositor::WlCompositor);
delegate_noop!(WindowState: ignore wl_surface::WlSurface);
delegate_noop!(WindowState: ignore xdg_toplevel::XdgToplevel);

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WindowState {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<xdg_wm_base::XdgWmBase, ()> for WindowState {
    fn event(
        _: &mut Self,
        shell: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            shell.pong(serial);
        }
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for WindowState {
    fn event(
        state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            state.configured = true;
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TestWindowHandle {
    window: NonNull<c_void>,
    display: NonNull<c_void>,
}

unsafe impl Send for TestWindowHandle {}
unsafe impl Sync for TestWindowHandle {}

impl HasWindowHandle for TestWindowHandle {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let handle = raw_window_handle::WaylandWindowHandle::new(self.window);
        Ok(unsafe {
            raw_window_handle::WindowHandle::borrow_raw(
                raw_window_handle::RawWindowHandle::Wayland(handle),
            )
        })
    }
}

impl HasDisplayHandle for TestWindowHandle {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        let handle = raw_window_handle::WaylandDisplayHandle::new(self.display);
        Ok(unsafe {
            raw_window_handle::DisplayHandle::borrow_raw(
                raw_window_handle::RawDisplayHandle::Wayland(handle),
            )
        })
    }
}

#[test]
#[ignore = "requires a Wayland compositor and Vulkan; opens a temporary test window"]
fn failed_frames_release_surface_images() -> anyhow::Result<()> {
    let connection = Connection::connect_to_env()?;
    let (globals, mut events) = registry_queue_init::<WindowState>(&connection)?;
    let queue = events.handle();
    let compositor: wl_compositor::WlCompositor = globals.bind(&queue, 1..=4, ())?;
    let shell: xdg_wm_base::XdgWmBase = globals.bind(&queue, 1..=1, ())?;
    let surface = compositor.create_surface(&queue, ());
    let shell_surface = shell.get_xdg_surface(&surface, &queue, ());
    let toplevel = shell_surface.get_toplevel(&queue, ());
    toplevel.set_title("GPUI surface recovery test".into());
    surface.commit();

    let mut state = WindowState::default();
    events.roundtrip(&mut state)?;
    anyhow::ensure!(
        state.configured,
        "compositor did not configure the test window"
    );
    connection.flush()?;

    let window = TestWindowHandle {
        window: NonNull::new(surface.id().as_ptr().cast())
            .ok_or_else(|| anyhow::anyhow!("Wayland surface returned a null native handle"))?,
        display: NonNull::new(connection.backend().display_ptr().cast())
            .ok_or_else(|| anyhow::anyhow!("Wayland connection returned a null display handle"))?,
    };
    let context = Rc::new(std::cell::RefCell::new(None));
    let mut renderer = WgpuRenderer::new(
        context,
        &window,
        WgpuSurfaceConfig {
            size: Size {
                width: DevicePixels(128),
                height: DevicePixels(128),
            },
            transparent: false,
            preferred_present_mode: None,
        },
        None,
    )?;
    let mut scene = Scene::default();
    scene.finish();
    anyhow::ensure!(renderer.draw(&scene), "initial frame did not present");
    events.roundtrip(&mut state)?;

    for attempt in 0..8 {
        renderer.fail_after_surface_acquire = true;
        anyhow::ensure!(
            !renderer.draw(&scene),
            "injected frame {attempt} unexpectedly presented"
        );
        anyhow::ensure!(
            !renderer.fail_after_surface_acquire,
            "frame {attempt} did not acquire an image; earlier images were not released"
        );
        events.roundtrip(&mut state)?;
    }
    anyhow::ensure!(
        renderer.draw(&scene),
        "healthy frame did not present after repeated failures"
    );

    drop(renderer);
    toplevel.destroy();
    shell_surface.destroy();
    surface.destroy();
    connection.flush()?;
    Ok(())
}
