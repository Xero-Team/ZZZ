use std::rc::Rc;

use call::ActiveCall;
use channel::ChannelStore;
use gpui::{AnyElement, IntoElement, ScreenCaptureSource, Styled, WeakEntity};
use gpui::{App, Task, Window};
use i18n::tr;
use icons::IconName;
use livekit_client::ConnectionQuality;
use project::WorktreeSettings;
use remote_connection::RemoteConnectionModal;
use rpc::proto::{self};
use settings::{Settings as _, SettingsLocation};
use ui::{
    ContextMenu, ContextMenuItem, Divider, DividerColor, PopoverMenu, SplitButton,
    SplitButtonStyle, TintColor, Tooltip, prelude::*,
};
use util::rel_path::RelPath;
use workspace::notifications::DetachAndPromptErr;
use zed_actions::ShowCallStats;

use crate::TitleBar;

fn format_stat(value: Option<f64>, format: impl Fn(f64) -> String) -> String {
    match value {
        Some(v) => format(v),
        None => "—".to_owned(),
    }
}

pub fn toggle_screen_sharing(
    screen: anyhow::Result<Option<Rc<dyn ScreenCaptureSource>>>,
    window: &mut Window,
    cx: &mut App,
) {
    let call = ActiveCall::global(cx).read(cx);
    let toggle_screen_sharing = match screen {
        Ok(screen) => {
            let Some(room) = call.room().cloned() else {
                return;
            };

            room.update(cx, |room, cx| {
                let clicked_on_currently_shared_screen =
                    room.shared_screen_id().is_some_and(|screen_id| {
                        Some(screen_id)
                            == screen
                                .as_deref()
                                .and_then(|s| s.metadata().ok().map(|meta| meta.id))
                    });
                let should_unshare_current_screen = room.is_sharing_screen();
                let unshared_current_screen = should_unshare_current_screen.then(|| {
                    room.unshare_screen(clicked_on_currently_shared_screen || screen.is_none(), cx)
                });
                if let Some(screen) = screen {
                    cx.spawn(async move |room, cx| {
                        unshared_current_screen.transpose()?;
                        if !clicked_on_currently_shared_screen {
                            room.update(cx, |room, cx| room.share_screen(screen, cx))?
                                .await
                        } else {
                            Ok(())
                        }
                    })
                } else {
                    Task::ready(Ok(()))
                }
            })
        }
        Err(e) => Task::ready(Err(e)),
    };
    toggle_screen_sharing.detach_and_prompt_err(
        &tr(
            cx,
            "title_bar.collab.sharing_screen_failed",
            "Sharing Screen Failed",
        ),
        window,
        cx,
        |e, _, cx| {
            Some(
                tr(
                    cx,
                    "title_bar.collab.sharing_screen_failed_detail",
                    "{:?}\n\nPlease check that you have given ZZZ permissions to record your screen in Settings.",
                )
                .replacen("{:?}", &format!("{:?}", e), 1),
            )
        },
    );
}

pub fn toggle_mute(cx: &mut App) {
    let call = ActiveCall::global(cx).read(cx);
    if let Some(room) = call.room().cloned() {
        room.update(cx, |room, cx| room.toggle_mute(cx));
    }
}

pub fn toggle_deafen(cx: &mut App) {
    if let Some(room) = ActiveCall::global(cx).read(cx).room().cloned() {
        room.update(cx, |room, cx| room.toggle_deafen(cx));
    }
}

