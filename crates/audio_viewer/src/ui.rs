use std::sync::Arc;

use gpui::{
    AnyElement, Bounds, Context, CursorStyle, EventEmitter, Focusable, InteractiveElement,
    IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    ParentElement, Pixels, Render, Styled, WeakEntity, Window, canvas, div, img, px, relative,
};
use i18n::tr;
use ui::{Tooltip, prelude::*};
use workspace::{
    ItemHandle, StatusItemView, ToolbarItemEvent, ToolbarItemLocation, ToolbarItemView, Workspace,
};

use crate::{
    Stop, ToggleMute, TogglePlay,
    view::{AudioView, LoadState},
    waveform::paint_waveform,
};

impl AudioView {
    fn render_content(&self, cx: &mut Context<Self>) -> AnyElement {
        match &self.load_state {
            LoadState::Loading => centered(
                Label::new(tr(cx, "audio_viewer.loading", "Loading audio...")).color(Color::Muted),
            ),
            LoadState::Error(message) => centered(
                h_flex()
                    .gap_2()
                    .child(Icon::new(IconName::Warning).color(Color::Error))
                    .child(Label::new(message.clone()).color(Color::Error)),
            ),
            LoadState::Loaded(_) => centered(self.render_player(cx)),
        }
    }

    fn render_player(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .w_full()
            .max_w_128()
            .gap_4()
            .p_6()
            .when_some(self.render_identity(cx), |this, identity| {
                this.child(identity)
            })
            .child(self.render_waveform(cx))
            .child(self.render_transport(cx))
            .when_some(self.playback_error.clone(), |this, message| {
                this.child(
                    Label::new(message)
                        .size(LabelSize::Small)
                        .color(Color::Error),
                )
            })
            .when_some(self.metadata_parts(cx), |this, parts| {
                this.child(
                    Label::new(parts.join(" • "))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
            })
            .into_any_element()
    }

    fn render_identity(&self, _cx: &mut Context<Self>) -> Option<AnyElement> {
        let cover = self.cover();
        let title = self.tag_title().map(ToOwned::to_owned);
        let artist = self.tag_artist().map(ToOwned::to_owned);
        let album = self.tag_album().map(ToOwned::to_owned);
        if cover.is_none() && title.is_none() && artist.is_none() && album.is_none() {
            return None;
        }

        let subtitle = match (artist, album) {
            (Some(artist), Some(album)) => Some(format!("{artist} • {album}")),
            (artist, album) => artist.or(album),
        };
        let details = v_flex()
            .min_w_0()
            .gap_1()
            .when_some(title, |this, title| {
                this.child(Label::new(title).single_line())
            })
            .when_some(subtitle, |this, subtitle| {
                this.child(
                    Label::new(subtitle)
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .single_line(),
                )
            });

        Some(
            h_flex()
                .w_full()
                .gap_4()
                .items_center()
                .when_some(cover, |this, cover| {
                    this.child(
                        img(cover)
                            .id("audio-cover")
                            .size(px(96.))
                            .rounded_md()
                            .object_fit(ObjectFit::Cover),
                    )
                })
                .child(details)
                .into_any_element(),
        )
    }

    fn render_waveform(&self, cx: &mut Context<Self>) -> AnyElement {
        let peaks = self
            .loaded()
            .and_then(|loaded| loaded.peaks.clone())
            .unwrap_or_else(|| Arc::from(Vec::<(f32, f32)>::new()));
        let analyzing = self.loaded().is_some_and(|loaded| loaded.analyzing);
        let playhead_ratio = self.playhead_ratio();
        let unplayed = cx.theme().colors().text.opacity(0.28);
        let played = cx.theme().status().info;
        let view = cx.entity().downgrade();

        let waveform = div()
            .id("audio-waveform")
            .w_full()
            .h(px(96.))
            .cursor(CursorStyle::PointingHand)
            .child(
                canvas(
                    move |bounds, _window, cx| {
                        view.update(cx, |view, _cx| {
                            view.waveform_bounds = Some(bounds);
                        })
                        .ok();
                    },
                    {
                        let peaks = peaks.clone();
                        move |bounds, _, window, _cx| {
                            paint_waveform(
                                window,
                                bounds,
                                &peaks,
                                playhead_ratio,
                                unplayed,
                                played,
                            );
                        }
                    },
                )
                .size_full(),
            )
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_waveform_mouse_down));

