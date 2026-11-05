# Focus and key dispatch

Focus decides which element receives keyboard input. GPUI tracks a focused
handle per window and walks the element tree to dispatch actions bound in key
contexts.

## Core types

| Type            | Defined in                             | Role                              |
| --------------- | -------------------------------------- | --------------------------------- |
| `FocusHandle`   | `crates/gpui/src/window.rs:301`        | Handle for one focusable element  |
| `FocusId`       | `crates/gpui/src/window.rs:181`        | Stable identity of a focus target |
| `DispatchPhase` | `crates/gpui/src/window.rs:124`        | Capture / Bubble dispatch phase   |
| `KeyContext`    | `crates/gpui/src/keymap/context.rs:10` | Named key-binding scope           |
| `Focusable`     | `crates/gpui/src/window.rs`            | Trait providing `focus_handle`    |

`App::focus_handle` (`crates/gpui/src/app.rs:2606`) creates handles. The focus
and dispatch machinery lives in `crates/gpui/src/key_dispatch.rs` and
`crates/gpui/src/window.rs`.

## Dispatch call chain

```text
platform key event
    -> Window::dispatch_keystroke / handle_input (window/methods_input.rs)
    -> KeyDispatcher resolves the active KeyContext stack
    -> action matched against the keymap
    -> dispatch through DispatchPhase::Capture then DispatchPhase::Bubble
    -> on_action handler on the focused element or an ancestor
```

`key_context` declares a scope on part of the tree; `on_action` binds a
handler. Actions are user-defined structs converted from keystrokes. See
[Key Dispatch](./key_dispatch.md) for the binding syntax and keymap format.

## State during dispatch

- The focused element tree is walked from root to leaf in `Capture`, then back
  up in `Bubble`. A handler can call `cx.stop_propagation()` to stop the
  event reaching lower z-index / ancestor handlers.
- `Window::with_element_state` (`window/methods_paint.rs:279`) is unrelated to
  focus; it keys arbitrary element state by `ElementId`.

## Menu bar and key-binding order

`App::set_menus` (`crates/gpui/src/app.rs:2410`) passes the current keymap to
the platform when you call it. The macOS backend
(`crates/gpui_macos/src/platform.rs:1076`) bakes each item's shortcut into the
native menu at that moment. Call `App::bind_keys` (`app.rs:2226`) **before**
`set_menus`; a binding added afterwards does not appear next to its menu item.
When the keymap changes later (a user keymap file or a menu rebuild), call
`set_menus` again.

## Known limits

- An element without a stable `ElementId` cannot be focused reliably.
- Key bindings are resolved against the current `KeyContext` stack. An action
  bound in an inactive context is not dispatched.
- The menu shortcut baking happens in platform code and is not exercised by
  host-side unit tests.

## See also

- [Key Dispatch](./key_dispatch.md) for actions and keymap JSON
- [Elements](./element.md) for `ElementId` and hitboxes
- [Contexts](./contexts.md) for `Window` access
