use gpui::{
    AnyElement, Bounds, Context, CursorStyle, EventEmitter, Focusable, InteractiveElement,
    IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    ParentElement, Pixels, Render, Styled, WeakEntity, Window, canvas, div, img, px, relative,
};
use i18n::tr;
use ui::{IconName, Label, LabelSize, Tooltip, prelude::*};
use workspace::{
    ItemHandle, StatusItemView, ToolbarItemEvent, ToolbarItemLocation, ToolbarItemView, Workspace,
};

use crate::{
    DecreaseSpeed, DecreaseVolume, IncreaseSpeed, IncreaseVolume, Stop, ToggleLoop, ToggleMute,
    TogglePlay,
    view::{LoadState, MAX_VOLUME, VideoView},
};

impl VideoView {
    fn render_content(&self, cx: &mut Context<Self>) -> AnyElement {
        match &self.load_state {
            LoadState::Loading => centered(
                Label::new(tr(cx, "video_viewer.loading", "Loading video...")).color(Color::Muted),
            ),
            LoadState::Error(message) => centered(
                h_flex()
                    .gap_2()
                    .child(Icon::new(IconName::Warning).color(Color::Error))
                    .child(Label::new(message.clone()).color(Color::Error)),
            ),
            LoadState::Loaded(_) => self.render_player(cx),
        }
    }

    fn render_player(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .size_full()
            .child(self.render_stage(cx))
            .child(self.render_transport(cx))
            .into_any_element()
    }

