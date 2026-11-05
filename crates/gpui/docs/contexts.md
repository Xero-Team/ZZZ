# Contexts

GPUI makes extensive use of _context parameters_ (typically named `cx`) to provide access to application state and services. These contexts are references passed to functions, enabling interaction with global state, windows, entities, and system services.

---

## `App`

The root context granting access to the application's global state. This context owns all entities' data and can be used to read or update the data referenced by an `Entity<T>`.

## `Context<T>`

A context provided when interacting with an `Entity<T>`, with additional methods related to that specific entity such as notifying observers and emitting events. This context dereferences into `App`, meaning any function which can take an `App` reference can also take a `Context<T>` reference, allowing you to access the application's global state.

## `AsyncApp` and `AsyncWindowContext`

Whereas the above contexts are always passed to your code as references, you can call `to_async` on the reference to create an async context, which has a static lifetime and can be held across `await` points in async code. When you interact with entities with an async context, the calls become fallible, because the context may outlive the window or even the app itself.

## `TestAppContext`

These are similar to the async contexts above, but they panic if you attempt to access a non-existent app or window, and they also contain other features specific to tests.

---

# Non-Context Core Types

## `Window`

Provides access to the state of an application window. This type has a root view (an `Entity` implementing `Render`) which it can read/update, but since it is not a context, you must pass a `&mut App` (or a context which dereferences to it) to do so, along with other functions interacting with global state. You can obtain a `Window` from an `WindowHandle` by calling `WindowHandle::update`.

## `Entity<T>`

A handle to a structure requiring state. This data is owned by the `App` and can be accessed and modified via references to contexts. If `T` implements `Render`, then the entity is sometimes referred to as a view. Entities can be observed by other entities and windows, allowing a closure to be called when `notify` is called on the entity's `Context`.

---

# Where the types live

| Type             | Defined in                                |
| ---------------- | ----------------------------------------- |
| `App`            | `crates/gpui/src/app.rs`                  |
| `AppContext`     | `crates/gpui/src/gpui.rs:185`             |
| `VisualContext`  | `crates/gpui/src/gpui.rs:274`             |
| `Context<'a, T>` | `crates/gpui/src/app/context.rs:20`       |
| `AsyncApp`       | `crates/gpui/src/app/async_context.rs:22` |
| `TestAppContext` | `crates/gpui/src/app/test_context.rs:18`  |
| `Entity<T>`      | `crates/gpui/src/app/entity_map.rs:504`   |
| `Window`         | `crates/gpui/src/window.rs`               |

## Ownership flow

```text
Application::run provides &mut App
    -> App owns the entity map, windows, and platform
    -> Context<T> derefs to App (context.rs)
    -> AsyncApp / AsyncWindowContext carry access across await
    -> TestAppContext substitutes a deterministic backend in tests
```

`Context<T>` is created for an `Entity<T>` and adds entity-scoped methods such
as `notify` and `observe`. `VisualContext` extends `AppContext` with window
and view operations. Because `Context<T>` dereferences to `App`, a function
taking `&App` also accepts `&Context<T>`.

## Known limits

- Context references are not `'static`. Convert to an async context with
  `to_async` before holding state across `await` points.
- Async and test contexts differ in failure behavior: `AsyncApp` returns a
  `Result` when the app or window is gone, while `TestAppContext` panics.
- `Window` is not a context. Pass `&mut App` (or a context that derefs to it)
  alongside it when you need global state.

## See also

- [Entities](./entity.md) for entity ownership and observation
- [Async contexts](./async.md) for access across `await`
- [Testing](./test.md) for `TestAppContext`
