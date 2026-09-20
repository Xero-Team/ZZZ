use std::sync::Arc;

use gpui::{IntoElement, ParentElement};
use i18n as app_i18n;
use ui::prelude::*;

use super::ZedAiOnboarding;

pub struct EditPredictionOnboarding {
    dismiss: Arc<dyn Fn(&mut Window, &mut App)>,
}

impl EditPredictionOnboarding {
    pub fn new(dismiss: Arc<dyn Fn(&mut Window, &mut App)>, _cx: &mut Context<Self>) -> Self {
        Self { dismiss }
    }
}

impl Render for EditPredictionOnboarding {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().gap_2().child(ZedAiOnboarding::new()).child(
            Button::new(
                "use-local-provider",
                app_i18n::tr(
                    cx,
                    "ai_onboarding.edit_prediction.use_local_provider",
                    "Use local provider",
                ),
            )
            .full_width()
            .style(ButtonStyle::Tinted(ui::TintColor::Accent))
            .on_click({
                let callback = self.dismiss.clone();
                move |_, window, cx| callback(window, cx)
            }),
        )
    }
}
