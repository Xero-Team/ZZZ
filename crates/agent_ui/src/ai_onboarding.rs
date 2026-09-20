mod agent_api_keys_onboarding;
mod agent_panel_onboarding_card;
mod agent_panel_onboarding_content;
mod edit_prediction_onboarding_content;

pub use agent_api_keys_onboarding::{ApiKeysWithProviders, ApiKeysWithoutProviders};
pub use agent_panel_onboarding_card::AgentPanelOnboardingCard;
pub use agent_panel_onboarding_content::AgentPanelOnboarding;
pub use edit_prediction_onboarding_content::EditPredictionOnboarding;

use std::sync::Arc;

use gpui::{AnyElement, IntoElement, ParentElement};
use i18n as app_i18n;
use ui::{List, ListBulletItem, RegisterComponent, Tooltip, prelude::*};

fn tr(cx: &App, key: &'static str, fallback: &'static str) -> SharedString {
    app_i18n::tr(cx, key, fallback).into()
}

#[derive(RegisterComponent, IntoElement)]
pub struct ZZZAiOnboarding {
    pub dismiss_onboarding: Option<Arc<dyn Fn(&mut Window, &mut App)>>,
}

impl ZZZAiOnboarding {
    pub fn new() -> Self {
        Self {
            dismiss_onboarding: None,
        }
    }

    pub fn with_dismiss(
        mut self,
        dismiss_callback: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.dismiss_onboarding = Some(Arc::new(dismiss_callback));
        self
    }

    fn render_dismiss_button(&self, cx: &App) -> Option<AnyElement> {
        self.dismiss_onboarding.as_ref().map(|dismiss_callback| {
            let callback = dismiss_callback.clone();
            let tooltip = tr(cx, "ai_onboarding.common.dismiss", "Dismiss");

            h_flex()
                .absolute()
                .top_0()
                .right_0()
                .child(
                    IconButton::new("dismiss_onboarding", IconName::Close)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text(tooltip))
                        .on_click(move |_, window, cx| callback(window, cx)),
                )
                .into_any_element()
        })
    }

    fn render_configure_local_provider(&self, cx: &mut App) -> AnyElement {
        v_flex()
            .w_full()
            .relative()
            .gap_1()
            .child(Headline::new(tr(
                cx,
                "ai_onboarding.configure_local_provider",
                "Configure a local provider",
            )))
            .child(
                Label::new(tr(
                    cx,
                    "ai_onboarding.configure_local_provider_body",
                    "Configure a local or user-managed provider to use AI features.",
                ))
                .color(Color::Muted)
                .mb_2(),
            )
            .child(
                List::new()
                    .child(ListBulletItem::new(tr(
                        cx,
                        "ai_onboarding.configure_local_provider.bullet1",
                        "Configure a local provider",
                    )))
                    .child(ListBulletItem::new(tr(
                        cx,
                        "ai_onboarding.configure_local_provider.bullet2",
                        "Ollama and llama.cpp are preferred",
                    )))
                    .child(ListBulletItem::new(tr(
                        cx,
                        "ai_onboarding.configure_local_provider.bullet3",
                        "No ZZZ account required",
                    ))),
            )
            .children(self.render_dismiss_button(cx))
            .into_any_element()
    }
}

impl RenderOnce for ZZZAiOnboarding {
    fn render(self, _window: &mut ui::Window, cx: &mut App) -> impl IntoElement {
        self.render_configure_local_provider(cx)
    }
}

impl Component for ZZZAiOnboarding {
    fn scope() -> ComponentScope {
        ComponentScope::Onboarding
    }

    fn name() -> &'static str {
        "Agent New User Onboarding"
    }

    fn preview(_window: &mut Window, _cx: &mut App) -> Option<AnyElement> {
        fn onboarding() -> AnyElement {
            div()
                .w_full()
                .min_w_40()
                .max_w(px(1100.))
                .child(
                    AgentPanelOnboardingCard::new()
                        .child(ZZZAiOnboarding::new().into_any_element()),
                )
                .into_any_element()
        }

        Some(
            v_flex()
                .min_w_0()
                .gap_4()
                .children(vec![single_example(
                    "Configure a local provider",
                    onboarding(),
                )])
                .into_any_element(),
        )
    }
}
#[derive(RegisterComponent)]
pub struct AgentLayoutOnboarding {
    pub use_agent_layout: Arc<dyn Fn(&mut Window, &mut App)>,
    pub revert_to_editor_layout: Arc<dyn Fn(&mut Window, &mut App)>,
    pub dismissed: Arc<dyn Fn(&mut Window, &mut App)>,
    pub is_agent_layout: bool,
}

