use gpui::{
    AnyWindowHandle, AppContext as _, Context, Entity, FocusHandle, IntoElement,
    ParentElement as _, Render, TestAppContext, Window, div,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use ui::{
    AnnouncementToast, Button, ButtonCommon as _, Checkbox, ChoiceCard, Clickable as _,
    ContextMenu, Disableable as _, Disclosure, DropdownMenu, IconPosition, List, ListAccessibility,
    ListItem, Modal, ModalHeader, Switch, Tab, TabBar, Table, TableAccessibility, ToggleState,
    Toggleable as _, TreeViewItem,
};

struct SemanticComponents {
    action_count: Rc<Cell<usize>>,
    focused_button: FocusHandle,
    target_button: FocusHandle,
}

struct SemanticContextMenu {
    menu: Entity<ContextMenu>,
}

struct SemanticDropdown {
    menu: Entity<ContextMenu>,
}

impl Render for SemanticContextMenu {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.menu.clone()
    }
}

impl Render for SemanticDropdown {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        DropdownMenu::new("semantic-dropdown", "Project actions", self.menu.clone())
    }
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
            .child(
                TabBar::new("semantic-tab-bar").child(
                    Tab::new("editor-tab", "Editor")
                        .toggle_state(true)
                        .child("Editor"),
                ),
            )
            .child(
                TreeViewItem::new("workspace-tree", "Workspace")
                    .root_item(true)
                    .expanded(true)
                    .toggle_state(true)
                    .on_click({
                        let action_count = self.action_count.clone();
                        move |_, _, _| action_count.set(action_count.get() + 1)
                    })
                    .on_toggle({
                        let action_count = self.action_count.clone();
                        move |_, _, _| action_count.set(action_count.get() + 1)
                    }),
            )
            .child(TreeViewItem::new("settings-tree", "Settings"))
            .child(
                TreeViewItem::new("disabled-tree", "Unavailable")
                    .root_item(true)
                    .disabled(true)
                    .on_toggle(|_, _, _| {}),
            )
            .child(
                ListItem::new("list-item")
                    .aria_label("Item")
                    .toggle(true)
                    .toggle_state(true)
                    .on_click({
                        let action_count = self.action_count.clone();
                        move |_, _, _| action_count.set(action_count.get() + 1)
                    })
                    .on_toggle({
                        let action_count = self.action_count.clone();
                        move |_, _, _| action_count.set(action_count.get() + 1)
                    }),
            )
            .child(
                ListItem::new("menu-item")
                    .aria_label("Open recent project")
                    .accessibility_role(gpui::accesskit::Role::MenuItemCheckBox)
                    .accessibility_toggled(gpui::accesskit::Toggled::True)
                    .on_click({
                        let action_count = self.action_count.clone();
                        move |_, _, _| action_count.set(action_count.get() + 1)
                    }),
            )
            .child(
                List::new()
                    .with_accessibility(ListAccessibility::new(
                        "recent-projects-list",
                        "Recent projects",
                    ))
                    .child(ListItem::new("recent-project").aria_label("Project sunrise")),
            )
            .child(
                Disclosure::new("advanced-options", true)
                    .aria_label("Advanced options")
                    .on_click({
                        let action_count = self.action_count.clone();
                        move |_, _, _| action_count.set(action_count.get() + 1)
                    }),
            )
            .child(Disclosure::new("default-disclosure", false).on_click({
                let action_count = self.action_count.clone();
                move |_, _, _| action_count.set(action_count.get() + 1)
            }))
            .child(
                Table::new(2)
                    .with_accessibility(TableAccessibility::new(
                        "recent-files-table",
                        "Recent files",
                        ["Name", "Status"],
                        |row_index, column_index, _| match (row_index, column_index) {
                            (0, 0) => "Cargo.toml".into(),
                            (0, 1) => "Modified".into(),
                            _ => "Unknown".into(),
                        },
                    ))
                    .expect("table semantic headers should match its column count")
                    .header(vec!["Name", "Status"])
                    .row(vec!["Cargo.toml", "Modified"]),
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

    let (tab_id, tab) = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::Tab && node.label() == Some("Editor")
        })
        .expect("tab semantic node should exist");
    assert_eq!(tab.is_selected(), Some(true));
    let tab_list = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == gpui::accesskit::Role::TabList)
        .map(|(_, node)| node)
        .expect("tab list semantic node should exist");
    assert!(tab_list.children().contains(tab_id));

    let tree_item = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::TreeItem && node.label() == Some("Workspace")
        })
        .map(|(_, node)| node)
        .expect("tree item semantic node should exist");
    assert_eq!(tree_item.label(), Some("Workspace"));
    assert_eq!(tree_item.is_expanded(), Some(true));
    assert_eq!(tree_item.is_selected(), Some(true));
    assert!(tree_item.supports_action(gpui::accesskit::Action::Click));
    assert!(tree_item.supports_action(gpui::accesskit::Action::Collapse));
    assert!(!tree_item.supports_action(gpui::accesskit::Action::Expand));
    assert!(!snapshot.update.nodes.iter().any(|(_, node)| {
        node.role() == gpui::accesskit::Role::Button && node.label() == Some("Collapse")
    }));
    let (tree_item_id, _) = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::TreeItem && node.label() == Some("Workspace")
        })
        .expect("expandable tree item semantic node should exist");
    let leaf_tree_item = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::TreeItem && node.label() == Some("Settings")
        })
        .map(|(_, node)| node)
        .expect("leaf tree item semantic node should exist");
    assert_eq!(leaf_tree_item.is_expanded(), None);
    assert!(!leaf_tree_item.supports_action(gpui::accesskit::Action::Expand));
    assert!(!leaf_tree_item.supports_action(gpui::accesskit::Action::Collapse));
    let disabled_tree_item = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::TreeItem && node.label() == Some("Unavailable")
        })
        .map(|(_, node)| node)
        .expect("disabled tree item semantic node should exist");
    assert!(disabled_tree_item.is_disabled());
    assert_eq!(disabled_tree_item.is_expanded(), Some(false));
    assert!(!disabled_tree_item.supports_action(gpui::accesskit::Action::Expand));
    assert!(!disabled_tree_item.supports_action(gpui::accesskit::Action::Collapse));

    let (list_item_id, list_item) = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == gpui::accesskit::Role::ListItem)
        .expect("list item semantic node should exist");
    assert_eq!(list_item.label(), Some("Item"));
    assert_eq!(list_item.is_expanded(), Some(true));
    assert_eq!(list_item.is_selected(), Some(true));
    assert!(list_item.supports_action(gpui::accesskit::Action::Click));
    assert!(list_item.supports_action(gpui::accesskit::Action::Collapse));
    assert!(!list_item.supports_action(gpui::accesskit::Action::Expand));

    let (recent_project_id, _) = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::ListItem
                && node.label() == Some("Project sunrise")
        })
        .expect("semantic list item should exist");
    let recent_projects = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::List && node.label() == Some("Recent projects")
        })
        .map(|(_, node)| node)
        .expect("semantic list node should exist");
    assert!(recent_projects.children().contains(recent_project_id));

    let (menu_item_id, menu_item) = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::MenuItemCheckBox
                && node.label() == Some("Open recent project")
        })
        .expect("menu item semantic node should exist");
    assert_eq!(menu_item.toggled(), Some(gpui::accesskit::Toggled::True));
    assert!(menu_item.supports_action(gpui::accesskit::Action::Click));

    let (disclosure_id, disclosure) = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::Button && node.label() == Some("Advanced options")
        })
        .expect("disclosure semantic node should exist");
    assert_eq!(disclosure.is_expanded(), Some(true));
    assert_eq!(disclosure.toggled(), None);
    assert!(disclosure.supports_action(gpui::accesskit::Action::Click));

    let (default_disclosure_id, default_disclosure) = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::Button && node.label() == Some("Expand")
        })
        .expect("default disclosure semantic node should exist");
    assert_eq!(default_disclosure.is_expanded(), Some(false));
    assert_eq!(default_disclosure.toggled(), None);
    assert!(default_disclosure.supports_action(gpui::accesskit::Action::Click));

    let table = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::Table && node.label() == Some("Recent files")
        })
        .map(|(_, node)| node)
        .expect("table semantic node should exist");
    assert_eq!(table.children().len(), 2);
    let table_rows = table
        .children()
        .iter()
        .map(|id| {
            snapshot
                .update
                .nodes
                .iter()
                .find(|(node_id, _)| node_id == id)
                .map(|(_, node)| node)
                .expect("table child should have a semantic node")
        })
        .collect::<Vec<_>>();
    assert!(
        table_rows
            .iter()
            .all(|node| node.role() == gpui::accesskit::Role::Row)
    );
    assert!(table_rows.iter().any(|row| {
        row.children().iter().any(|id| {
            snapshot.update.nodes.iter().any(|(node_id, node)| {
                node_id == id
                    && node.role() == gpui::accesskit::Role::ColumnHeader
                    && node.label() == Some("Name")
            })
        })
    }));
    assert!(table_rows.iter().any(|row| {
        row.children().iter().any(|id| {
            snapshot.update.nodes.iter().any(|(node_id, node)| {
                node_id == id
                    && node.role() == gpui::accesskit::Role::ColumnHeader
                    && node.label() == Some("Status")
            })
        })
    }));
    assert!(table_rows.iter().any(|row| {
        row.children().iter().any(|id| {
            snapshot.update.nodes.iter().any(|(node_id, node)| {
                node_id == id
                    && node.role() == gpui::accesskit::Role::Cell
                    && node.label() == Some("Cargo.toml")
            })
        })
    }));
    assert!(table_rows.iter().any(|row| {
        row.children().iter().any(|id| {
            snapshot.update.nodes.iter().any(|(node_id, node)| {
                node_id == id
                    && node.role() == gpui::accesskit::Role::Cell
                    && node.label() == Some("Modified")
            })
        })
    }));

    cx.update_window(handle, |_, window, cx| {
        assert!(window.dispatch_accessibility_action(
            *list_item_id,
            gpui::accesskit::Action::Click,
            None,
            cx,
        ));
        assert!(window.dispatch_accessibility_action(
            *menu_item_id,
            gpui::accesskit::Action::Click,
            None,
            cx,
        ));
        assert!(window.dispatch_accessibility_action(
            *disclosure_id,
            gpui::accesskit::Action::Click,
            None,
            cx,
        ));
        assert!(window.dispatch_accessibility_action(
            *default_disclosure_id,
            gpui::accesskit::Action::Click,
            None,
            cx,
        ));
        assert!(window.dispatch_accessibility_action(
            *tree_item_id,
            gpui::accesskit::Action::Click,
            None,
            cx,
        ));
        assert!(window.dispatch_accessibility_action(
            *tree_item_id,
            gpui::accesskit::Action::Collapse,
            None,
            cx,
        ));
        assert!(window.dispatch_accessibility_action(
            *list_item_id,
            gpui::accesskit::Action::Collapse,
            None,
            cx,
        ));
    })
    .expect("semantic test window should remain open");
    assert_eq!(action_count.get(), 12);

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