    fn render_stage(&self, cx: &mut Context<Self>) -> AnyElement {
        let has_frame = self.current_frame.is_some();
        let playing = self.is_playing();

        let mut stage = div()
            .id("video-stage")
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor(CursorStyle::PointingHand)
            .bg(cx.theme().colors().editor_background)
            .on_click(cx.listener(|view, _, window, cx| {
                let focus_handle = view.focus_handle(cx);
                window.focus(&focus_handle, cx);
                view.toggle_play(&TogglePlay, window, cx);
            }));

        match self.current_frame.clone() {
            Some(frame) => {
                stage = stage.child(
                    img(frame)
                        .id("video-frame")
                        .max_w_full()
                        .max_h_full()
                        .object_fit(ObjectFit::Contain),
                );
            }
            None => {
                stage = stage.child(
                    Icon::new(IconName::File)
                        .size(IconSize::XLarge)
                        .color(Color::Muted),
                );
            }
        }

        if has_frame && !playing {
            stage = stage.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        Icon::new(IconName::PlayFilled)
                            .size(IconSize::XLarge)
                            .color(Color::Muted),
                    ),
            );
        }

        if let Some(error) = self.playback_error.clone() {
            stage = stage.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .p_2()
                    .flex()
                    .justify_center()
                    .child(Label::new(error).size(LabelSize::Small).color(Color::Error)),
            );
        }

        stage.into_any_element()
    }

    fn render_transport(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let playing = self.is_playing();
        let play_icon = if playing {
            IconName::DebugPause
        } else {
            IconName::PlayFilled
        };
        let mute_icon = if self.muted {
            IconName::AudioOff
        } else {
            IconName::AudioOn
        };
        let time_label = self.current_time_label();
        let playhead_ratio = self.playhead_ratio();
        let volume = self.effective_volume();
        let looping = self.is_looping();
        let speed_label = self.speed_label();
        let view = cx.entity().downgrade();

        h_flex()
            .w_full()
            .px_3()
            .py_2()
            .gap_2()
            .items_center()
            .child(
                IconButton::new("video-play", play_icon)
                    .icon_size(IconSize::Small)
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action(
                            tr(cx, "video_viewer.play_pause", "Play/Pause"),
                            &TogglePlay,
                            cx,
                        )
                    })
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.toggle_play(&TogglePlay, window, cx);
                    })),
            )
            .child(
                IconButton::new("video-stop", IconName::Stop)
                    .icon_size(IconSize::Small)
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action(tr(cx, "video_viewer.stop", "Stop"), &Stop, cx)
                    })
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.stop(&Stop, window, cx);
                    })),
            )
            .child(
                IconButton::new("video-step-backward", IconName::ChevronLeft)
                    .icon_size(IconSize::Small)
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action(
                            tr(cx, "video_viewer.step_backward", "Previous frame"),
                            &crate::StepBackward,
                            cx,
                        )
                    })
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.step_backward(&crate::StepBackward, window, cx);
                    })),
            )
            .child(
                IconButton::new("video-step-forward", IconName::ChevronRight)
                    .icon_size(IconSize::Small)
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action(
                            tr(cx, "video_viewer.step_forward", "Next frame"),
                            &crate::StepForward,
                            cx,
                        )
                    })
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.step_forward(&crate::StepForward, window, cx);
                    })),
            )
            .child(
                Label::new(time_label)
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(self.render_seek_track(playhead_ratio, view.clone(), cx))
            .child(
                IconButton::new("video-speed-down", IconName::Dash)
                    .icon_size(IconSize::Small)
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action(
                            tr(cx, "video_viewer.speed_down", "Decrease speed"),
                            &DecreaseSpeed,
                            cx,
                        )
                    })
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.decrease_speed(&DecreaseSpeed, window, cx);
                    })),
            )
            .child(
                IconButton::new("video-speed-reset", IconName::RotateCcw)
                    .icon_size(IconSize::Small)
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action(
                            tr(cx, "video_viewer.speed_reset", "Reset speed"),
                            &crate::ResetSpeed,
                            cx,
                        )
                    })
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.reset_speed(&crate::ResetSpeed, window, cx);
                    })),
            )
            .child(
                IconButton::new("video-speed-up", IconName::Plus)
                    .icon_size(IconSize::Small)
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action(
                            tr(cx, "video_viewer.speed_up", "Increase speed"),
                            &IncreaseSpeed,
                            cx,
                        )
                    })
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.increase_speed(&IncreaseSpeed, window, cx);
                    })),
            )
            .child(
                Label::new(speed_label)
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                IconButton::new("video-loop", IconName::RefreshTitle)
                    .icon_size(IconSize::Small)
                    .toggle_state(looping)
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action(tr(cx, "video_viewer.loop", "Loop"), &ToggleLoop, cx)
                    })
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.toggle_loop(&ToggleLoop, window, cx);
                    })),
            )
            .child(
                IconButton::new("video-mute", mute_icon)
                    .icon_size(IconSize::Small)
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action(tr(cx, "video_viewer.mute", "Mute"), &ToggleMute, cx)
                    })
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.toggle_mute(&ToggleMute, window, cx);
                    })),
            )
            .child(
                IconButton::new("video-volume-down", IconName::Dash)
                    .icon_size(IconSize::Small)
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action(
                            tr(cx, "video_viewer.volume_down", "Decrease volume"),
                            &DecreaseVolume,
                            cx,
                        )
                    })
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.decrease_volume(&DecreaseVolume, window, cx);
                    })),
            )
            .child(self.render_volume_track(volume, view, cx))
            .child(
                IconButton::new("video-volume-up", IconName::Plus)
                    .icon_size(IconSize::Small)
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action(
                            tr(cx, "video_viewer.volume_up", "Increase volume"),
                            &IncreaseVolume,
                            cx,
                        )
                    })
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.increase_volume(&IncreaseVolume, window, cx);
                    })),
            )
    }

    fn render_seek_track(
        &self,
        ratio: f32,
        view: WeakEntity<VideoView>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let fill = cx.theme().status().info;
        let track = cx.theme().colors().elevated_surface_background;

        div()
            .id("video-seek-track")
            .flex_1()
            .min_w_0()
            .h_2()
            .rounded_full()
            .cursor(CursorStyle::PointingHand)
            .bg(track)
            .relative()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .h_full()
                    .rounded_full()
                    .bg(fill)
                    .w(relative(ratio.clamp(0.02, 1.0))),
            )
            .child(bounds_capture(view, |view, bounds| {
                view.seek_track_bounds = Some(bounds);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &MouseDownEvent, _window, cx| {
                    view.dragging_seek = true;
                    if let Some(bounds) = view.seek_track_bounds {
                        let ratio = VideoView::ratio_from_position(event.position, bounds);
                        view.seek_from_ratio(ratio, cx);
                    }
                }),
            )
    }

    fn render_volume_track(
        &self,
        volume: f32,
        view: WeakEntity<VideoView>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let fill = cx.theme().status().info;
        let track = cx.theme().colors().elevated_surface_background;

        div()
            .id("video-volume-track")
            .w(px(72.))
            .h_2()
            .rounded_full()
            .cursor(CursorStyle::PointingHand)
            .bg(track)
            .relative()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .h_full()
                    .rounded_full()
                    .bg(fill)
                    .w(relative((volume / MAX_VOLUME).clamp(0.02, 1.0))),
            )
            .child(bounds_capture(view, |view, bounds| {
                view.volume_track_bounds = Some(bounds);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &MouseDownEvent, _window, cx| {
                    view.dragging_volume = true;
                    if let Some(bounds) = view.volume_track_bounds {
                        let ratio = VideoView::ratio_from_position(event.position, bounds);
                        view.set_volume_from_ratio(ratio, cx);
                    }
                }),
            )
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dragging_seek {
            if let Some(bounds) = self.seek_track_bounds {
                let ratio = Self::ratio_from_position(event.position, bounds);
                self.seek_from_ratio(ratio, cx);
            }
        } else if self.dragging_volume
            && let Some(bounds) = self.volume_track_bounds
        {
            let ratio = Self::ratio_from_position(event.position, bounds);
            self.set_volume_from_ratio(ratio, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let committed_seek = self.dragging_seek;
        self.dragging_seek = false;
        self.dragging_volume = false;
        if committed_seek {
            self.seek_to_position(cx);
        } else {
            cx.notify();
        }
    }
}

fn bounds_capture(
    view: WeakEntity<VideoView>,
    assign: impl Fn(&mut VideoView, Bounds<Pixels>) + 'static,
) -> impl IntoElement {
    canvas(
        move |bounds, _window, cx| {
            view.update(cx, |view, _cx| assign(view, bounds)).ok();
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

fn centered(child: impl IntoElement) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size_full()
        .child(child)
        .into_any_element()
}

impl Render for VideoView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = self.render_content(cx);

        div()
            .id("VideoView")
            .key_context("VideoViewer")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::toggle_play))
            .on_action(cx.listener(Self::stop))
            .on_action(cx.listener(Self::seek_forward))
            .on_action(cx.listener(Self::seek_backward))
            .on_action(cx.listener(Self::seek_to_start))
            .on_action(cx.listener(Self::seek_to_end))
            .on_action(cx.listener(Self::step_forward))
            .on_action(cx.listener(Self::step_backward))
            .on_action(cx.listener(Self::toggle_mute))
            .on_action(cx.listener(Self::increase_volume))
            .on_action(cx.listener(Self::decrease_volume))
            .on_action(cx.listener(Self::increase_speed))
            .on_action(cx.listener(Self::decrease_speed))
            .on_action(cx.listener(Self::reset_speed))
            .on_action(cx.listener(Self::toggle_loop))
            .on_action(cx.listener(Self::reveal_in_file_manager))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .size_full()
            .flex()
            .bg(cx.theme().colors().editor_background)
            .child(content)
    }
}

