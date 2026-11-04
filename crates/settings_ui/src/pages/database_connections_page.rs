use gpui::{AnyElement, App, Context, ScrollHandle, SharedString, Window, prelude::*};
use i18n as app_i18n;
use settings::SettingsStore;
use ui::{Color, Label, LabelSize, prelude::*};

use crate::SettingsWindow;

fn tr(cx: &App, key: &'static str, fallback: &'static str) -> SharedString {
    app_i18n::tr(cx, key, fallback).into()
}

/// Read-only summary of `database.connections`. Adding, editing, and deleting
/// happen in the Database panel so the keychain is written exactly once per
/// change; the JSON link covers bulk edits.
pub(crate) fn render_database_connections_page(
    _settings_window: &SettingsWindow,
    scroll_handle: &ScrollHandle,
    _window: &mut Window,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let merged = cx.global::<SettingsStore>().merged_settings();
    let connections = merged
        .database
        .as_ref()
        .map(|database| database.connections.as_slice())
        .unwrap_or_default();

    let mut list = v_flex().gap_2();
    if connections.is_empty() {
        list = list.child(
            Label::new(tr(
                cx,
                "database.settings.empty",
                "No connections are saved yet.",
            ))
            .color(Color::Muted),
        );
    }
    for connection in connections {
        let target = connection
            .path
            .as_ref()
            .map(|path| path.clone())
            .or_else(|| match (connection.host.as_ref(), connection.port) {
                (Some(host), Some(port)) => Some(format!("{host}:{port}")),
                (Some(host), None) => Some(host.clone()),
                _ => None,
            })
            .unwrap_or_default();
        list = list.child(
            v_flex()
                .p_2()
                .gap_1()
                .border_1()
                .rounded_md()
                .border_color(cx.theme().colors().border)
                .child(Label::new(connection.name.clone()))
                .child(
                    Label::new(format!("{} · {target}", connection.driver))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
        );
    }

    v_flex()
        .id("database-connections-page")
        .size_full()
        .pt_2p5()
        .px_8()
        .pb_16()
        .gap_3()
        .track_scroll(scroll_handle)
        .overflow_y_scroll()
        .child(
            Label::new(tr(
                cx,
                "database.settings.description",
                "Manage saved database connections. Passwords are stored in the system keychain; the connection list can be edited in JSON.",
            ))
            .color(Color::Muted),
        )
        .child(list)
        .into_any_element()
}
