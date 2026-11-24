use agent::TerminalTool;
use agent_settings::AgentSettings;
use gpui::{HighlightStyle, ReadGlobal, ScrollHandle, SharedString, StyledText, point, prelude::*};
use i18n as app_i18n;
use settings::{Settings as _, SettingsStore, ToolPermissionMode};
use std::sync::Arc;
use ui::{Banner, ContextMenu, Divider, PopoverMenu, prelude::*};

use crate::SettingsWindow;

const HARDCODED_RULES_DESCRIPTION: &str =
    "`rm -rf` commands are always blocked when run on `$HOME`, `~`, `.`, `..`, or `/`";
const SETTINGS_DISCLAIMER: &str = "Note: custom tool permissions apply to all agents in ZZZ, including external agents connected through the Agent Client Protocol (ACP). Only the default action for each tool is consulted today: agents gate a tool by its id, so that default is the only permission setting that changes behaviour.";

/// Tools that support per-tool permission settings
const TOOLS: &[ToolInfo] = &[
    ToolInfo {
        id: "terminal",
        name_key: "settings_ui.tool_permissions.tool.terminal.name",
        name: "Terminal",
        description_key: "settings_ui.tool_permissions.tool.terminal.description",
        description: "Commands executed in the terminal",
    },
    ToolInfo {
        id: "edit_file",
        name_key: "settings_ui.tool_permissions.tool.edit_file.name",
        name: "Edit File",
        description_key: "settings_ui.tool_permissions.tool.edit_file.description",
        description: "File editing operations",
    },
    ToolInfo {
        id: "delete_path",
        name_key: "settings_ui.tool_permissions.tool.delete_path.name",
        name: "Delete Path",
        description_key: "settings_ui.tool_permissions.tool.delete_path.description",
        description: "File and directory deletion",
    },
    ToolInfo {
        id: "copy_path",
        name_key: "settings_ui.tool_permissions.tool.copy_path.name",
        name: "Copy Path",
        description_key: "settings_ui.tool_permissions.tool.copy_path.description",
        description: "File and directory copying",
    },
    ToolInfo {
        id: "move_path",
        name_key: "settings_ui.tool_permissions.tool.move_path.name",
        name: "Move Path",
        description_key: "settings_ui.tool_permissions.tool.move_path.description",
        description: "File and directory moves/renames",
    },
    ToolInfo {
        id: "create_directory",
        name_key: "settings_ui.tool_permissions.tool.create_directory.name",
        name: "Create Directory",
        description_key: "settings_ui.tool_permissions.tool.create_directory.description",
        description: "Directory creation",
    },
    ToolInfo {
        id: "save_file",
        name_key: "settings_ui.tool_permissions.tool.save_file.name",
        name: "Save File",
        description_key: "settings_ui.tool_permissions.tool.save_file.description",
        description: "File saving operations",
    },
    ToolInfo {
        id: "fetch",
        name_key: "settings_ui.tool_permissions.tool.fetch.name",
        name: "Fetch",
        description_key: "settings_ui.tool_permissions.tool.fetch.description",
        description: "HTTP requests to URLs",
    },
    ToolInfo {
        id: "skill",
        name_key: "settings_ui.tool_permissions.tool.skill.name",
        name: "Skill",
        description_key: "settings_ui.tool_permissions.tool.skill.description",
        description: "Loading agent skill instructions",
    },
    ToolInfo {
        id: "restore_file_from_disk",
        name_key: "settings_ui.tool_permissions.tool.restore_file_from_disk.name",
        name: "Restore File from Disk",
        description_key: "settings_ui.tool_permissions.tool.restore_file_from_disk.description",
        description: "Discards unsaved changes by reloading from disk",
    },
];

pub(crate) struct ToolInfo {
    id: &'static str,
    name_key: &'static str,
    name: &'static str,
    description_key: &'static str,
    description: &'static str,
}

impl ToolInfo {
    fn name(&self, cx: &App) -> SharedString {
        app_i18n::tr(cx, self.name_key, self.name).into()
    }

    fn description(&self, cx: &App) -> SharedString {
        app_i18n::tr(cx, self.description_key, self.description).into()
    }
}

