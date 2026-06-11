use gpui::{App, IntoElement, Modifiers, RenderOnce, Window};
use i18n as app_i18n;
use ui::{prelude::*, render_modifiers};

fn tr(cx: &App, key: &'static str, fallback: &'static str) -> SharedString {
    app_i18n::tr(cx, key, fallback).into()
}

#[derive(IntoElement)]
pub struct HoldForDefault {
    is_default: bool,
    more_content: bool,
}

impl HoldForDefault {
    pub fn new(is_default: bool) -> Self {
        Self {
            is_default,
            more_content: true,
        }
    }

    #[allow(dead_code)]
    pub fn more_content(mut self, more_content: bool) -> Self {
        self.more_content = more_content;
        self
    }
}

impl RenderOnce for HoldForDefault {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        h_flex()
            .when(self.more_content, |this| {
                this.pt_1()
                    .border_t_1()
                    .border_color(cx.theme().colors().border_variant)
            })
            .gap_0p5()
            .text_sm()
            .text_color(Color::Muted.color(cx))
            .child(tr(cx, "agent_ui.hold_for_default.hold", "Hold"))
            .child(h_flex().flex_shrink_0().children(render_modifiers(
                &Modifiers::secondary_key(),
                PlatformStyle::platform(),
                None,
                Some(TextSize::Default.rems(cx).into()),
                false,
            )))
            .child(div().map(|this| {
                if self.is_default {
                    this.child(tr(
                        cx,
                        "agent_ui.hold_for_default.to_unset_as_default",
                        "to unset as default",
                    ))
                } else {
                    this.child(tr(
                        cx,
                        "agent_ui.hold_for_default.to_set_as_default",
                        "to set as default",
                    ))
                }
            }))
    }
}
