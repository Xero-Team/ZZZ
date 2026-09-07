use std::sync::Arc;

use gpui::{IntoElement, ParentElement};
use ui::prelude::*;

use crate::ZedAiOnboarding;

pub struct EditPredictionOnboarding {
    dismiss: Arc<dyn Fn(&mut Window, &mut App)>,
}

impl EditPredictionOnboarding {
    pub fn new(dismiss: Arc<dyn Fn(&mut Window, &mut App)>, _cx: &mut Context<Self>) -> Self {
        Self { dismiss }
    }
}

impl Render for EditPredictionOnboarding {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().gap_2().child(ZedAiOnboarding::new()).child(
            Button::new("use-local-provider", "Use local provider")
                .full_width()
                .style(ButtonStyle::Tinted(ui::TintColor::Accent))
                .on_click({
                    let callback = self.dismiss.clone();
                    move |_, window, cx| callback(window, cx)
                }),
        )
    }
}
