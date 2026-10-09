# Accessibility semantics

GPUI exposes accessibility through the optional `accessibility` Cargo feature.
The feature builds an AccessKit semantic tree for each completed frame and
routes native actions back to the window that owns the node.

```toml
[dependencies]
gpui = { version = "*", features = ["accessibility"] }
```

This feature does not infer a reliable semantic tree from visual text, icons,
or arbitrary child elements. Components must declare their role, name, state,
and actions explicitly.

## Semantic nodes

Give a semantic element a stable [`ElementId`] before assigning its role. The
ID is part of the node's identity across frames. Use a role that matches the
actual interaction, then add only the state that control owns.

```rust
use gpui::{
    StatefulInteractiveElement as _, div,
    accesskit::{Action, Role},
};

let control = div()
    .id("save-document")
    .role(Role::Button)
    .aria_label("Save document")
    .on_a11y_action(Action::Click, |_, window, cx| {
        // Invoke the same operation used by pointer or keyboard activation.
        window.refresh();
    });
```

Use `aria_description` for supplemental instructions, not to replace a name.
Use `aria_selected`, `aria_expanded`, `aria_toggled`, `aria_value`, and the
numeric range methods only when the component owns the corresponding state.
For range controls, provide finite current/minimum/maximum values and set an
orientation for horizontal or vertical controls.

Do not put semantic roles on decorative icons, visual indicators, or wrapper
elements merely because they receive a layout ID. A composite control should
normally expose one semantic owner; mark its internal indicator decorative at
the component layer.

## Action lifecycle

Accessibility actions are registered for the semantic nodes emitted in the
completed frame. An action for a node removed in a later frame is rejected.
Action handlers must update the same authoritative state as pointer and
keyboard handlers. Do not synthesize pointer events or capture state that can
outlive the window, entity, or rendered node.

If an action changes semantic state, notify or refresh the owning render path
so that the next frame reports the new value, selection, expansion, or range.

## Text input

Platform IME input remains owned by [`TextInputClient`] and its
[`InputHandler`]. These APIs use UTF-16 ranges for selections, marked text,
replacement, and candidate bounds. Do not maintain a second text model solely
for semantics.

AccessKit text selection requires stable `TextRun` nodes and character-length
metadata. A TextInput role and string value alone are not sufficient to expose
selection or caret actions correctly. Until an editor supplies that full text
semantic subtree, expose only the text properties and actions backed by its
authoritative input handler.

## Testing

Accessibility-enabled GPUI tests can inspect the completed frame with the
test-only semantic snapshot API and dispatch an AccessKit action by node ID.
Test stable, user-observable facts: role, name, description, state, range,
parent/child relation, action support, and removal of stale actions. Do not
freeze layout noise or platform-specific adapter details in a semantic golden.

Run both the default and accessibility-enabled configurations for any semantic
change:

```sh
cargo test --locked -p gpui --lib
cargo test --locked -p gpui --lib --features accessibility
```

## Platform contract

Check [`PlatformCapabilities`] before relying on a native service. A successful
feature build is not native assistive-technology runtime evidence. In-tree
desktop adapters are feature-gated; WebAssembly currently reports accessibility
as unavailable. The repository's platform matrix and native-validation runbook
record the current tested scope.

[`ElementId`]: ../src/window.rs
[`InputHandler`]: ../src/input.rs
[`PlatformCapabilities`]: ../src/platform.rs
[`TextInputClient`]: ../src/platform.rs