const fn const_str_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// Finds the index of a tool in `TOOLS` by its ID. Panics (compile error in
/// const context) if the ID is not found, so every macro-generated render
/// function is validated at compile time.
const fn tool_index(id: &str) -> usize {
    let mut i = 0;
    while i < TOOLS.len() {
        if const_str_eq(TOOLS[i].id, id) {
            return i;
        }
        i += 1;
    }
    panic!("tool ID not found in TOOLS array")
}

/// Parses a string containing backtick-delimited code spans into a `StyledText`
/// with code background highlights applied to each span.
fn render_inline_code_markdown(text: &str, cx: &App) -> StyledText {
    let code_background = cx.theme().colors().surface_background;
    let mut plain = String::new();
    let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
    let mut in_code = false;
    let mut code_start = 0;

    for ch in text.chars() {
        if ch == '`' {
            if in_code {
                highlights.push((
                    code_start..plain.len(),
                    HighlightStyle {
                        background_color: Some(code_background),
                        ..Default::default()
                    },
                ));
            } else {
                code_start = plain.len();
            }
            in_code.toggle();
        } else {
            plain.push(ch);
        }
    }

    StyledText::new(plain).with_highlights(highlights)
}

fn tr(cx: &App, key: &'static str, fallback: &'static str) -> SharedString {
    app_i18n::tr(cx, key, fallback).into()
}

fn mode_display_label(mode: ToolPermissionMode, cx: &App) -> SharedString {
    match mode {
        ToolPermissionMode::Allow => tr(cx, "settings_ui.tool_permissions.mode.allow", "Allow"),
        ToolPermissionMode::Deny => tr(cx, "settings_ui.tool_permissions.mode.deny", "Deny"),
        ToolPermissionMode::Confirm => {
            tr(cx, "settings_ui.tool_permissions.mode.confirm", "Confirm")
        }
    }
}

/// Renders the main tool permissions setup page showing a list of tools
pub(crate) fn render_tool_permissions_setup_page(
    settings_window: &SettingsWindow,
    scroll_handle: &ScrollHandle,
    window: &mut Window,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let tool_items: Vec<AnyElement> = TOOLS
        .iter()
        .enumerate()
        .map(|(i, tool)| render_tool_list_item(settings_window, tool, i, window, cx))
        .collect();

    let settings = AgentSettings::get_global(cx);
    let global_default = settings.tool_permissions.default;

    let scroll_step = px(40.);

    v_flex()
        .id("tool-permissions-page")
        .on_action({
            let scroll_handle = scroll_handle.clone();
            move |_: &menu::SelectNext, window, cx| {
                window.focus_next(cx);
                let current_offset = scroll_handle.offset();
                scroll_handle.set_offset(point(current_offset.x, current_offset.y - scroll_step));
            }
        })
        .on_action({
            let scroll_handle = scroll_handle.clone();
            move |_: &menu::SelectPrevious, window, cx| {
                window.focus_prev(cx);
                let current_offset = scroll_handle.offset();
                scroll_handle.set_offset(point(current_offset.x, current_offset.y + scroll_step));
            }
        })
        .min_w_0()
        .size_full()
        .pt_2p5()
        .px_8()
        .pb_16()
        .overflow_y_scroll()
        .track_scroll(scroll_handle)
        .child(
            Banner::new().child(
                Label::new(tr(
                    cx,
                    "settings_ui.tool_permissions.settings_disclaimer",
                    SETTINGS_DISCLAIMER,
                ))
                .size(LabelSize::Small)
                .color(Color::Muted)
                .mt_0p5(),
            ),
        )
        .child(
            v_flex()
                .child(render_global_default_mode_section(global_default, cx))
                .child(Divider::horizontal())
                .children(tool_items.into_iter().enumerate().flat_map(|(i, item)| {
                    let mut elements: Vec<AnyElement> = vec![item];
                    if i + 1 < TOOLS.len() {
                        elements.push(Divider::horizontal().into_any_element());
                    }
                    elements
                })),
        )
        .into_any_element()
}

