# Rendering and the frame lifecycle

GPUI renders on demand. A view requests a frame, GPUI runs a prepaint/layout/
paint pass over the element tree, and the resulting `Scene` is submitted to the
platform. Nothing is drawn between frames.

## Core types

| Type            | Defined in                         | Role                                    |
| --------------- | ---------------------------------- | --------------------------------------- |
| `WindowOptions` | `crates/gpui/src/platform.rs:1632` | Configuration for a new window          |
| `WindowBounds`  | `crates/gpui/src/platform.rs:1789` | Initial size/position state             |
| `Window`        | `crates/gpui/src/window.rs`        | Per-window state and frame pipeline     |
| `Scene`         | `crates/gpui/src/scene.rs:41`      | Collected draw primitives for one frame |
| `BuiltFrame`    | `crates/gpui/src/frame.rs:776`     | Crate-private assembled frame           |
| `DispatchPhase` | `crates/gpui/src/window.rs:124`    | Event dispatch phase within a frame     |

`Window` is constructed by `Window::new` in
`crates/gpui/src/window/methods.rs`. It owns the root view, the frame
scheduler, hitboxes, and the current `Scene`.

## How a frame is requested and produced

```text
view calls cx.notify / window.refresh / app.refresh_windows
    -> frame scheduler marks the window dirty
    -> platform on_request_frame callback fires
    -> Window::draw drives the element tree
        -> Render::render builds a fresh tree
        -> prepaint / layout / paint (element.rs)
        -> Scene collects quads, glyphs, images
    -> Frame::finish assembles BuiltFrame (frame.rs:776)
    -> PlatformRenderTarget presents it
```

`App::refresh_windows` (`crates/gpui/src/app.rs:1091`) and
`Window::refresh` request a redraw. `Window::defer` (`app.rs:1974`) schedules
work to run after the current frame. The three element phases are described in
[Elements](./element.md).

## Window creation

`App::open_window` (`crates/gpui/src/app.rs:1274`) takes `WindowOptions` and a
closure that builds the root view. It returns a `WindowHandle`, which you use
to update the window later:

```rust
let handle = cx.open_window(WindowOptions::default(), |_window, cx| {
    cx.new(|_cx| MyView::new())
});
```

## Known limits

- Rendering is not continuous. A view that changes state must call `notify`
  (or an equivalent) or the screen will not update.
- `BuiltFrame` and most of `frame.rs` are crate-private; they are not part of
  the stable API.
- Frame timing and diagnostics are feature-gated. The `frame-diagnostics`
  feature instruments the pipeline for profiling.

## See also

- [Elements](./element.md) for the three element phases
- [Entities](./entity.md) for state ownership
- [Platform](./platform.md) for the render target and capabilities
