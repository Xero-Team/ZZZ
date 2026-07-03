use gpui::{AnyElement, Context, ScrollHandle, Window, prelude::*};
use ui::prelude::*;

use crate::{PROJECT, SettingField, SettingItem, SettingsPageItem, SettingsWindow, USER};

pub(crate) fn render_mcp_servers_page(
    settings_window: &SettingsWindow,
    _scroll_handle: &ScrollHandle,
    window: &mut Window,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let item = SettingsPageItem::SettingItem(SettingItem {
        title: "MCP Server Timeout".into(),
        description:
            "Default timeout in seconds for MCP server tool calls. Can be overridden per server."
                .into(),
        field: Box::new(SettingField {
            json_path: Some("context_server_timeout"),
            pick: |settings_content| settings_content.project.context_server_timeout.as_ref(),
            write: |settings_content, value, _| {
                settings_content.project.context_server_timeout = value;
            },
        }),
        metadata: None,
        files: USER | PROJECT,
    });

    v_flex()
        .id("mcp-servers-page")
        .size_full()
        .pt_2p5()
        .pb_16()
        .track_scroll(_scroll_handle)
        .overflow_y_scroll()
        .child(item.render(settings_window, 0, false, false, window, cx))
        .into_any_element()
}
