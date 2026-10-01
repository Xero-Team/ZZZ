use crate::{TintColor, prelude::*};
use gpui::{
    App, Context, EventEmitter, IntoElement, PlatformDisplay, Render, Size, Window,
    WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind, WindowOptions,
    linear_color_stop, linear_gradient, point,
};
use release_channel::ReleaseChannel;
use std::rc::Rc;

/// A transient notification rendered in a borderless popup window.
pub struct NotificationWindow {
    title: SharedString,
    caption: SharedString,
    icon: IconName,
    context: Option<SharedString>,
    view_label: SharedString,
    dismiss_label: SharedString,
}

impl NotificationWindow {
    pub fn new(
        title: impl Into<SharedString>,
        caption: impl Into<SharedString>,
        icon: IconName,
        context: Option<impl Into<SharedString>>,
        view_label: impl Into<SharedString>,
        dismiss_label: impl Into<SharedString>,
    ) -> Self {
        Self {
            title: title.into(),
            caption: caption.into(),
            icon,
            context: context.map(|value| value.into()),
            view_label: view_label.into(),
            dismiss_label: dismiss_label.into(),
        }
    }

    pub fn window_options(screen: Rc<dyn PlatformDisplay>, cx: &App) -> WindowOptions {
        let size = Size {
            width: px(450.),
            height: px(72.),
        };

        let notification_margin_width = px(16.);
        let notification_margin_height = px(-48.);

        let bounds = gpui::Bounds::<Pixels> {
            origin: screen.bounds().top_right()
                - point(
                    size.width + notification_margin_width,
                    notification_margin_height,
                ),
            size,
        };

        let app_id = ReleaseChannel::global(cx).app_id();

        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: false,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            display_id: Some(screen.id()),
            window_background: WindowBackgroundAppearance::Transparent,
            app_id: Some(app_id.to_owned()),
            window_min_size: None,
            window_decorations: Some(WindowDecorations::Client),
            tabbing_identifier: None,
            ..Default::default()
        }
    }
}

pub enum NotificationWindowEvent {
    Accepted,
    Dismissed,
}

impl EventEmitter<NotificationWindowEvent> for NotificationWindow {}

impl NotificationWindow {
    pub fn accept(&mut self, cx: &mut Context<Self>) {
        cx.emit(NotificationWindowEvent::Accepted);
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        cx.emit(NotificationWindowEvent::Dismissed);
    }
}

impl Render for NotificationWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui_font = theme_settings::setup_ui_font(window, cx);
        let line_height = window.line_height();

        let bg = cx.theme().colors().elevated_surface_background;
        let gradient_overflow = || {
            div()
                .h_full()
                .absolute()
                .w_8()
                .bottom_0()
                .right_0()
                .bg(linear_gradient(
                    90.,
                    linear_color_stop(bg, 1.),
                    linear_color_stop(bg.opacity(0.2), 0.),
                ))
        };

        h_flex()
            .id("notification-window")
            .size_full()
            .p_3()
            .gap_4()
            .justify_between()
            .elevation_3(cx)
            .text_ui(cx)
            .font(ui_font)
            .border_color(cx.theme().colors().border)
            .rounded_xl()
            .child(
                h_flex()
                    .items_start()
                    .gap_2()
                    .flex_1()
                    .child(
                        h_flex().h(line_height).justify_center().child(
                            Icon::new(self.icon)
                                .color(Color::Muted)
                                .size(IconSize::Small),
                        ),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .max_w(px(300.))
                            .child(
                                div()
                                    .relative()
                                    .text_size(px(14.))
                                    .text_color(cx.theme().colors().text)
                                    .truncate()
                                    .child(self.title.clone())
                                    .child(gradient_overflow()),
                            )
                            .child(
                                h_flex()
                                    .relative()
                                    .gap_1p5()
                                    .text_size(px(12.))
                                    .text_color(cx.theme().colors().text_muted)
                                    .truncate()
                                    .when_some(self.context.clone(), |description, context| {
                                        description.child(
                                            h_flex()
                                                .gap_1p5()
                                                .child(div().max_w_16().truncate().child(context))
                                                .child(
                                                    div().size(px(3.)).rounded_full().bg(cx
                                                        .theme()
                                                        .colors()
                                                        .text
                                                        .opacity(0.5)),
                                                ),
                                        )
                                    })
                                    .child(self.caption.clone())
                                    .child(gradient_overflow()),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        Button::new("open", self.view_label.clone())
                            .style(ButtonStyle::Tinted(TintColor::Accent))
                            .full_width()
                            .on_click({
                                cx.listener(|this, _event, _, cx| {
                                    this.accept(cx);
                                })
                            }),
                    )
                    .child(
                        Button::new("dismiss", self.dismiss_label.clone())
                            .full_width()
                            .on_click({
                                cx.listener(|this, _event, _, cx| {
                                    this.dismiss(cx);
                                })
                            }),
                    ),
            )
    }
}