#[test]
fn dropdown_trigger_reports_expanded_menu_state() {
    let mut cx = TestAppContext::single();
    cx.update(|cx| {
        let settings = settings::SettingsStore::test(cx);
        cx.set_global(settings);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
    });

    let menu_slot = Rc::new(RefCell::new(None));
    let window = cx.add_window({
        let menu_slot = menu_slot.clone();
        move |window, cx| {
            let menu = ContextMenu::build(window, cx, |menu, _, _| {
                menu.entry("Open project", None, |_, _| {})
            });
            *menu_slot.borrow_mut() = Some(menu.clone());
            SemanticDropdown { menu }
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
                .expect("dropdown should build a semantic snapshot")
        })
        .expect("dropdown window should remain open");

    let (trigger_id, trigger) = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::Button && node.label() == Some("Project actions")
        })
        .expect("dropdown trigger semantic node should exist");
    assert_eq!(trigger.is_expanded(), Some(false));
    assert_eq!(trigger.toggled(), None);
    assert!(trigger.supports_action(gpui::accesskit::Action::Click));

    let expanded_snapshot = cx
        .update_window(handle, |_, window, cx| {
            assert!(window.dispatch_accessibility_action(
                *trigger_id,
                gpui::accesskit::Action::Click,
                None,
                cx,
            ));
            window.draw(cx).clear();
            window
                .accessibility_snapshot_for_test()
                .expect("opened dropdown should build a semantic snapshot")
        })
        .expect("dropdown window should remain open");

    let trigger = expanded_snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::Button && node.label() == Some("Project actions")
        })
        .map(|(_, node)| node)
        .expect("opened dropdown trigger semantic node should exist");
    assert_eq!(trigger.is_expanded(), Some(true));
    assert_eq!(trigger.toggled(), None);
    assert!(
        expanded_snapshot
            .update
            .nodes
            .iter()
            .any(|(_, node)| node.role() == gpui::accesskit::Role::Menu)
    );

    let menu = menu_slot
        .borrow()
        .clone()
        .expect("dropdown should retain its context menu");
    cx.update(|cx| {
        menu.update(cx, |_, cx| cx.emit(gpui::DismissEvent));
    });
    let collapsed_snapshot = cx
        .update_window(handle, |_, window, cx| {
            window.draw(cx).clear();
            window
                .accessibility_snapshot_for_test()
                .expect("dismissed dropdown should build a semantic snapshot")
        })
        .expect("dropdown window should remain open");
    let trigger = collapsed_snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::Button && node.label() == Some("Project actions")
        })
        .map(|(_, node)| node)
        .expect("dismissed dropdown trigger semantic node should exist");
    assert_eq!(trigger.is_expanded(), Some(false));
    assert_eq!(trigger.toggled(), None);
}

