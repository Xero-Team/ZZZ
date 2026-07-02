use call::{ActiveCall, Room, room};
use gpui::{
    DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, Render, Subscription,
    Window,
};
use i18n::tr;
use livekit_client::ConnectionQuality;
use ui::prelude::*;
use workspace::{ModalView, Workspace};
use zed_actions::ShowCallStats;

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _cx| {
        workspace.register_action(|workspace, _: &ShowCallStats, window, cx| {
            workspace.toggle_modal(window, cx, |_window, cx| CallStatsModal::new(cx));
        });
    })
    .detach();
}

pub struct CallStatsModal {
    focus_handle: FocusHandle,
    _active_call_subscription: Option<Subscription>,
    _diagnostics_subscription: Option<Subscription>,
}

impl CallStatsModal {
    fn new(cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            _active_call_subscription: None,
            _diagnostics_subscription: None,
        };

        if let Some(active_call) = ActiveCall::try_global(cx) {
            this._active_call_subscription =
                Some(cx.subscribe(&active_call, Self::handle_call_event));
            this.observe_diagnostics(cx);
        }

        this
    }

    fn observe_diagnostics(&mut self, cx: &mut Context<Self>) {
        let diagnostics = active_room(cx).and_then(|room| room.read(cx).diagnostics().cloned());

        if let Some(diagnostics) = diagnostics {
            self._diagnostics_subscription = Some(cx.observe(&diagnostics, |_, _, cx| cx.notify()));
        } else {
            self._diagnostics_subscription = None;
        }
    }

    fn handle_call_event(
        &mut self,
        _: Entity<ActiveCall>,
        event: &room::Event,
        cx: &mut Context<Self>,
    ) {
        match event {
            room::Event::RoomJoined { .. } => {
                self.observe_diagnostics(cx);
            }
            room::Event::RoomLeft { .. } => {
                self._diagnostics_subscription = None;
                cx.notify();
            }
            _ => {}
        }
    }

    fn dismiss(&mut self, _: &menu::Cancel, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }
}

fn active_room(cx: &App) -> Option<Entity<Room>> {
    ActiveCall::try_global(cx)?.read(cx).room().cloned()
}

fn quality_label(quality: Option<ConnectionQuality>, cx: &App) -> (String, Color) {
    match quality {
        Some(ConnectionQuality::Excellent) => (
            tr(cx, "collab_ui.call_stats.quality.excellent", "Excellent"),
            Color::Success,
        ),
        Some(ConnectionQuality::Good) => (
            tr(cx, "collab_ui.call_stats.quality.good", "Good"),
            Color::Success,
        ),
        Some(ConnectionQuality::Poor) => (
            tr(cx, "collab_ui.call_stats.quality.poor", "Poor"),
            Color::Warning,
        ),
        Some(ConnectionQuality::Lost) => (
            tr(cx, "collab_ui.call_stats.quality.lost", "Lost"),
            Color::Error,
        ),
        None => ("—".to_owned(), Color::Muted),
    }
}

fn rating_label(
    cx: &App,
    key: &'static str,
    fallback: &'static str,
    color: Color,
) -> (String, Color) {
    (tr(cx, key, fallback), color)
}

fn metric_rating(label: &str, value_ms: f64, cx: &App) -> (String, Color) {
    match label {
        "Latency" => {
            if value_ms < 100.0 {
                rating_label(
                    cx,
                    "collab_ui.call_stats.rating.normal",
                    "Normal",
                    Color::Success,
                )
            } else if value_ms < 300.0 {
                rating_label(
                    cx,
                    "collab_ui.call_stats.rating.high",
                    "High",
                    Color::Warning,
                )
            } else {
                rating_label(cx, "collab_ui.call_stats.rating.poor", "Poor", Color::Error)
            }
        }
        "Jitter" => {
            if value_ms < 30.0 {
                rating_label(
                    cx,
                    "collab_ui.call_stats.rating.normal",
                    "Normal",
                    Color::Success,
                )
            } else if value_ms < 75.0 {
                rating_label(
                    cx,
                    "collab_ui.call_stats.rating.high",
                    "High",
                    Color::Warning,
                )
            } else {
                rating_label(cx, "collab_ui.call_stats.rating.poor", "Poor", Color::Error)
            }
        }
        _ => rating_label(
            cx,
            "collab_ui.call_stats.rating.normal",
            "Normal",
            Color::Success,
        ),
    }
}

fn input_lag_rating(value_ms: f64, cx: &App) -> (String, Color) {
    if value_ms < 20.0 {
        rating_label(
            cx,
            "collab_ui.call_stats.rating.normal",
            "Normal",
            Color::Success,
        )
    } else if value_ms < 50.0 {
        rating_label(
            cx,
            "collab_ui.call_stats.rating.high",
            "High",
            Color::Warning,
        )
    } else {
        rating_label(cx, "collab_ui.call_stats.rating.poor", "Poor", Color::Error)
    }
}

