# Platform abstraction

GPUI separates the framework from the operating system through a set of
traits in `crates/gpui/src/platform.rs`. Each backend implements the traits it
supports; the rest either return an error or expose a capability flag.

## Core types

| Type                       | Defined in                        | Role                                       |
| -------------------------- | --------------------------------- | ------------------------------------------ |
| `Platform`                 | `crates/gpui/src/platform.rs`     | Top-level backend (extends `AppLifecycle`) |
| `AppLifecycle`             | `crates/gpui/src/platform.rs:125` | Lifecycle: quit, restart, activate, hide   |
| `PlatformWindow`           | `crates/gpui/src/platform.rs:954` | Composite window trait                     |
| `WindowHost`               | `crates/gpui/src/platform.rs:863` | Window frame, bounds, minimize, zoom       |
| `PlatformCapabilities`     | `crates/gpui/src/platform.rs:550` | Optional services matrix                   |
| `WindowControls`           | `crates/gpui/src/platform.rs:507` | Which window controls exist                |
| `AppLifecycleCapabilities` | `crates/gpui/src/platform.rs`     | Lifecycle support flags                    |

`PlatformWindow` is a composite of `WindowHost`, `AccessibilityBridge`,
`InputSource`, `PlatformRenderTarget`, `SystemServices`, and
`TextInputBridge`. Backends are selected per target: macOS
(`gpui_macos`), Windows (`gpui_windows`), Linux/FreeBSD (`gpui_linux`),
and WebAssembly (`gpui_web`).

## Unsupported operations are errors

Lifecycle and window-control methods return `Result<()>`:

```text
App::quit / App::hide / Window::minimize_window
    -> platform trait method
    -> backend either performs the operation and returns Ok(())
       or inherits the trait's "unsupported" error
```

Each `AppLifecycle` method and `WindowHost::minimize` / `zoom` has a default
implementation that returns an "unsupported" error. A backend overrides only
what it actually implements; an unimplemented operation is never reported as
a successful no-op. Callers that cannot recover log the error with
`.log_err()`; callers that can branch use `?` or `if let Err(..)`.

`PlatformCapabilities` and `AppLifecycleCapabilities` describe what the UI
should offer (for example, whether to show a minimize button). They are
presentation metadata, not a second source of truth about behavior. Query
them to decide what to render, not to guess whether a call succeeded.

## Backend evidence

The in-tree platform crates have target-specific implementations for macOS,
Windows, Linux/FreeBSD, and WebAssembly. Runtime coverage differs: the Web
backend cannot quit, restart, activate, hide, minimize, zoom, or open native
prompts, and it reports those calls as errors. The current capability matrix
and native validation runbooks are recorded under
`docs/src/development/gui-framework-research/`.

## Known limits

- A capability flag being `true` means the backend implements the operation,
  not that the OS always grants it (for example, window management under a
  compositor).
- The Web backend needs `RUSTC_BOOTSTRAP=1` to compile with the pinned stable
  toolchain because a transitive dependency uses a nightly-only feature.
  This is unrelated to the GPUI API.
- `cargo test -p gpui_web` does not run on a native host; the crate's
  dependencies are gated on `target_family = "wasm"`.

## See also

- [README](../README.md) for the target availability table
- [Testing](./test.md) for capability regression tests
- [Accessibility semantics](./accessibility.md) for feature-gated services
