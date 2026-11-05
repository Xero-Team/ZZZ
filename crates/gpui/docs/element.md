# Elements

Elements are the building blocks of a render tree. A `Render` implementation
returns a tree of `IntoElement` values; GPUI lays them out and paints them each
frame. Elements range from the general-purpose `div` to custom imperative
implementations of the `Element` trait.

## Core types

| Type          | Defined in                               | Role                                    |
| ------------- | ---------------------------------------- | --------------------------------------- |
| `Element`     | `crates/gpui/src/element.rs:50`          | Per-frame imperative draw/layout/paint  |
| `IntoElement` | `crates/gpui/src/element.rs:126`         | Converts a builder into an element node |
| `Render`      | `crates/gpui/src/element.rs:144`         | Implemented by a view's root entity     |
| `RenderOnce`  | `crates/gpui/src/element.rs:160`         | Stateless, consumed in one render       |
| `ElementId`   | `crates/gpui/src/window/element_id.rs:8` | Stable identity across frames           |
| `div`         | `crates/gpui/src/elements/div.rs`        | General-purpose styled container        |

`div` is the swiss-army-knife element: it composes styling (`Styled`),
interaction (`InteractiveElement`, `StatefulInteractiveElement`), children
(`ParentElement`), and layout through a tailwind-style API. Prefer it until a
custom `Element` is needed for control over layout or hit testing.

## The per-frame call chain

`Render::render` builds a fresh element tree on each frame. GPUI then drives
each node through three phases:

```text
Render::render(&mut self, &mut Window, &mut Context<Self>)
    -> IntoElement::into_element
    -> Element::prepaint (element.rs:784)   // measure, allocate hitboxes
    -> Element::layout / layout_as_root (element.rs:802)
    -> Element::paint (element.rs:797)      // emit draw commands
    -> Scene (scene.rs:41) collects primitives
    -> Frame::finish builds a BuiltFrame (frame.rs:776)
```

`prepaint` runs before layout so an element can reserve state and hitboxes;
`paint` runs after layout with final bounds. The frame pipeline is documented
further in [Rendering](./render.md).

## Element state across frames

Use `Window::with_element_state` (`crates/gpui/src/window/methods_paint.rs:279`)
to keep state keyed by `ElementId` between frames. The id is part of the
node's identity; reusing it for a different element type is a bug. Elements
that need focus or accessibility assign a stable id before setting a role.

## Known limits

- The element tree is rebuilt every frame. It is immutable-mode: you build a
  fresh tree rather than mutating last frame's nodes.
- `BuiltFrame` (frame.rs:776) is crate-private. Application code interacts
  with the frame through `Window`, not by inspecting the built frame.
- A custom `Element` must handle prepaint and paint consistently; skipping
  hitbox registration in prepaint makes the element unclickable.

## See also

- [Rendering](./render.md) for the frame lifecycle
- [Contexts](./contexts.md) for `Window` and `App` access
- [Accessibility semantics](./accessibility.md) for roles and ids