#[test]
fn context_menu_emits_menu_and_item_semantics() {
    let mut cx = TestAppContext::single();
    cx.update(|cx| {
        let settings = settings::SettingsStore::test(cx);
        cx.set_global(settings);
        theme_settings::init(theme::LoadThemes::JustBase, cx);
    });

    let action_count = Rc::new(Cell::new(0));
    let window = cx.add_window({
        let action_count = action_count.clone();
        move |window, cx| {
            let menu = ContextMenu::build(window, cx, move |menu, _, _| {
                let open_action_count = action_count.clone();
                menu.entry("Open project", None, move |_, _| {
                    open_action_count.set(open_action_count.get() + 1);
                })
                .toggleable_entry(
                    "Show hidden files",
                    false,
                    IconPosition::Start,
                    None,
                    move |_, _| {
                        action_count.set(action_count.get() + 1);
                    },
                )
            });
            SemanticContextMenu { menu }
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
                .expect("context menu should build a semantic snapshot")
        })
        .expect("context menu window should remain open");

    assert!(
        snapshot
            .update
            .nodes
            .iter()
            .any(|(_, node)| node.role() == gpui::accesskit::Role::Menu)
    );

    let (open_item_id, open_item) = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::MenuItem && node.label() == Some("Open project")
        })
        .expect("menu item semantic node should exist");
    assert!(open_item.supports_action(gpui::accesskit::Action::Click));

    let (toggle_item_id, toggle_item) = snapshot
        .update
        .nodes
        .iter()
        .find(|(_, node)| {
            node.role() == gpui::accesskit::Role::MenuItemCheckBox
                && node.label() == Some("Show hidden files")
        })
        .expect("checkbox menu item semantic node should exist");
    assert_eq!(toggle_item.toggled(), Some(gpui::accesskit::Toggled::False));
    assert!(toggle_item.supports_action(gpui::accesskit::Action::Click));

    cx.update_window(handle, |_, window, cx| {
        assert!(window.dispatch_accessibility_action(
            *open_item_id,
            gpui::accesskit::Action::Click,
            None,
            cx,
        ));
        assert!(window.dispatch_accessibility_action(
            *toggle_item_id,
            gpui::accesskit::Action::Click,
            None,
            cx,
        ));
    })
    .expect("context menu window should remain open");
    assert_eq!(action_count.get(), 2);
}
