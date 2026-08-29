use std::sync::Arc;

use ai_onboarding::AgentPanelOnboardingCard;
use gpui::{AnyElement, App, IntoElement, RenderOnce, Window};
use i18n as app_i18n;
use ui::{Tooltip, prelude::*};

fn tr(cx: &App, key: &'static str, fallback: &'static str) -> SharedString {
    app_i18n::tr(cx, key, fallback).into()
}

#[derive(IntoElement, RegisterComponent)]
pub struct EndTrialUpsell {
    dismiss_upsell: Arc<dyn Fn(&mut Window, &mut App)>,
}

impl EndTrialUpsell {
    pub fn new(dismiss_upsell: Arc<dyn Fn(&mut Window, &mut App)>) -> Self {
        Self { dismiss_upsell }
    }
}

impl RenderOnce for EndTrialUpsell {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        AgentPanelOnboardingCard::new()
            .child(Headline::new(tr(
                cx,
                "agent_ui.end_trial.trial_expired",
                "Local provider configuration is available in settings",
            )))
            .child(
                Label::new(tr(
                    cx,
                    "agent_ui.end_trial.reset_to_free",
                    "Cloud subscriptions are not part of the default experience.",
                ))
                    .color(Color::Muted)
                    .mb_2(),
            )
            .child(
                h_flex().absolute().top_4().right_4().child(
                    IconButton::new("dismiss_onboarding", IconName::Close)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text(tr(
                            cx,
                            "agent_ui.end_trial.dismiss",
                            "Dismiss",
                        )))
                        .on_click({
                            let callback = self.dismiss_upsell.clone();
                            move |_, window, cx| callback(window, cx)
                        }),
                ),
            )
    }
}

impl Component for EndTrialUpsell {
    fn scope() -> ComponentScope {
        ComponentScope::Onboarding
    }

    fn name() -> &'static str {
        "End of Trial Upsell Banner"
    }

    fn sort_name() -> &'static str {
        "End of Trial Upsell Banner"
    }

    fn preview(_window: &mut Window, _cx: &mut App) -> Option<AnyElement> {
        Some(
            v_flex()
                .child(EndTrialUpsell {
                    dismiss_upsell: Arc::new(|_, _| {}),
                })
                .into_any_element(),
        )
    }
}