pub struct VideoToolbarControls {
    video_view: Option<WeakEntity<VideoView>>,
    _subscription: Option<gpui::Subscription>,
}

impl VideoToolbarControls {
    pub fn new() -> Self {
        Self {
            video_view: None,
            _subscription: None,
        }
    }
}

impl Default for VideoToolbarControls {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for VideoToolbarControls {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(video_view) = self.video_view.as_ref().and_then(|view| view.upgrade()) else {
            return div().into_any_element();
        };
        let view = video_view.read(cx);
        let playing = view.is_playing();
        let time_label = view.current_time_label();
        let play_icon = if playing {
            IconName::DebugPause
        } else {
            IconName::PlayFilled
        };
        let weak = video_view.downgrade();

        h_flex()
            .gap_1()
            .child(
                IconButton::new("video-toolbar-play", play_icon)
                    .icon_size(IconSize::Small)
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action(
                            tr(cx, "video_viewer.play_pause", "Play/Pause"),
                            &TogglePlay,
                            cx,
                        )
                    })
                    .on_click(move |_, window, cx| {
                        if let Some(view) = weak.upgrade() {
                            view.update(cx, |view, cx| view.toggle_play(&TogglePlay, window, cx));
                        }
                    }),
            )
            .child(Label::new(time_label).size(LabelSize::Small))
            .into_any_element()
    }
}

impl EventEmitter<ToolbarItemEvent> for VideoToolbarControls {}

impl ToolbarItemView for VideoToolbarControls {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn ItemHandle>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ToolbarItemLocation {
        self.video_view = None;
        self._subscription = None;

        if let Some(item) = active_pane_item.and_then(|item| item.downcast::<VideoView>()) {
            self._subscription = Some(cx.observe(&item, |_, _, cx| cx.notify()));
            self.video_view = Some(item.downgrade());
            cx.notify();
            return ToolbarItemLocation::PrimaryRight;
        }

        ToolbarItemLocation::Hidden
    }
}

pub struct VideoInfo {
    parts: Option<Vec<String>>,
    _observe_active: Option<gpui::Subscription>,
}

impl VideoInfo {
    pub fn new(_workspace: &Workspace) -> Self {
        Self {
            parts: None,
            _observe_active: None,
        }
    }
}

impl Render for VideoInfo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let Some(parts) = self.parts.as_ref() else {
            return div().hidden();
        };

        div().child(Label::new(parts.join(" • ")).size(LabelSize::Small))
    }
}

impl StatusItemView for VideoInfo {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn ItemHandle>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self._observe_active = None;

        if let Some(video_view) = active_pane_item.and_then(|item| item.act_as::<VideoView>(cx)) {
            self.parts = video_view.read(cx).metadata_parts(cx);
            self._observe_active = Some(cx.observe(&video_view, |this, view, cx| {
                this.parts = view.read(cx).metadata_parts(cx);
                cx.notify();
            }));
        } else {
            self.parts = None;
        }
        cx.notify();
    }
}