        if analyzing && peaks.is_empty() {
            div()
                .w_full()
                .relative()
                .child(waveform)
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Label::new(tr(
                                cx,
                                "audio_viewer.analyzing_waveform",
                                "Analyzing waveform...",
                            ))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                        ),
                )
                .into_any_element()
        } else {
            waveform.into_any_element()
        }
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
        let view = cx.entity().downgrade();

        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(
                IconButton::new("audio-play", play_icon)
                    .icon_size(IconSize::Small)
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action(
                            tr(cx, "audio_viewer.play_pause", "Play/Pause"),
                            &TogglePlay,
                            cx,
                        )
                    })
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.toggle_play(&TogglePlay, window, cx);
                    })),
            )
            .child(
                IconButton::new("audio-stop", IconName::Stop)
                    .icon_size(IconSize::Small)
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action(tr(cx, "audio_viewer.stop", "Stop"), &Stop, cx)
                    })
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.stop(&Stop, window, cx);
                    })),
            )
            .child(
                Label::new(time_label)
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(self.render_seek_track(playhead_ratio, view.clone(), cx))
            .child(
                IconButton::new("audio-mute", mute_icon)
                    .icon_size(IconSize::Small)
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action(tr(cx, "audio_viewer.mute", "Mute"), &ToggleMute, cx)
                    })
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.toggle_mute(&ToggleMute, window, cx);
                    })),
            )
            .child(self.render_volume_track(volume, view, cx))
    }

    fn render_seek_track(
        &self,
        ratio: f32,
        view: WeakEntity<AudioView>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let fill = cx.theme().status().info;
        let track = cx.theme().colors().elevated_surface_background;

        div()
            .id("audio-seek-track")
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
                        let ratio = AudioView::ratio_from_position(event.position, bounds);
                        view.seek_from_ratio(ratio, cx);
                    }
                }),
            )
    }

    fn render_volume_track(
        &self,
        volume: f32,
        view: WeakEntity<AudioView>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let fill = cx.theme().status().info;
        let track = cx.theme().colors().elevated_surface_background;

        div()
            .id("audio-volume-track")
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
                    .w(relative(volume.clamp(0.02, 1.0))),
            )
            .child(bounds_capture(view, |view, bounds| {
                view.volume_track_bounds = Some(bounds);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &MouseDownEvent, _window, cx| {
                    view.dragging_volume = true;
                    if let Some(bounds) = view.volume_track_bounds {
                        let ratio = AudioView::ratio_from_position(event.position, bounds);
                        view.set_volume_from_ratio(ratio, cx);
                    }
                }),
            )
    }

    fn on_waveform_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dragging_seek = true;
        if let Some(bounds) = self.waveform_bounds {
            let ratio = Self::ratio_from_position(event.position, bounds);
            self.seek_from_ratio(ratio, cx);
        }
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dragging_seek {
            let bounds = self.waveform_bounds.or(self.seek_track_bounds);
            if let Some(bounds) = bounds {
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
        self.dragging_seek = false;
        self.dragging_volume = false;
        cx.notify();
    }
}

fn bounds_capture(
    view: WeakEntity<AudioView>,
    assign: impl Fn(&mut AudioView, Bounds<Pixels>) + 'static,
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

impl Render for AudioView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = self.render_content(cx);

        div()
            .id("AudioView")
            .key_context("AudioViewer")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::toggle_play))
            .on_action(cx.listener(Self::stop))
            .on_action(cx.listener(Self::seek_forward))
            .on_action(cx.listener(Self::seek_backward))
            .on_action(cx.listener(Self::seek_to_start))
            .on_action(cx.listener(Self::seek_to_end))
            .on_action(cx.listener(Self::toggle_mute))
            .on_action(cx.listener(Self::reveal_in_file_manager))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .size_full()
            .flex()
            .bg(cx.theme().colors().editor_background)
            .child(content)
    }
}

pub struct AudioToolbarControls {
    audio_view: Option<WeakEntity<AudioView>>,
    _subscription: Option<gpui::Subscription>,
}

impl AudioToolbarControls {
    pub fn new() -> Self {
        Self {
            audio_view: None,
            _subscription: None,
        }
    }
}

impl Default for AudioToolbarControls {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for AudioToolbarControls {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(audio_view) = self.audio_view.as_ref().and_then(|view| view.upgrade()) else {
            return div().into_any_element();
        };
        let view = audio_view.read(cx);
        let playing = view.is_playing();
        let time_label = view.current_time_label();
        let play_icon = if playing {
            IconName::DebugPause
        } else {
            IconName::PlayFilled
        };
        let weak = audio_view.downgrade();

        h_flex()
            .gap_1()
            .child(
                IconButton::new("audio-toolbar-play", play_icon)
                    .icon_size(IconSize::Small)
                    .tooltip(move |_window, cx| {
                        Tooltip::for_action(
                            tr(cx, "audio_viewer.play_pause", "Play/Pause"),
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

impl EventEmitter<ToolbarItemEvent> for AudioToolbarControls {}

impl ToolbarItemView for AudioToolbarControls {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn ItemHandle>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ToolbarItemLocation {
        self.audio_view = None;
        self._subscription = None;

        if let Some(item) = active_pane_item.and_then(|item| item.downcast::<AudioView>()) {
            self._subscription = Some(cx.observe(&item, |_, _, cx| cx.notify()));
            self.audio_view = Some(item.downgrade());
            cx.notify();
            return ToolbarItemLocation::PrimaryRight;
        }

        ToolbarItemLocation::Hidden
    }
}

pub struct AudioInfo {
    parts: Option<Vec<String>>,
    _observe_active: Option<gpui::Subscription>,
}

impl AudioInfo {
    pub fn new(_workspace: &Workspace) -> Self {
        Self {
            parts: None,
            _observe_active: None,
        }
    }
}

impl Render for AudioInfo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let Some(parts) = self.parts.as_ref() else {
            return div().hidden();
        };

        div().child(Label::new(parts.join(" • ")).size(LabelSize::Small))
    }
}

impl StatusItemView for AudioInfo {
    fn set_active_pane_item(
        &mut self,
        active_pane_item: Option<&dyn ItemHandle>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self._observe_active = None;

        if let Some(audio_view) = active_pane_item.and_then(|item| item.act_as::<AudioView>(cx)) {
            self.parts = audio_view.read(cx).metadata_parts(cx);
            self._observe_active = Some(cx.observe(&audio_view, |this, view, cx| {
                this.parts = view.read(cx).metadata_parts(cx);
                cx.notify();
            }));
        } else {
            self.parts = None;
        }
        cx.notify();
    }
}