impl TitleBar {
    pub(crate) fn render_call_controls(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(room) = ActiveCall::global(cx).read(cx).room().cloned() else {
            return Vec::new();
        };

        let is_connecting_to_project = self
            .workspace
            .update(cx, |workspace, cx| {
                workspace
                    .active_modal::<RemoteConnectionModal>(cx)
                    .is_some()
            })
            .unwrap_or(false);

        let room = room.read(cx);
        let project = self.project.read(cx);
        let is_local = project.is_local() || project.is_via_remote_server();
        let is_shared = is_local && project.is_shared();
        let is_muted = room.is_muted();
        let muted_by_user = room.muted_by_user();
        let is_deafened = room.is_deafened().unwrap_or(false);
        let is_screen_sharing = room.is_sharing_screen();
        let can_use_microphone = room.can_use_microphone();
        let can_share_projects = room.can_share_projects();
        let screen_sharing_supported = cx.is_screen_capture_supported();

        let stats = room
            .diagnostics()
            .map(|d| d.read(cx).stats().clone())
            .unwrap_or_default();

        let channel_store = ChannelStore::global(cx);
        let channel = room
            .channel_id()
            .and_then(|channel_id| channel_store.read(cx).channel_for_id(channel_id).cloned());

        let mut children = Vec::new();

        let effective_quality = stats.effective_quality.unwrap_or(ConnectionQuality::Lost);
        let (signal_icon, signal_color, quality_label) = match effective_quality {
            ConnectionQuality::Excellent => (
                IconName::SignalHigh,
                Some(Color::Success),
                tr(cx, "title_bar.collab.quality.excellent", "Excellent"),
            ),
            ConnectionQuality::Good => (
                IconName::SignalHigh,
                None,
                tr(cx, "title_bar.collab.quality.good", "Good"),
            ),
            ConnectionQuality::Poor => (
                IconName::SignalMedium,
                Some(Color::Warning),
                tr(cx, "title_bar.collab.quality.poor", "Poor"),
            ),
            ConnectionQuality::Lost => (
                IconName::SignalLow,
                Some(Color::Error),
                tr(cx, "title_bar.collab.quality.lost", "Lost"),
            ),
        };

        let quality_label: SharedString = quality_label.into();

        children.push(
            h_flex()
                .gap_1()
                .child(
                    IconButton::new("leave-call", IconName::Exit)
                        .style(ButtonStyle::Subtle)
                        .tooltip(Tooltip::text(tr(
                            cx,
                            "title_bar.collab.leave_call",
                            "Leave Call",
                        )))
                        .icon_size(IconSize::Small)
                        .on_click(move |_, _window, cx| {
                            ActiveCall::global(cx)
                                .update(cx, |call, cx| call.hang_up(cx))
                                .detach_and_log_err(cx);
                        }),
                )
                .child(Divider::vertical().color(DividerColor::Border))
                .into_any_element(),
        );

        children.push(
            IconButton::new("call-quality", signal_icon)
                .icon_size(IconSize::Small)
                .when_some(signal_color, |button, color| button.icon_color(color))
                .tooltip(move |_window, cx| {
                    let quality_label = quality_label.clone();
                    let latency = format_stat(stats.latency_ms, |v| format!("{:.0}ms", v));
                    let jitter = format_stat(stats.jitter_ms, |v| format!("{:.0}ms", v));
                    let packet_loss = format_stat(stats.packet_loss_pct, |v| format!("{:.1}%", v));
                    let input_lag =
                        format_stat(stats.input_lag.map(|d| d.as_secs_f64() * 1000.0), |v| {
                            format!("{:.1}ms", v)
                        });

                    Tooltip::with_meta(
                        tr(cx, "title_bar.collab.connection_status", "Connection: {}").replacen(
                            "{}",
                            &quality_label,
                            1,
                        ),
                        Some(&ShowCallStats),
                        tr(
                            cx,
                            "title_bar.collab.connection_stats",
                            "Latency: {} · Jitter: {} · Loss: {} · Input lag: {}",
                        )
                        .replacen("{}", &latency, 1)
                        .replacen("{}", &jitter, 1)
                        .replacen("{}", &packet_loss, 1)
                        .replacen("{}", &input_lag, 1),
                        cx,
                    )
                })
                .on_click(move |_, window, cx| {
                    window.dispatch_action(Box::new(ShowCallStats), cx);
                })
                .into_any_element(),
        );

        if is_local && can_share_projects && !is_connecting_to_project {
            let is_sharing_disabled = channel.is_some_and(|channel| match channel.visibility {
                proto::ChannelVisibility::Public => project.visible_worktrees(cx).any(|worktree| {
                    let worktree_id = worktree.read(cx).id();

                    let settings_location = Some(SettingsLocation {
                        worktree_id,
                        path: RelPath::empty(),
                    });

                    WorktreeSettings::get(settings_location, cx).prevent_sharing_in_public_channels
                }),
                proto::ChannelVisibility::Members => false,
            });

            children.push(
                Button::new(
                    "toggle_sharing",
                    if is_shared {
                        tr(cx, "title_bar.collab.unshare", "Unshare")
                    } else {
                        tr(cx, "title_bar.collab.share", "Share")
                    },
                )
                .tooltip(Tooltip::text(if is_shared {
                    tr(
                        cx,
                        "title_bar.collab.stop_sharing_project",
                        "Stop sharing project with call participants",
                    )
                } else {
                    tr(
                        cx,
                        "title_bar.collab.share_project",
                        "Share project with call participants",
                    )
                }))
                .style(ButtonStyle::Subtle)
                .selected_style(ButtonStyle::Tinted(TintColor::Accent))
                .toggle_state(is_shared)
                .label_size(LabelSize::Small)
                .when(is_sharing_disabled, |parent| {
                    parent.disabled(true).tooltip(Tooltip::text(tr(
                        cx,
                        "title_bar.collab.project_may_not_be_shared",
                        "This project may not be shared in a public channel.",
                    )))
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    if is_shared {
                        this.unshare_project(window, cx);
                    } else {
                        this.share_project(cx);
                    }
                }))
                .into_any_element(),
            );
        }

