use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{AppContext, Context, Entity, PlatformDisplay, Subscription, Window, WindowHandle};
use ui::{NotificationWindow, NotificationWindowEvent};
use util::ResultExt;

/// The displays on which a background notification window should appear.
#[derive(Clone, Copy, Debug)]
pub enum NotificationWindowDisplay {
    Primary,
    All,
}

/// Content and controls for a [`NotificationWindow`].
#[derive(Clone)]
pub struct NotificationWindowData {
    pub title: gpui::SharedString,
    pub caption: gpui::SharedString,
    pub context: Option<gpui::SharedString>,
    pub icon: ui::IconName,
    pub view_label: gpui::SharedString,
    pub dismiss_label: gpui::SharedString,
}

/// Owns background notification windows and the subscriptions that keep their
/// parent entity connected to popup events.
#[derive(Default)]
pub struct NotificationWindowManager {
    windows: Vec<WindowHandle<NotificationWindow>>,
    subscriptions: HashMap<WindowHandle<NotificationWindow>, Vec<Subscription>>,
}

impl NotificationWindowManager {
    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    pub fn show<T, OnEvent, OnOpen>(
        &mut self,
        data: NotificationWindowData,
        display: NotificationWindowDisplay,
        request_attention: bool,
        source_window: &mut Window,
        cx: &mut Context<T>,
        on_event: OnEvent,
        on_open: OnOpen,
    ) where
        T: 'static,
        OnEvent: Fn(&mut T, &NotificationWindowEvent, &mut Window, &mut Context<T>) + 'static,
        OnOpen: Fn(&Entity<NotificationWindow>, &mut Window, &mut Context<T>) -> Vec<Subscription>
            + 'static,
    {
        if request_attention {
            source_window.request_attention();
        }

        let screens: Vec<Rc<dyn PlatformDisplay>> = match display {
            NotificationWindowDisplay::Primary => cx.primary_display().into_iter().collect(),
            NotificationWindowDisplay::All => cx.displays(),
        };
        let on_event: Arc<
            dyn Fn(&mut T, &NotificationWindowEvent, &mut Window, &mut Context<T>) + 'static,
        > = Arc::new(on_event);

        for screen in screens {
            let options = NotificationWindow::window_options(screen, cx);
            let Some(screen_window) = cx
                .open_window(options, {
                    let data = data.clone();
                    move |_window, cx| {
                        cx.new(|_cx| {
                            NotificationWindow::new(
                                data.title,
                                data.caption,
                                data.icon,
                                data.context,
                                data.view_label,
                                data.dismiss_label,
                            )
                        })
                    }
                })
                .log_err()
            else {
                continue;
            };

            let Some(notification) = screen_window.entity(cx).log_err() else {
                continue;
            };

            let mut subscriptions = on_open(&notification, source_window, cx);
            let on_event = on_event.clone();
            subscriptions.push(cx.subscribe_in(
                &notification,
                source_window,
                move |this, _, event, window, cx| on_event(this, event, window, cx),
            ));

            self.subscriptions.insert(screen_window, subscriptions);
            self.windows.push(screen_window);
        }
    }

    pub fn dismiss_all<C: AppContext>(&mut self, cx: &mut C) {
        for window in self.windows.drain(..) {
            window
                .update(cx, |_, window, _| window.remove_window())
                .log_err();
            self.subscriptions.remove(&window);
        }
    }
}
