use gpui::{
    AnyWindowHandle, AppContext as _, Context, IntoElement, ParentElement as _, Render,
    TestAppContext, Window, div,
};
use std::{cell::Cell, rc::Rc};
use ui::{Button, Clickable as _, Disableable as _, ListItem, Tab, Toggleable as _, TreeViewItem};

struct SemanticComponents {
    action_count: Rc<Cell<usize>>,
}

impl Render for SemanticComponents {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .child(
                Button::new("save", "Save")
                    .disabled(true)
                    .toggle_state(true),
            )
            .child(Button::new("action", "Action").on_click({
                let action_count = self.action_count.clone();
                move |_, _, _| action_count.set(action_count.get() + 1)
            }))
            .child(Tab::new("editor-tab").toggle_state(true).child("Editor"))
            .child(
                TreeViewItem::new("workspace-tree", "Workspace")
                    .expanded(true)
                    .toggle_state(true),
            )
            .child(
                ListItem::new("list-item")
                    .aria_label("Item")
                    .toggle(true)
                    .toggle_state(true),
            )
    }
}

#[test]
fn components_emit_roles_labels_and_state() {
    let mut cx = TestAppContext::single();
    cx.update(|cx| {
        let settings = settings::SettingsStore::test(cx);
        cx.set_global(settings);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
    });
    let action_count = Rc::new(Cell::new(0));
    let window = cx.add_window({
        let action_count = action_count.clone();
        move |_, _| SemanticComponents { action_count }
    });
    cx.run_until_parked();
    let handle: AnyWindowHandle = window.into();
    let snapshot = cx
        .update_window(handle, |_, window, cx| {
            window.refresh();
            window.draw(cx).clear();
            window
                .accessibility_snapshot_for_test()
                .expect("accessibility snapshot should be built")
        })
        .expect("semantic test window should remain open");

    let button = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::Button && node.label() == Some("Save")
        })
        .map(|(_, node)| node)
        .expect("button semantic node should exist");
    assert_eq!(button.label(), Some("Save"));
    assert!(button.is_disabled());
    assert_eq!(button.toggled(), Some(gpui::accesskit::Toggled::True));

    let action_button_id = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("Action"))
        .map(|(node_id, _)| *node_id)
        .expect("action button semantic node should exist");
    cx.update_window(handle, |_, window, cx| {
        assert!(window.dispatch_accessibility_action(
            action_button_id,
            gpui::accesskit::Action::Click,
            None,
            cx,
        ));
    })
    .expect("semantic test window should remain open");
    assert_eq!(action_count.get(), 1);

    let tab = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == gpui::accesskit::Role::Tab)
        .map(|(_, node)| node)
        .expect("tab semantic node should exist");
    assert_eq!(tab.is_selected(), Some(true));

    let tree_item = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == gpui::accesskit::Role::TreeItem)
        .map(|(_, node)| node)
        .expect("tree item semantic node should exist");
    assert_eq!(tree_item.label(), Some("Workspace"));
    assert_eq!(tree_item.is_expanded(), Some(true));
    assert_eq!(tree_item.is_selected(), Some(true));

    let list_item = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == gpui::accesskit::Role::ListItem)
        .map(|(_, node)| node)
        .expect("list item semantic node should exist");
    assert_eq!(list_item.label(), Some("Item"));
    assert_eq!(list_item.is_expanded(), Some(true));
    assert_eq!(list_item.is_selected(), Some(true));
}