        if can_use_microphone {
            children.push(
                IconButton::new(
                    "mute-microphone",
                    if is_muted {
                        IconName::MicMute
                    } else {
                        IconName::Mic
                    },
                )
                .tooltip(move |_window, cx| {
                    if is_muted {
                        if is_deafened {
                            Tooltip::with_meta(
                                tr(
                                    cx,
                                    "title_bar.collab.unmute_microphone",
                                    "Unmute Microphone",
                                ),
                                None,
                                tr(
                                    cx,
                                    "title_bar.collab.audio_will_be_unmuted",
                                    "Audio will be unmuted",
                                ),
                                cx,
                            )
                        } else {
                            Tooltip::simple(
                                tr(
                                    cx,
                                    "title_bar.collab.unmute_microphone",
                                    "Unmute Microphone",
                                ),
                                cx,
                            )
                        }
                    } else {
                        Tooltip::simple(
                            tr(cx, "title_bar.collab.mute_microphone", "Mute Microphone"),
                            cx,
                        )
                    }
                })
                .style(ButtonStyle::Subtle)
                .icon_size(IconSize::Small)
                .toggle_state(is_muted)
                .selected_style(ButtonStyle::Tinted(TintColor::Error))
                .on_click(move |_, _window, cx| toggle_mute(cx))
                .into_any_element(),
            );
        }

        children.push(
            IconButton::new(
                "mute-sound",
                if is_deafened {
                    IconName::AudioOff
                } else {
                    IconName::AudioOn
                },
            )
            .style(ButtonStyle::Subtle)
            .selected_style(ButtonStyle::Tinted(TintColor::Error))
            .icon_size(IconSize::Small)
            .toggle_state(is_deafened)
            .tooltip(move |_window, cx| {
                if is_deafened {
                    let label = tr(cx, "title_bar.collab.unmute_audio", "Unmute Audio");

                    if !muted_by_user {
                        Tooltip::with_meta(
                            label,
                            None,
                            tr(
                                cx,
                                "title_bar.collab.microphone_will_be_unmuted",
                                "Microphone will be unmuted",
                            ),
                            cx,
                        )
                    } else {
                        Tooltip::simple(label, cx)
                    }
                } else {
                    let label = tr(cx, "title_bar.collab.mute_audio", "Mute Audio");

                    if !muted_by_user {
                        Tooltip::with_meta(
                            label,
                            None,
                            tr(
                                cx,
                                "title_bar.collab.microphone_will_be_muted",
                                "Microphone will be muted",
                            ),
                            cx,
                        )
                    } else {
                        Tooltip::simple(label, cx)
                    }
                }
            })
            .on_click(move |_, _, cx| toggle_deafen(cx))
            .into_any_element(),
        );

