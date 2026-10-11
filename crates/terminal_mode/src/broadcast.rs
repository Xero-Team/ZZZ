use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, Styled, WeakEntity, Window,
};
use i18n::tr;
use ui::{Button, Label, prelude::*};
use ui_input::InputField;

use crate::session::SessionStore;

/// A small form that sends one command to every session terminal at once.
pub struct BroadcastModal {
    store: WeakEntity<SessionStore>,
    input: Entity<InputField>,
    focus_handle: FocusHandle,
}

impl BroadcastModal {
    pub fn new(store: Entity<SessionStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let placeholder = tr(
            cx,
            "terminal_mode.broadcast.placeholder",
            "Command to send to every session",
        );
        let label = tr(cx, "terminal_mode.broadcast.label", "Command");
        let input = cx.new(|cx| {
            InputField::new(window, cx, &placeholder)
                .label(label)
                .tab_stop(true)
                .tab_index(1)
        });
        let focus_handle = input.focus_handle(cx);
        window.focus(&focus_handle, cx);

        Self {
            store: store.downgrade(),
            input,
            focus_handle,
        }
    }

    fn send(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).text(cx);
        if text.trim().is_empty() {
            cx.emit(DismissEvent);
            return;
        }
        if let Some(store) = self.store.upgrade() {
            let command = format!("{text}\n");
            store.update(cx, |store, cx| {
                store.send_to_all_sessions(&command, cx);
            });
        }
        cx.emit(DismissEvent);
    }
}

impl Focusable for BroadcastModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<DismissEvent> for BroadcastModal {}

impl Render for BroadcastModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w(rems(28.))
            .p_3()
            .gap_3()
            .elevation_3(cx)
            .track_focus(&self.focus_handle)
            .child(Label::new(tr(
                cx,
                "terminal_mode.broadcast.title",
                "Send to All Sessions",
            )))
            .child(self.input.clone())
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new(
                            "terminal-mode-broadcast-cancel",
                            tr(cx, "terminal_mode.broadcast.cancel", "Cancel"),
                        )
                        .on_click(cx.listener(|_this, _event, _window, cx| cx.emit(DismissEvent))),
                    )
                    .child(
                        Button::new(
                            "terminal-mode-broadcast-send",
                            tr(cx, "terminal_mode.broadcast.send", "Send"),
                        )
                        .on_click(cx.listener(|this, _event, window, cx| this.send(window, cx))),
                    ),
            )
    }
}

impl workspace::ModalView for BroadcastModal {}