fn render_tool_list_item(
    _settings_window: &SettingsWindow,
    tool: &'static ToolInfo,
    tool_index: usize,
    _window: &mut Window,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let render_fn = get_tool_render_fn(tool.id);

    h_flex()
        .w_full()
        .min_w_0()
        .py_3()
        .justify_between()
        .child(
            v_flex()
                .w_full()
                .min_w_0()
                .child(h_flex().gap_1().child(Label::new(tool.name(cx))))
                .child(
                    Label::new(tool.description(cx))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
        )
        .child({
            let tool_name = tool.name(cx);
            let section_header = tr(cx, "settings_ui.tool_permissions.title", "Tool Permissions");
            Button::new(
                format!("configure-{}", tool.id),
                tr(cx, "settings_ui.common.button.configure", "Configure"),
            )
            .tab_index(tool_index as isize)
            .style(ButtonStyle::OutlinedGhost)
            .size(ButtonSize::Medium)
            .end_icon(
                Icon::new(IconName::ChevronRight)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.push_dynamic_sub_page(
                    tool_name.clone(),
                    section_header.clone(),
                    None,
                    render_fn,
                    window,
                    cx,
                );
            }))
        })
        .into_any_element()
}

fn get_tool_render_fn(
    tool_id: &str,
) -> fn(&SettingsWindow, &ScrollHandle, &mut Window, &mut Context<SettingsWindow>) -> AnyElement {
    match tool_id {
        "terminal" => render_terminal_tool_config,
        "edit_file" => render_edit_file_tool_config,
        "delete_path" => render_delete_path_tool_config,
        "copy_path" => render_copy_path_tool_config,
        "move_path" => render_move_path_tool_config,
        "create_directory" => render_create_directory_tool_config,
        "save_file" => render_save_file_tool_config,
        "fetch" => render_fetch_tool_config,
        "skill" => render_skill_tool_config,
        "restore_file_from_disk" => render_restore_file_from_disk_tool_config,
        _ => render_terminal_tool_config, // fallback
    }
}

/// Renders an individual tool's permission configuration page
pub(crate) fn render_tool_config_page(
    tool: &ToolInfo,
    _settings_window: &SettingsWindow,
    scroll_handle: &ScrollHandle,
    _window: &mut Window,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let default_mode = get_tool_default(tool.id, cx);
    let page_title = tool.name(cx);
    let scroll_step = px(80.);

    v_flex()
        .id(format!("tool-config-page-{}", tool.id))
        .on_action({
            let scroll_handle = scroll_handle.clone();
            move |_: &menu::SelectNext, window, cx| {
                window.focus_next(cx);
                let current_offset = scroll_handle.offset();
                scroll_handle.set_offset(point(current_offset.x, current_offset.y - scroll_step));
            }
        })
        .on_action({
            let scroll_handle = scroll_handle.clone();
            move |_: &menu::SelectPrevious, window, cx| {
                window.focus_prev(cx);
                let current_offset = scroll_handle.offset();
                scroll_handle.set_offset(point(current_offset.x, current_offset.y + scroll_step));
            }
        })
        .min_w_0()
        .size_full()
        .pt_2p5()
        .px_8()
        .pb_16()
        .overflow_y_scroll()
        .track_scroll(scroll_handle)
        .child(
            v_flex()
                .min_w_0()
                .child(Label::new(page_title).size(LabelSize::Large)),
        )
        .when(tool.id == TerminalTool::NAME, |this| {
            this.child(render_hardcoded_security_banner(cx))
        })
        .child(
            v_flex()
                .mt_6()
                .min_w_0()
                .w_full()
                .child(render_default_mode_section(tool.id, default_mode, cx)),
        )
        .into_any_element()
}

fn render_hardcoded_security_banner(cx: &mut Context<SettingsWindow>) -> AnyElement {
    let description = tr(
        cx,
        "settings_ui.tool_permissions.hardcoded_rules_description",
        HARDCODED_RULES_DESCRIPTION,
    );

    div()
        .mt_3()
        .child(
            Banner::new().child(
                div()
                    .text_sm()
                    .text_color(cx.theme().colors().text_muted)
                    .child(render_inline_code_markdown(description.as_ref(), cx)),
            ),
        )
        .into_any_element()
}

