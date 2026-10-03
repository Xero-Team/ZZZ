//! Convenience crate that re-exports GPUI's platform traits and the
//! `current_platform` constructor so consumers don't need `#[cfg]` gating.

pub use gpui::Platform;

use std::rc::Rc;

/// Returns a background executor for the current platform.
pub fn background_executor() -> gpui::BackgroundExecutor {
    current_platform(true).background_executor()
}

pub fn application() -> gpui::Application {
    gpui::Application::with_platform(current_platform(false))
}

pub fn headless() -> gpui::Application {
    gpui::Application::with_platform(current_platform(true))
}

/// Unlike `application`, this function returns a single-threaded web application.
#[cfg(target_family = "wasm")]
pub fn single_threaded_web() -> gpui::Application {
    gpui::Application::with_platform(Rc::new(gpui_web::WebPlatform::new(false)))
}

/// Initializes panic hooks and logging for the web platform.
/// Call this before running the application in a wasm_bindgen entrypoint.
#[cfg(target_family = "wasm")]
pub fn web_init() {
    console_error_panic_hook::set_once();
    gpui_web::init_logging();
}

/// Returns the default [`Platform`] for the current OS.
#[cfg(not(target_family = "wasm"))]
pub fn current_platform(headless: bool) -> Rc<dyn Platform> {
    #[cfg(target_os = "macos")]
    {
        Rc::new(gpui_macos::MacPlatform::new(headless))
    }

    #[cfg(target_os = "windows")]
    {
        Rc::new(
            gpui_windows::WindowsPlatform::new(headless)
                .expect("failed to initialize Windows platform"),
        )
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        gpui_linux::current_platform(headless)
    }
}

/// Returns the multithreaded web [`Platform`].
#[cfg(target_family = "wasm")]
pub fn current_platform(_headless: bool) -> Rc<dyn Platform> {
    Rc::new(gpui_web::WebPlatform::new(true))
}

/// Returns a new [`HeadlessRenderer`] for the current platform, if available.
#[cfg(feature = "test-support")]
pub fn current_headless_renderer() -> Option<Box<dyn gpui::PlatformHeadlessRenderer>> {
    #[cfg(target_os = "macos")]
    {
        Some(Box::new(
            gpui_macos::metal_renderer::MetalHeadlessRenderer::new(),
        ))
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "windows"))]
    {
        gpui_wgpu::WgpuHeadlessRenderer::new()
            .map(|renderer| Box::new(renderer) as Box<dyn gpui::PlatformHeadlessRenderer>)
            .map_err(|error| log::error!("failed to initialize WGPU headless renderer: {error}"))
            .ok()
    }

    #[cfg(target_family = "wasm")]
    {
        None
    }
}

#[cfg(all(
    test,
    feature = "test-support",
    not(target_os = "macos"),
    not(target_family = "wasm")
))]
mod headless_tests {
    use super::*;
    use gpui::{DevicePixels, Scene, Size};

    #[test]
    fn current_platform_provides_real_headless_renderer() {
        let mut renderer = current_headless_renderer().expect("headless renderer should exist");
        let mut scene = Scene::default();
        scene.finish();
        let image = renderer
            .render_scene_to_image(
                &scene,
                Size {
                    width: DevicePixels(16),
                    height: DevicePixels(16),
                },
            )
            .expect("empty scene should render");
        assert_eq!(image.dimensions(), (16, 16));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use gpui::{AppContext, Empty, VisualTestAppContext};
    use std::cell::RefCell;
    use std::time::Duration;

    // These tests require the macOS main thread. Standard Rust tests run on worker threads, which
    // abort when they interact with AppKit or Cocoa.
    // Run them with: cargo test -p gpui_platform -- --ignored --test-threads=1

    #[test]
    #[ignore = "Requires macOS main thread"]
    fn test_foreground_tasks_run_with_run_until_parked() {
        let mut cx = VisualTestAppContext::new(current_platform(false));

        let task_ran = Rc::new(RefCell::new(false));

        {
            let task_ran = task_ran.clone();
            cx.update(|cx| {
                cx.spawn(async move |_| {
                    *task_ran.borrow_mut() = true;
                })
                .detach();
            });
        }

        assert!(!*task_ran.borrow());

        cx.run_until_parked();

        assert!(*task_ran.borrow());
    }

    #[test]
    #[ignore = "Requires macOS main thread"]
    fn test_advance_clock_triggers_delayed_tasks() {
        let mut cx = VisualTestAppContext::new(current_platform(false));

        let task_ran = Rc::new(RefCell::new(false));

        {
            let task_ran = task_ran.clone();
            let executor = cx.background_executor.clone();
            cx.update(|cx| {
                cx.spawn(async move |_| {
                    executor.timer(Duration::from_millis(500)).await;
                    *task_ran.borrow_mut() = true;
                })
                .detach();
            });
        }

        cx.run_until_parked();
        assert!(!*task_ran.borrow());

        cx.advance_clock(Duration::from_millis(600));

        assert!(*task_ran.borrow());
    }

    #[test]
    #[ignore = "Requires macOS main thread - window creation fails on test threads"]
    fn test_window_spawn_uses_test_dispatcher() {
        let mut cx = VisualTestAppContext::new(current_platform(false));

        let task_ran = Rc::new(RefCell::new(false));

        let window = cx
            .open_offscreen_window_default(|_, cx| cx.new(|_| Empty))
            .expect("Failed to open window");

        {
            let task_ran = task_ran.clone();
            cx.update_window(window.into(), |_, window, cx| {
                window
                    .spawn(cx, async move |_| {
                        *task_ran.borrow_mut() = true;
                    })
                    .detach();
            })
            .expect("offscreen window should remain open");
        }

        assert!(!*task_ran.borrow());

        cx.run_until_parked();

        assert!(*task_ran.borrow());
    }
}
