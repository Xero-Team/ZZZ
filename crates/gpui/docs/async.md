# Async execution

GPUI integrates an async executor with the platform event loop. Background
work runs off the main thread; foreground tasks and async contexts tie back to
the `App` without blocking the UI.

## Core types

| Type                 | Defined in                                | Role                          |
| -------------------- | ----------------------------------------- | ----------------------------- |
| `BackgroundExecutor` | `crates/gpui/src/executor.rs:17`          | Spawns work on a thread pool  |
| `ForegroundExecutor` | `crates/gpui/src/executor.rs:25`          | Runs on the main thread       |
| `Task<T>`            | `crates/gpui/src/executor.rs:39`          | A spawned, cancellable future |
| `AsyncApp`           | `crates/gpui/src/app/async_context.rs:22` | App access across `await`     |
| `AsyncWindowContext` | `crates/gpui/src/app/async_context.rs`    | Window-scoped async access    |

`App::spawn` (`crates/gpui/src/app.rs:1939`) creates a `Task<R>` from an async
closure that receives an `AsyncApp`. The executor implementation is in
`crates/gpui/src/executor.rs`; the app-level spawn helpers are in
`crates/gpui/src/app.rs`.

## From a synchronous context to async

```text
&App / &Context<T>
    -> .to_async() / App::spawn (app.rs:1939)
    -> AsyncApp (async_context.rs:22)
    -> await background work (BackgroundExecutor)
    -> AsyncApp::update(..) to re-enter the app
```

An async context has a `'static` lifetime, so it survives `await` points.
Because the app or window may be gone when the future resumes, calls back into
it are fallible: `AsyncApp::update` returns a `Result` rather than a plain
value.

## Task lifetime rules

- A dropped `Task` is cancelled. If you spawn with `cx.spawn` or
  `cx.background_spawn`, either await it, store it, or detach it explicitly.
- Use `BackgroundExecutor` for CPU or I/O work; use `ForegroundExecutor`
  (or `App::spawn`) when the work must run on the main thread.
- For timers in tests, use the executor's timer APIs rather than
  `smol::Timer::after`, so the test dispatcher controls time.

## Known limits

- `Entity<T>` is not `Send`. Carry `EntityId` or an `AsyncApp` across `await`,
  then re-enter the app to read the entity.
- Async state access is fallible. Do not assume the window still exists after
  an await; handle the `Err` case.

## See also

- [Contexts](./contexts.md) for `App` / `Context<T>` and `to_async`
- [Entities](./entity.md) for what can cross an await boundary
- [Testing](./test.md) for deterministic executor timers
