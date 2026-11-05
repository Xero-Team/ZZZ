# Entities

An `Entity<T>` owns application state that other parts of the app need to
observe or update. GPUI stores the data inside the `App`; the handle you hold
is a reference-counted pointer, not the value itself. You access the value
only through a context (`&mut App` or `&mut Context<T>`).

## Core types and ownership

| Type             | Defined in                              | Role                                  |
| ---------------- | --------------------------------------- | ------------------------------------- |
| `Entity<T>`      | `crates/gpui/src/app/entity_map.rs:504` | Owned handle; keeps the data alive    |
| `WeakEntity<T>`  | `crates/gpui/src/app/entity_map.rs:830` | Non-owning handle; must be upgraded   |
| `EntityId`       | `crates/gpui/src/app/entity_map.rs:30`  | Stable identity of one entity         |
| `Context<'a, T>` | `crates/gpui/src/app/context.rs:20`     | Entity-scoped access; derefs to `App` |

The `App` owns the entity map (`app/entity_map.rs`). Dropping the last
`Entity<T>` releases the value; a `WeakEntity<T>` does not keep it alive and
must be upgraded before use.

## Creating and updating an entity

Create entities from an `App` with `App::new`, then read or mutate them through
`Entity::read` and `Entity::update`:

```rust
let counter = cx.new(|_cx| 0usize);
counter.update(cx, |value, _cx| *value += 1);
let current = counter.read(cx);
```

`App::new` and the entity access methods live in
`crates/gpui/src/app/entity_map.rs` and `crates/gpui/src/app.rs`.
`Context<T>` is passed to `Render::render`; because it dereferences to `App`,
any function taking `&App` also accepts `&Context<T>`.

## Observation call chain

```text
Entity::update(cx, ..) / Context::notify
    -> App::notify (app.rs:2611)
    -> observers registered by App::observe (app.rs:1114)
    -> App::refresh_windows (app.rs:1091)
    -> next frame calls Render::render on dirty views
```

`App::observe` (app.rs:1114) and `App::subscribe` (app.rs:1196) register
callbacks that run when an entity notifies or emits an event. Emission is
declared with `EventEmitter` in `crates/gpui/src/app/context.rs`.
Notifications are coalesced within a frame; they request a redraw rather than
rendering immediately.

## Known limits

- An entity is not `Send`. GPUI state lives on the thread that owns the `App`;
  use the async contexts (`AsyncApp`, `AsyncWindowContext`) to carry work
  across await points.
- `Entity<T>` gives you `&T` / `&mut T` only inside a context closure. You
  cannot hold a reference to the data past the closure.
- `WeakEntity::upgrade` is fallible. Always handle the case where the entity
  was dropped.

## See also

- [Contexts](./contexts.md) for the different context types
- [Async contexts](./async.md) for state access across `await`
- [Rendering](./render.md) for how a view becomes a frame