fn render_global_default_mode_section(
    current_mode: ToolPermissionMode,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let mode_label = mode_display_label(current_mode, cx);

    h_flex()
        .my_4()
        .min_w_0()
        .justify_between()
        .child(
            v_flex()
                .w_full()
                .min_w_0()
                .child(Label::new(tr(
                    cx,
                    "settings_ui.tool_permissions.default_permission",
                    "Default Permission",
                )))
                .child(
                    Label::new(tr(
                        cx,
                        "settings_ui.tool_permissions.default_permission_description",
                        "Controls the default behavior for all tool actions. Per-tool defaults can override this.",
                    ))
                    .size(LabelSize::Small)
                    .color(Color::Muted),
                ),
        )
        .child(
            PopoverMenu::new("global-default-mode")
                .trigger(
                    Button::new("global-mode-trigger", mode_label)
                        .tab_index(0_isize)
                        .style(ButtonStyle::Outlined)
                        .size(ButtonSize::Medium)
                        .end_icon(Icon::new(IconName::ChevronDown).size(IconSize::Small)),
                )
                .menu(move |window, cx| {
                    let confirm_label =
                        tr(cx, "settings_ui.tool_permissions.mode.confirm", "Confirm");
                    let allow_label = tr(cx, "settings_ui.tool_permissions.mode.allow", "Allow");
                    let deny_label = tr(cx, "settings_ui.tool_permissions.mode.deny", "Deny");
                    Some(ContextMenu::build(window, cx, move |menu, _, _| {
                        menu.entry(
                            confirm_label.clone(),
                            None,
                            move |_, cx| {
                                set_global_default_permission(ToolPermissionMode::Confirm, cx);
                            },
                        )
                        .entry(
                            allow_label.clone(),
                            None,
                            move |_, cx| {
                                set_global_default_permission(ToolPermissionMode::Allow, cx);
                            },
                        )
                        .entry(
                            deny_label,
                            None,
                            move |_, cx| {
                                set_global_default_permission(ToolPermissionMode::Deny, cx);
                            },
                        )
                    }))
                })
                .anchor(gpui::Anchor::TopRight),
        )
        .into_any_element()
}

fn render_default_mode_section(
    tool_id: &'static str,
    current_mode: ToolPermissionMode,
    cx: &mut Context<SettingsWindow>,
) -> AnyElement {
    let mode_label = mode_display_label(current_mode, cx);

    let tool_id_owned = tool_id.to_owned();

    h_flex()
        .min_w_0()
        .justify_between()
        .child(
            v_flex()
                .w_full()
                .min_w_0()
                .child(Label::new(tr(
                    cx,
                    "settings_ui.tool_permissions.default_action",
                    "Default Action",
                )))
                .child(
                    Label::new(tr(
                        cx,
                        "settings_ui.tool_permissions.default_action_description",
                        "Action to take for every call to this tool.",
                    ))
                    .size(LabelSize::Small)
                    .color(Color::Muted),
                ),
        )
        .child(
            PopoverMenu::new(format!("default-mode-{}", tool_id))
                .trigger(
                    Button::new(format!("mode-trigger-{}", tool_id), mode_label)
                        .tab_index(0_isize)
                        .style(ButtonStyle::Outlined)
                        .size(ButtonSize::Medium)
                        .end_icon(Icon::new(IconName::ChevronDown).size(IconSize::Small)),
                )
                .menu(move |window, cx| {
                    let tool_id = tool_id_owned.clone();
                    let confirm_label =
                        tr(cx, "settings_ui.tool_permissions.mode.confirm", "Confirm");
                    let allow_label = tr(cx, "settings_ui.tool_permissions.mode.allow", "Allow");
                    let deny_label = tr(cx, "settings_ui.tool_permissions.mode.deny", "Deny");
                    Some(ContextMenu::build(window, cx, move |menu, _, _| {
                        let tool_id_confirm = tool_id.clone();
                        let tool_id_allow = tool_id.clone();
                        let tool_id_deny = tool_id;

                        menu.entry(confirm_label.clone(), None, move |_, cx| {
                            set_default_mode(&tool_id_confirm, ToolPermissionMode::Confirm, cx);
                        })
                        .entry(allow_label.clone(), None, move |_, cx| {
                            set_default_mode(&tool_id_allow, ToolPermissionMode::Allow, cx);
                        })
                        .entry(deny_label, None, move |_, cx| {
                            set_default_mode(&tool_id_deny, ToolPermissionMode::Deny, cx);
                        })
                    }))
                })
                .anchor(gpui::Anchor::TopRight),
        )
        .into_any_element()
}

fn get_tool_default(tool_name: &str, cx: &App) -> ToolPermissionMode {
    let settings = AgentSettings::get_global(cx);
    settings
        .tool_permissions
        .tools
        .get(tool_name)
        .and_then(|rules| rules.default)
        .unwrap_or(settings.tool_permissions.default)
}