        if can_use_microphone && screen_sharing_supported {
            #[cfg(target_os = "linux")]
            let is_wayland = gpui::guess_compositor() == "Wayland";
            #[cfg(not(target_os = "linux"))]
            let is_wayland = false;

            let trigger = IconButton::new("screen-share", IconName::Screen)
                .style(ButtonStyle::Subtle)
                .icon_size(IconSize::Small)
                .toggle_state(is_screen_sharing)
                .selected_style(ButtonStyle::Tinted(TintColor::Accent))
                .tooltip(Tooltip::text(if is_screen_sharing {
                    tr(
                        cx,
                        "title_bar.collab.stop_sharing_screen",
                        "Stop Sharing Screen",
                    )
                } else {
                    tr(cx, "title_bar.collab.share_screen", "Share Screen")
                }))
                .on_click(move |_, window, cx| {
                    let should_share = ActiveCall::global(cx)
                        .read(cx)
                        .room()
                        .is_some_and(|room| !room.read(cx).is_sharing_screen());

                    #[cfg(target_os = "linux")]
                    {
                        if is_wayland
                            && let Some(room) = ActiveCall::global(cx).read(cx).room().cloned()
                        {
                            let task = room.update(cx, |room, cx| {
                                if should_share {
                                    room.share_screen_wayland(cx)
                                } else {
                                    room.unshare_screen(true, cx)
                                        .map(|()| Task::ready(Ok(())))
                                        .unwrap_or_else(|e| Task::ready(Err(e)))
                                }
                            });
                            task.detach_and_prompt_err(
                                &tr(
                                    cx,
                                    "title_bar.collab.sharing_screen_failed",
                                    "Sharing Screen Failed",
                                ),
                                window,
                                cx,
                                |e, _, _| Some(format!("{e:?}")),
                            );
                        }
                    }
                    if !is_wayland {
                        window
                            .spawn(cx, async move |cx| {
                                let screen = if should_share {
                                    cx.update(|_, cx| pick_default_screen(cx))?.await
                                } else {
                                    Ok(None)
                                };
                                cx.update(|window, cx| toggle_screen_sharing(screen, window, cx))?;

                                Result::<_, anyhow::Error>::Ok(())
                            })
                            .detach();
                    }
                });

            if is_wayland {
                children.push(trigger.into_any_element());
            } else {
                children.push(
                    SplitButton::new(
                        trigger.render(window, cx),
                        self.render_screen_list().into_any_element(),
                    )
                    .style(SplitButtonStyle::Transparent)
                    .into_any_element(),
                );
            }
        }

        children.push(div().pr_2().into_any_element());

        children
    }

    fn render_screen_list(&self) -> impl IntoElement {
        PopoverMenu::new("screen-share-screen-list")
            .with_handle(self.screen_share_popover_handle.clone())
            .trigger(
                ui::ButtonLike::new_rounded_right("screen-share-screen-list-trigger")
                    .child(
                        h_flex()
                            .mx_neg_0p5()
                            .h_full()
                            .justify_center()
                            .child(Icon::new(IconName::ChevronDown).size(IconSize::XSmall)),
                    )
                    .toggle_state(self.screen_share_popover_handle.is_deployed()),
            )
            .menu(|window, cx| {
                let screens = cx.screen_capture_sources();
                Some(ContextMenu::build(window, cx, |context_menu, _, cx| {
                    cx.spawn(async move |this: WeakEntity<ContextMenu>, cx| {
                        let screens = screens.await??;
                        this.update(cx, |this, cx| {
                            let active_screenshare_id = ActiveCall::global(cx)
                                .read(cx)
                                .room()
                                .and_then(|room| room.read(cx).shared_screen_id());
                            for screen in screens {
                                let Ok(meta) = screen.metadata() else {
                                    continue;
                                };

                                let label = meta.label.clone().unwrap_or_else(|| {
                                    tr(cx, "title_bar.collab.unknown_screen", "Unknown screen")
                                        .into()
                                });
                                let resolution = SharedString::from(format!(
                                    "{} × {}",
                                    meta.resolution.width.0, meta.resolution.height.0
                                ));
                                this.push_item(ContextMenuItem::CustomEntry {
                                    entry_render: Box::new(move |_, _| {
                                        h_flex()
                                            .gap_2()
                                            .child(
                                                Icon::new(IconName::Screen)
                                                    .size(IconSize::XSmall)
                                                    .map(|this| {
                                                        if active_screenshare_id == Some(meta.id) {
                                                            this.color(Color::Accent)
                                                        } else {
                                                            this.color(Color::Muted)
                                                        }
                                                    }),
                                            )
                                            .child(Label::new(label.clone()))
                                            .child(
                                                Label::new(resolution.clone())
                                                    .color(Color::Muted)
                                                    .size(LabelSize::Small),
                                            )
                                            .into_any()
                                    }),
                                    selectable: true,
                                    documentation_aside: None,
                                    handler: Rc::new(move |_, window, cx| {
                                        toggle_screen_sharing(Ok(Some(screen.clone())), window, cx);
                                    }),
                                });
                            }
                        })
                    })
                    .detach_and_log_err(cx);
                    context_menu
                }))
            })
    }
}

/// Picks the screen to share when clicking on the main screen sharing button.
fn pick_default_screen(cx: &App) -> Task<anyhow::Result<Option<Rc<dyn ScreenCaptureSource>>>> {
    let source = cx.screen_capture_sources();
    cx.spawn(async move |_| {
        let available_sources = source.await??;
        Ok(available_sources
            .iter()
            .find(|it| {
                it.as_ref()
                    .metadata()
                    .is_ok_and(|meta| meta.is_main.unwrap_or_default())
            })
            .or_else(|| available_sources.first())
            .cloned())
    })
}
