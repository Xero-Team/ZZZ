use std::rc::Rc;

use gpui::{Action, FocusHandle, Focusable};
use i18n::tr;
use ui::{ContextMenu, Divider, KeyBinding, PopoverMenu, Tooltip, prelude::*};

use crate::preview::Layout;
use crate::{
    Picker, PickerDelegate, SetPreviewBelow, SetPreviewRight, ToggleActionsMenu, TogglePreview,
};

/// Line in the default picker actions menu.
pub enum PickerAction {
    Header(SharedString),
    Separator,
    Entry {
        label: SharedString,
        action: Box<dyn Action>,
        toggled: Option<bool>,
    },
}

impl PickerAction {
    pub fn button(label: impl Into<SharedString>, action: Box<dyn Action>) -> Self {
        Self::Entry {
            label: label.into(),
            action,
            toggled: None,
        }
    }

    pub fn header(label: impl Into<SharedString>) -> Self {
        Self::Header(label.into())
    }

    pub fn separator() -> Self {
        Self::Separator
    }

    pub fn toggled(mut self, toggled: bool) -> Self {
        if let Self::Entry {
            toggled: toggle_state,
            ..
        } = &mut self
        {
            *toggle_state = Some(toggled);
        }
        self
    }

    fn add_to_menu(&self, menu: ContextMenu, focus_handle: &FocusHandle) -> ContextMenu {
        match self {
            Self::Header(label) => menu.header(label),
            Self::Separator => menu.separator(),
            Self::Entry {
                label,
                action,
                toggled: Some(toggled),
            } => {
                let dispatched = action.boxed_clone();
                let handler_focus = focus_handle.clone();
                menu.toggleable_entry(
                    label,
                    *toggled,
                    ui::IconPosition::End,
                    Some(action.boxed_clone()),
                    move |window, cx| {
                        window.focus(&handler_focus, cx);
                        window.dispatch_action(dispatched.boxed_clone(), cx);
                    },
                )
            }
            Self::Entry {
                label,
                action,
                toggled: None,
            } => menu.action(label, action.boxed_clone()),
        }
    }
}

impl<D: PickerDelegate> Picker<D> {
    pub(crate) fn render_footer(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if let Some(footer) = self.delegate.render_footer(window, cx) {
            return Some(footer);
        }

        let actions = self.delegate.actions_menu(window, cx);
        if self.preview.is_none() && actions.is_empty() {
            return None;
        }

        let focus_handle = self.focus_handle(cx);
        Some(
            h_flex()
                .w_full()
                .p_1p5()
                .justify_between()
                .border_t_1()
                .border_color(cx.theme().colors().border_variant)
                .child(div().when(self.preview.is_some(), |this| {
                    this.child(self.render_preview_controls(focus_handle.clone(), window, cx))
                }))
                .when(!actions.is_empty(), |this| {
                    this.child(self.render_actions_button(actions.into(), focus_handle, window, cx))
                })
                .into_any_element(),
        )
    }

    fn render_preview_controls(
        &self,
        focus_handle: FocusHandle,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let toggle_focus_handle = focus_handle.clone();
        let right_focus_handle = focus_handle.clone();
        let below_focus_handle = focus_handle.clone();
        let current = self.preview_layout().unwrap_or(Layout::Hidden);
        let preview_visible = current != Layout::Hidden;

        h_flex()
            .gap_1()
            .child(
                Button::new(
                    "picker-preview-toggle",
                    tr(cx, "picker.footer.preview", "Preview"),
                )
                .when(preview_visible, |this| {
                    this.selected_style(ui::ButtonStyle::Tinted(ui::TintColor::Accent))
                })
                .key_binding(
                    KeyBinding::for_action_in(&TogglePreview, &focus_handle, cx)
                        .size(rems_from_px(12.)),
                )
                .tooltip(move |_window, cx| {
                    Tooltip::for_action_in(
                        tr(cx, "picker.footer.toggle_preview", "Toggle Preview"),
                        &TogglePreview,
                        &toggle_focus_handle,
                        cx,
                    )
                })
                .on_click(cx.listener(|this, _, window, cx| {
                    this.toggle_preview_visible(window, cx);
                })),
            )
            .when(preview_visible, |this| {
                this.child(div().child(Divider::vertical().color(ui::DividerColor::Border)))
                    .child(
                        IconButton::new("picker-preview-right", IconName::DiffSplit)
                            .icon_size(IconSize::Small)
                            .toggle_state(current == Layout::Right)
                            .tooltip(move |_window, cx| {
                                Tooltip::for_action_in(
                                    tr(cx, "picker.footer.preview_right", "Preview to the Right"),
                                    &SetPreviewRight,
                                    &right_focus_handle,
                                    cx,
                                )
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_preview_layout(Layout::Right, window, cx);
                            })),
                    )
                    .child(
                        IconButton::new("picker-preview-below", IconName::DiffUnified)
                            .icon_size(IconSize::Small)
                            .toggle_state(current == Layout::Below)
                            .tooltip(move |_window, cx| {
                                Tooltip::for_action_in(
                                    tr(cx, "picker.footer.preview_below", "Preview Below"),
                                    &SetPreviewBelow,
                                    &below_focus_handle,
                                    cx,
                                )
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_preview_layout(Layout::Below, window, cx);
                            })),
                    )
            })
    }

    fn render_actions_button(
        &self,
        actions: Rc<[PickerAction]>,
        focus_handle: FocusHandle,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        PopoverMenu::new("picker-actions-menu")
            .with_handle(self.actions_menu_handle.clone())
            .attach(gpui::Anchor::TopRight)
            .anchor(gpui::Anchor::BottomRight)
            .trigger_with_tooltip(
                Button::new(
                    "picker-actions-trigger",
                    tr(cx, "picker.footer.actions", "Actions..."),
                )
                .key_binding(
                    KeyBinding::for_action_in(&ToggleActionsMenu, &focus_handle, cx)
                        .size(rems_from_px(12.)),
                )
                .selected_style(ui::ButtonStyle::Tinted(ui::TintColor::Accent)),
                {
                    let tooltip_focus_handle = focus_handle.clone();
                    move |_window, cx| {
                        Tooltip::for_action_in(
                            tr(cx, "picker.footer.actions_tooltip", "Actions"),
                            &ToggleActionsMenu,
                            &tooltip_focus_handle,
                            cx,
                        )
                    }
                },
            )
            .menu(move |window, cx| {
                let actions = Rc::clone(&actions);
                let focus_handle = focus_handle.clone();
                Some(ContextMenu::build(window, cx, move |mut menu, _, _| {
                    menu = menu.context(focus_handle.clone());
                    for item in actions.iter() {
                        menu = item.add_to_menu(menu, &focus_handle);
                    }
                    menu
                }))
            })
    }
}
