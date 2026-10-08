use gpui::{
    AnyWindowHandle, AppContext as _, Context, FocusHandle, IntoElement, ParentElement as _,
    Render, TestAppContext, Window, div,
};
use std::{cell::Cell, rc::Rc};
use ui::{
    AnnouncementToast, Button, ButtonCommon as _, Checkbox, ChoiceCard, Clickable as _,
    Disableable as _, ListItem, Modal, ModalHeader, Switch, Tab, ToggleState, Toggleable as _,
    TreeViewItem,
};

struct SemanticComponents {
    action_count: Rc<Cell<usize>>,
    focused_button: FocusHandle,
    target_button: FocusHandle,
}

impl Render for SemanticComponents {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .child(
                Button::new("save", "Save")
                    .disabled(true)
                    .toggle_state(true)
                    .track_focus(&self.focused_button),
            )
            .child(
                Button::new("action", "Action")
                    .track_focus(&self.target_button)
                    .on_click({
                        let action_count = self.action_count.clone();
                        move |_, _, _| action_count.set(action_count.get() + 1)
                    }),
            )
            .child(
                Checkbox::new("checkbox", ToggleState::Selected)
                    .label("Include diagnostics")
                    .on_click({
                        let action_count = self.action_count.clone();
                        move |_, _, _| action_count.set(action_count.get() + 1)
                    }),
            )
            .child(
                Switch::new("switch", ToggleState::Indeterminate)
                    .label("Enable previews")
                    .on_click({
                        let action_count = self.action_count.clone();
                        move |_, _, _| action_count.set(action_count.get() + 1)
                    }),
            )
            .child(
                ChoiceCard::radio("radio-card", "Desktop notifications", true)
                    .description("Show alerts for completed tasks")
                    .on_click({
                        let action_count = self.action_count.clone();
                        move |_, _, _| action_count.set(action_count.get() + 1)
                    }),
            )
            .child(
                ChoiceCard::checkbox("checkbox-card", "Include terminal output", true)
                    .description("Attach recent terminal output to the report")
                    .on_click({
                        let action_count = self.action_count.clone();
                        move |_, _, _| action_count.set(action_count.get() + 1)
                    }),
            )
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
            .child(
                Modal::new("preferences", None).header(ModalHeader::new().headline("Preferences")),
            )
            .child(AnnouncementToast::new().heading("Update available"))
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
    let (focused_button, target_button) = cx.update(|cx| (cx.focus_handle(), cx.focus_handle()));
    let window = cx.add_window({
        let action_count = action_count.clone();
        let target_button = target_button.clone();
        move |window, cx| {
            focused_button.focus(window, cx);
            SemanticComponents {
                action_count,
                focused_button,
                target_button,
            }
        }
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
    let focused_button_id = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("Save"))
        .map(|(node_id, _)| *node_id)
        .expect("focused button semantic node should exist");
    assert_eq!(snapshot.focused_node, focused_button_id);

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

    let (checkbox_id, checkbox) = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::CheckBox
                && node.label() == Some("Include diagnostics")
        })
        .expect("checkbox semantic node should exist");
    assert_eq!(checkbox.toggled(), Some(gpui::accesskit::Toggled::True));
    assert!(checkbox.supports_action(gpui::accesskit::Action::Click));

    let (switch_id, switch) = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::Switch && node.label() == Some("Enable previews")
        })
        .expect("switch semantic node should exist");
    assert_eq!(switch.toggled(), Some(gpui::accesskit::Toggled::Mixed));
    assert!(switch.supports_action(gpui::accesskit::Action::Click));

    cx.update_window(handle, |_, window, cx| {
        assert!(window.dispatch_accessibility_action(
            *checkbox_id,
            gpui::accesskit::Action::Click,
            None,
            cx,
        ));
        assert!(window.dispatch_accessibility_action(
            *switch_id,
            gpui::accesskit::Action::Click,
            None,
            cx,
        ));
    })
    .expect("semantic test window should remain open");
    assert_eq!(action_count.get(), 3);

    let (radio_card_id, radio_card) = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::RadioButton
                && node.label() == Some("Desktop notifications")
        })
        .expect("radio choice card semantic node should exist");
    assert_eq!(
        radio_card.description(),
        Some("Show alerts for completed tasks")
    );
    assert_eq!(radio_card.is_selected(), Some(true));
    assert!(radio_card.supports_action(gpui::accesskit::Action::Click));

    let (checkbox_card_id, checkbox_card) = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::CheckBox
                && node.label() == Some("Include terminal output")
        })
        .expect("checkbox choice card semantic node should exist");
    assert_eq!(
        checkbox_card.description(),
        Some("Attach recent terminal output to the report")
    );
    assert_eq!(
        checkbox_card.toggled(),
        Some(gpui::accesskit::Toggled::True)
    );
    assert!(checkbox_card.supports_action(gpui::accesskit::Action::Click));
    assert!(!snapshot.update.nodes.iter().any(|(_, node)| {
        node.role() == gpui::accesskit::Role::CheckBox && node.label().is_none()
    }));

    cx.update_window(handle, |_, window, cx| {
        assert!(window.dispatch_accessibility_action(
            *radio_card_id,
            gpui::accesskit::Action::Click,
            None,
            cx,
        ));
        assert!(window.dispatch_accessibility_action(
            *checkbox_card_id,
            gpui::accesskit::Action::Click,
            None,
            cx,
        ));
    })
    .expect("semantic test window should remain open");
    assert_eq!(action_count.get(), 5);

    cx.update_window(handle, |_, window, cx| {
        assert!(window.dispatch_accessibility_action(
            *radio_card_id,
            gpui::accesskit::Action::Focus,
            None,
            cx,
        ));
        window.draw(cx).clear();
        let focused_snapshot = window
            .accessibility_snapshot_for_test()
            .expect("completed frame should contain a semantic snapshot");
        assert_eq!(focused_snapshot.focused_node, *radio_card_id);
    })
    .expect("semantic test window should remain open");

    cx.update_window(handle, |_, window, cx| {
        assert!(window.dispatch_accessibility_action(
            action_button_id,
            gpui::accesskit::Action::Focus,
            None,
            cx,
        ));
        assert!(target_button.is_focused(window));
    })
    .expect("semantic test window should remain open");

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

    let dialog = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == gpui::accesskit::Role::Dialog)
        .map(|(_, node)| node)
        .expect("dialog semantic node should exist");
    assert_eq!(dialog.label(), Some("Preferences"));

    let status = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == gpui::accesskit::Role::Status)
        .map(|(_, node)| node)
        .expect("status semantic node should exist");
    assert_eq!(status.label(), Some("Update available"));
}
