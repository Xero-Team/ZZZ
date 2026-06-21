use std::sync::Arc;

use gpui::{AnyView, ScrollHandle, prelude::*};
use i18n as app_i18n;
use language_model::{
    ConfigurationViewTargetAgent, IconOrSvg, LanguageModelProvider, LanguageModelProviderId,
    LanguageModelRegistry, ZED_CLOUD_PROVIDER_ID,
};
use ui::{Divider, DividerColor, prelude::*};

use crate::SettingsWindow;

fn tr(cx: &App, key: &'static str, fallback: &'static str) -> SharedString {
    app_i18n::tr(cx, key, fallback).into()
}

pub(crate) fn render_llm_providers_page(
    settings_window: &SettingsWindow,
    scroll_handle: &ScrollHandle,
    window: &mut Window,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let providers = LanguageModelRegistry::read_global(cx)
        .visible_providers()
        .into_iter()
        .filter(|provider| provider.id() != ZED_CLOUD_PROVIDER_ID)
        .collect::<Vec<_>>();

    v_flex()
        .id("llm-providers-page")
        .size_full()
        .pt_2p5()
        .px_8()
        .pb_16()
        .track_scroll(scroll_handle)
        .overflow_y_scroll()
        .when(providers.is_empty(), |this| {
            this.child(
                Label::new(tr(
                    cx,
                    "settings_ui.llm_providers_page.no_visible_providers",
                    "No configurable local or third-party providers are currently available.",
                ))
                .color(Color::Muted),
            )
        })
        .children(
            providers
                .iter()
                .map(|provider| render_provider_row(settings_window, provider, window, cx))
                .collect::<Vec<_>>(),
        )
        .into_any_element()
}

fn render_provider_row(
    settings_window: &SettingsWindow,
    provider: &Arc<dyn LanguageModelProvider>,
    window: &mut Window,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let provider_id = provider.id();
    let provider_name = provider.name().0;
    let is_authenticated = provider.is_authenticated(cx);

    let icon = match provider.icon() {
        IconOrSvg::Svg(path) => Icon::from_external_svg(path),
        IconOrSvg::Icon(name) => Icon::new(name),
    }
    .size(IconSize::Small)
    .color(Color::Muted);

    let configuration_view =
        get_or_create_configuration_view(settings_window, &provider_id, provider, window, cx);

    v_flex()
        .min_w_0()
        .w_full()
        .child(
            div()
                .px_2()
                .child(Divider::horizontal().color(DividerColor::BorderFaded)),
        )
        .child(
            v_flex()
                .min_w_0()
                .w_full()
                .py_2()
                .px_2()
                .gap_2()
                .child(
                    h_flex()
                        .min_w_0()
                        .gap_1p5()
                        .items_center()
                        .child(icon)
                        .child(Label::new(provider_name).color(Color::Default).truncate())
                        .when(is_authenticated, |this| {
                            this.child(
                                Icon::new(IconName::Check)
                                    .size(IconSize::Small)
                                    .color(Color::Success),
                            )
                        }),
                )
                .child(div().min_w_0().w_full().pl_6().child(configuration_view)),
        )
        .into_any_element()
}

fn get_or_create_configuration_view(
    settings_window: &SettingsWindow,
    provider_id: &LanguageModelProviderId,
    provider: &Arc<dyn LanguageModelProvider>,
    window: &mut Window,
    cx: &mut Context<SettingsWindow>,
) -> AnyView {
    if let Some(view) = settings_window
        .provider_configuration_views
        .get(provider_id)
    {
        return view.clone();
    }

    let view = provider.configuration_view(
        ConfigurationViewTargetAgent::Other("ZZZ".into()),
        window,
        cx,
    );

    let provider_id = provider_id.clone();
    let view_clone = view.clone();
    cx.defer_in(window, move |this, _window, _cx| {
        this.provider_configuration_views
            .insert(provider_id, view_clone);
    });

    view
}
