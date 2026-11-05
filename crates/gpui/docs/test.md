# Testing GPUI code

`#[gpui::test]` builds a deterministic `App` for each test and injects a
`TestAppContext`. GPUI tests use executor-controlled timers instead of real
time, so they run without a real event loop.

## Core types

| Type             | Defined in                                       | Role                              |
| ---------------- | ------------------------------------------------ | --------------------------------- |
| `#[gpui::test]`  | `crates/gpui_macros/src/test.rs`                 | Test harness macro                |
| `TestAppContext` | `crates/gpui/src/app/test_context.rs:18`         | Test context with simulated input |
| `TestDispatcher` | `crates/gpui/src/platform/test/dispatcher.rs:17` | Deterministic task scheduler      |
| `TestPlatform`   | `crates/gpui/src/platform/test/platform.rs:21`   | Crate-private test backend        |

The macro is documented inline in `crates/gpui_macros/src/gpui_macros.rs:160`.
The test support module is `crates/gpui/src/test.rs`. `TestAppContext::quit`
(`app/test_context.rs:176`) ends a test by running its cleanup callbacks.

## A minimal test

```rust
#[gpui::test]
async fn my_view_renders(cx: &mut TestAppContext) {
    let _view = cx.new(|_cx| MyView::new());
    // Advance the deterministic executor; no real sleep is involved.
    cx.run_until_parked();
}
```

The macro supports both synchronous and async test functions. A
`TestAppContext` dereferences to the test app, so entity creation and updates
use the same APIs as production code.

## Determinism rules

- Use executor timers (`cx.background_executor().timer(..)`) rather than
  `smol::Timer::after`. Real timers are not controlled by `TestDispatcher` and
  make tests flaky.
- Dropped `cx.spawn` / `cx.background_spawn` tasks are cancelled. Await them,
  detach them, or store them; an orphaned future silently never runs.
- Capability tests live in `crates/gpui/src/platform.rs` `mod tests` and in
  the platform crates. They assert the advertised capability matrix matches
  the backend's actual behavior.

## About `doctest = false`

`crates/gpui/Cargo.toml:47` sets `doctest = false`. git history shows it was
added in 2022 by the repository-wide commit "Disable doctests for all
libraries" (`3a1a9ecb45`); it is not a GPUI-specific platform-link
constraint. The in-tree examples and docs are the API reference. This plan
does not re-enable doctests; doing so would need a separate decision covering
every library in the workspace.

## Known limits

- `TestAppContext` panics on access to a non-existent app or window (see
  [Contexts](./contexts.md)). `AsyncApp` returns a `Result` instead.
- Platform-specific rendering (Metal, WGPU, WebGPU) is not exercised by unit
  tests. Use the native runbooks under
  `docs/src/development/gui-framework-research/` for runtime validation.

## See also

- [Contexts](./contexts.md) for `TestAppContext` vs `AsyncApp`
- [Async contexts](./async.md) for task lifetime
- [Platform](./platform.md) for capability matrices