fn set_global_default_permission(mode: ToolPermissionMode, cx: &mut App) {
    SettingsStore::global(cx).update_settings_file(<dyn fs::Fs>::global(cx), move |settings, _| {
        settings
            .agent
            .get_or_insert_default()
            .tool_permissions
            .get_or_insert_default()
            .default = Some(mode);
    });
}

fn set_default_mode(tool_name: &str, mode: ToolPermissionMode, cx: &mut App) {
    let tool_name = tool_name.to_owned();

    SettingsStore::global(cx).update_settings_file(<dyn fs::Fs>::global(cx), move |settings, _| {
        let tool_permissions = settings
            .agent
            .get_or_insert_default()
            .tool_permissions
            .get_or_insert_default();
        let tool_rules = tool_permissions
            .tools
            .entry(Arc::from(tool_name.as_str()))
            .or_default();
        tool_rules.default = Some(mode);
    });
}

macro_rules! tool_config_page_fn {
    ($fn_name:ident, $tool_id:literal) => {
        pub fn $fn_name(
            settings_window: &SettingsWindow,
            scroll_handle: &ScrollHandle,
            window: &mut Window,
            cx: &mut Context<SettingsWindow>,
        ) -> AnyElement {
            const INDEX: usize = tool_index($tool_id);
            render_tool_config_page(&TOOLS[INDEX], settings_window, scroll_handle, window, cx)
        }
    };
}

tool_config_page_fn!(render_terminal_tool_config, "terminal");
tool_config_page_fn!(render_edit_file_tool_config, "edit_file");
tool_config_page_fn!(render_delete_path_tool_config, "delete_path");
tool_config_page_fn!(render_copy_path_tool_config, "copy_path");
tool_config_page_fn!(render_move_path_tool_config, "move_path");
tool_config_page_fn!(render_create_directory_tool_config, "create_directory");
tool_config_page_fn!(render_save_file_tool_config, "save_file");
tool_config_page_fn!(render_fetch_tool_config, "fetch");
tool_config_page_fn!(render_skill_tool_config, "skill");
tool_config_page_fn!(
    render_restore_file_from_disk_tool_config,
    "restore_file_from_disk"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_tools_are_in_tool_info_or_excluded() {
        // Tools that intentionally don't appear in the permissions UI.
        // If you add a new tool and this test fails, either:
        //   1. Add a ToolInfo entry to TOOLS (if the tool has permission checks), or
        //   2. Add it to this list with a comment explaining why it's excluded.
        const EXCLUDED_TOOLS: &[&str] = &[
            // Read-only / low-risk tools that don't call decide_permission_from_settings
            "apply_code_action",
            "diagnostics",
            "find_path",
            "find_references",
            "get_code_actions",
            "go_to_definition",
            "grep",
            "list_directory",
            "now",
            "open",
            "read_file",
            "rename_symbol",
            "thinking",
            // streaming_edit_file uses "edit_file" for permission lookups,
            // so its rules are configured under the edit_file entry.
            "streaming_edit_file",
            // Subagent permission checks happen at the level of individual
            // tool calls within the subagent, not at the spawning level.
            "spawn_agent",
            // update_plan updates UI-visible planning state but does not use
            // tool permission rules.
            "update_plan",
        ];

        let tool_info_ids: Vec<&str> = TOOLS.iter().map(|t| t.id).collect();

        for tool_name in agent::ALL_TOOL_NAMES {
            if EXCLUDED_TOOLS.contains(tool_name) {
                assert!(
                    !tool_info_ids.contains(tool_name),
                    "Tool '{}' is in both EXCLUDED_TOOLS and TOOLS — pick one.",
                    tool_name,
                );
                continue;
            }
            assert!(
                tool_info_ids.contains(tool_name),
                "Tool '{}' is in ALL_TOOL_NAMES but has no entry in TOOLS and \
                 is not in EXCLUDED_TOOLS. Either add a ToolInfo entry (if the \
                 tool has permission checks) or add it to EXCLUDED_TOOLS with \
                 a comment explaining why.",
                tool_name,
            );
        }

        for tool_id in &tool_info_ids {
            assert!(
                agent::ALL_TOOL_NAMES.contains(tool_id),
                "TOOLS contains '{}' but it is not in ALL_TOOL_NAMES. \
                 Is this a valid built-in tool?",
                tool_id,
            );
        }
    }
}