impl Render for AgentLayoutOnboarding {
    fn render(&mut self, _window: &mut ui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let description = tr(
            cx,
            "ai_onboarding.agent_layout.description",
            "With the new Threads Sidebar, you can manage multiple agents across several projects, all in one window.",
        );

        let dismiss_button = div().absolute().top_0().right_0().child(
            IconButton::new("dismiss", IconName::Close)
                .icon_size(IconSize::Small)
                .on_click({
                    let dismiss = self.dismissed.clone();
                    move |_, window, cx| dismiss(window, cx)
                }),
        );

        let primary_button = if self.is_agent_layout {
            Button::new(
                "revert",
                tr(
                    cx,
                    "ai_onboarding.agent_layout.use_previous_layout",
                    "Use Previous Layout",
                ),
            )
            .label_size(LabelSize::Small)
            .style(ButtonStyle::Outlined)
            .on_click({
                let revert = self.revert_to_editor_layout.clone();
                let dismiss = self.dismissed.clone();
                move |_, window, cx| {
                    revert(window, cx);
                    dismiss(window, cx);
                }
            })
        } else {
            Button::new(
                "start",
                tr(
                    cx,
                    "ai_onboarding.agent_layout.use_new_layout",
                    "Use New Layout",
                ),
            )
            .label_size(LabelSize::Small)
            .style(ButtonStyle::Outlined)
            .on_click({
                let use_layout = self.use_agent_layout.clone();
                let dismiss = self.dismissed.clone();
                move |_, window, cx| {
                    use_layout(window, cx);
                    dismiss(window, cx);
                }
            })
        };

        let content = v_flex()
            .min_w_0()
            .w_full()
            .relative()
            .gap_1()
            .child(Label::new(tr(
                cx,
                "ai_onboarding.agent_layout.title",
                "A new workspace layout for agentic workflows",
            )))
            .child(Label::new(description).color(Color::Muted).mb_2())
            .child(
                List::new()
                    .child(ListBulletItem::new(tr(
                        cx,
                        "ai_onboarding.agent_layout.bullet.sidebar_left",
                        "The Sidebar and Agent Panel are on the left by default",
                    )))
                    .child(ListBulletItem::new(tr(
                        cx,
                        "ai_onboarding.agent_layout.bullet.panels_shift_right",
                        "The Project Panel and all other panels shift to the right",
                    )))
                    .child(ListBulletItem::new(tr(
                        cx,
                        "ai_onboarding.agent_layout.bullet.customize_settings",
                        "You can always customize your workspace layout in your Settings",
                    ))),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .flex_wrap()
                    .justify_end()
                    .child(
                        Button::new(
                            "learn",
                            tr(cx, "ai_onboarding.agent_layout.learn_more", "Learn More"),
                        )
                        .label_size(LabelSize::Small)
                        .style(ButtonStyle::OutlinedGhost)
                        .on_click(|_, _, _| {}),
                    )
                    .child(primary_button),
            )
            .child(dismiss_button);

        AgentPanelOnboardingCard::new().child(content)
    }
}

impl Component for AgentLayoutOnboarding {
    fn scope() -> ComponentScope {
        ComponentScope::Onboarding
    }

    fn name() -> &'static str {
        "Agent Layout Onboarding"
    }

    fn preview(_window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let onboarding = cx.new(|_cx| AgentLayoutOnboarding {
            use_agent_layout: Arc::new(|_, _| {}),
            revert_to_editor_layout: Arc::new(|_, _| {}),
            dismissed: Arc::new(|_, _| {}),
            is_agent_layout: false,
        });

        Some(
            v_flex()
                .min_w_0()
                .gap_4()
                .child(single_example(
                    "Agent Layout Onboarding",
                    div()
                        .w_full()
                        .min_w_40()
                        .max_w(px(1100.))
                        .child(onboarding)
                        .into_any_element(),
                ))
                .into_any_element(),
        )
    }
}