fn packet_loss_rating(loss_pct: f64, cx: &App) -> (String, Color) {
    if loss_pct < 1.0 {
        rating_label(
            cx,
            "collab_ui.call_stats.rating.normal",
            "Normal",
            Color::Success,
        )
    } else if loss_pct < 5.0 {
        rating_label(
            cx,
            "collab_ui.call_stats.rating.high",
            "High",
            Color::Warning,
        )
    } else {
        rating_label(cx, "collab_ui.call_stats.rating.poor", "Poor", Color::Error)
    }
}

impl EventEmitter<DismissEvent> for CallStatsModal {}
impl ModalView for CallStatsModal {}

impl Focusable for CallStatsModal {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CallStatsModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let room = active_room(cx);
        let is_connected = room.is_some();
        let stats = room
            .and_then(|room| {
                let diagnostics = room.read(cx).diagnostics()?;
                Some(diagnostics.read(cx).stats().clone())
            })
            .unwrap_or_default();

        let (quality_text, quality_color) = quality_label(stats.connection_quality, cx);

        v_flex()
            .key_context("CallStatsModal")
            .on_action(cx.listener(Self::dismiss))
            .track_focus(&self.focus_handle)
            .elevation_3(cx)
            .w(rems(24.))
            .p_4()
            .gap_3()
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        Label::new(tr(cx, "collab_ui.call_stats.title", "Call Diagnostics"))
                            .size(LabelSize::Large),
                    )
                    .child(
                        Label::new(quality_text)
                            .size(LabelSize::Large)
                            .color(quality_color),
                    ),
            )
            .when(!is_connected, |this| {
                this.child(
                    h_flex().justify_center().py_4().child(
                        Label::new(tr(cx, "collab_ui.call_stats.not_in_call", "Not in a call"))
                            .color(Color::Muted),
                    ),
                )
            })
            .when(is_connected, |this| {
                this.child(
                    v_flex()
                        .gap_1()
                        .child(
                            h_flex().gap_2().child(
                                Label::new(tr(cx, "collab_ui.call_stats.network", "Network"))
                                    .weight(FontWeight::SEMIBOLD),
                            ),
                        )
                        .child(
                            self.render_metric_row(
                                tr(cx, "collab_ui.call_stats.metric.latency", "Latency").into(),
                                tr(
                                    cx,
                                    "collab_ui.call_stats.description.latency",
                                    "Time for data to travel to the server",
                                )
                                .into(),
                                stats.latency_ms,
                                |v| format!("{:.0}ms", v),
                                |v| metric_rating("Latency", v, cx),
                            ),
                        )
                        .child(
                            self.render_metric_row(
                                tr(cx, "collab_ui.call_stats.metric.jitter", "Jitter").into(),
                                tr(
                                    cx,
                                    "collab_ui.call_stats.description.jitter",
                                    "Variance or fluctuation in latency",
                                )
                                .into(),
                                stats.jitter_ms,
                                |v| format!("{:.0}ms", v),
                                |v| metric_rating("Jitter", v, cx),
                            ),
                        )
                        .child(
                            self.render_metric_row(
                                tr(cx, "collab_ui.call_stats.metric.packet_loss", "Packet loss")
                                    .into(),
                                tr(
                                    cx,
                                    "collab_ui.call_stats.description.packet_loss",
                                    "Amount of data lost during transfer",
                                )
                                .into(),
                                stats.packet_loss_pct,
                                |v| format!("{:.1}%", v),
                                |v| packet_loss_rating(v, cx),
                            ),
                        )
                        .child(
                            self.render_metric_row(
                                tr(cx, "collab_ui.call_stats.metric.input_lag", "Input lag").into(),
                                tr(
                                    cx,
                                    "collab_ui.call_stats.description.input_lag",
                                    "Delay from audio capture to WebRTC",
                                )
                                .into(),
                                stats.input_lag.map(|d| d.as_secs_f64() * 1000.0),
                                |v| format!("{:.1}ms", v),
                                |v| input_lag_rating(v, cx),
                            ),
                        ),
                )
            })
    }
}

impl CallStatsModal {
    fn render_metric_row(
        &self,
        title: SharedString,
        description: SharedString,
        value: Option<f64>,
        format_value: impl Fn(f64) -> String,
        rate: impl Fn(f64) -> (String, Color),
    ) -> impl IntoElement {
        let (rating_text, rating_color, value_text) = match value {
            Some(v) => {
                let (rt, rc) = rate(v);
                (rt, rc, format_value(v))
            }
            None => ("—".to_owned(), Color::Muted, "—".to_owned()),
        };

        h_flex()
            .px_2()
            .py_1()
            .rounded_md()
            .justify_between()
            .child(
                v_flex()
                    .child(Label::new(title).size(LabelSize::Default))
                    .child(
                        Label::new(description)
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
            .child(
                v_flex()
                    .items_end()
                    .child(
                        Label::new(rating_text)
                            .size(LabelSize::Default)
                            .color(rating_color),
                    )
                    .child(
                        Label::new(value_text)
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
            )
    }
}
