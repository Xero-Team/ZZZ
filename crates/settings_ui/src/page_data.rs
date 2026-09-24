use gpui::{Action as _, App};
use itertools::Itertools as _;
use settings::{LanguageSettingsContent, SemanticTokens, SettingsContent};
use std::sync::{Arc, OnceLock};
use strum::{EnumMessage, IntoDiscriminant as _, VariantArray};
use theme::SystemAppearance;
use ui::IntoElement;

use crate::{
    ActionLink, DynamicItem, PROJECT, SettingField, SettingItem, SettingsFieldMetadata,
    SettingsPage, SettingsPageItem, SubPageLink, USER, UiText, active_language, all_language_names,
    pages::{
        render_edit_prediction_setup_page, render_llm_providers_page, render_mcp_servers_page,
        render_tool_permissions_setup_page,
    },
};

const DEFAULT_STRING: String = String::new();
/// A default empty string reference. Useful in `pick` functions for cases either in dynamic item fields, or when dealing with `settings::Maybe`
/// to avoid the "NO DEFAULT" case.
const DEFAULT_EMPTY_STRING: Option<&String> = Some(&DEFAULT_STRING);

fn lt(key: &'static str, fallback: &'static str) -> UiText {
    UiText::localized(key, fallback)
}

macro_rules! concat_sections {
    (@vec, $($arr:expr),+ $(,)?) => {{
        let total_len = 0_usize $(+ $arr.len())+;
        let mut out = Vec::with_capacity(total_len);

        $(
            out.extend($arr);
        )+

        out.into_boxed_slice()
    }};

    ($($arr:expr),+ $(,)?) => {{
        let total_len = 0_usize $(+ $arr.len())+;

        let mut out: Box<[std::mem::MaybeUninit<_>]> = Box::new_uninit_slice(total_len);

        let mut index = 0usize;
        $(
            let array = $arr;
            for item in array {
                out[index].write(item);
                index += 1;
            }
        )+

        debug_assert_eq!(index, total_len);

        // SAFETY: we wrote exactly `total_len` elements.
        unsafe { out.assume_init() }
    }};
}

pub(crate) fn settings_data(cx: &App) -> Vec<SettingsPage> {
    let mut pages = vec![
        general_page(cx),
        appearance_page(),
        keymap_page(),
        editor_page(),
        languages_and_tools_page(cx),
        search_and_files_page(),
        window_and_layout_page(),
        panels_page(),
        debugger_page(),
        terminal_page(),
        version_control_page(),
        ai_page(cx),
        network_page(),
    ];

    use feature_flags::FeatureFlagAppExt as _;
    if cx.is_staff() || cfg!(debug_assertions) {
        pages.push(developer_page());
    }

    pages
}

fn developer_page() -> SettingsPage {
    SettingsPage {
        title: lt("settings_ui.page_data.title.developer", "Developer"),
        items: Box::new([
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.feature.flags",
                "Feature Flags",
            )),
            SettingsPageItem::SubPageLink(SubPageLink {
                title: lt("settings_ui.page_data.title.feature.flags", "Feature Flags"),
                r#type: Default::default(),
                description: None,
                search_aliases: &[],
                json_path: Some("feature_flags"),
                in_json: true,
                files: USER,
                render: crate::pages::render_feature_flags_page,
            }),
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.instrumentation",
                "Instrumentation",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.performance.profiler",
                    "Performance Profiler",
                ),
                description: lt(
                    "settings_ui.page_data.description.collect.timing.data.for.foreground.and.background.executor.tasks.so.they.can.be.inspected.via.zzz.open.performance.profiler.may.lead.to.increased.memory.usage",
                    "Collect timing data for foreground and background executor tasks so they can be inspected via `zzz: open performance profiler`. May lead to increased memory usage.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("instrumentation.performance_profiler.enabled"),
                    pick: |settings_content| {
                        settings_content
                            .instrumentation
                            .as_ref()
                            .and_then(|i| i.performance_profiler.as_ref())
                            .and_then(|p| p.enabled.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .instrumentation
                            .get_or_insert_default()
                            .performance_profiler
                            .get_or_insert_default()
                            .enabled = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]),
    }
}

fn general_page(cx: &App) -> SettingsPage {
    fn general_settings_section(_cx: &App) -> Vec<SettingsPageItem> {
        vec![
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.general.settings",
                "General Settings",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.display.language",
                    "Display Language",
                ),
                description: lt(
                    "settings_ui.page_data.description.choose.the.language.used.for.the.zzz.user.interface",
                    "Choose the language used for the ZZZ user interface.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("display_language"),
                    pick: |settings_content| settings_content.workspace.display_language.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.workspace.display_language = value;
                    },
                }),
                metadata: Some(Box::new(SettingsFieldMetadata {
                    should_do_titlecase: Some(false),
                    ..Default::default()
                })),
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.when.closing.with.no.tabs",
                    "When Closing With No Tabs",
                ),
                description: lt(
                    "settings_ui.page_data.description.what.to.do.when.using.the.close.active.item.action.with.no.tabs",
                    "What to do when using the 'close active item' action with no tabs.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("when_closing_with_no_tabs"),
                    pick: |settings_content| {
                        settings_content
                            .workspace
                            .when_closing_with_no_tabs
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.workspace.when_closing_with_no_tabs = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.on.new.window", "On New Window"),
                description: lt(
                    "settings_ui.page_data.description.what.to.show.when.opening.a.new.window",
                    "What to show when opening a new window.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("on_new_window"),
                    pick: |settings_content| settings_content.workspace.on_new_window.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.workspace.on_new_window = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.on.last.window.closed",
                    "On Last Window Closed",
                ),
                description: lt(
                    "settings_ui.page_data.description.what.to.do.when.the.last.window.is.closed",
                    "What to do when the last window is closed.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("on_last_window_closed"),
                    pick: |settings_content| {
                        settings_content.workspace.on_last_window_closed.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.workspace.on_last_window_closed = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.use.system.path.prompts",
                    "Use System Path Prompts",
                ),
                description: lt(
                    "settings_ui.page_data.description.use.native.os.dialogs.for.open.and.save.as",
                    "Use native OS dialogs for 'Open' and 'Save As'.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("use_system_path_prompts"),
                    pick: |settings_content| {
                        settings_content.workspace.use_system_path_prompts.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.workspace.use_system_path_prompts = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.use.system.prompts",
                    "Use System Prompts",
                ),
                description: lt(
                    "settings_ui.page_data.description.use.native.os.dialogs.for.confirmations",
                    "Use native OS dialogs for confirmations.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("use_system_prompts"),
                    pick: |settings_content| settings_content.workspace.use_system_prompts.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.workspace.use_system_prompts = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.redact.private.values",
                    "Redact Private Values",
                ),
                description: lt(
                    "settings_ui.page_data.description.hide.the.values.of.variables.in.private.files",
                    "Hide the values of variables in private files.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("redact_private_values"),
                    pick: |settings_content| settings_content.editor.redact_private_values.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.redact_private_values = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.private.files", "Private Files"),
                description: lt(
                    "settings_ui.page_data.description.globs.to.match.against.file.paths.to.determine.if.a.file.is.private",
                    "Globs to match against file paths to determine if a file is private.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("worktree.private_files"),
                        pick: |settings_content| {
                            settings_content.project.worktree.private_files.as_ref()
                        },
                        write: |settings_content, value, _| {
                            settings_content.project.worktree.private_files = value;
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.cli.default.open.behavior",
                    "CLI Default Open Behavior",
                ),
                description: lt(
                    "settings_ui.page_data.description.how.zzz.path.opens.directories.when.no.flag.is.specified",
                    "How `zzz <path>` opens directories when no flag is specified.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("cli_default_open_behavior"),
                    pick: |settings_content| {
                        settings_content
                            .workspace
                            .cli_default_open_behavior
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.workspace.cli_default_open_behavior = value;
                    },
                }),
                metadata: Some(Box::new(SettingsFieldMetadata {
                    should_do_titlecase: Some(false),
                    ..Default::default()
                })),
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.default.open.behavior",
                    "Default Open Behavior",
                ),
                description: lt(
                    "settings_ui.page_data.description.how.projects.open.from.the.ui.by.default",
                    "How projects open from the UI by default.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("default_open_behavior"),
                    pick: |settings_content| {
                        settings_content.workspace.default_open_behavior.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.workspace.default_open_behavior = value;
                    },
                }),
                metadata: Some(Box::new(SettingsFieldMetadata {
                    should_do_titlecase: Some(false),
                    ..Default::default()
                })),
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.reveal.if.open",
                    "Reveal If Open",
                ),
                description: lt(
                    "settings_ui.page_data.description.when.enabled.zzz.prefers.an.already.open.file.in.another.pane",
                    "When enabled, ZZZ prefers an already-open file in another pane.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("reveal_if_open"),
                    pick: |settings_content| settings_content.workspace.reveal_if_open.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.workspace.reveal_if_open = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }
    fn security_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.security",
                "Security",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.trust.all.projects.by.default",
                    "Trust All Projects By Default",
                ),
                description: lt(
                    "settings_ui.page_data.description.when.opening.zzz.avoid.restricted.mode.by.auto.trusting.all.projects.enabling.use.of.all.features.without.having.to.give.permission.to.each.new.project",
                    "When opening ZZZ, avoid Restricted Mode by auto-trusting all projects, enabling use of all features without having to give permission to each new project.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("session.trust_all_projects"),
                    pick: |settings_content| {
                        settings_content
                            .session
                            .as_ref()
                            .and_then(|session| session.trust_all_worktrees.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .session
                            .get_or_insert_default()
                            .trust_all_worktrees = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn workspace_restoration_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.workspace.restoration",
                "Workspace Restoration",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.restore.unsaved.buffers",
                    "Restore Unsaved Buffers",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.or.not.to.restore.unsaved.buffers.on.restart",
                    "Whether or not to restore unsaved buffers on restart.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("session.restore_unsaved_buffers"),
                    pick: |settings_content| {
                        settings_content
                            .session
                            .as_ref()
                            .and_then(|session| session.restore_unsaved_buffers.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .session
                            .get_or_insert_default()
                            .restore_unsaved_buffers = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.restore.on.startup",
                    "Restore On Startup",
                ),
                description: lt(
                    "settings_ui.page_data.description.what.to.restore.from.the.previous.session.when.opening.zzz",
                    "What to restore from the previous session when opening ZZZ.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("restore_on_startup"),
                    pick: |settings_content| settings_content.workspace.restore_on_startup.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.workspace.restore_on_startup = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn scoped_settings_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.scoped.settings",
                "Scoped Settings",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                files: USER,
                title: lt(
                    "settings_ui.page_data.title.settings.profiles",
                    "Settings Profiles",
                ),
                description: lt(
                    "settings_ui.page_data.description.any.number.of.settings.profiles.that.are.temporarily.applied.on.top.of.your.existing.user.settings",
                    "Any number of settings profiles that are temporarily applied on top of your existing user settings.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("settings_profiles"),
                        pick: |settings_content| Some(settings_content),
                        write: |_settings_content, _value, _| {},
                    }
                    .unimplemented(),
                ),
                metadata: None,
            }),
        ]
    }

    SettingsPage {
        title: lt("settings_ui.page_data.title.general", "General"),
        items: concat_sections!(
            @vec,
            general_settings_section(cx),
            security_section(),
            workspace_restoration_section(),
            scoped_settings_section(),
        ),
    }
}

fn appearance_page() -> SettingsPage {
    fn theme_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.theme", "Theme")),
            SettingsPageItem::DynamicItem(DynamicItem {
                discriminant: SettingItem {
                    files: USER,
                    title: lt("settings_ui.page_data.title.theme.mode", "Theme Mode"),
                    description: lt("settings_ui.page_data.description.choose.a.static.fixed.theme.or.dynamically.select.themes.based.on.appearance.and.light.dark.modes", "Choose a static, fixed theme or dynamically select themes based on appearance and light/dark modes."),
                    field: Box::new(SettingField {
                        json_path: Some("theme$"),
                        pick: |settings_content| {
                            Some(&dynamic_variants::<settings::ThemeSelection>()[
                                settings_content
                                    .theme
                                    .theme
                                    .as_ref()?
                                    .discriminant() as usize])
                        },
                        write: |settings_content, value, app: &App| {
                            let Some(value) = value else {
                                settings_content.theme.theme = None;
                                return;
                            };
                            let settings_value = settings_content.theme.theme.get_or_insert_default();
                            *settings_value = match value {
                                settings::ThemeSelectionDiscriminants::Static => {
                                    let name = match settings_value {
                                        settings::ThemeSelection::Static(_) => return,
                                        settings::ThemeSelection::Dynamic { mode, light, dark } => {
                                            match mode {
                                                theme_settings::ThemeAppearanceMode::Light => light.clone(),
                                                theme_settings::ThemeAppearanceMode::Dark => dark.clone(),
                                                theme_settings::ThemeAppearanceMode::System => {
                                                    if SystemAppearance::global(app).is_light() {
                                                        light.clone()
                                                    } else {
                                                        dark.clone()
                                                    }
                                                }
                                            }
                                        },
                                    };
                                    settings::ThemeSelection::Static(name)
                                },
                                settings::ThemeSelectionDiscriminants::Dynamic => {
                                    let static_name = match settings_value {
                                        settings::ThemeSelection::Static(theme_name) => theme_name.clone(),
                                        settings::ThemeSelection::Dynamic {..} => return,
                                    };

                                    settings::ThemeSelection::Dynamic {
                                        mode: settings::ThemeAppearanceMode::System,
                                        light: static_name.clone(),
                                        dark: static_name,
                                    }
                                },
                            };
                        },
                    }),
                    metadata: None,
                },
                pick_discriminant: |settings_content| {
                    Some(settings_content.theme.theme.as_ref()?.discriminant() as usize)
                },
                fields: dynamic_variants::<settings::ThemeSelection>().into_iter().map(|variant| {
                    match variant {
                        settings::ThemeSelectionDiscriminants::Static => vec![
                            SettingItem {
                                files: USER,
                                title: lt("settings_ui.page_data.title.theme.name", "Theme Name"),
                                description: lt("settings_ui.page_data.description.the.name.of.your.selected.theme", "The name of your selected theme."),
                                field: Box::new(SettingField {
                                    json_path: Some("theme"),
                                    pick: |settings_content| {
                                        match settings_content.theme.theme.as_ref() {
                                            Some(settings::ThemeSelection::Static(name)) => Some(name),
                                            _ => None
                                        }
                                    },
                                    write: |settings_content, value, _| {
                                        let Some(value) = value else {
                                            return;
                                        };
                                        match settings_content
                                            .theme
                                            .theme.get_or_insert_default() {
                                                settings::ThemeSelection::Static(theme_name) => *theme_name = value,
                                                _ => return
                                            }
                                    },
                                }),
                                metadata: None,
                            }
                        ],
                        settings::ThemeSelectionDiscriminants::Dynamic => vec![
                            SettingItem {
                                files: USER,
                                title: lt("settings_ui.page_data.title.mode", "Mode"),
                                description: lt("settings_ui.page_data.description.choose.whether.to.use.the.selected.light.or.dark.theme.or.to.follow.your.os.appearance.configuration", "Choose whether to use the selected light or dark theme or to follow your OS appearance configuration."),
                                field: Box::new(SettingField {
                                    json_path: Some("theme.mode"),
                                    pick: |settings_content| {
                                        match settings_content.theme.theme.as_ref() {
                                            Some(settings::ThemeSelection::Dynamic { mode, ..}) => Some(mode),
                                            _ => None
                                        }
                                    },
                                    write: |settings_content, value, _| {
                                        let Some(value) = value else {
                                            return;
                                        };
                                        match settings_content
                                            .theme
                                            .theme.get_or_insert_default() {
                                                settings::ThemeSelection::Dynamic{ mode, ..} => *mode = value,
                                                _ => return
                                            }
                                    },
                                }),
                                metadata: None,
                            },
                            SettingItem {
                                files: USER,
                                title: lt("settings_ui.page_data.title.light.theme", "Light Theme"),
                                description: lt("settings_ui.page_data.description.the.theme.to.use.when.mode.is.set.to.light.or.when.mode.is.set.to.system.and.it.is.in.light.mode", "The theme to use when mode is set to light, or when mode is set to system and it is in light mode."),
                                field: Box::new(SettingField {
                                    json_path: Some("theme.light"),
                                    pick: |settings_content| {
                                        match settings_content.theme.theme.as_ref() {
                                            Some(settings::ThemeSelection::Dynamic { light, ..}) => Some(light),
                                            _ => None
                                        }
                                    },
                                    write: |settings_content, value, _| {
                                        let Some(value) = value else {
                                            return;
                                        };
                                        match settings_content
                                            .theme
                                            .theme.get_or_insert_default() {
                                                settings::ThemeSelection::Dynamic{ light, ..} => *light = value,
                                                _ => return
                                            }
                                    },
                                }),
                                metadata: None,
                            },
                            SettingItem {
                                files: USER,
                                title: lt("settings_ui.page_data.title.dark.theme", "Dark Theme"),
                                description: lt("settings_ui.page_data.description.the.theme.to.use.when.mode.is.set.to.dark.or.when.mode.is.set.to.system.and.it.is.in.dark.mode", "The theme to use when mode is set to dark, or when mode is set to system and it is in dark mode."),
                                field: Box::new(SettingField {
                                    json_path: Some("theme.dark"),
                                    pick: |settings_content| {
                                        match settings_content.theme.theme.as_ref() {
                                            Some(settings::ThemeSelection::Dynamic { dark, ..}) => Some(dark),
                                            _ => None
                                        }
                                    },
                                    write: |settings_content, value, _| {
                                        let Some(value) = value else {
                                            return;
                                        };
                                        match settings_content
                                            .theme
                                            .theme.get_or_insert_default() {
                                                settings::ThemeSelection::Dynamic{ dark, ..} => *dark = value,
                                                _ => return
                                            }
                                    },
                                }),
                                metadata: None,
                            }
                        ],
                    }
                }).collect(),
            }),
            SettingsPageItem::DynamicItem(DynamicItem {
                discriminant: SettingItem {
                    files: USER,
                    title: lt("settings_ui.page_data.title.icon.theme", "Icon Theme"),
                    description: lt("settings_ui.page_data.description.the.custom.set.of.icons.zzz.will.associate.with.files.and.directories", "The custom set of icons ZZZ will associate with files and directories."),
                    field: Box::new(SettingField {
                        json_path: Some("icon_theme$"),
                        pick: |settings_content| {
                            Some(&dynamic_variants::<settings::IconThemeSelection>()[
                                settings_content
                                    .theme
                                    .icon_theme
                                    .as_ref()?
                                    .discriminant() as usize])
                        },
                        write: |settings_content, value, app| {
                            let Some(value) = value else {
                                settings_content.theme.icon_theme = None;
                                return;
                            };
                            let settings_value = settings_content.theme.icon_theme.get_or_insert_with(|| {
                                settings::IconThemeSelection::Static(settings::IconThemeName(theme::default_icon_theme().name.clone().into()))
                            });
                            *settings_value = match value {
                                settings::IconThemeSelectionDiscriminants::Static => {
                                    let name = match settings_value {
                                        settings::IconThemeSelection::Static(_) => return,
                                        settings::IconThemeSelection::Dynamic { mode, light, dark } => {
                                            match mode {
                                                theme_settings::ThemeAppearanceMode::Light => light.clone(),
                                                theme_settings::ThemeAppearanceMode::Dark => dark.clone(),
                                                theme_settings::ThemeAppearanceMode::System => {
                                                    if SystemAppearance::global(app).is_light() {
                                                        light.clone()
                                                    } else {
                                                        dark.clone()
                                                    }
                                                }
                                            }
                                        },
                                    };
                                    settings::IconThemeSelection::Static(name)
                                },
                                settings::IconThemeSelectionDiscriminants::Dynamic => {
                                    let static_name = match settings_value {
                                        settings::IconThemeSelection::Static(theme_name) => theme_name.clone(),
                                        settings::IconThemeSelection::Dynamic {..} => return,
                                    };

                                    settings::IconThemeSelection::Dynamic {
                                        mode: settings::ThemeAppearanceMode::System,
                                        light: static_name.clone(),
                                        dark: static_name,
                                    }
                                },
                            };
                        },
                    }),
                    metadata: None,
                },
                pick_discriminant: |settings_content| {
                    Some(settings_content.theme.icon_theme.as_ref()?.discriminant() as usize)
                },
                fields: dynamic_variants::<settings::IconThemeSelection>().into_iter().map(|variant| {
                    match variant {
                        settings::IconThemeSelectionDiscriminants::Static => vec![
                            SettingItem {
                                files: USER,
                                title: lt("settings_ui.page_data.title.icon.theme.name", "Icon Theme Name"),
                                description: lt("settings_ui.page_data.description.the.name.of.your.selected.icon.theme", "The name of your selected icon theme."),
                                field: Box::new(SettingField {
                                    json_path: Some("icon_theme$string"),
                                    pick: |settings_content| {
                                        match settings_content.theme.icon_theme.as_ref() {
                                            Some(settings::IconThemeSelection::Static(name)) => Some(name),
                                            _ => None
                                        }
                                    },
                                    write: |settings_content, value, _| {
                                        let Some(value) = value else {
                                            return;
                                        };
                                        match settings_content
                                            .theme
                                            .icon_theme.as_mut() {
                                                Some(settings::IconThemeSelection::Static(theme_name)) => *theme_name = value,
                                                _ => return
                                            }
                                    },
                                }),
                                metadata: None,
                            }
                        ],
                        settings::IconThemeSelectionDiscriminants::Dynamic => vec![
                            SettingItem {
                                files: USER,
                                title: lt("settings_ui.page_data.title.mode", "Mode"),
                                description: lt("settings_ui.page_data.description.choose.whether.to.use.the.selected.light.or.dark.icon.theme.or.to.follow.your.os.appearance.configuration", "Choose whether to use the selected light or dark icon theme or to follow your OS appearance configuration."),
                                field: Box::new(SettingField {
                                    json_path: Some("icon_theme"),
                                    pick: |settings_content| {
                                        match settings_content.theme.icon_theme.as_ref() {
                                            Some(settings::IconThemeSelection::Dynamic { mode, ..}) => Some(mode),
                                            _ => None
                                        }
                                    },
                                    write: |settings_content, value, _| {
                                        let Some(value) = value else {
                                            return;
                                        };
                                        match settings_content
                                            .theme
                                            .icon_theme.as_mut() {
                                                Some(settings::IconThemeSelection::Dynamic{ mode, ..}) => *mode = value,
                                                _ => return
                                            }
                                    },
                                }),
                                metadata: None,
                            },
                            SettingItem {
                                files: USER,
                                title: lt("settings_ui.page_data.title.light.icon.theme", "Light Icon Theme"),
                                description: lt("settings_ui.page_data.description.the.icon.theme.to.use.when.mode.is.set.to.light.or.when.mode.is.set.to.system.and.it.is.in.light.mode", "The icon theme to use when mode is set to light, or when mode is set to system and it is in light mode."),
                                field: Box::new(SettingField {
                                    json_path: Some("icon_theme.light"),
                                    pick: |settings_content| {
                                        match settings_content.theme.icon_theme.as_ref() {
                                            Some(settings::IconThemeSelection::Dynamic { light, ..}) => Some(light),
                                            _ => None
                                        }
                                    },
                                    write: |settings_content, value, _| {
                                        let Some(value) = value else {
                                            return;
                                        };
                                        match settings_content
                                            .theme
                                            .icon_theme.as_mut() {
                                                Some(settings::IconThemeSelection::Dynamic{ light, ..}) => *light = value,
                                                _ => return
                                            }
                                    },
                                }),
                                metadata: None,
                            },
                            SettingItem {
                                files: USER,
                                title: lt("settings_ui.page_data.title.dark.icon.theme", "Dark Icon Theme"),
                                description: lt("settings_ui.page_data.description.the.icon.theme.to.use.when.mode.is.set.to.dark.or.when.mode.is.set.to.system.and.it.is.in.dark.mode", "The icon theme to use when mode is set to dark, or when mode is set to system and it is in dark mode."),
                                field: Box::new(SettingField {
                                    json_path: Some("icon_theme.dark"),
                                    pick: |settings_content| {
                                        match settings_content.theme.icon_theme.as_ref() {
                                            Some(settings::IconThemeSelection::Dynamic { dark, ..}) => Some(dark),
                                            _ => None
                                        }
                                    },
                                    write: |settings_content, value, _| {
                                        let Some(value) = value else {
                                            return;
                                        };
                                        match settings_content
                                            .theme
                                            .icon_theme.as_mut() {
                                                Some(settings::IconThemeSelection::Dynamic{ dark, ..}) => *dark = value,
                                                _ => return
                                            }
                                    },
                                }),
                                metadata: None,
                            }
                        ],
                    }
                }).collect(),
            }),
        ]
    }

    fn buffer_font_section() -> [SettingsPageItem; 7] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.buffer.font", "Buffer Font")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.font.family", "Font Family"),
                description: lt("settings_ui.page_data.description.font.family.for.editor.text", "Font family for editor text."),
                field: Box::new(SettingField {
                    json_path: Some("buffer_font_family"),
                    pick: |settings_content| settings_content.theme.buffer_font_family.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.theme.buffer_font_family = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.font.size", "Font Size"),
                description: lt("settings_ui.page_data.description.font.size.for.editor.text", "Font size for editor text."),
                field: Box::new(SettingField {
                    json_path: Some("buffer_font_size"),
                    pick: |settings_content| settings_content.theme.buffer_font_size.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.theme.buffer_font_size = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.font.weight", "Font Weight"),
                description: lt("settings_ui.page_data.description.font.weight.for.editor.text.100.900", "Font weight for editor text (100-900)."),
                field: Box::new(SettingField {
                    json_path: Some("buffer_font_weight"),
                    pick: |settings_content| settings_content.theme.buffer_font_weight.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.theme.buffer_font_weight = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::DynamicItem(DynamicItem {
                discriminant: SettingItem {
                    files: USER,
                    title: lt("settings_ui.page_data.title.line.height", "Line Height"),
                    description: lt("settings_ui.page_data.description.line.height.for.editor.text", "Line height for editor text."),
                    field: Box::new(SettingField {
                        json_path: Some("buffer_line_height$"),
                        pick: |settings_content| {
                            Some(
                                &dynamic_variants::<settings::BufferLineHeight>()[settings_content
                                    .theme
                                    .buffer_line_height
                                    .as_ref()?
                                    .discriminant()
                                    as usize],
                            )
                        },
                        write: |settings_content, value, _| {
                            let Some(value) = value else {
                                settings_content.theme.buffer_line_height = None;
                                return;
                            };
                            let settings_value = settings_content
                                .theme
                                .buffer_line_height
                                .get_or_insert_with(|| settings::BufferLineHeight::default());
                            *settings_value = match value {
                                settings::BufferLineHeightDiscriminants::Comfortable => {
                                    settings::BufferLineHeight::Comfortable
                                }
                                settings::BufferLineHeightDiscriminants::Standard => {
                                    settings::BufferLineHeight::Standard
                                }
                                settings::BufferLineHeightDiscriminants::Custom => {
                                    let custom_value =
                                        theme_settings::BufferLineHeight::from(*settings_value)
                                            .value();
                                    settings::BufferLineHeight::Custom(custom_value)
                                }
                            };
                        },
                    }),
                    metadata: None,
                },
                pick_discriminant: |settings_content| {
                    Some(
                        settings_content
                            .theme
                            .buffer_line_height
                            .as_ref()?
                            .discriminant() as usize,
                    )
                },
                fields: dynamic_variants::<settings::BufferLineHeight>()
                    .into_iter()
                    .map(|variant| match variant {
                        settings::BufferLineHeightDiscriminants::Comfortable => vec![],
                        settings::BufferLineHeightDiscriminants::Standard => vec![],
                        settings::BufferLineHeightDiscriminants::Custom => vec![SettingItem {
                            files: USER,
                            title: lt("settings_ui.page_data.title.custom.line.height", "Custom Line Height"),
                            description: lt("settings_ui.page_data.description.custom.line.height.value.must.be.at.least.1.0", "Custom line height value (must be at least 1.0)."),
                            field: Box::new(SettingField {
                                json_path: Some("buffer_line_height"),
                                pick: |settings_content| match settings_content
                                    .theme
                                    .buffer_line_height
                                    .as_ref()
                                {
                                    Some(settings::BufferLineHeight::Custom(value)) => Some(value),
                                    _ => None,
                                },
                                write: |settings_content, value, _| {
                                    let Some(value) = value else {
                                        return;
                                    };
                                    match settings_content.theme.buffer_line_height.as_mut() {
                                        Some(settings::BufferLineHeight::Custom(line_height)) => {
                                            *line_height = f32::max(value, 1.0)
                                        }
                                        _ => return,
                                    }
                                },
                            }),
                            metadata: None,
                        }],
                    })
                    .collect(),
            }),
            SettingsPageItem::SettingItem(SettingItem {
                files: USER,
                title: lt("settings_ui.page_data.title.font.features", "Font Features"),
                description: lt("settings_ui.page_data.description.the.opentype.features.to.enable.for.rendering.in.text.buffers", "The OpenType features to enable for rendering in text buffers."),
                field: Box::new(
                    SettingField {
                        json_path: Some("buffer_font_features"),
                        pick: |settings_content| {
                            settings_content.theme.buffer_font_features.as_ref()
                        },
                        write: |settings_content, value, _| {
                            settings_content.theme.buffer_font_features = value;
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                files: USER,
                title: lt("settings_ui.page_data.title.font.fallbacks", "Font Fallbacks"),
                description: lt("settings_ui.page_data.description.the.font.fallbacks.to.use.for.rendering.in.text.buffers", "The font fallbacks to use for rendering in text buffers."),
                field: Box::new(
                    SettingField {
                        json_path: Some("buffer_font_fallbacks"),
                        pick: |settings_content| {
                            settings_content.theme.buffer_font_fallbacks.as_ref()
                        },
                        write: |settings_content, value, _| {
                            settings_content.theme.buffer_font_fallbacks = value;
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
            }),
        ]
    }

    fn ui_font_section() -> [SettingsPageItem; 6] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.ui.font", "UI Font")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.font.family", "Font Family"),
                description: lt(
                    "settings_ui.page_data.description.font.family.for.ui.elements",
                    "Font family for UI elements.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("ui_font_family"),
                    pick: |settings_content| settings_content.theme.ui_font_family.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.theme.ui_font_family = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.font.size", "Font Size"),
                description: lt(
                    "settings_ui.page_data.description.font.size.for.ui.elements",
                    "Font size for UI elements.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("ui_font_size"),
                    pick: |settings_content| settings_content.theme.ui_font_size.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.theme.ui_font_size = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.font.weight", "Font Weight"),
                description: lt(
                    "settings_ui.page_data.description.font.weight.for.ui.elements.100.900",
                    "Font weight for UI elements (100-900).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("ui_font_weight"),
                    pick: |settings_content| settings_content.theme.ui_font_weight.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.theme.ui_font_weight = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                files: USER,
                title: lt("settings_ui.page_data.title.font.features", "Font Features"),
                description: lt(
                    "settings_ui.page_data.description.the.opentype.features.to.enable.for.rendering.in.ui.elements",
                    "The OpenType features to enable for rendering in UI elements.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("ui_font_features"),
                        pick: |settings_content| settings_content.theme.ui_font_features.as_ref(),
                        write: |settings_content, value, _| {
                            settings_content.theme.ui_font_features = value;
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                files: USER,
                title: lt(
                    "settings_ui.page_data.title.font.fallbacks",
                    "Font Fallbacks",
                ),
                description: lt(
                    "settings_ui.page_data.description.the.font.fallbacks.to.use.for.rendering.in.the.ui",
                    "The font fallbacks to use for rendering in the UI.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("ui_font_fallbacks"),
                        pick: |settings_content| settings_content.theme.ui_font_fallbacks.as_ref(),
                        write: |settings_content, value, _| {
                            settings_content.theme.ui_font_fallbacks = value;
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
            }),
        ]
    }

    fn agent_panel_font_section() -> [SettingsPageItem; 5] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.agent.panel.font",
                "Agent Panel Font",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.ui.font.family",
                    "UI Font Family",
                ),
                description: lt(
                    "settings_ui.page_data.description.font.family.for.agent.response.text.in.the.agent.panel.falls.back.to.the.regular.ui.font.family",
                    "Font family for agent response text in the agent panel. Falls back to the regular UI font family.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("agent_ui_font_family"),
                    pick: |settings_content| {
                        settings_content
                            .theme
                            .agent_ui_font_family
                            .as_ref()
                            .or(settings_content.theme.ui_font_family.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content.theme.agent_ui_font_family = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.ui.font.size", "UI Font Size"),
                description: lt(
                    "settings_ui.page_data.description.font.size.for.agent.response.text.in.the.agent.panel.falls.back.to.the.regular.ui.font.size",
                    "Font size for agent response text in the agent panel. Falls back to the regular UI font size.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("agent_ui_font_size"),
                    pick: |settings_content| {
                        settings_content
                            .theme
                            .agent_ui_font_size
                            .as_ref()
                            .or(settings_content.theme.ui_font_size.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content.theme.agent_ui_font_size = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.buffer.font.family",
                    "Buffer Font Family",
                ),
                description: lt(
                    "settings_ui.page_data.description.font.family.for.user.messages.in.the.agent.panel.falls.back.to.the.regular.buffer.font.family",
                    "Font family for user messages in the agent panel. Falls back to the regular buffer font family.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("agent_buffer_font_family"),
                    pick: |settings_content| {
                        settings_content
                            .theme
                            .agent_buffer_font_family
                            .as_ref()
                            .or(settings_content.theme.buffer_font_family.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content.theme.agent_buffer_font_family = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.buffer.font.size",
                    "Buffer Font Size",
                ),
                description: lt(
                    "settings_ui.page_data.description.font.size.for.user.messages.text.in.the.agent.panel",
                    "Font size for user messages text in the agent panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("agent_buffer_font_size"),
                    pick: |settings_content| {
                        settings_content
                            .theme
                            .agent_buffer_font_size
                            .as_ref()
                            .or(settings_content.theme.buffer_font_size.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content.theme.agent_buffer_font_size = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn text_rendering_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.text.rendering",
                "Text Rendering",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.text.rendering.mode",
                    "Text Rendering Mode",
                ),
                description: lt(
                    "settings_ui.page_data.description.the.text.rendering.mode.to.use",
                    "The text rendering mode to use.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("text_rendering_mode"),
                    pick: |settings_content| {
                        settings_content.workspace.text_rendering_mode.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.workspace.text_rendering_mode = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn cursor_section() -> [SettingsPageItem; 7] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.cursor", "Cursor")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.multi.cursor.modifier",
                    "Multi Cursor Modifier",
                ),
                description: lt(
                    "settings_ui.page_data.description.modifier.key.for.adding.multiple.cursors",
                    "Modifier key for adding multiple cursors.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("multi_cursor_modifier"),
                    pick: |settings_content| settings_content.editor.multi_cursor_modifier.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.multi_cursor_modifier = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.cursor.blink", "Cursor Blink"),
                description: lt(
                    "settings_ui.page_data.description.whether.the.cursor.blinks.in.the.editor",
                    "Whether the cursor blinks in the editor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("cursor_blink"),
                    pick: |settings_content| settings_content.editor.cursor_blink.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.cursor_blink = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.cursor.shape", "Cursor Shape"),
                description: lt(
                    "settings_ui.page_data.description.cursor.shape.for.the.editor",
                    "Cursor shape for the editor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("cursor_shape"),
                    pick: |settings_content| settings_content.editor.cursor_shape.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.cursor_shape = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.cursor.animation",
                    "Cursor Animation",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.the.cursor.smoothly.animates",
                    "Whether the cursor smoothly animates when moving around the editor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("cursor_animation.enabled"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .cursor_animation
                            .as_ref()?
                            .enabled
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .cursor_animation
                            .get_or_insert_default()
                            .enabled = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.smooth.scrolling",
                    "Smooth Scrolling",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.scrolling.animates.smoothly",
                    "Whether scrolling animates smoothly.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("smooth_scroll.enabled"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .smooth_scroll
                            .as_ref()?
                            .enabled
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .smooth_scroll
                            .get_or_insert_default()
                            .enabled = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.hide.mouse", "Hide Mouse"),
                description: lt(
                    "settings_ui.page_data.description.when.to.hide.the.mouse.cursor",
                    "When to hide the mouse cursor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("hide_mouse"),
                    pick: |settings_content| settings_content.hide_mouse.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.hide_mouse = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn highlighting_section() -> [SettingsPageItem; 6] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.highlighting",
                "Highlighting",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.unnecessary.code.fade",
                    "Unnecessary Code Fade",
                ),
                description: lt(
                    "settings_ui.page_data.description.how.much.to.fade.out.unused.code.0.0.0.9",
                    "How much to fade out unused code (0.0 - 0.9).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("unnecessary_code_fade"),
                    pick: |settings_content| settings_content.theme.unnecessary_code_fade.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.theme.unnecessary_code_fade = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.current.line.highlight",
                    "Current Line Highlight",
                ),
                description: lt(
                    "settings_ui.page_data.description.how.to.highlight.the.current.line",
                    "How to highlight the current line.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("current_line_highlight"),
                    pick: |settings_content| {
                        settings_content.editor.current_line_highlight.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.current_line_highlight = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.selection.highlight",
                    "Selection Highlight",
                ),
                description: lt(
                    "settings_ui.page_data.description.highlight.all.occurrences.of.selected.text",
                    "Highlight all occurrences of selected text.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("selection_highlight"),
                    pick: |settings_content| settings_content.editor.selection_highlight.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.selection_highlight = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.rounded.selection",
                    "Rounded Selection",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.the.text.selection.should.have.rounded.corners",
                    "Whether the text selection should have rounded corners.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("rounded_selection"),
                    pick: |settings_content| settings_content.editor.rounded_selection.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.rounded_selection = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.minimum.contrast.for.highlights",
                    "Minimum Contrast For Highlights",
                ),
                description: lt(
                    "settings_ui.page_data.description.the.minimum.apca.perceptual.contrast.to.maintain.when.rendering.text.over.highlight.backgrounds",
                    "The minimum APCA perceptual contrast to maintain when rendering text over highlight backgrounds.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("minimum_contrast_for_highlights"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .minimum_contrast_for_highlights
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.minimum_contrast_for_highlights = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn guides_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.guides", "Guides")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.wrap.guides",
                    "Show Wrap Guides",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.wrap.guides.vertical.rulers",
                    "Show wrap guides (vertical rulers).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("show_wrap_guides"),
                    pick: |settings_content| {
                        settings_content
                            .project
                            .all_languages
                            .defaults
                            .show_wrap_guides
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project
                            .all_languages
                            .defaults
                            .show_wrap_guides = value;
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            // todo(settings_ui): This needs a custom component
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.wrap.guides", "Wrap Guides"),
                description: lt(
                    "settings_ui.page_data.description.character.counts.at.which.to.show.wrap.guides",
                    "Character counts at which to show wrap guides.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("wrap_guides"),
                        pick: |settings_content| {
                            settings_content
                                .project
                                .all_languages
                                .defaults
                                .wrap_guides
                                .as_ref()
                        },
                        write: |settings_content, value, _| {
                            settings_content.project.all_languages.defaults.wrap_guides = value;
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    let items: Box<[SettingsPageItem]> = concat_sections!(
        theme_section(),
        buffer_font_section(),
        ui_font_section(),
        agent_panel_font_section(),
        text_rendering_section(),
        cursor_section(),
        highlighting_section(),
        guides_section(),
    );

    SettingsPage {
        title: lt("settings_ui.page_data.title.appearance", "Appearance"),
        items,
    }
}

fn keymap_page() -> SettingsPage {
    fn keybindings_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.keybindings",
                "Keybindings",
            )),
            SettingsPageItem::ActionLink(ActionLink {
                title: lt(
                    "settings_ui.page_data.title.edit.keybindings",
                    "Edit Keybindings",
                ),
                description: Some(lt(
                    "settings_ui.page_data.description.customize.keybindings.in.the.keymap.editor",
                    "Customize keybindings in the keymap editor.",
                )),
                button_text: lt("settings_ui.page_data.button.open.keymap", "Open Keymap"),
                on_click: Arc::new(|settings_window, window, cx| {
                    let Some(original_window) = settings_window.original_window else {
                        return;
                    };
                    original_window
                        .update(cx, |_workspace, original_window, cx| {
                            original_window
                                .dispatch_action(zzz_actions::OpenKeymap.boxed_clone(), cx);
                            original_window.activate_window();
                        })
                        .ok();
                    window.remove_window();
                }),
                files: USER,
            }),
        ]
    }

    fn base_keymap_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.base.keymap",
                "Base Keymap",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.base.keymap", "Base Keymap"),
                description: lt(
                    "settings_ui.page_data.description.the.name.of.a.base.set.of.key.bindings.to.use",
                    "The name of a base set of key bindings to use.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("base_keymap"),
                    pick: |settings_content| settings_content.base_keymap.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.base_keymap = value;
                    },
                }),
                metadata: Some(Box::new(SettingsFieldMetadata {
                    should_do_titlecase: Some(false),
                    ..Default::default()
                })),
                files: USER,
            }),
        ]
    }

    fn modal_editing_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.modal.editing",
                "Modal Editing",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.vim.mode", "Vim Mode"),
                description: lt(
                    "settings_ui.page_data.description.enable.vim.mode.and.key.bindings",
                    "Enable Vim mode and key bindings.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("vim_mode"),
                    pick: |settings_content| settings_content.vim_mode.as_ref(),
                    write: write_vim_mode,
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.helix.mode", "Helix Mode"),
                description: lt(
                    "settings_ui.page_data.description.enable.helix.mode.and.key.bindings",
                    "Enable Helix mode and key bindings.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("helix_mode"),
                    pick: |settings_content| settings_content.helix_mode.as_ref(),
                    write: write_helix_mode,
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    let items: Box<[SettingsPageItem]> = concat_sections!(
        keybindings_section(),
        base_keymap_section(),
        modal_editing_section(),
    );

    SettingsPage {
        title: lt("settings_ui.page_data.title.keymap", "Keymap"),
        items,
    }
}

fn editor_page() -> SettingsPage {
    fn auto_save_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.auto.save", "Auto Save")),
            SettingsPageItem::DynamicItem(DynamicItem {
                discriminant: SettingItem {
                    files: USER,
                    title: lt("settings_ui.page_data.title.auto.save.mode", "Auto Save Mode"),
                    description: lt("settings_ui.page_data.description.when.to.auto.save.buffer.changes", "When to auto save buffer changes."),
                    field: Box::new(SettingField {
                        json_path: Some("autosave$"),
                        pick: |settings_content| {
                            Some(
                                &dynamic_variants::<settings::AutosaveSetting>()[settings_content
                                    .workspace
                                    .autosave
                                    .as_ref()?
                                    .discriminant()
                                    as usize],
                            )
                        },
                        write: |settings_content, value, _| {
                            let Some(value) = value else {
                                settings_content.workspace.autosave = None;
                                return;
                            };
                            let settings_value = settings_content
                                .workspace
                                .autosave
                                .get_or_insert_with(|| settings::AutosaveSetting::Off);
                            *settings_value = match value {
                                settings::AutosaveSettingDiscriminants::Off => {
                                    settings::AutosaveSetting::Off
                                }
                                settings::AutosaveSettingDiscriminants::AfterDelay => {
                                    let milliseconds = match settings_value {
                                        settings::AutosaveSetting::AfterDelay { milliseconds } => {
                                            *milliseconds
                                        }
                                        _ => settings::DelayMs(1000),
                                    };
                                    settings::AutosaveSetting::AfterDelay { milliseconds }
                                }
                                settings::AutosaveSettingDiscriminants::OnFocusChange => {
                                    settings::AutosaveSetting::OnFocusChange
                                }
                                settings::AutosaveSettingDiscriminants::OnWindowChange => {
                                    settings::AutosaveSetting::OnWindowChange
                                }
                            };
                        },
                    }),
                    metadata: None,
                },
                pick_discriminant: |settings_content| {
                    Some(settings_content.workspace.autosave.as_ref()?.discriminant() as usize)
                },
                fields: dynamic_variants::<settings::AutosaveSetting>()
                    .into_iter()
                    .map(|variant| match variant {
                        settings::AutosaveSettingDiscriminants::Off => vec![],
                        settings::AutosaveSettingDiscriminants::AfterDelay => vec![SettingItem {
                            files: USER,
                            title: lt("settings_ui.page_data.title.delay.milliseconds", "Delay (milliseconds)"),
                            description: lt("settings_ui.page_data.description.save.after.inactivity.period.in.milliseconds", "Save after inactivity period (in milliseconds)."),
                            field: Box::new(SettingField {
                                json_path: Some("autosave.after_delay.milliseconds"),
                                pick: |settings_content| match settings_content
                                    .workspace
                                    .autosave
                                    .as_ref()
                                {
                                    Some(settings::AutosaveSetting::AfterDelay {
                                        milliseconds,
                                    }) => Some(milliseconds),
                                    _ => None,
                                },
                                write: |settings_content, value, _| {
                                    let Some(value) = value else {
                                        settings_content.workspace.autosave = None;
                                        return;
                                    };
                                    match settings_content.workspace.autosave.as_mut() {
                                        Some(settings::AutosaveSetting::AfterDelay {
                                            milliseconds,
                                        }) => *milliseconds = value,
                                        _ => return,
                                    }
                                },
                            }),
                            metadata: None,
                        }],
                        settings::AutosaveSettingDiscriminants::OnFocusChange => vec![],
                        settings::AutosaveSettingDiscriminants::OnWindowChange => vec![],
                    })
                    .collect(),
            }),
        ]
    }

    fn which_key_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.which.key.menu",
                "Which-key Menu",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.which.key.menu",
                    "Show Which-key Menu",
                ),
                description: lt(
                    "settings_ui.page_data.description.display.the.which.key.menu.with.matching.bindings.while.a.multi.stroke.binding.is.pending",
                    "Display the which-key menu with matching bindings while a multi-stroke binding is pending.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("which_key.enabled"),
                    pick: |settings_content| {
                        settings_content
                            .which_key
                            .as_ref()
                            .and_then(|settings| settings.enabled.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content.which_key.get_or_insert_default().enabled = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.menu.delay", "Menu Delay"),
                description: lt(
                    "settings_ui.page_data.description.delay.in.milliseconds.before.the.which.key.menu.appears",
                    "Delay in milliseconds before the which-key menu appears.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("which_key.delay_ms"),
                    pick: |settings_content| {
                        settings_content
                            .which_key
                            .as_ref()
                            .and_then(|settings| settings.delay_ms.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content.which_key.get_or_insert_default().delay_ms = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn multibuffer_section() -> [SettingsPageItem; 7] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.multibuffer",
                "Multibuffer",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.double.click.in.multibuffer",
                    "Double Click In Multibuffer",
                ),
                description: lt(
                    "settings_ui.page_data.description.what.to.do.when.multibuffer.is.double.clicked.in.some.of.its.excerpts",
                    "What to do when multibuffer is double-clicked in some of its excerpts.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("double_click_in_multibuffer"),
                    pick: |settings_content| {
                        settings_content.editor.double_click_in_multibuffer.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.double_click_in_multibuffer = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.expand.excerpt.lines",
                    "Expand Excerpt Lines",
                ),
                description: lt(
                    "settings_ui.page_data.description.how.many.lines.to.expand.the.multibuffer.excerpts.by.default",
                    "How many lines to expand the multibuffer excerpts by default.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("expand_excerpt_lines"),
                    pick: |settings_content| settings_content.editor.expand_excerpt_lines.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.expand_excerpt_lines = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.excerpt.context.lines",
                    "Excerpt Context Lines",
                ),
                description: lt(
                    "settings_ui.page_data.description.how.many.lines.of.context.to.provide.in.multibuffer.excerpts.by.default",
                    "How many lines of context to provide in multibuffer excerpts by default.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("excerpt_context_lines"),
                    pick: |settings_content| settings_content.editor.excerpt_context_lines.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.excerpt_context_lines = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.expand.outlines.with.depth",
                    "Expand Outlines With Depth",
                ),
                description: lt(
                    "settings_ui.page_data.description.default.depth.to.expand.outline.items.in.the.current.file",
                    "Default depth to expand outline items in the current file.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("outline_panel.expand_outlines_with_depth"),
                    pick: |settings_content| {
                        settings_content
                            .outline_panel
                            .as_ref()
                            .and_then(|outline_panel| {
                                outline_panel.expand_outlines_with_depth.as_ref()
                            })
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .outline_panel
                            .get_or_insert_default()
                            .expand_outlines_with_depth = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.diff.view.style",
                    "Diff View Style",
                ),
                description: lt(
                    "settings_ui.page_data.description.how.to.display.diffs.in.the.editor",
                    "How to display diffs in the editor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("diff_view_style"),
                    pick: |settings_content| settings_content.editor.diff_view_style.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.diff_view_style = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.minimum.split.diff.width",
                    "Minimum Split Diff Width",
                ),
                description: lt(
                    "settings_ui.page_data.description.the.minimum.width.in.columns.at.which.the.split.diff.view.is.used.when.the.editor.is.narrower.the.diff.view.automatically.switches.to.unified.mode.set.to.0.to.disable",
                    "The minimum width (in columns) at which the split diff view is used. When the editor is narrower, the diff view automatically switches to unified mode. Set to 0 to disable.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("minimum_split_diff_width"),
                    pick: |settings_content| {
                        settings_content.editor.minimum_split_diff_width.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.minimum_split_diff_width = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn scrolling_section() -> [SettingsPageItem; 9] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.scrolling",
                "Scrolling",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.scroll.beyond.last.line",
                    "Scroll Beyond Last Line",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.the.editor.will.scroll.beyond.the.last.line",
                    "Whether the editor will scroll beyond the last line.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("scroll_beyond_last_line"),
                    pick: |settings_content| {
                        settings_content.editor.scroll_beyond_last_line.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.scroll_beyond_last_line = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.vertical.scroll.margin",
                    "Vertical Scroll Margin",
                ),
                description: lt(
                    "settings_ui.page_data.description.the.number.of.lines.to.keep.above.below.the.cursor.when.auto.scrolling",
                    "The number of lines to keep above/below the cursor when auto-scrolling.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("vertical_scroll_margin"),
                    pick: |settings_content| {
                        settings_content.editor.vertical_scroll_margin.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.vertical_scroll_margin = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.horizontal.scroll.margin",
                    "Horizontal Scroll Margin",
                ),
                description: lt(
                    "settings_ui.page_data.description.the.number.of.characters.to.keep.on.either.side.when.scrolling.with.the.mouse",
                    "The number of characters to keep on either side when scrolling with the mouse.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("horizontal_scroll_margin"),
                    pick: |settings_content| {
                        settings_content.editor.horizontal_scroll_margin.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.horizontal_scroll_margin = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.scroll.sensitivity",
                    "Scroll Sensitivity",
                ),
                description: lt(
                    "settings_ui.page_data.description.scroll.sensitivity.multiplier.for.both.horizontal.and.vertical.scrolling",
                    "Scroll sensitivity multiplier for both horizontal and vertical scrolling.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("scroll_sensitivity"),
                    pick: |settings_content| settings_content.editor.scroll_sensitivity.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.scroll_sensitivity = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.mouse.wheel.zoom",
                    "Mouse Wheel Zoom",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.zoom.the.editor.font.size.with.the.mouse.wheel.while.holding.the.primary.modifier.key",
                    "Whether to zoom the editor font size with the mouse wheel while holding the primary modifier key.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("mouse_wheel_zoom"),
                    pick: |settings_content| settings_content.editor.mouse_wheel_zoom.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.mouse_wheel_zoom = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.fast.scroll.sensitivity",
                    "Fast Scroll Sensitivity",
                ),
                description: lt(
                    "settings_ui.page_data.description.fast.scroll.sensitivity.multiplier.for.both.horizontal.and.vertical.scrolling",
                    "Fast scroll sensitivity multiplier for both horizontal and vertical scrolling.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("fast_scroll_sensitivity"),
                    pick: |settings_content| {
                        settings_content.editor.fast_scroll_sensitivity.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.fast_scroll_sensitivity = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.autoscroll.on.clicks",
                    "Autoscroll On Clicks",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.scroll.when.clicking.near.the.edge.of.the.visible.text.area",
                    "Whether to scroll when clicking near the edge of the visible text area.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("autoscroll_on_clicks"),
                    pick: |settings_content| settings_content.editor.autoscroll_on_clicks.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.autoscroll_on_clicks = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.sticky.scroll", "Sticky Scroll"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.stick.scopes.to.the.top.of.the.editor",
                    "Whether to stick scopes to the top of the editor",
                ),
                field: Box::new(SettingField {
                    json_path: Some("sticky_scroll.enabled"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .sticky_scroll
                            .as_ref()
                            .and_then(|sticky_scroll| sticky_scroll.enabled.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .sticky_scroll
                            .get_or_insert_default()
                            .enabled = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn signature_help_section() -> [SettingsPageItem; 4] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.signature.help",
                "Signature Help",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.auto.signature.help",
                    "Auto Signature Help",
                ),
                description: lt(
                    "settings_ui.page_data.description.automatically.show.a.signature.help.pop.up",
                    "Automatically show a signature help pop-up.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("auto_signature_help"),
                    pick: |settings_content| settings_content.editor.auto_signature_help.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.auto_signature_help = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.signature.help.after.edits",
                    "Show Signature Help After Edits",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.signature.help.pop.up.after.completions.or.bracket.pairs.are.inserted",
                    "Show the signature help pop-up after completions or bracket pairs are inserted.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("show_signature_help_after_edits"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .show_signature_help_after_edits
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.show_signature_help_after_edits = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.snippet.sort.order",
                    "Snippet Sort Order",
                ),
                description: lt(
                    "settings_ui.page_data.description.determines.how.snippets.are.sorted.relative.to.other.completion.items",
                    "Determines how snippets are sorted relative to other completion items.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("snippet_sort_order"),
                    pick: |settings_content| settings_content.editor.snippet_sort_order.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.snippet_sort_order = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn hover_popover_section() -> [SettingsPageItem; 5] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.hover.popover",
                "Hover Popover",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.enabled", "Enabled"),
                description: lt(
                    "settings_ui.page_data.description.show.the.informational.hover.box.when.moving.the.mouse.over.symbols.in.the.editor",
                    "Show the informational hover box when moving the mouse over symbols in the editor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("hover_popover_enabled"),
                    pick: |settings_content| settings_content.editor.hover_popover_enabled.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.hover_popover_enabled = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            // todo(settings ui): add units to this number input
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.delay", "Delay"),
                description: lt(
                    "settings_ui.page_data.description.time.to.wait.in.milliseconds.before.showing.the.informational.hover.box",
                    "Time to wait in milliseconds before showing the informational hover box.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("hover_popover_delay"),
                    pick: |settings_content| settings_content.editor.hover_popover_delay.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.hover_popover_delay = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.sticky", "Sticky"),
                description: lt(
                    "settings_ui.page_data.description.whether.the.hover.popover.sticks.when.the.mouse.moves.toward.it.allowing.interaction.with.its.contents",
                    "Whether the hover popover sticks when the mouse moves toward it, allowing interaction with its contents.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("hover_popover_sticky"),
                    pick: |settings_content| settings_content.editor.hover_popover_sticky.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.hover_popover_sticky = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            // todo(settings ui): add units to this number input
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.hiding.delay", "Hiding Delay"),
                description: lt(
                    "settings_ui.page_data.description.time.to.wait.in.milliseconds.before.hiding.the.hover.popover.after.the.mouse.moves.away",
                    "Time to wait in milliseconds before hiding the hover popover after the mouse moves away.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("hover_popover_hiding_delay"),
                    pick: |settings_content| {
                        settings_content.editor.hover_popover_hiding_delay.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.hover_popover_hiding_delay = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn drag_and_drop_selection_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.drag.and.drop.selection",
                "Drag And Drop Selection",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.enabled", "Enabled"),
                description: lt(
                    "settings_ui.page_data.description.enable.drag.and.drop.selection",
                    "Enable drag and drop selection.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("drag_and_drop_selection.enabled"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .drag_and_drop_selection
                            .as_ref()
                            .and_then(|drag_and_drop| drag_and_drop.enabled.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .drag_and_drop_selection
                            .get_or_insert_default()
                            .enabled = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.delay", "Delay"),
                description: lt(
                    "settings_ui.page_data.description.delay.in.milliseconds.before.drag.and.drop.selection.starts",
                    "Delay in milliseconds before drag and drop selection starts.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("drag_and_drop_selection.delay"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .drag_and_drop_selection
                            .as_ref()
                            .and_then(|drag_and_drop| drag_and_drop.delay.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .drag_and_drop_selection
                            .get_or_insert_default()
                            .delay = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn gutter_section() -> [SettingsPageItem; 11] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.gutter", "Gutter")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.git.gutter.width",
                    "Git Gutter Width",
                ),
                description: lt(
                    "settings_ui.page_data.description.git.gutter.width",
                    "Width, in pixels, of the git diff indicators in the gutter. When unset, the width scales with the buffer font size.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("gutter.git_gutter_width"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .gutter
                            .as_ref()
                            .and_then(|gutter| gutter.git_gutter_width.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .gutter
                            .get_or_insert_default()
                            .git_gutter_width = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.line.numbers",
                    "Show Line Numbers",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.line.numbers.in.the.gutter",
                    "Show line numbers in the gutter.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("gutter.line_numbers"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .gutter
                            .as_ref()
                            .and_then(|gutter| gutter.line_numbers.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .gutter
                            .get_or_insert_default()
                            .line_numbers = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.relative.line.numbers",
                    "Relative Line Numbers",
                ),
                description: lt(
                    "settings_ui.page_data.description.controls.line.number.display.in.the.editor.s.gutter.disabled.shows.absolute.line.numbers.enabled.shows.relative.line.numbers.for.each.absolute.line.and.wrapped.shows.relative.line.numbers.for.every.line.absolute.or.wrapped",
                    "Controls line number display in the editor's gutter. \\\"disabled\\\" shows absolute line numbers, \\\"enabled\\\" shows relative line numbers for each absolute line, and \\\"wrapped\\\" shows relative line numbers for every line, absolute or wrapped.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("relative_line_numbers"),
                    pick: |settings_content| settings_content.editor.relative_line_numbers.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.relative_line_numbers = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.line.number.scale",
                    "Line Number Scale",
                ),
                description: lt(
                    "settings_ui.page_data.description.scale.of.the.line.number.font.size.relative.to.the.buffer.font.size",
                    "Scale of the line number font size, relative to the buffer font size.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("line_number_scale"),
                    pick: |settings_content| settings_content.editor.line_number_scale.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.line_number_scale = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.runnables",
                    "Show Runnables",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.runnable.buttons.in.the.gutter",
                    "Show runnable buttons in the gutter.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("gutter.runnables"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .gutter
                            .as_ref()
                            .and_then(|gutter| gutter.runnables.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .gutter
                            .get_or_insert_default()
                            .runnables = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.breakpoints",
                    "Show Breakpoints",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.breakpoints.in.the.gutter",
                    "Show breakpoints in the gutter.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("gutter.breakpoints"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .gutter
                            .as_ref()
                            .and_then(|gutter| gutter.breakpoints.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .gutter
                            .get_or_insert_default()
                            .breakpoints = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.bookmarks",
                    "Show Bookmarks",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.bookmarks.in.the.gutter",
                    "Show bookmarks in the gutter.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("gutter.bookmarks"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .gutter
                            .as_ref()
                            .and_then(|gutter| gutter.bookmarks.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .gutter
                            .get_or_insert_default()
                            .bookmarks = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.show.folds", "Show Folds"),
                description: lt(
                    "settings_ui.page_data.description.show.code.folding.controls.in.the.gutter",
                    "Show code folding controls in the gutter.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("gutter.folds"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .gutter
                            .as_ref()
                            .and_then(|gutter| gutter.folds.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.gutter.get_or_insert_default().folds = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.min.line.number.digits",
                    "Min Line Number Digits",
                ),
                description: lt(
                    "settings_ui.page_data.description.minimum.number.of.characters.to.reserve.space.for.in.the.gutter",
                    "Minimum number of characters to reserve space for in the gutter.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("gutter.min_line_number_digits"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .gutter
                            .as_ref()
                            .and_then(|gutter| gutter.min_line_number_digits.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .gutter
                            .get_or_insert_default()
                            .min_line_number_digits = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.inline.code.actions",
                    "Inline Code Actions",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.code.action.button.at.start.of.buffer.line",
                    "Show code action button at start of buffer line.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("inline_code_actions"),
                    pick: |settings_content| settings_content.editor.inline_code_actions.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.inline_code_actions = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn scrollbar_section() -> [SettingsPageItem; 10] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.scrollbar",
                "Scrollbar",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.show", "Show"),
                description: lt(
                    "settings_ui.page_data.description.when.to.show.the.scrollbar.in.the.editor",
                    "When to show the scrollbar in the editor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("scrollbar"),
                    pick: |settings_content| {
                        settings_content.editor.scrollbar.as_ref()?.show.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .scrollbar
                            .get_or_insert_default()
                            .show = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.cursors", "Cursors"),
                description: lt(
                    "settings_ui.page_data.description.show.cursor.positions.in.the.scrollbar",
                    "Show cursor positions in the scrollbar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("scrollbar.cursors"),
                    pick: |settings_content| {
                        settings_content.editor.scrollbar.as_ref()?.cursors.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .scrollbar
                            .get_or_insert_default()
                            .cursors = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.git.diff", "Git Diff"),
                description: lt(
                    "settings_ui.page_data.description.show.git.diff.indicators.in.the.scrollbar",
                    "Show Git diff indicators in the scrollbar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("scrollbar.git_diff"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .scrollbar
                            .as_ref()?
                            .git_diff
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .scrollbar
                            .get_or_insert_default()
                            .git_diff = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.search.results",
                    "Search Results",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.buffer.search.result.indicators.in.the.scrollbar",
                    "Show buffer search result indicators in the scrollbar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("scrollbar.search_results"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .scrollbar
                            .as_ref()?
                            .search_results
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .scrollbar
                            .get_or_insert_default()
                            .search_results = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.selected.text", "Selected Text"),
                description: lt(
                    "settings_ui.page_data.description.show.selected.text.occurrences.in.the.scrollbar",
                    "Show selected text occurrences in the scrollbar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("scrollbar.selected_text"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .scrollbar
                            .as_ref()?
                            .selected_text
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .scrollbar
                            .get_or_insert_default()
                            .selected_text = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.selected.symbol",
                    "Selected Symbol",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.selected.symbol.occurrences.in.the.scrollbar",
                    "Show selected symbol occurrences in the scrollbar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("scrollbar.selected_symbol"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .scrollbar
                            .as_ref()?
                            .selected_symbol
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .scrollbar
                            .get_or_insert_default()
                            .selected_symbol = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.diagnostics", "Diagnostics"),
                description: lt(
                    "settings_ui.page_data.description.which.diagnostic.indicators.to.show.in.the.scrollbar",
                    "Which diagnostic indicators to show in the scrollbar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("scrollbar.diagnostics"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .scrollbar
                            .as_ref()?
                            .diagnostics
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .scrollbar
                            .get_or_insert_default()
                            .diagnostics = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.horizontal.scrollbar",
                    "Horizontal Scrollbar",
                ),
                description: lt(
                    "settings_ui.page_data.description.when.false.forcefully.disables.the.horizontal.scrollbar",
                    "When false, forcefully disables the horizontal scrollbar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("scrollbar.axes.horizontal"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .scrollbar
                            .as_ref()?
                            .axes
                            .as_ref()?
                            .horizontal
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .scrollbar
                            .get_or_insert_default()
                            .axes
                            .get_or_insert_default()
                            .horizontal = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.vertical.scrollbar",
                    "Vertical Scrollbar",
                ),
                description: lt(
                    "settings_ui.page_data.description.when.false.forcefully.disables.the.vertical.scrollbar",
                    "When false, forcefully disables the vertical scrollbar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("scrollbar.axes.vertical"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .scrollbar
                            .as_ref()?
                            .axes
                            .as_ref()?
                            .vertical
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .scrollbar
                            .get_or_insert_default()
                            .axes
                            .get_or_insert_default()
                            .vertical = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn minimap_section() -> [SettingsPageItem; 7] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.minimap", "Minimap")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.show", "Show"),
                description: lt(
                    "settings_ui.page_data.description.when.to.show.the.minimap.in.the.editor",
                    "When to show the minimap in the editor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("minimap.show"),
                    pick: |settings_content| {
                        settings_content.editor.minimap.as_ref()?.show.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.minimap.get_or_insert_default().show = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.display.in", "Display In"),
                description: lt(
                    "settings_ui.page_data.description.where.to.show.the.minimap.in.the.editor",
                    "Where to show the minimap in the editor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("minimap.display_in"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .minimap
                            .as_ref()?
                            .display_in
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .minimap
                            .get_or_insert_default()
                            .display_in = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.thumb", "Thumb"),
                description: lt(
                    "settings_ui.page_data.description.when.to.show.the.minimap.thumb",
                    "When to show the minimap thumb.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("minimap.thumb"),
                    pick: |settings_content| {
                        settings_content.editor.minimap.as_ref()?.thumb.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .minimap
                            .get_or_insert_default()
                            .thumb = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.thumb.border", "Thumb Border"),
                description: lt(
                    "settings_ui.page_data.description.border.style.for.the.minimap.s.scrollbar.thumb",
                    "Border style for the minimap's scrollbar thumb.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("minimap.thumb_border"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .minimap
                            .as_ref()?
                            .thumb_border
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .minimap
                            .get_or_insert_default()
                            .thumb_border = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.current.line.highlight",
                    "Current Line Highlight",
                ),
                description: lt(
                    "settings_ui.page_data.description.how.to.highlight.the.current.line.in.the.minimap",
                    "How to highlight the current line in the minimap.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("minimap.current_line_highlight"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .minimap
                            .as_ref()
                            .and_then(|minimap| minimap.current_line_highlight.as_ref())
                            .or(settings_content.editor.current_line_highlight.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .minimap
                            .get_or_insert_default()
                            .current_line_highlight = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.max.width.columns",
                    "Max Width Columns",
                ),
                description: lt(
                    "settings_ui.page_data.description.maximum.number.of.columns.to.display.in.the.minimap",
                    "Maximum number of columns to display in the minimap.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("minimap.max_width_columns"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .minimap
                            .as_ref()?
                            .max_width_columns
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .minimap
                            .get_or_insert_default()
                            .max_width_columns = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn toolbar_section() -> [SettingsPageItem; 6] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.toolbar", "Toolbar")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.breadcrumbs", "Breadcrumbs"),
                description: lt(
                    "settings_ui.page_data.description.show.breadcrumbs",
                    "Show breadcrumbs.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("toolbar.breadcrumbs"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .toolbar
                            .as_ref()?
                            .breadcrumbs
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .toolbar
                            .get_or_insert_default()
                            .breadcrumbs = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.quick.actions", "Quick Actions"),
                description: lt(
                    "settings_ui.page_data.description.show.quick.action.buttons.e.g.search.selection.editor.controls.etc",
                    "Show quick action buttons (e.g., search, selection, editor controls, etc.).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("toolbar.quick_actions"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .toolbar
                            .as_ref()?
                            .quick_actions
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .toolbar
                            .get_or_insert_default()
                            .quick_actions = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.selections.menu",
                    "Selections Menu",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.selections.menu.in.the.editor.toolbar",
                    "Show the selections menu in the editor toolbar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("toolbar.selections_menu"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .toolbar
                            .as_ref()?
                            .selections_menu
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .toolbar
                            .get_or_insert_default()
                            .selections_menu = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.agent.review", "Agent Review"),
                description: lt(
                    "settings_ui.page_data.description.show.agent.review.buttons.in.the.editor.toolbar",
                    "Show agent review buttons in the editor toolbar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("toolbar.agent_review"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .toolbar
                            .as_ref()?
                            .agent_review
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .toolbar
                            .get_or_insert_default()
                            .agent_review = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.code.actions", "Code Actions"),
                description: lt(
                    "settings_ui.page_data.description.show.code.action.buttons.in.the.editor.toolbar",
                    "Show code action buttons in the editor toolbar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("toolbar.code_actions"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .toolbar
                            .as_ref()?
                            .code_actions
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .toolbar
                            .get_or_insert_default()
                            .code_actions = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn vim_settings_section() -> [SettingsPageItem; 14] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.vim", "Vim")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.default.mode", "Default Mode"),
                description: lt(
                    "settings_ui.page_data.description.the.default.mode.when.vim.starts",
                    "The default mode when Vim starts.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("vim.default_mode"),
                    pick: |settings_content| settings_content.vim.as_ref()?.default_mode.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.vim.get_or_insert_default().default_mode = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.toggle.relative.line.numbers",
                    "Toggle Relative Line Numbers",
                ),
                description: lt(
                    "settings_ui.page_data.description.toggle.relative.line.numbers.in.vim.mode",
                    "Toggle relative line numbers in Vim mode.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("vim.toggle_relative_line_numbers"),
                    pick: |settings_content| {
                        settings_content
                            .vim
                            .as_ref()?
                            .toggle_relative_line_numbers
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .vim
                            .get_or_insert_default()
                            .toggle_relative_line_numbers = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.use.system.clipboard",
                    "Use System Clipboard",
                ),
                description: lt(
                    "settings_ui.page_data.description.controls.when.to.use.system.clipboard.in.vim.mode",
                    "Controls when to use system clipboard in Vim mode.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("vim.use_system_clipboard"),
                    pick: |settings_content| {
                        settings_content.vim.as_ref()?.use_system_clipboard.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .vim
                            .get_or_insert_default()
                            .use_system_clipboard = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.use.smartcase.find",
                    "Use Smartcase Find",
                ),
                description: lt(
                    "settings_ui.page_data.description.enable.smartcase.searching.in.vim.mode",
                    "Enable smartcase searching in Vim mode.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("vim.use_smartcase_find"),
                    pick: |settings_content| {
                        settings_content.vim.as_ref()?.use_smartcase_find.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .vim
                            .get_or_insert_default()
                            .use_smartcase_find = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.global.substitution.default",
                    "Global Substitution Default",
                ),
                description: lt(
                    "settings_ui.page_data.description.when.enabled.the.substitute.command.replaces.all.matches.in.a.line.by.default.the.g.flag.then.toggles.this.behavior",
                    "When enabled, the :substitute command replaces all matches in a line by default. The 'g' flag then toggles this behavior.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("vim.gdefault"),
                    pick: |settings_content| settings_content.vim.as_ref()?.gdefault.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.vim.get_or_insert_default().gdefault = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.highlight.on.yank.duration",
                    "Highlight on Yank Duration",
                ),
                description: lt(
                    "settings_ui.page_data.description.duration.in.milliseconds.to.highlight.yanked.text.in.vim.mode",
                    "Duration in milliseconds to highlight yanked text in Vim mode.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("vim.highlight_on_yank_duration"),
                    pick: |settings_content| {
                        settings_content
                            .vim
                            .as_ref()?
                            .highlight_on_yank_duration
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .vim
                            .get_or_insert_default()
                            .highlight_on_yank_duration = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.regex.search", "Regex Search"),
                description: lt(
                    "settings_ui.page_data.description.use.regex.search.by.default.in.vim.search",
                    "Use regex search by default in Vim search.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("vim.use_regex_search"),
                    pick: |settings_content| {
                        settings_content.vim.as_ref()?.use_regex_search.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .vim
                            .get_or_insert_default()
                            .use_regex_search = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.edit.predictions.in.normal.mode",
                    "Show Edit Predictions in Normal Mode",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.edit.predictions.are.shown.in.normal.mode.by.default.edit.predictions.are.only.shown.in.insert.and.replace.modes",
                    "Whether edit predictions are shown in normal mode. By default, edit predictions are only shown in insert and replace modes.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("vim.show_edit_predictions_in_normal_mode"),
                    pick: |settings_content| {
                        settings_content
                            .vim
                            .as_ref()?
                            .show_edit_predictions_in_normal_mode
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .vim
                            .get_or_insert_default()
                            .show_edit_predictions_in_normal_mode = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.cursor.shape.normal.mode",
                    "Cursor Shape - Normal Mode",
                ),
                description: lt(
                    "settings_ui.page_data.description.cursor.shape.for.normal.mode",
                    "Cursor shape for normal mode.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("vim.cursor_shape.normal"),
                    pick: |settings_content| {
                        settings_content
                            .vim
                            .as_ref()?
                            .cursor_shape
                            .as_ref()?
                            .normal
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .vim
                            .get_or_insert_default()
                            .cursor_shape
                            .get_or_insert_default()
                            .normal = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.cursor.shape.insert.mode",
                    "Cursor Shape - Insert Mode",
                ),
                description: lt(
                    "settings_ui.page_data.description.cursor.shape.for.insert.mode.inherit.uses.the.editor.s.cursor.shape",
                    "Cursor shape for insert mode. Inherit uses the editor's cursor shape.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("vim.cursor_shape.insert"),
                    pick: |settings_content| {
                        settings_content
                            .vim
                            .as_ref()?
                            .cursor_shape
                            .as_ref()?
                            .insert
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .vim
                            .get_or_insert_default()
                            .cursor_shape
                            .get_or_insert_default()
                            .insert = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.cursor.shape.replace.mode",
                    "Cursor Shape - Replace Mode",
                ),
                description: lt(
                    "settings_ui.page_data.description.cursor.shape.for.replace.mode",
                    "Cursor shape for replace mode.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("vim.cursor_shape.replace"),
                    pick: |settings_content| {
                        settings_content
                            .vim
                            .as_ref()?
                            .cursor_shape
                            .as_ref()?
                            .replace
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .vim
                            .get_or_insert_default()
                            .cursor_shape
                            .get_or_insert_default()
                            .replace = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.cursor.shape.visual.mode",
                    "Cursor Shape - Visual Mode",
                ),
                description: lt(
                    "settings_ui.page_data.description.cursor.shape.for.visual.mode",
                    "Cursor shape for visual mode.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("vim.cursor_shape.visual"),
                    pick: |settings_content| {
                        settings_content
                            .vim
                            .as_ref()?
                            .cursor_shape
                            .as_ref()?
                            .visual
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .vim
                            .get_or_insert_default()
                            .cursor_shape
                            .get_or_insert_default()
                            .visual = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.custom.digraphs",
                    "Custom Digraphs",
                ),
                description: lt(
                    "settings_ui.page_data.description.custom.digraph.mappings.for.vim.mode",
                    "Custom digraph mappings for Vim mode.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("vim.custom_digraphs"),
                        pick: |settings_content| {
                            settings_content.vim.as_ref()?.custom_digraphs.as_ref()
                        },
                        write: |settings_content, value, _| {
                            settings_content.vim.get_or_insert_default().custom_digraphs = value;
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER,
            }),
        ]
    }

    let items = concat_sections!(
        auto_save_section(),
        which_key_section(),
        multibuffer_section(),
        scrolling_section(),
        signature_help_section(),
        hover_popover_section(),
        drag_and_drop_selection_section(),
        gutter_section(),
        scrollbar_section(),
        minimap_section(),
        toolbar_section(),
        vim_settings_section(),
        language_settings_data(),
    );

    SettingsPage {
        title: lt("settings_ui.page_data.title.editor", "Editor"),
        items,
    }
}

fn languages_and_tools_page(cx: &App) -> SettingsPage {
    fn file_types_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.file.types",
                "File Types",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.file.type.associations",
                    "File Type Associations",
                ),
                description: lt(
                    "settings_ui.page_data.description.a.mapping.from.languages.to.files.and.file.extensions.that.should.be.treated.as.that.language",
                    "A mapping from languages to files and file extensions that should be treated as that language.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("file_type_associations"),
                        pick: |settings_content| {
                            settings_content.project.all_languages.file_types.as_ref()
                        },
                        write: |settings_content, value, _| {
                            settings_content.project.all_languages.file_types = value;
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    fn diagnostics_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.diagnostics",
                "Diagnostics",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.max.severity", "Max Severity"),
                description: lt(
                    "settings_ui.page_data.description.which.level.to.use.to.filter.out.diagnostics.displayed.in.the.editor",
                    "Which level to use to filter out diagnostics displayed in the editor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("diagnostics_max_severity"),
                    pick: |settings_content| {
                        settings_content.editor.diagnostics_max_severity.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.diagnostics_max_severity = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.include.warnings",
                    "Include Warnings",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.show.warnings.or.not.by.default",
                    "Whether to show warnings or not by default.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("diagnostics.include_warnings"),
                    pick: |settings_content| {
                        settings_content
                            .diagnostics
                            .as_ref()?
                            .include_warnings
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .diagnostics
                            .get_or_insert_default()
                            .include_warnings = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn inline_diagnostics_section() -> [SettingsPageItem; 5] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.inline.diagnostics",
                "Inline Diagnostics",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.enabled", "Enabled"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.show.diagnostics.inline.or.not",
                    "Whether to show diagnostics inline or not.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("diagnostics.inline.enabled"),
                    pick: |settings_content| {
                        settings_content
                            .diagnostics
                            .as_ref()?
                            .inline
                            .as_ref()?
                            .enabled
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .diagnostics
                            .get_or_insert_default()
                            .inline
                            .get_or_insert_default()
                            .enabled = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.update.debounce",
                    "Update Debounce",
                ),
                description: lt(
                    "settings_ui.page_data.description.the.delay.in.milliseconds.to.show.inline.diagnostics.after.the.last.diagnostic.update",
                    "The delay in milliseconds to show inline diagnostics after the last diagnostic update.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("diagnostics.inline.update_debounce_ms"),
                    pick: |settings_content| {
                        settings_content
                            .diagnostics
                            .as_ref()?
                            .inline
                            .as_ref()?
                            .update_debounce_ms
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .diagnostics
                            .get_or_insert_default()
                            .inline
                            .get_or_insert_default()
                            .update_debounce_ms = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.padding", "Padding"),
                description: lt(
                    "settings_ui.page_data.description.the.amount.of.padding.between.the.end.of.the.source.line.and.the.start.of.the.inline.diagnostic",
                    "The amount of padding between the end of the source line and the start of the inline diagnostic.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("diagnostics.inline.padding"),
                    pick: |settings_content| {
                        settings_content
                            .diagnostics
                            .as_ref()?
                            .inline
                            .as_ref()?
                            .padding
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .diagnostics
                            .get_or_insert_default()
                            .inline
                            .get_or_insert_default()
                            .padding = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.minimum.column",
                    "Minimum Column",
                ),
                description: lt(
                    "settings_ui.page_data.description.the.minimum.column.at.which.to.display.inline.diagnostics",
                    "The minimum column at which to display inline diagnostics.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("diagnostics.inline.min_column"),
                    pick: |settings_content| {
                        settings_content
                            .diagnostics
                            .as_ref()?
                            .inline
                            .as_ref()?
                            .min_column
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .diagnostics
                            .get_or_insert_default()
                            .inline
                            .get_or_insert_default()
                            .min_column = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn lsp_pull_diagnostics_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.lsp.pull.diagnostics",
                "LSP Pull Diagnostics",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.enabled", "Enabled"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.pull.for.language.server.powered.diagnostics.or.not",
                    "Whether to pull for language server-powered diagnostics or not.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("diagnostics.lsp_pull_diagnostics.enabled"),
                    pick: |settings_content| {
                        settings_content
                            .diagnostics
                            .as_ref()?
                            .lsp_pull_diagnostics
                            .as_ref()?
                            .enabled
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .diagnostics
                            .get_or_insert_default()
                            .lsp_pull_diagnostics
                            .get_or_insert_default()
                            .enabled = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            // todo(settings_ui): Needs unit
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.debounce", "Debounce"),
                description: lt(
                    "settings_ui.page_data.description.minimum.time.to.wait.before.pulling.diagnostics.from.the.language.server.s",
                    "Minimum time to wait before pulling diagnostics from the language server(s).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("diagnostics.lsp_pull_diagnostics.debounce_ms"),
                    pick: |settings_content| {
                        settings_content
                            .diagnostics
                            .as_ref()?
                            .lsp_pull_diagnostics
                            .as_ref()?
                            .debounce_ms
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .diagnostics
                            .get_or_insert_default()
                            .lsp_pull_diagnostics
                            .get_or_insert_default()
                            .debounce_ms = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn lsp_highlights_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.lsp.highlights",
                "LSP Highlights",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.debounce", "Debounce"),
                description: lt(
                    "settings_ui.page_data.description.the.debounce.delay.before.querying.highlights.from.the.language",
                    "The debounce delay before querying highlights from the language.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("lsp_highlight_debounce"),
                    pick: |settings_content| {
                        settings_content.editor.lsp_highlight_debounce.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.lsp_highlight_debounce = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn languages_list_section(cx: &App) -> Box<[SettingsPageItem]> {
        // todo(settings_ui): Refresh on extension (un)/installed
        // Note that `crates/json_schema_store` solves the same problem, there is probably a way to unify the two
        std::iter::once(SettingsPageItem::SectionHeader(lt(
            "settings_ui.page_data.section.languages",
            "Languages",
        )))
        .chain(all_language_names(cx).into_iter().map(|language_name| {
            let link = format!("languages.{language_name}");
            SettingsPageItem::SubPageLink(SubPageLink {
                title: language_name.into(),
                r#type: crate::SubPageType::Language,
                description: None,
                search_aliases: &[],
                json_path: Some(link.leak()),
                in_json: true,
                files: USER | PROJECT,
                render: |this, scroll_handle, window, cx| {
                    let items: Box<[SettingsPageItem]> = concat_sections!(
                        language_settings_data(),
                        non_editor_language_settings_data(),
                        edit_prediction_language_settings_section()
                    );
                    this.render_sub_page_items(items.iter().enumerate(), scroll_handle, window, cx)
                        .into_any_element()
                },
            })
        }))
        .collect()
    }

    SettingsPage {
        title: lt(
            "settings_ui.page_data.title.languages.tools",
            "Languages & Tools",
        ),
        items: {
            concat_sections!(
                non_editor_language_settings_data(),
                file_types_section(),
                diagnostics_section(),
                inline_diagnostics_section(),
                lsp_pull_diagnostics_section(),
                lsp_highlights_section(),
                languages_list_section(cx),
            )
        },
    }
}

fn search_and_files_page() -> SettingsPage {
    fn search_section() -> [SettingsPageItem; 9] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.search", "Search")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.whole.word", "Whole Word"),
                description: lt(
                    "settings_ui.page_data.description.search.for.whole.words.by.default",
                    "Search for whole words by default.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("search.whole_word"),
                    pick: |settings_content| {
                        settings_content.editor.search.as_ref()?.whole_word.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .search
                            .get_or_insert_default()
                            .whole_word = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.case.sensitive",
                    "Case Sensitive",
                ),
                description: lt(
                    "settings_ui.page_data.description.search.case.sensitively.by.default",
                    "Search case-sensitively by default.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("search.case_sensitive"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .search
                            .as_ref()?
                            .case_sensitive
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .search
                            .get_or_insert_default()
                            .case_sensitive = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.use.smartcase.search",
                    "Use Smartcase Search",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.automatically.enable.case.sensitive.search.based.on.the.search.query",
                    "Whether to automatically enable case-sensitive search based on the search query.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("use_smartcase_search"),
                    pick: |settings_content| settings_content.editor.use_smartcase_search.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.use_smartcase_search = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.include.ignored",
                    "Include Ignored",
                ),
                description: lt(
                    "settings_ui.page_data.description.include.ignored.files.in.search.results.by.default",
                    "Include ignored files in search results by default.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("search.include_ignored"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .search
                            .as_ref()?
                            .include_ignored
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .search
                            .get_or_insert_default()
                            .include_ignored = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.regex", "Regex"),
                description: lt(
                    "settings_ui.page_data.description.use.regex.search.by.default",
                    "Use regex search by default.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("search.regex"),
                    pick: |settings_content| {
                        settings_content.editor.search.as_ref()?.regex.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.search.get_or_insert_default().regex = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.search.wrap", "Search Wrap"),
                description: lt(
                    "settings_ui.page_data.description.whether.the.editor.search.results.will.loop",
                    "Whether the editor search results will loop.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("search_wrap"),
                    pick: |settings_content| settings_content.editor.search_wrap.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.search_wrap = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.center.on.match",
                    "Center on Match",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.center.the.current.match.in.the.editor",
                    "Whether to center the current match in the editor",
                ),
                field: Box::new(SettingField {
                    json_path: Some("editor.search.center_on_match"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .search
                            .as_ref()
                            .and_then(|search| search.center_on_match.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .search
                            .get_or_insert_default()
                            .center_on_match = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.seed.search.query.from.cursor",
                    "Seed Search Query From Cursor",
                ),
                description: lt(
                    "settings_ui.page_data.description.when.to.populate.a.new.search.s.query.based.on.the.text.under.the.cursor",
                    "When to populate a new search's query based on the text under the cursor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("seed_search_query_from_cursor"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .seed_search_query_from_cursor
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.seed_search_query_from_cursor = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn file_finder_section() -> [SettingsPageItem; 5] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.file.finder",
                "File Finder",
            )),
            // todo: null by default
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.include.ignored.in.search",
                    "Include Ignored in Search",
                ),
                description: lt(
                    "settings_ui.page_data.description.use.gitignored.files.when.searching",
                    "Use gitignored files when searching.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("file_finder.include_ignored"),
                    pick: |settings_content| {
                        settings_content
                            .file_finder
                            .as_ref()?
                            .include_ignored
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .file_finder
                            .get_or_insert_default()
                            .include_ignored = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.file.icons", "File Icons"),
                description: lt(
                    "settings_ui.page_data.description.show.file.icons.in.the.file.finder",
                    "Show file icons in the file finder.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("file_finder.file_icons"),
                    pick: |settings_content| {
                        settings_content.file_finder.as_ref()?.file_icons.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .file_finder
                            .get_or_insert_default()
                            .file_icons = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.modal.max.width",
                    "Modal Max Width",
                ),
                description: lt(
                    "settings_ui.page_data.description.determines.how.much.space.the.file.finder.can.take.up.in.relation.to.the.available.window.width",
                    "Determines how much space the file finder can take up in relation to the available window width.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("file_finder.modal_max_width"),
                    pick: |settings_content| {
                        settings_content
                            .file_finder
                            .as_ref()?
                            .modal_max_width
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .file_finder
                            .get_or_insert_default()
                            .modal_max_width = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.skip.focus.for.active.in.search",
                    "Skip Focus For Active In Search",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.the.file.finder.should.skip.focus.for.the.active.file.in.search.results",
                    "Whether the file finder should skip focus for the active file in search results.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("file_finder.skip_focus_for_active_in_search"),
                    pick: |settings_content| {
                        settings_content
                            .file_finder
                            .as_ref()?
                            .skip_focus_for_active_in_search
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .file_finder
                            .get_or_insert_default()
                            .skip_focus_for_active_in_search = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn file_scan_section() -> [SettingsPageItem; 5] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.file.scan",
                "File Scan",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.file.scan.exclusions",
                    "File Scan Exclusions",
                ),
                description: lt(
                    "settings_ui.page_data.description.files.or.globs.of.files.that.will.be.excluded.by.zzz.entirely.they.will.be.skipped.during.file.scans.file.searches.and.not.be.displayed.in.the.project.file.tree.takes.precedence.over.file.scan.inclusions",
                    "Files or globs of files that will be excluded by ZZZ entirely. They will be skipped during file scans, file searches, and not be displayed in the project file tree. Takes precedence over \\\"File Scan Inclusions\\\"",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("file_scan_exclusions"),
                        pick: |settings_content| {
                            settings_content
                                .project
                                .worktree
                                .file_scan_exclusions
                                .as_ref()
                        },
                        write: |settings_content, value, _| {
                            settings_content.project.worktree.file_scan_exclusions = value;
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.file.scan.inclusions",
                    "File Scan Inclusions",
                ),
                description: lt(
                    "settings_ui.page_data.description.files.or.globs.of.files.that.will.be.included.by.zzz.even.when.ignored.by.git.this.is.useful.for.files.that.are.not.tracked.by.git.but.are.still.important.to.your.project.note.that.globs.that.are.overly.broad.can.slow.down.zzz.s.file.scanning.file.scan.exclusions.takes.precedence.over.these.inclusions",
                    "Files or globs of files that will be included by ZZZ, even when ignored by git. This is useful for files that are not tracked by git, but are still important to your project. Note that globs that are overly broad can slow down ZZZ's file scanning. \\\"File Scan Exclusions\\\" takes precedence over these inclusions",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("file_scan_inclusions"),
                        pick: |settings_content| {
                            settings_content
                                .project
                                .worktree
                                .file_scan_inclusions
                                .as_ref()
                        },
                        write: |settings_content, value, _| {
                            settings_content.project.worktree.file_scan_inclusions = value;
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.restore.file.state",
                    "Restore File State",
                ),
                description: lt(
                    "settings_ui.page_data.description.restore.previous.file.state.when.reopening",
                    "Restore previous file state when reopening.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("restore_on_file_reopen"),
                    pick: |settings_content| {
                        settings_content.workspace.restore_on_file_reopen.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.workspace.restore_on_file_reopen = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.close.on.file.delete",
                    "Close on File Delete",
                ),
                description: lt(
                    "settings_ui.page_data.description.automatically.close.files.that.have.been.deleted",
                    "Automatically close files that have been deleted.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("close_on_file_delete"),
                    pick: |settings_content| {
                        settings_content.workspace.close_on_file_delete.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.workspace.close_on_file_delete = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    SettingsPage {
        title: lt("settings_ui.page_data.title.search.files", "Search & Files"),
        items: concat_sections![search_section(), file_finder_section(), file_scan_section()],
    }
}

fn window_and_layout_page() -> SettingsPage {
    fn status_bar_section() -> [SettingsPageItem; 12] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.status.bar",
                "Status Bar",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.project.panel.button",
                    "Project Panel Button",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.project.panel.button.in.the.status.bar",
                    "Show the project panel button in the status bar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.button"),
                    pick: |settings_content| {
                        settings_content.project_panel.as_ref()?.button.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .button = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.active.language.button",
                    "Active Language Button",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.active.language.button.in.the.status.bar",
                    "Show the active language button in the status bar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("status_bar.active_language_button"),
                    pick: |settings_content| {
                        settings_content
                            .status_bar
                            .as_ref()?
                            .active_language_button
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .status_bar
                            .get_or_insert_default()
                            .active_language_button = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.active.encoding.button",
                    "Active Encoding Button",
                ),
                description: lt(
                    "settings_ui.page_data.description.control.when.to.show.the.active.encoding.in.the.status.bar",
                    "Control when to show the active encoding in the status bar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("status_bar.active_encoding_button"),
                    pick: |settings_content| {
                        settings_content
                            .status_bar
                            .as_ref()?
                            .active_encoding_button
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .status_bar
                            .get_or_insert_default()
                            .active_encoding_button = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.position", "Position"),
                description: lt(
                    "settings_ui.page_data.description.where.to.show.the.status.bar.in.the.workspace",
                    "Where to show the status bar in the workspace.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("status_bar.position"),
                    pick: |settings_content| {
                        settings_content.status_bar.as_ref()?.position.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.status_bar.get_or_insert_default().position = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.cursor.position.button",
                    "Cursor Position Button",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.cursor.position.button.in.the.status.bar",
                    "Show the cursor position button in the status bar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("status_bar.cursor_position_button"),
                    pick: |settings_content| {
                        settings_content
                            .status_bar
                            .as_ref()?
                            .cursor_position_button
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .status_bar
                            .get_or_insert_default()
                            .cursor_position_button = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.line.endings.button",
                    "Line Endings Button",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.active.line.endings.button.in.the.status.bar",
                    "Show the active line endings button in the status bar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("status_bar.line_endings_button"),
                    pick: |settings_content| {
                        settings_content
                            .status_bar
                            .as_ref()?
                            .line_endings_button
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .status_bar
                            .get_or_insert_default()
                            .line_endings_button = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.terminal.button",
                    "Terminal Button",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.terminal.button.in.the.status.bar",
                    "Show the terminal button in the status bar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.button"),
                    pick: |settings_content| settings_content.terminal.as_ref()?.button.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.terminal.get_or_insert_default().button = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.diagnostics.button",
                    "Diagnostics Button",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.project.diagnostics.button.in.the.status.bar",
                    "Show the project diagnostics button in the status bar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("diagnostics.button"),
                    pick: |settings_content| settings_content.diagnostics.as_ref()?.button.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.diagnostics.get_or_insert_default().button = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.project.search.button",
                    "Project Search Button",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.project.search.button.in.the.status.bar",
                    "Show the project search button in the status bar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("search.button"),
                    pick: |settings_content| {
                        settings_content.editor.search.as_ref()?.button.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .editor
                            .search
                            .get_or_insert_default()
                            .button = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.debugger.button",
                    "Debugger Button",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.debugger.button.in.the.status.bar",
                    "Show the debugger button in the status bar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("debugger.button"),
                    pick: |settings_content| settings_content.debugger.as_ref()?.button.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.debugger.get_or_insert_default().button = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.active.file.name",
                    "Active File Name",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.name.of.the.active.file.in.the.status.bar",
                    "Show the name of the active file in the status bar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("status_bar.show_active_file"),
                    pick: |settings_content| {
                        settings_content
                            .status_bar
                            .as_ref()?
                            .show_active_file
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .status_bar
                            .get_or_insert_default()
                            .show_active_file = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn title_bar_section() -> [SettingsPageItem; 8] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.title.bar", "Title Bar")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.show.branch.status.icon", "Show Branch Status Icon"),
                description: lt("settings_ui.page_data.description.show.git.status.indicators.on.the.branch.icon.in.the.titlebar", "Show git status indicators on the branch icon in the titlebar."),
                field: Box::new(SettingField {
                    json_path: Some("title_bar.show_branch_status_icon"),
                    pick: |settings_content| {
                        settings_content
                            .title_bar
                            .as_ref()?
                            .show_branch_status_icon
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .title_bar
                            .get_or_insert_default()
                            .show_branch_status_icon = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.show.branch.name", "Show Branch Name"),
                description: lt("settings_ui.page_data.description.show.the.branch.name.button.in.the.titlebar", "Show the branch name button in the titlebar."),
                field: Box::new(SettingField {
                    json_path: Some("title_bar.show_branch_name"),
                    pick: |settings_content| {
                        settings_content
                            .title_bar
                            .as_ref()?
                            .show_branch_name
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .title_bar
                            .get_or_insert_default()
                            .show_branch_name = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.show.project.items", "Show Project Items"),
                description: lt("settings_ui.page_data.description.show.the.project.host.and.name.in.the.titlebar", "Show the project host and name in the titlebar."),
                field: Box::new(SettingField {
                    json_path: Some("title_bar.show_project_items"),
                    pick: |settings_content| {
                        settings_content
                            .title_bar
                            .as_ref()?
                            .show_project_items
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .title_bar
                            .get_or_insert_default()
                            .show_project_items = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.show.onboarding.banner", "Show Onboarding Banner"),
                description: lt("settings_ui.page_data.description.show.banners.announcing.new.features.in.the.titlebar", "Show banners announcing new features in the titlebar."),
                field: Box::new(SettingField {
                    json_path: Some("title_bar.show_onboarding_banner"),
                    pick: |settings_content| {
                        settings_content
                            .title_bar
                            .as_ref()?
                            .show_onboarding_banner
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .title_bar
                            .get_or_insert_default()
                            .show_onboarding_banner = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.show.user.menu", "Show User Menu"),
                description: lt("settings_ui.page_data.description.show.the.user.menu.button.in.the.titlebar", "Show the user menu button in the titlebar."),
                field: Box::new(SettingField {
                    json_path: Some("title_bar.show_user_menu"),
                    pick: |settings_content| {
                        settings_content.title_bar.as_ref()?.show_user_menu.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .title_bar
                            .get_or_insert_default()
                            .show_user_menu = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.show.menus", "Show Menus"),
                description: lt("settings_ui.page_data.description.show.the.menus.in.the.titlebar", "Show the menus in the titlebar."),
                field: Box::new(SettingField {
                    json_path: Some("title_bar.show_menus"),
                    pick: |settings_content| {
                        settings_content.title_bar.as_ref()?.show_menus.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .title_bar
                            .get_or_insert_default()
                            .show_menus = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::DynamicItem(DynamicItem {
                discriminant: SettingItem {
                    files: USER,
                    title: lt("settings_ui.page_data.title.button.layout", "Button Layout"),
                    description: lt("settings_ui.page_data.description.linux.only.choose.how.window.control.buttons.are.laid.out.in.the.titlebar", "(Linux only) choose how window control buttons are laid out in the titlebar."),
                    field: Box::new(SettingField {
                        json_path: Some("title_bar.button_layout$"),
                        pick: |settings_content| {
                            Some(
                                &dynamic_variants::<settings::WindowButtonLayoutContent>()[settings_content
                                    .title_bar
                                    .as_ref()?
                                    .button_layout
                                    .as_ref()?
                                    .discriminant()
                                    as usize],
                            )
                        },
                        write: |settings_content, value, _| {
                            let Some(value) = value else {
                                settings_content
                                    .title_bar
                                    .get_or_insert_default()
                                    .button_layout = None;
                                return;
                            };

                            let current_custom_layout = settings_content
                                .title_bar
                                .as_ref()
                                .and_then(|title_bar| title_bar.button_layout.as_ref())
                                .and_then(|button_layout| match button_layout {
                                    settings::WindowButtonLayoutContent::Custom(layout) => {
                                        Some(layout.clone())
                                    }
                                    _ => None,
                                });

                            let button_layout = match value {
                                settings::WindowButtonLayoutContentDiscriminants::PlatformDefault => {
                                    settings::WindowButtonLayoutContent::PlatformDefault
                                }
                                settings::WindowButtonLayoutContentDiscriminants::Standard => {
                                    settings::WindowButtonLayoutContent::Standard
                                }
                                settings::WindowButtonLayoutContentDiscriminants::Custom => {
                                    settings::WindowButtonLayoutContent::Custom(
                                        current_custom_layout.unwrap_or_else(|| {
                                            "close:minimize,maximize".to_owned()
                                        }),
                                    )
                                }
                            };

                            settings_content
                                .title_bar
                                .get_or_insert_default()
                                .button_layout = Some(button_layout);
                        },
                    }),
                    metadata: None,
                },
                pick_discriminant: |settings_content| {
                    Some(
                        settings_content
                            .title_bar
                            .as_ref()?
                            .button_layout
                            .as_ref()?
                            .discriminant() as usize,
                    )
                },
                fields: dynamic_variants::<settings::WindowButtonLayoutContent>()
                    .into_iter()
                    .map(|variant| match variant {
                        settings::WindowButtonLayoutContentDiscriminants::PlatformDefault => {
                            vec![]
                        }
                        settings::WindowButtonLayoutContentDiscriminants::Standard => vec![],
                        settings::WindowButtonLayoutContentDiscriminants::Custom => vec![
                            SettingItem {
                                files: USER,
                                title: lt("settings_ui.page_data.title.custom.button.layout", "Custom Button Layout"),
                                description: lt("settings_ui.page_data.description.gnome.style.layout.string.such.as.close.minimize.maximize", "GNOME-style layout string such as \\\"close:minimize,maximize\\\"."),
                                field: Box::new(SettingField {
                                    json_path: Some("title_bar.button_layout"),
                                    pick: |settings_content| match settings_content
                                        .title_bar
                                        .as_ref()?
                                        .button_layout
                                        .as_ref()?
                                    {
                                        settings::WindowButtonLayoutContent::Custom(layout) => {
                                            Some(layout)
                                        }
                                        _ => DEFAULT_EMPTY_STRING,
                                    },
                                    write: |settings_content, value, _| {
                                        settings_content
                                            .title_bar
                                            .get_or_insert_default()
                                            .button_layout = value
                                            .map(settings::WindowButtonLayoutContent::Custom);
                                    },
                                }),
                                metadata: Some(Box::new(SettingsFieldMetadata {
                                    placeholder: Some("close:minimize,maximize"),
                                    ..Default::default()
                                })),
                            },
                        ],
                    })
                    .collect(),
            }),
        ]
    }

    fn tab_bar_section() -> [SettingsPageItem; 9] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.tab.bar", "Tab Bar")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.show.tab.bar", "Show Tab Bar"),
                description: lt(
                    "settings_ui.page_data.description.show.the.tab.bar.in.the.editor",
                    "Show the tab bar in the editor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("tab_bar.show"),
                    pick: |settings_content| settings_content.tab_bar.as_ref()?.show.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.tab_bar.get_or_insert_default().show = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.git.status.in.tabs",
                    "Show Git Status In Tabs",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.git.file.status.on.a.tab.item",
                    "Show the Git file status on a tab item.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("tabs.git_status"),
                    pick: |settings_content| settings_content.tabs.as_ref()?.git_status.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.tabs.get_or_insert_default().git_status = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.file.icons.in.tabs",
                    "Show File Icons In Tabs",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.file.icon.for.a.tab",
                    "Show the file icon for a tab.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("tabs.file_icons"),
                    pick: |settings_content| settings_content.tabs.as_ref()?.file_icons.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.tabs.get_or_insert_default().file_icons = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.tab.close.position",
                    "Tab Close Position",
                ),
                description: lt(
                    "settings_ui.page_data.description.position.of.the.close.button.in.a.tab",
                    "Position of the close button in a tab.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("tabs.close_position"),
                    pick: |settings_content| {
                        settings_content.tabs.as_ref()?.close_position.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.tabs.get_or_insert_default().close_position = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                files: USER,
                title: lt("settings_ui.page_data.title.maximum.tabs", "Maximum Tabs"),
                description: lt(
                    "settings_ui.page_data.description.maximum.open.tabs.in.a.pane.will.not.close.an.unsaved.tab",
                    "Maximum open tabs in a pane. Will not close an unsaved tab.",
                ),
                // todo(settings_ui): The default for this value is null and it's use in code
                // is complex, so I'm going to come back to this later
                field: Box::new(
                    SettingField {
                        json_path: Some("max_tabs"),
                        pick: |settings_content| settings_content.workspace.max_tabs.as_ref(),
                        write: |settings_content, value, _| {
                            settings_content.workspace.max_tabs = value;
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.navigation.history.buttons",
                    "Show Navigation History Buttons",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.navigation.history.buttons.in.the.tab.bar",
                    "Show the navigation history buttons in the tab bar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("tab_bar.show_nav_history_buttons"),
                    pick: |settings_content| {
                        settings_content
                            .tab_bar
                            .as_ref()?
                            .show_nav_history_buttons
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .tab_bar
                            .get_or_insert_default()
                            .show_nav_history_buttons = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.tab.bar.buttons",
                    "Show Tab Bar Buttons",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.tab.bar.buttons.new.split.pane.zoom",
                    "Show the tab bar buttons (New, Split Pane, Zoom).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("tab_bar.show_tab_bar_buttons"),
                    pick: |settings_content| {
                        settings_content
                            .tab_bar
                            .as_ref()?
                            .show_tab_bar_buttons
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .tab_bar
                            .get_or_insert_default()
                            .show_tab_bar_buttons = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.pinned.tabs.layout",
                    "Pinned Tabs Layout",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.pinned.tabs.in.a.separate.row.above.unpinned.tabs",
                    "Show pinned tabs in a separate row above unpinned tabs.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("tab_bar.show_pinned_tabs_in_separate_row"),
                    pick: |settings_content| {
                        settings_content
                            .tab_bar
                            .as_ref()?
                            .show_pinned_tabs_in_separate_row
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .tab_bar
                            .get_or_insert_default()
                            .show_pinned_tabs_in_separate_row = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn tab_settings_section() -> [SettingsPageItem; 5] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.tab.settings",
                "Tab Settings",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.activate.on.close",
                    "Activate On Close",
                ),
                description: lt(
                    "settings_ui.page_data.description.what.to.do.after.closing.the.current.tab",
                    "What to do after closing the current tab.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("tabs.activate_on_close"),
                    pick: |settings_content| {
                        settings_content.tabs.as_ref()?.activate_on_close.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .tabs
                            .get_or_insert_default()
                            .activate_on_close = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.tab.show.diagnostics",
                    "Tab Show Diagnostics",
                ),
                description: lt(
                    "settings_ui.page_data.description.which.files.containing.diagnostic.errors.warnings.to.mark.in.the.tabs",
                    "Which files containing diagnostic errors/warnings to mark in the tabs.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("tabs.show_diagnostics"),
                    pick: |settings_content| {
                        settings_content.tabs.as_ref()?.show_diagnostics.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .tabs
                            .get_or_insert_default()
                            .show_diagnostics = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.close.button",
                    "Show Close Button",
                ),
                description: lt(
                    "settings_ui.page_data.description.controls.the.appearance.behavior.of.the.tab.s.close.button",
                    "Controls the appearance behavior of the tab's close button.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("tabs.show_close_button"),
                    pick: |settings_content| {
                        settings_content.tabs.as_ref()?.show_close_button.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .tabs
                            .get_or_insert_default()
                            .show_close_button = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.unsaved.indicator",
                    "Show Unsaved Indicator",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.an.indicator.on.tabs.with.unsaved.changes",
                    "Show an indicator on tabs with unsaved changes.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("tabs.show_unsaved_indicator"),
                    pick: |settings_content| {
                        settings_content
                            .tabs
                            .as_ref()?
                            .show_unsaved_indicator
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .tabs
                            .get_or_insert_default()
                            .show_unsaved_indicator = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn preview_tabs_section() -> [SettingsPageItem; 8] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.preview.tabs",
                "Preview Tabs",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.preview.tabs.enabled",
                    "Preview Tabs Enabled",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.opened.editors.as.preview.tabs",
                    "Show opened editors as preview tabs.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("preview_tabs.enabled"),
                    pick: |settings_content| {
                        settings_content.preview_tabs.as_ref()?.enabled.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .preview_tabs
                            .get_or_insert_default()
                            .enabled = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.enable.preview.from.project.panel",
                    "Enable Preview From Project Panel",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.open.tabs.in.preview.mode.when.opened.from.the.project.panel.with.a.single.click",
                    "Whether to open tabs in preview mode when opened from the project panel with a single click or the Open action.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("preview_tabs.enable_preview_from_project_panel"),
                    pick: |settings_content| {
                        settings_content
                            .preview_tabs
                            .as_ref()?
                            .enable_preview_from_project_panel
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .preview_tabs
                            .get_or_insert_default()
                            .enable_preview_from_project_panel = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.enable.preview.from.file.finder",
                    "Enable Preview From File Finder",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.open.tabs.in.preview.mode.when.selected.from.the.file.finder",
                    "Whether to open tabs in preview mode when selected from the file finder.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("preview_tabs.enable_preview_from_file_finder"),
                    pick: |settings_content| {
                        settings_content
                            .preview_tabs
                            .as_ref()?
                            .enable_preview_from_file_finder
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .preview_tabs
                            .get_or_insert_default()
                            .enable_preview_from_file_finder = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.enable.preview.from.multibuffer",
                    "Enable Preview From Multibuffer",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.open.tabs.in.preview.mode.when.opened.from.a.multibuffer",
                    "Whether to open tabs in preview mode when opened from a multibuffer.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("preview_tabs.enable_preview_from_multibuffer"),
                    pick: |settings_content| {
                        settings_content
                            .preview_tabs
                            .as_ref()?
                            .enable_preview_from_multibuffer
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .preview_tabs
                            .get_or_insert_default()
                            .enable_preview_from_multibuffer = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.enable.preview.multibuffer.from.code.navigation",
                    "Enable Preview Multibuffer From Code Navigation",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.open.tabs.in.preview.mode.when.code.navigation.is.used.to.open.a.multibuffer",
                    "Whether to open tabs in preview mode when code navigation is used to open a multibuffer.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("preview_tabs.enable_preview_multibuffer_from_code_navigation"),
                    pick: |settings_content| {
                        settings_content
                            .preview_tabs
                            .as_ref()?
                            .enable_preview_multibuffer_from_code_navigation
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .preview_tabs
                            .get_or_insert_default()
                            .enable_preview_multibuffer_from_code_navigation = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.enable.preview.file.from.code.navigation",
                    "Enable Preview File From Code Navigation",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.open.tabs.in.preview.mode.when.code.navigation.is.used.to.open.a.single.file",
                    "Whether to open tabs in preview mode when code navigation is used to open a single file.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("preview_tabs.enable_preview_file_from_code_navigation"),
                    pick: |settings_content| {
                        settings_content
                            .preview_tabs
                            .as_ref()?
                            .enable_preview_file_from_code_navigation
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .preview_tabs
                            .get_or_insert_default()
                            .enable_preview_file_from_code_navigation = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.enable.keep.preview.on.code.navigation",
                    "Enable Keep Preview On Code Navigation",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.keep.tabs.in.preview.mode.when.code.navigation.is.used.to.navigate.away.from.them.if.enable.preview.file.from.code.navigation.or.enable.preview.multibuffer.from.code.navigation.is.also.true.the.new.tab.may.replace.the.existing.one",
                    "Whether to keep tabs in preview mode when code navigation is used to navigate away from them. If `enable_preview_file_from_code_navigation` or `enable_preview_multibuffer_from_code_navigation` is also true, the new tab may replace the existing one.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("preview_tabs.enable_keep_preview_on_code_navigation"),
                    pick: |settings_content| {
                        settings_content
                            .preview_tabs
                            .as_ref()?
                            .enable_keep_preview_on_code_navigation
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .preview_tabs
                            .get_or_insert_default()
                            .enable_keep_preview_on_code_navigation = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn layout_section() -> [SettingsPageItem; 6] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.layout", "Layout")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.bottom.dock.layout",
                    "Bottom Dock Layout",
                ),
                description: lt(
                    "settings_ui.page_data.description.layout.mode.for.the.bottom.dock",
                    "Layout mode for the bottom dock.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("bottom_dock_layout"),
                    pick: |settings_content| settings_content.workspace.bottom_dock_layout.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.workspace.bottom_dock_layout = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                files: USER,
                title: lt(
                    "settings_ui.page_data.title.centered.layout.left.padding",
                    "Centered Layout Left Padding",
                ),
                description: lt(
                    "settings_ui.page_data.description.left.padding.for.centered.layout",
                    "Left padding for centered layout.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("centered_layout.left_padding"),
                    pick: |settings_content| {
                        settings_content
                            .workspace
                            .centered_layout
                            .as_ref()?
                            .left_padding
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .workspace
                            .centered_layout
                            .get_or_insert_default()
                            .left_padding = value;
                    },
                }),
                metadata: None,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                files: USER,
                title: lt(
                    "settings_ui.page_data.title.centered.layout.right.padding",
                    "Centered Layout Right Padding",
                ),
                description: lt(
                    "settings_ui.page_data.description.right.padding.for.centered.layout",
                    "Right padding for centered layout.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("centered_layout.right_padding"),
                    pick: |settings_content| {
                        settings_content
                            .workspace
                            .centered_layout
                            .as_ref()?
                            .right_padding
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .workspace
                            .centered_layout
                            .get_or_insert_default()
                            .right_padding = value;
                    },
                }),
                metadata: None,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.focus.follows.mouse",
                    "Focus Follows Mouse",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.change.focus.to.a.pane.when.the.mouse.hovers.over.it",
                    "Whether to change focus to a pane when the mouse hovers over it.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("focus_follows_mouse.enabled"),
                    pick: |settings_content| {
                        settings_content
                            .workspace
                            .focus_follows_mouse
                            .as_ref()
                            .and_then(|s| s.enabled.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .workspace
                            .focus_follows_mouse
                            .get_or_insert_default()
                            .enabled = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.focus.follows.mouse.debounce.ms",
                    "Focus Follows Mouse Debounce ms",
                ),
                description: lt(
                    "settings_ui.page_data.description.amount.of.time.to.wait.before.changing.focus",
                    "Amount of time to wait before changing focus.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("focus_follows_mouse.debounce_ms"),
                    pick: |settings_content| {
                        settings_content
                            .workspace
                            .focus_follows_mouse
                            .as_ref()
                            .and_then(|s| s.debounce_ms.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .workspace
                            .focus_follows_mouse
                            .get_or_insert_default()
                            .debounce_ms = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn window_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.window", "Window")),
            // todo(settings_ui): Should we filter by platform.as_ref()?
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.use.system.window.tabs",
                    "Use System Window Tabs",
                ),
                description: lt(
                    "settings_ui.page_data.description.macos.only.whether.to.allow.windows.to.tab.together",
                    "(macOS only) whether to allow Windows to tab together.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("use_system_window_tabs"),
                    pick: |settings_content| {
                        settings_content.workspace.use_system_window_tabs.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.workspace.use_system_window_tabs = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.window.decorations",
                    "Window Decorations",
                ),
                description: lt(
                    "settings_ui.page_data.description.linux.only.whether.zzz.or.your.compositor.should.draw.window.decorations",
                    "(Linux only) whether ZZZ or your compositor should draw window decorations.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("window_decorations"),
                    pick: |settings_content| settings_content.workspace.window_decorations.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.workspace.window_decorations = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn pane_modifiers_section() -> [SettingsPageItem; 5] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.pane.modifiers",
                "Pane Modifiers",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.inactive.opacity",
                    "Inactive Opacity",
                ),
                description: lt(
                    "settings_ui.page_data.description.opacity.of.inactive.panels.0.0.1.0",
                    "Opacity of inactive panels (0.0 - 1.0).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("active_pane_modifiers.inactive_opacity"),
                    pick: |settings_content| {
                        settings_content
                            .workspace
                            .active_pane_modifiers
                            .as_ref()?
                            .inactive_opacity
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .workspace
                            .active_pane_modifiers
                            .get_or_insert_default()
                            .inactive_opacity = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.border.size", "Border Size"),
                description: lt(
                    "settings_ui.page_data.description.size.of.the.border.surrounding.the.active.pane",
                    "Size of the border surrounding the active pane.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("active_pane_modifiers.border_size"),
                    pick: |settings_content| {
                        settings_content
                            .workspace
                            .active_pane_modifiers
                            .as_ref()?
                            .border_size
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .workspace
                            .active_pane_modifiers
                            .get_or_insert_default()
                            .border_size = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.zoomed.padding",
                    "Zoomed Padding",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.padding.for.zoomed.panes",
                    "Show padding for zoomed panes.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("zoomed_padding"),
                    pick: |settings_content| settings_content.workspace.zoomed_padding.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.workspace.zoomed_padding = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.close.panel.on.toggle",
                    "Close Panel on Toggle",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.invoking.a.panel.s.togglefocus.action.while.it.s.already.focused.closes.the.panel.instead.of.just.moving.focus.back.to.the.editor",
                    "Whether invoking a panel's ToggleFocus action while it's already focused closes the panel, instead of just moving focus back to the editor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("close_panel_on_toggle"),
                    pick: |settings_content| {
                        settings_content.workspace.close_panel_on_toggle.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.workspace.close_panel_on_toggle = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn pane_split_direction_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.pane.split.direction",
                "Pane Split Direction",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.vertical.split.direction",
                    "Vertical Split Direction",
                ),
                description: lt(
                    "settings_ui.page_data.description.direction.to.split.vertically",
                    "Direction to split vertically.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("pane_split_direction_vertical"),
                    pick: |settings_content| {
                        settings_content
                            .workspace
                            .pane_split_direction_vertical
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.workspace.pane_split_direction_vertical = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.horizontal.split.direction",
                    "Horizontal Split Direction",
                ),
                description: lt(
                    "settings_ui.page_data.description.direction.to.split.horizontally",
                    "Direction to split horizontally.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("pane_split_direction_horizontal"),
                    pick: |settings_content| {
                        settings_content
                            .workspace
                            .pane_split_direction_horizontal
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.workspace.pane_split_direction_horizontal = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    SettingsPage {
        title: lt(
            "settings_ui.page_data.title.window.layout",
            "Window & Layout",
        ),
        items: concat_sections![
            status_bar_section(),
            title_bar_section(),
            tab_bar_section(),
            tab_settings_section(),
            preview_tabs_section(),
            layout_section(),
            window_section(),
            pane_modifiers_section(),
            pane_split_direction_section(),
        ],
    }
}

fn panels_page() -> SettingsPage {
    fn project_panel_section() -> [SettingsPageItem; 32] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.project.panel",
                "Project Panel",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.project.panel.dock",
                    "Project Panel Dock",
                ),
                description: lt(
                    "settings_ui.page_data.description.where.to.dock.the.project.panel",
                    "Where to dock the project panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.dock"),
                    pick: |settings_content| settings_content.project_panel.as_ref()?.dock.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.project_panel.get_or_insert_default().dock = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.project.panel.title.tooltip.delay",
                    "Path Tooltip Delay",
                ),
                description: lt(
                    "settings_ui.page_data.description.delay.before.showing.a.path.tooltip",
                    "Delay before showing a path tooltip when hovering a project panel entry.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.title_tooltip_delay"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .title_tooltip_delay
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .title_tooltip_delay = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.project.panel.default.width",
                    "Project Panel Default Width",
                ),
                description: lt(
                    "settings_ui.page_data.description.default.width.of.the.project.panel.in.pixels",
                    "Default width of the project panel in pixels.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.default_width"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .default_width
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .default_width = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.hide.gitignore",
                    "Hide .gitignore",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.hide.the.gitignore.entries.in.the.project.panel",
                    "Whether to hide the gitignore entries in the project panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.hide_gitignore"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .hide_gitignore
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .hide_gitignore = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.entry.spacing", "Entry Spacing"),
                description: lt(
                    "settings_ui.page_data.description.spacing.between.worktree.entries.in.the.project.panel",
                    "Spacing between worktree entries in the project panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.entry_spacing"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .entry_spacing
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .entry_spacing = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.file.icons", "File Icons"),
                description: lt(
                    "settings_ui.page_data.description.show.file.icons.in.the.project.panel",
                    "Show file icons in the project panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.file_icons"),
                    pick: |settings_content| {
                        settings_content.project_panel.as_ref()?.file_icons.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .file_icons = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.folder.icons", "Folder Icons"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.show.folder.icons.or.chevrons.for.directories.in.the.project.panel",
                    "Whether to show folder icons or chevrons for directories in the project panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.folder_icons"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .folder_icons
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .folder_icons = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.git.status", "Git Status"),
                description: lt(
                    "settings_ui.page_data.description.show.the.git.status.in.the.project.panel",
                    "Show the Git status in the project panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.git_status"),
                    pick: |settings_content| {
                        settings_content.project_panel.as_ref()?.git_status.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .git_status = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.indent.size", "Indent Size"),
                description: lt(
                    "settings_ui.page_data.description.amount.of.indentation.for.nested.items",
                    "Amount of indentation for nested items.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.indent_size"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .indent_size
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .indent_size = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.auto.reveal.entries",
                    "Auto Reveal Entries",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.reveal.entries.in.the.project.panel.automatically.when.a.corresponding.project.entry.becomes.active",
                    "Whether to reveal entries in the project panel automatically when a corresponding project entry becomes active.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.auto_reveal_entries"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .auto_reveal_entries
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .auto_reveal_entries = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.starts.open", "Starts Open"),
                description: lt(
                    "settings_ui.page_data.description.whether.the.project.panel.should.open.on.startup",
                    "Whether the project panel should open on startup.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.starts_open"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .starts_open
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .starts_open = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.auto.fold.directories",
                    "Auto Fold Directories",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.fold.directories.automatically.and.show.compact.folders.when.a.directory.has.only.one.subdirectory.inside",
                    "Whether to fold directories automatically and show compact folders when a directory has only one subdirectory inside.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.auto_fold_dirs"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .auto_fold_dirs
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .auto_fold_dirs = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.bold.folder.labels",
                    "Bold Folder Labels",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.show.folder.names.with.bold.text.in.the.project.panel",
                    "Whether to show folder names with bold text in the project panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.bold_folder_labels"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .bold_folder_labels
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .bold_folder_labels = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.scrollbar",
                    "Show Scrollbar",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.scrollbar.in.the.project.panel",
                    "Show the scrollbar in the project panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.scrollbar.show"),
                    pick: |settings_content| {
                        show_scrollbar_or_editor(settings_content, |settings_content| {
                            settings_content
                                .project_panel
                                .as_ref()?
                                .scrollbar
                                .as_ref()?
                                .show
                                .as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .scrollbar
                            .get_or_insert_default()
                            .show = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.horizontal.scroll",
                    "Horizontal Scroll",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.allow.horizontal.scrolling.in.the.project.panel.when.disabled.the.view.is.always.locked.to.the.leftmost.position.and.long.file.names.are.clipped",
                    "Whether to allow horizontal scrolling in the project panel. When disabled, the view is always locked to the leftmost position and long file names are clipped.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.scrollbar.horizontal_scroll"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .scrollbar
                            .as_ref()?
                            .horizontal_scroll
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .scrollbar
                            .get_or_insert_default()
                            .horizontal_scroll = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.diagnostics",
                    "Show Diagnostics",
                ),
                description: lt(
                    "settings_ui.page_data.description.which.files.containing.diagnostic.errors.warnings.to.mark.in.the.project.panel",
                    "Which files containing diagnostic errors/warnings to mark in the project panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.show_diagnostics"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .show_diagnostics
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .show_diagnostics = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.diagnostic.badges",
                    "Diagnostic Badges",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.error.and.warning.count.badges.next.to.file.names.in.the.project.panel",
                    "Show error and warning count badges next to file names in the project panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.diagnostic_badges"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .diagnostic_badges
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .diagnostic_badges = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.git.status.indicator",
                    "Git Status Indicator",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.a.git.status.indicator.next.to.file.names.in.the.project.panel",
                    "Show a git status indicator next to file names in the project panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.git_status_indicator"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .git_status_indicator
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .git_status_indicator = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.sticky.scroll", "Sticky Scroll"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.stick.parent.directories.at.top.of.the.project.panel",
                    "Whether to stick parent directories at top of the project panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.sticky_scroll"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .sticky_scroll
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .sticky_scroll = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                files: USER,
                title: lt(
                    "settings_ui.page_data.title.show.indent.guides",
                    "Show Indent Guides",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.indent.guides.in.the.project.panel",
                    "Show indent guides in the project panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.indent_guides.show"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .indent_guides
                            .as_ref()?
                            .show
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .indent_guides
                            .get_or_insert_default()
                            .show = value;
                    },
                }),
                metadata: None,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.drag.and.drop", "Drag and Drop"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.enable.drag.and.drop.operations.in.the.project.panel",
                    "Whether to enable drag-and-drop operations in the project panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.drag_and_drop"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .drag_and_drop
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .drag_and_drop = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.hide.root", "Hide Root"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.hide.the.root.entry.when.only.one.folder.is.open.in.the.window",
                    "Whether to hide the root entry when only one folder is open in the window.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.hide_root"),
                    pick: |settings_content| {
                        settings_content.project_panel.as_ref()?.hide_root.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .hide_root = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.hide.hidden", "Hide Hidden"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.hide.the.hidden.entries.in.the.project.panel",
                    "Whether to hide the hidden entries in the project panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.hide_hidden"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .hide_hidden
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .hide_hidden = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.sort.mode", "Sort Mode"),
                description: lt(
                    "settings_ui.page_data.description.sort.order.for.entries.in.the.project.panel",
                    "Sort order for entries in the project panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.sort_mode"),
                    pick: |settings_content| {
                        settings_content.project_panel.as_ref()?.sort_mode.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .sort_mode = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.sort.order", "Sort Order"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.sort.file.and.folder.names.case.sensitively.in.the.project.panel",
                    "Whether to sort file and folder names case-sensitively in the project panel.",
                ),
                field: Box::new(SettingField {
                    pick: |settings_content| {
                        settings_content.project_panel.as_ref()?.sort_order.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .sort_order = value;
                    },
                    json_path: Some("project_panel.sort_order"),
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.auto.open.files.on.create",
                    "Auto Open Files On Create",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.automatically.open.newly.created.files.in.the.editor",
                    "Whether to automatically open newly created files in the editor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.auto_open.on_create"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .auto_open
                            .as_ref()?
                            .on_create
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .auto_open
                            .get_or_insert_default()
                            .on_create = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.auto.open.files.on.paste",
                    "Auto Open Files On Paste",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.automatically.open.files.after.pasting.or.duplicating.them",
                    "Whether to automatically open files after pasting or duplicating them.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.auto_open.on_paste"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .auto_open
                            .as_ref()?
                            .on_paste
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .auto_open
                            .get_or_insert_default()
                            .on_paste = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.auto.open.files.on.drop",
                    "Auto Open Files On Drop",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.automatically.open.files.dropped.from.external.sources",
                    "Whether to automatically open files dropped from external sources.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.auto_open.on_drop"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .auto_open
                            .as_ref()?
                            .on_drop
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .auto_open
                            .get_or_insert_default()
                            .on_drop = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.should.focus", "Should Focus"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.focus.on.files.automatically.opened",
                    "Whether to focus on files automatically opened.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.auto_open.should_focus"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .auto_open
                            .as_ref()?
                            .should_focus
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .auto_open
                            .get_or_insert_default()
                            .should_focus = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.should.focus", "Should Focus"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.focus.on.files.automatically.opened",
                    "Whether to focus on files automatically opened.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("project_panel.auto_open.should_focus"),
                    pick: |settings_content| {
                        settings_content
                            .project_panel
                            .as_ref()?
                            .auto_open
                            .as_ref()?
                            .should_focus
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project_panel
                            .get_or_insert_default()
                            .auto_open
                            .get_or_insert_default()
                            .should_focus = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.hidden.files", "Hidden Files"),
                description: lt(
                    "settings_ui.page_data.description.globs.to.match.files.that.will.be.considered.hidden.and.can.be.hidden.from.the.project.panel",
                    "Globs to match files that will be considered \\\"hidden\\\" and can be hidden from the project panel.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("worktree.hidden_files"),
                        pick: |settings_content| {
                            settings_content.project.worktree.hidden_files.as_ref()
                        },
                        write: |settings_content, value, _| {
                            settings_content.project.worktree.hidden_files = value;
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn terminal_panel_section() -> [SettingsPageItem; 4] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.terminal.panel",
                "Terminal Panel",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.terminal.dock", "Terminal Dock"),
                description: lt(
                    "settings_ui.page_data.description.where.to.dock.the.terminal.panel",
                    "Where to dock the terminal panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.dock"),
                    pick: |settings_content| settings_content.terminal.as_ref()?.dock.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.terminal.get_or_insert_default().dock = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.terminal.panel.flexible.sizing",
                    "Terminal Panel Flexible Sizing",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.the.terminal.panel.should.use.flexible.proportional.sizing.when.docked.to.the.left.or.right",
                    "Whether the terminal panel should use flexible (proportional) sizing when docked to the left or right.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.flexible"),
                    pick: |settings_content| settings_content.terminal.as_ref()?.flexible.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.terminal.get_or_insert_default().flexible = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.count.badge",
                    "Show Count Badge",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.a.badge.on.the.terminal.panel.icon.with.the.count.of.open.terminals",
                    "Show a badge on the terminal panel icon with the count of open terminals.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.show_count_badge"),
                    pick: |settings_content| {
                        settings_content
                            .terminal
                            .as_ref()?
                            .show_count_badge
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .show_count_badge = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn outline_panel_section() -> [SettingsPageItem; 11] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.outline.panel",
                "Outline Panel",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.outline.panel.button",
                    "Outline Panel Button",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.outline.panel.button.in.the.status.bar",
                    "Show the outline panel button in the status bar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("outline_panel.button"),
                    pick: |settings_content| {
                        settings_content.outline_panel.as_ref()?.button.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .outline_panel
                            .get_or_insert_default()
                            .button = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.outline.panel.dock",
                    "Outline Panel Dock",
                ),
                description: lt(
                    "settings_ui.page_data.description.where.to.dock.the.outline.panel",
                    "Where to dock the outline panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("outline_panel.dock"),
                    pick: |settings_content| settings_content.outline_panel.as_ref()?.dock.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.outline_panel.get_or_insert_default().dock = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.outline.panel.default.width",
                    "Outline Panel Default Width",
                ),
                description: lt(
                    "settings_ui.page_data.description.default.width.of.the.outline.panel.in.pixels",
                    "Default width of the outline panel in pixels.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("outline_panel.default_width"),
                    pick: |settings_content| {
                        settings_content
                            .outline_panel
                            .as_ref()?
                            .default_width
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .outline_panel
                            .get_or_insert_default()
                            .default_width = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.file.icons", "File Icons"),
                description: lt(
                    "settings_ui.page_data.description.show.file.icons.in.the.outline.panel",
                    "Show file icons in the outline panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("outline_panel.file_icons"),
                    pick: |settings_content| {
                        settings_content.outline_panel.as_ref()?.file_icons.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .outline_panel
                            .get_or_insert_default()
                            .file_icons = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.folder.icons", "Folder Icons"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.show.folder.icons.or.chevrons.for.directories.in.the.outline.panel",
                    "Whether to show folder icons or chevrons for directories in the outline panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("outline_panel.folder_icons"),
                    pick: |settings_content| {
                        settings_content
                            .outline_panel
                            .as_ref()?
                            .folder_icons
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .outline_panel
                            .get_or_insert_default()
                            .folder_icons = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.git.status", "Git Status"),
                description: lt(
                    "settings_ui.page_data.description.show.the.git.status.in.the.outline.panel",
                    "Show the Git status in the outline panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("outline_panel.git_status"),
                    pick: |settings_content| {
                        settings_content.outline_panel.as_ref()?.git_status.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .outline_panel
                            .get_or_insert_default()
                            .git_status = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.indent.size", "Indent Size"),
                description: lt(
                    "settings_ui.page_data.description.amount.of.indentation.for.nested.items",
                    "Amount of indentation for nested items.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("outline_panel.indent_size"),
                    pick: |settings_content| {
                        settings_content
                            .outline_panel
                            .as_ref()?
                            .indent_size
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .outline_panel
                            .get_or_insert_default()
                            .indent_size = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.auto.reveal.entries",
                    "Auto Reveal Entries",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.reveal.when.a.corresponding.outline.entry.becomes.active",
                    "Whether to reveal when a corresponding outline entry becomes active.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("outline_panel.auto_reveal_entries"),
                    pick: |settings_content| {
                        settings_content
                            .outline_panel
                            .as_ref()?
                            .auto_reveal_entries
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .outline_panel
                            .get_or_insert_default()
                            .auto_reveal_entries = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.auto.fold.directories",
                    "Auto Fold Directories",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.fold.directories.automatically.when.a.directory.contains.only.one.subdirectory",
                    "Whether to fold directories automatically when a directory contains only one subdirectory.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("outline_panel.auto_fold_dirs"),
                    pick: |settings_content| {
                        settings_content
                            .outline_panel
                            .as_ref()?
                            .auto_fold_dirs
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .outline_panel
                            .get_or_insert_default()
                            .auto_fold_dirs = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                files: USER,
                title: lt(
                    "settings_ui.page_data.title.show.indent.guides",
                    "Show Indent Guides",
                ),
                description: lt(
                    "settings_ui.page_data.description.when.to.show.indent.guides.in.the.outline.panel",
                    "When to show indent guides in the outline panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("outline_panel.indent_guides.show"),
                    pick: |settings_content| {
                        settings_content
                            .outline_panel
                            .as_ref()?
                            .indent_guides
                            .as_ref()?
                            .show
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .outline_panel
                            .get_or_insert_default()
                            .indent_guides
                            .get_or_insert_default()
                            .show = value;
                    },
                }),
                metadata: None,
            }),
        ]
    }

    fn git_panel_section() -> [SettingsPageItem; 16] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.git.panel",
                "Git Panel",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.git.panel.button",
                    "Git Panel Button",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.the.git.panel.button.in.the.status.bar",
                    "Show the Git panel button in the status bar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git_panel.button"),
                    pick: |settings_content| settings_content.git_panel.as_ref()?.button.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.git_panel.get_or_insert_default().button = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.git.panel.dock",
                    "Git Panel Dock",
                ),
                description: lt(
                    "settings_ui.page_data.description.where.to.dock.the.git.panel",
                    "Where to dock the Git panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git_panel.dock"),
                    pick: |settings_content| settings_content.git_panel.as_ref()?.dock.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.git_panel.get_or_insert_default().dock = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.git.panel.default.width",
                    "Git Panel Default Width",
                ),
                description: lt(
                    "settings_ui.page_data.description.default.width.of.the.git.panel.in.pixels",
                    "Default width of the Git panel in pixels.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git_panel.default_width"),
                    pick: |settings_content| {
                        settings_content.git_panel.as_ref()?.default_width.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git_panel
                            .get_or_insert_default()
                            .default_width = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.git.panel.status.style",
                    "Git Panel Status Style",
                ),
                description: lt(
                    "settings_ui.page_data.description.how.entry.statuses.are.displayed",
                    "How entry statuses are displayed.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git_panel.status_style"),
                    pick: |settings_content| {
                        settings_content.git_panel.as_ref()?.status_style.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git_panel
                            .get_or_insert_default()
                            .status_style = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.fallback.branch.name",
                    "Fallback Branch Name",
                ),
                description: lt(
                    "settings_ui.page_data.description.default.branch.name.will.be.when.init.defaultbranch.is.not.set.in.git",
                    "Default branch name will be when init.defaultbranch is not set in Git.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git_panel.fallback_branch_name"),
                    pick: |settings_content| {
                        settings_content
                            .git_panel
                            .as_ref()?
                            .fallback_branch_name
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git_panel
                            .get_or_insert_default()
                            .fallback_branch_name = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.sort.by.path", "Sort By Path"),
                description: lt(
                    "settings_ui.page_data.description.enable.to.sort.entries.in.the.panel.by.path.disable.to.sort.by.status",
                    "Enable to sort entries in the panel by path, disable to sort by status.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git_panel.sort_by_path"),
                    pick: |settings_content| {
                        settings_content.git_panel.as_ref()?.sort_by_path.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git_panel
                            .get_or_insert_default()
                            .sort_by_path = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.collapse.untracked.diff",
                    "Collapse Untracked Diff",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.collapse.untracked.files.in.the.diff.panel",
                    "Whether to collapse untracked files in the diff panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git_panel.collapse_untracked_diff"),
                    pick: |settings_content| {
                        settings_content
                            .git_panel
                            .as_ref()?
                            .collapse_untracked_diff
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git_panel
                            .get_or_insert_default()
                            .collapse_untracked_diff = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.tree.view", "Tree View"),
                description: lt(
                    "settings_ui.page_data.description.enable.to.show.entries.in.tree.view.list.disable.to.show.in.flat.view.list",
                    "Enable to show entries in tree view list, disable to show in flat view list.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git_panel.tree_view"),
                    pick: |settings_content| {
                        settings_content.git_panel.as_ref()?.tree_view.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.git_panel.get_or_insert_default().tree_view = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.file.icons", "File Icons"),
                description: lt(
                    "settings_ui.page_data.description.show.file.icons.next.to.the.git.status.icon",
                    "Show file icons next to the Git status icon.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git_panel.file_icons"),
                    pick: |settings_content| {
                        settings_content.git_panel.as_ref()?.file_icons.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git_panel
                            .get_or_insert_default()
                            .file_icons = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.folder.icons", "Folder Icons"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.show.folder.icons.or.chevrons.for.directories.in.the.git.panel",
                    "Whether to show folder icons or chevrons for directories in the git panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git_panel.folder_icons"),
                    pick: |settings_content| {
                        settings_content.git_panel.as_ref()?.folder_icons.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git_panel
                            .get_or_insert_default()
                            .folder_icons = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.diff.stats", "Diff Stats"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.show.the.addition.deletion.change.count.next.to.each.file.in.the.git.panel",
                    "Whether to show the addition/deletion change count next to each file in the Git panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git_panel.diff_stats"),
                    pick: |settings_content| {
                        settings_content.git_panel.as_ref()?.diff_stats.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git_panel
                            .get_or_insert_default()
                            .diff_stats = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.primary.click.behavior",
                    "Primary Click Behavior",
                ),
                description: lt(
                    "settings_ui.page_data.description.default.action.when.clicking.a.changed.file.in.the.git.panel",
                    "Default action when clicking a changed file in the Git panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git_panel.entry_primary_click_action"),
                    pick: |settings_content| {
                        settings_content
                            .git_panel
                            .as_ref()?
                            .entry_primary_click_action
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git_panel
                            .get_or_insert_default()
                            .entry_primary_click_action = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.count.badge",
                    "Show Count Badge",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.show.a.badge.on.the.git.panel.icon.with.the.count.of.uncommitted.changes",
                    "Whether to show a badge on the git panel icon with the count of uncommitted changes.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git_panel.show_count_badge"),
                    pick: |settings_content| {
                        settings_content
                            .git_panel
                            .as_ref()?
                            .show_count_badge
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git_panel
                            .get_or_insert_default()
                            .show_count_badge = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.commit.title.max.length",
                    "Commit Title Max Length",
                ),
                description: lt(
                    "settings_ui.page_data.description.maximum.length.of.the.commit.message.title.before.a.warning.is.shown.set.to.0.to.disable",
                    "Maximum length of the commit message title before a warning is shown. Set to 0 to disable.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git_panel.commit_title_max_length"),
                    pick: |settings_content| {
                        settings_content
                            .git_panel
                            .as_ref()?
                            .commit_title_max_length
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git_panel
                            .get_or_insert_default()
                            .commit_title_max_length = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.scroll.bar", "Scroll Bar"),
                description: lt(
                    "settings_ui.page_data.description.how.and.when.the.scrollbar.should.be.displayed",
                    "How and when the scrollbar should be displayed.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git_panel.scrollbar.show"),
                    pick: |settings_content| {
                        show_scrollbar_or_editor(settings_content, |settings_content| {
                            settings_content
                                .git_panel
                                .as_ref()?
                                .scrollbar
                                .as_ref()?
                                .show
                                .as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git_panel
                            .get_or_insert_default()
                            .scrollbar
                            .get_or_insert_default()
                            .show = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn debugger_panel_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.debugger.panel",
                "Debugger Panel",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.debugger.panel.dock",
                    "Debugger Panel Dock",
                ),
                description: lt(
                    "settings_ui.page_data.description.the.dock.position.of.the.debug.panel",
                    "The dock position of the debug panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("debugger.dock"),
                    pick: |settings_content| settings_content.debugger.as_ref()?.dock.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.debugger.get_or_insert_default().dock = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn agent_panel_section() -> [SettingsPageItem; 7] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.agent.panel",
                "Agent Panel",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.agent.panel.button",
                    "Agent Panel Button",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.show.the.agent.panel.button.in.the.status.bar",
                    "Whether to show the agent panel button in the status bar.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("agent.button"),
                    pick: |settings_content| settings_content.agent.as_ref()?.button.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.agent.get_or_insert_default().button = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.agent.panel.dock",
                    "Agent Panel Dock",
                ),
                description: lt(
                    "settings_ui.page_data.description.where.to.dock.the.agent.panel",
                    "Where to dock the agent panel.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("agent.dock"),
                    pick: |settings_content| settings_content.agent.as_ref()?.dock.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.agent.get_or_insert_default().dock = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.agent.panel.flexible.sizing",
                    "Agent Panel Flexible Sizing",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.the.agent.panel.should.use.flexible.proportional.sizing.when.docked.to.the.left.or.right",
                    "Whether the agent panel should use flexible (proportional) sizing when docked to the left or right. When enabled, the default width does not control the panel width, and resetting the panel restores the default proportion.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("agent.flexible"),
                    pick: |settings_content| settings_content.agent.as_ref()?.flexible.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.agent.get_or_insert_default().flexible = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.agent.panel.default.width",
                    "Agent Panel Default Width",
                ),
                description: lt(
                    "settings_ui.page_data.description.default.width.when.the.agent.panel.is.docked.to.the.left.or.right",
                    "Default fixed width when the agent panel is docked to the left or right and flexible sizing is disabled.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("agent.default_width"),
                    pick: |settings_content| {
                        settings_content.agent.as_ref()?.default_width.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.agent.get_or_insert_default().default_width = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.agent.panel.default.height",
                    "Agent Panel Default Height",
                ),
                description: lt(
                    "settings_ui.page_data.description.default.height.when.the.agent.panel.is.docked.to.the.bottom",
                    "Default height when the agent panel is docked to the bottom.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("agent.default_height"),
                    pick: |settings_content| {
                        settings_content.agent.as_ref()?.default_height.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .agent
                            .get_or_insert_default()
                            .default_height = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::DynamicItem(DynamicItem {
                discriminant: SettingItem {
                    files: USER,
                    title: lt(
                        "settings_ui.page_data.title.limit.content.width",
                        "Limit Content Width",
                    ),
                    description: lt(
                        "settings_ui.page_data.description.whether.to.constrain.the.agent.panel.content.to.a.maximum.width.centering.it.when.the.panel.is.wider.for.optimal.readability",
                        "Whether to constrain the agent panel content to a maximum width, centering it when the panel is wider, for optimal readability.",
                    ),
                    field: Box::new(SettingField::<bool> {
                        json_path: Some("agent.limit_content_width"),
                        pick: |settings_content| {
                            settings_content
                                .agent
                                .as_ref()?
                                .limit_content_width
                                .as_ref()
                        },
                        write: |settings_content, value, _| {
                            settings_content
                                .agent
                                .get_or_insert_default()
                                .limit_content_width = value;
                        },
                    }),
                    metadata: None,
                },
                pick_discriminant: |settings_content| {
                    let enabled = settings_content
                        .agent
                        .as_ref()?
                        .limit_content_width
                        .unwrap_or(true);
                    Some(if enabled { 1 } else { 0 })
                },
                fields: vec![
                    vec![],
                    vec![SettingItem {
                        files: USER,
                        title: lt(
                            "settings_ui.page_data.title.max.content.width",
                            "Max Content Width",
                        ),
                        description: lt(
                            "settings_ui.page_data.description.maximum.content.width.in.pixels.content.will.be.centered.when.the.panel.is.wider.than.this.value",
                            "Maximum content width in pixels. Content will be centered when the panel is wider than this value.",
                        ),
                        field: Box::new(SettingField {
                            json_path: Some("agent.max_content_width"),
                            pick: |settings_content| {
                                settings_content.agent.as_ref()?.max_content_width.as_ref()
                            },
                            write: |settings_content, value, _| {
                                settings_content
                                    .agent
                                    .get_or_insert_default()
                                    .max_content_width = value;
                            },
                        }),
                        metadata: None,
                    }],
                ],
            }),
        ]
    }

    SettingsPage {
        title: lt("settings_ui.page_data.title.panels", "Panels"),
        items: concat_sections![
            project_panel_section(),
            terminal_panel_section(),
            outline_panel_section(),
            git_panel_section(),
            debugger_panel_section(),
            agent_panel_section(),
        ],
    }
}

fn debugger_page() -> SettingsPage {
    fn general_section() -> [SettingsPageItem; 6] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.general", "General")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.stepping.granularity",
                    "Stepping Granularity",
                ),
                description: lt(
                    "settings_ui.page_data.description.determines.the.stepping.granularity.for.debug.operations",
                    "Determines the stepping granularity for debug operations.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("debugger.stepping_granularity"),
                    pick: |settings_content| {
                        settings_content
                            .debugger
                            .as_ref()?
                            .stepping_granularity
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .debugger
                            .get_or_insert_default()
                            .stepping_granularity = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.save.breakpoints",
                    "Save Breakpoints",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.breakpoints.should.be.reused.across.zzz.sessions",
                    "Whether breakpoints should be reused across ZZZ sessions.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("debugger.save_breakpoints"),
                    pick: |settings_content| {
                        settings_content
                            .debugger
                            .as_ref()?
                            .save_breakpoints
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .debugger
                            .get_or_insert_default()
                            .save_breakpoints = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.timeout", "Timeout"),
                description: lt(
                    "settings_ui.page_data.description.time.in.milliseconds.until.timeout.error.when.connecting.to.a.tcp.debug.adapter",
                    "Time in milliseconds until timeout error when connecting to a TCP debug adapter.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("debugger.timeout"),
                    pick: |settings_content| settings_content.debugger.as_ref()?.timeout.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.debugger.get_or_insert_default().timeout = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.log.dap.communications",
                    "Log DAP Communications",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.log.messages.between.active.debug.adapters.and.zzz",
                    "Whether to log messages between active debug adapters and ZZZ.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("debugger.log_dap_communications"),
                    pick: |settings_content| {
                        settings_content
                            .debugger
                            .as_ref()?
                            .log_dap_communications
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .debugger
                            .get_or_insert_default()
                            .log_dap_communications = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.format.dap.log.messages",
                    "Format DAP Log Messages",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.format.dap.messages.when.adding.them.to.debug.adapter.logger",
                    "Whether to format DAP messages when adding them to debug adapter logger.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("debugger.format_dap_log_messages"),
                    pick: |settings_content| {
                        settings_content
                            .debugger
                            .as_ref()?
                            .format_dap_log_messages
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .debugger
                            .get_or_insert_default()
                            .format_dap_log_messages = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    SettingsPage {
        title: lt("settings_ui.page_data.title.debugger", "Debugger"),
        items: concat_sections![general_section()],
    }
}

fn terminal_page() -> SettingsPage {
    fn environment_section() -> [SettingsPageItem; 5] {
        [
                SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.environment", "Environment")),
                SettingsPageItem::DynamicItem(DynamicItem {
                    discriminant: SettingItem {
                        files: USER | PROJECT,
                        title: lt("settings_ui.page_data.title.shell", "Shell"),
                        description: lt("settings_ui.page_data.description.what.shell.to.use.when.opening.a.terminal", "What shell to use when opening a terminal."),
                        field: Box::new(SettingField {
                            json_path: Some("terminal.shell$"),
                            pick: |settings_content| {
                                Some(&dynamic_variants::<settings::Shell>()[
                                    settings_content
                                        .terminal
                                        .as_ref()?
                                        .project
                                        .shell
                                        .as_ref()?
                                        .discriminant() as usize
                                ])
                            },
                            write: |settings_content, value, _| {
                                let Some(value) = value else {
                                    if let Some(terminal) = settings_content.terminal.as_mut() {
                                        terminal.project.shell = None;
                                    }
                                    return;
                                };
                                let settings_value = settings_content
                                    .terminal
                                    .get_or_insert_default()
                                    .project
                                    .shell
                                    .get_or_insert_with(|| settings::Shell::default());
                                let default_shell = if cfg!(target_os = "windows") {
                                    "powershell.exe"
                                } else {
                                    "sh"
                                };
                                *settings_value = match value {
                                    settings::ShellDiscriminants::System => settings::Shell::System,
                                    settings::ShellDiscriminants::Program => {
                                        let program = match settings_value {
                                            settings::Shell::Program(program) => program.clone(),
                                            settings::Shell::WithArguments { program, .. } => program.clone(),
                                            _ => String::from(default_shell),
                                        };
                                        settings::Shell::Program(program)
                                    }
                                    settings::ShellDiscriminants::WithArguments => {
                                        let (program, args, title_override) = match settings_value {
                                            settings::Shell::Program(program) => (program.clone(), vec![], None),
                                            settings::Shell::WithArguments {
                                                program,
                                                args,
                                                title_override,
                                            } => (program.clone(), args.clone(), title_override.clone()),
                                            _ => (String::from(default_shell), vec![], None),
                                        };
                                        settings::Shell::WithArguments {
                                            program,
                                            args,
                                            title_override,
                                        }
                                    }
                                };
                            },
                        }),
                        metadata: None,
                    },
                    pick_discriminant: |settings_content| {
                        Some(
                            settings_content
                                .terminal
                                .as_ref()?
                                .project
                                .shell
                                .as_ref()?
                                .discriminant() as usize,
                        )
                    },
                    fields: dynamic_variants::<settings::Shell>()
                        .into_iter()
                        .map(|variant| match variant {
                            settings::ShellDiscriminants::System => vec![],
                            settings::ShellDiscriminants::Program => vec![SettingItem {
                                files: USER | PROJECT,
                                title: lt("settings_ui.page_data.title.program", "Program"),
                                description: lt("settings_ui.page_data.description.the.shell.program.to.use", "The shell program to use."),
                                field: Box::new(SettingField {
                                    json_path: Some("terminal.shell"),
                                    pick: |settings_content| match settings_content.terminal.as_ref()?.project.shell.as_ref()
                                    {
                                        Some(settings::Shell::Program(program)) => Some(program),
                                        _ => None,
                                    },
                                    write: |settings_content, value, _| {
                                        let Some(value) = value else {
                                            return;
                                        };
                                        match settings_content
                                            .terminal
                                            .get_or_insert_default()
                                            .project
                                            .shell
                                            .as_mut()
                                        {
                                            Some(settings::Shell::Program(program)) => *program = value,
                                            _ => return,
                                        }
                                    },
                                }),
                                metadata: None,
                            }],
                            settings::ShellDiscriminants::WithArguments => vec![
                                SettingItem {
                                    files: USER | PROJECT,
                                    title: lt("settings_ui.page_data.title.program", "Program"),
                                    description: lt("settings_ui.page_data.description.the.shell.program.to.run", "The shell program to run."),
                                    field: Box::new(SettingField {
                                        json_path: Some("terminal.shell.program"),
                                        pick: |settings_content| {
                                            match settings_content.terminal.as_ref()?.project.shell.as_ref() {
                                                Some(settings::Shell::WithArguments { program, .. }) => Some(program),
                                                _ => None,
                                            }
                                        },
                                        write: |settings_content, value, _| {
                                            let Some(value) = value else {
                                                return;
                                            };
                                            match settings_content
                                                .terminal
                                                .get_or_insert_default()
                                                .project
                                                .shell
                                                .as_mut()
                                            {
                                                Some(settings::Shell::WithArguments { program, .. }) => {
                                                    *program = value
                                                }
                                                _ => return,
                                            }
                                        },
                                    }),
                                    metadata: None,
                                },
                                SettingItem {
                                    files: USER | PROJECT,
                                    title: lt("settings_ui.page_data.title.arguments", "Arguments"),
                                    description: lt("settings_ui.page_data.description.the.arguments.to.pass.to.the.shell.program", "The arguments to pass to the shell program."),
                                    field: Box::new(
                                        SettingField {
                                            json_path: Some("terminal.shell.args"),
                                            pick: |settings_content| {
                                                match settings_content.terminal.as_ref()?.project.shell.as_ref() {
                                                    Some(settings::Shell::WithArguments { args, .. }) => Some(args),
                                                    _ => None,
                                                }
                                            },
                                            write: |settings_content, value, _| {
                                                let Some(value) = value else {
                                                    return;
                                                };
                                                match settings_content
                                                    .terminal
                                                    .get_or_insert_default()
                                                    .project
                                                    .shell
                                                    .as_mut()
                                                {
                                                    Some(settings::Shell::WithArguments { args, .. }) => *args = value,
                                                    _ => return,
                                                }
                                            },
                                        }
                                        .unimplemented(),
                                    ),
                                    metadata: None,
                                },
                                SettingItem {
                                    files: USER | PROJECT,
                                    title: lt("settings_ui.page_data.title.title.override", "Title Override"),
                                    description: lt("settings_ui.page_data.description.an.optional.string.to.override.the.title.of.the.terminal.tab", "An optional string to override the title of the terminal tab."),
                                    field: Box::new(SettingField {
                                        json_path: Some("terminal.shell.title_override"),
                                        pick: |settings_content| {
                                            match settings_content.terminal.as_ref()?.project.shell.as_ref() {
                                                Some(settings::Shell::WithArguments { title_override, .. }) => {
                                                    title_override.as_ref().or(DEFAULT_EMPTY_STRING)
                                                }
                                                _ => None,
                                            }
                                        },
                                        write: |settings_content, value, _| {
                                            match settings_content
                                                .terminal
                                                .get_or_insert_default()
                                                .project
                                                .shell
                                                .as_mut()
                                            {
                                                Some(settings::Shell::WithArguments { title_override, .. }) => {
                                                    *title_override = value.filter(|s| !s.is_empty())
                                                }
                                                _ => return,
                                            }
                                        },
                                    }),
                                    metadata: None,
                                },
                            ],
                        })
                        .collect(),
                }),
                SettingsPageItem::DynamicItem(DynamicItem {
                    discriminant: SettingItem {
                        files: USER | PROJECT,
                        title: lt("settings_ui.page_data.title.working.directory", "Working Directory"),
                        description: lt("settings_ui.page_data.description.what.working.directory.to.use.when.launching.the.terminal", "What working directory to use when launching the terminal."),
                        field: Box::new(SettingField {
                            json_path: Some("terminal.working_directory$"),
                            pick: |settings_content| {
                                Some(&dynamic_variants::<settings::WorkingDirectory>()[
                                    settings_content
                                        .terminal
                                        .as_ref()?
                                        .project
                                        .working_directory
                                        .as_ref()?
                                        .discriminant() as usize
                                ])
                            },
                            write: |settings_content, value, _| {
                                let Some(value) = value else {
                                    if let Some(terminal) = settings_content.terminal.as_mut() {
                                        terminal.project.working_directory = None;
                                    }
                                    return;
                                };
                                let settings_value = settings_content
                                    .terminal
                                    .get_or_insert_default()
                                    .project
                                    .working_directory
                                    .get_or_insert_with(|| settings::WorkingDirectory::CurrentProjectDirectory);
                                *settings_value = match value {
                                    settings::WorkingDirectoryDiscriminants::CurrentFileDirectory => {
                                        settings::WorkingDirectory::CurrentFileDirectory
                                    },
                                    settings::WorkingDirectoryDiscriminants::CurrentProjectDirectory => {
                                        settings::WorkingDirectory::CurrentProjectDirectory
                                    }
                                    settings::WorkingDirectoryDiscriminants::FirstProjectDirectory => {
                                        settings::WorkingDirectory::FirstProjectDirectory
                                    }
                                    settings::WorkingDirectoryDiscriminants::AlwaysHome => {
                                        settings::WorkingDirectory::AlwaysHome
                                    }
                                    settings::WorkingDirectoryDiscriminants::Always => {
                                        let directory = match settings_value {
                                            settings::WorkingDirectory::Always { .. } => return,
                                            _ => String::new(),
                                        };
                                        settings::WorkingDirectory::Always { directory }
                                    }
                                };
                            },
                        }),
                        metadata: None,
                    },
                    pick_discriminant: |settings_content| {
                        Some(
                            settings_content
                                .terminal
                                .as_ref()?
                                .project
                                .working_directory
                                .as_ref()?
                                .discriminant() as usize,
                        )
                    },
                    fields: dynamic_variants::<settings::WorkingDirectory>()
                        .into_iter()
                        .map(|variant| match variant {
                            settings::WorkingDirectoryDiscriminants::CurrentFileDirectory => vec![],
                            settings::WorkingDirectoryDiscriminants::CurrentProjectDirectory => vec![],
                            settings::WorkingDirectoryDiscriminants::FirstProjectDirectory => vec![],
                            settings::WorkingDirectoryDiscriminants::AlwaysHome => vec![],
                            settings::WorkingDirectoryDiscriminants::Always => vec![SettingItem {
                                files: USER | PROJECT,
                                title: lt("settings_ui.page_data.title.directory", "Directory"),
                                description: lt("settings_ui.page_data.description.the.directory.path.to.use.will.be.shell.expanded", "The directory path to use (will be shell expanded)."),
                                field: Box::new(SettingField {
                                    json_path: Some("terminal.working_directory.always"),
                                    pick: |settings_content| {
                                        match settings_content.terminal.as_ref()?.project.working_directory.as_ref() {
                                            Some(settings::WorkingDirectory::Always { directory }) => Some(directory),
                                            _ => None,
                                        }
                                    },
                                    write: |settings_content, value, _| {
                                        let value = value.unwrap_or_default();
                                        match settings_content
                                            .terminal
                                            .get_or_insert_default()
                                            .project
                                            .working_directory
                                            .as_mut()
                                        {
                                            Some(settings::WorkingDirectory::Always { directory }) => *directory = value,
                                            _ => return,
                                        }
                                    },
                                }),
                                metadata: None,
                            }],
                        })
                        .collect(),
                }),
                SettingsPageItem::SettingItem(SettingItem {
                    title: lt("settings_ui.page_data.title.environment.variables", "Environment Variables"),
                    description: lt("settings_ui.page_data.description.key.value.pairs.to.add.to.the.terminal.s.environment", "Key-value pairs to add to the terminal's environment."),
                    field: Box::new(
                        SettingField {
                            json_path: Some("terminal.env"),
                            pick: |settings_content| settings_content.terminal.as_ref()?.project.env.as_ref(),
                            write: |settings_content, value, _| {
                                settings_content.terminal.get_or_insert_default().project.env = value;
                            },
                        }
                        .unimplemented(),
                    ),
                    metadata: None,
                    files: USER | PROJECT,
                }),
                SettingsPageItem::SettingItem(SettingItem {
                    title: lt("settings_ui.page_data.title.detect.virtual.environment", "Detect Virtual Environment"),
                    description: lt("settings_ui.page_data.description.activates.the.python.virtual.environment.if.one.is.found.in.the.terminal.s.working.directory", "Activates the Python virtual environment, if one is found, in the terminal's working directory."),
                    field: Box::new(
                        SettingField {
                            json_path: Some("terminal.detect_venv"),
                            pick: |settings_content| settings_content.terminal.as_ref()?.project.detect_venv.as_ref(),
                            write: |settings_content, value, _| {
                                settings_content
                                    .terminal
                                    .get_or_insert_default()
                                    .project
                                    .detect_venv = value;
                            },
                        }
                        .unimplemented(),
                    ),
                    metadata: None,
                    files: USER | PROJECT,
                }),
            ]
    }

    fn font_section() -> [SettingsPageItem; 6] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.font", "Font")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.font.size", "Font Size"),
                description: lt(
                    "settings_ui.page_data.description.font.size.for.terminal.text.if.not.set.defaults.to.buffer.font.size",
                    "Font size for terminal text. If not set, defaults to buffer font size.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.font_size"),
                    pick: |settings_content| {
                        settings_content
                            .terminal
                            .as_ref()
                            .and_then(|terminal| terminal.font_size.as_ref())
                            .or(settings_content.theme.buffer_font_size.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content.terminal.get_or_insert_default().font_size = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.font.family", "Font Family"),
                description: lt(
                    "settings_ui.page_data.description.font.family.for.terminal.text.if.not.set.defaults.to.buffer.font.family",
                    "Font family for terminal text. If not set, defaults to buffer font family.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.font_family"),
                    pick: |settings_content| {
                        settings_content
                            .terminal
                            .as_ref()
                            .and_then(|terminal| terminal.font_family.as_ref())
                            .or(settings_content.theme.buffer_font_family.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .font_family = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.font.fallbacks",
                    "Font Fallbacks",
                ),
                description: lt(
                    "settings_ui.page_data.description.font.fallbacks.for.terminal.text.if.not.set.defaults.to.buffer.font.fallbacks",
                    "Font fallbacks for terminal text. If not set, defaults to buffer font fallbacks.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("terminal.font_fallbacks"),
                        pick: |settings_content| {
                            settings_content
                                .terminal
                                .as_ref()
                                .and_then(|terminal| terminal.font_fallbacks.as_ref())
                                .or(settings_content.theme.buffer_font_fallbacks.as_ref())
                        },
                        write: |settings_content, value, _| {
                            settings_content
                                .terminal
                                .get_or_insert_default()
                                .font_fallbacks = value;
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.font.weight", "Font Weight"),
                description: lt(
                    "settings_ui.page_data.description.font.weight.for.terminal.text.in.css.weight.units.100.900",
                    "Font weight for terminal text in CSS weight units (100-900).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.font_weight"),
                    pick: |settings_content| {
                        settings_content.terminal.as_ref()?.font_weight.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .font_weight = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.font.features", "Font Features"),
                description: lt(
                    "settings_ui.page_data.description.font.features.for.terminal.text",
                    "Font features for terminal text.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("terminal.font_features"),
                        pick: |settings_content| {
                            settings_content
                                .terminal
                                .as_ref()
                                .and_then(|terminal| terminal.font_features.as_ref())
                                .or(settings_content.theme.buffer_font_features.as_ref())
                        },
                        write: |settings_content, value, _| {
                            settings_content
                                .terminal
                                .get_or_insert_default()
                                .font_features = value;
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn display_settings_section() -> [SettingsPageItem; 6] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.display.settings",
                "Display Settings",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.line.height", "Line Height"),
                description: lt(
                    "settings_ui.page_data.description.line.height.for.terminal.text",
                    "Line height for terminal text.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("terminal.line_height"),
                        pick: |settings_content| {
                            settings_content.terminal.as_ref()?.line_height.as_ref()
                        },
                        write: |settings_content, value, _| {
                            settings_content
                                .terminal
                                .get_or_insert_default()
                                .line_height = value;
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.cursor.shape", "Cursor Shape"),
                description: lt(
                    "settings_ui.page_data.description.default.cursor.shape.for.the.terminal.bar.block.underline.or.hollow",
                    "Default cursor shape for the terminal (bar, block, underline, or hollow).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.cursor_shape"),
                    pick: |settings_content| {
                        settings_content.terminal.as_ref()?.cursor_shape.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .cursor_shape = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.cursor.blinking",
                    "Cursor Blinking",
                ),
                description: lt(
                    "settings_ui.page_data.description.sets.the.cursor.blinking.behavior.in.the.terminal",
                    "Sets the cursor blinking behavior in the terminal.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.blinking"),
                    pick: |settings_content| settings_content.terminal.as_ref()?.blinking.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.terminal.get_or_insert_default().blinking = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.alternate.scroll",
                    "Alternate Scroll",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.alternate.scroll.mode.is.active.by.default.converts.mouse.scroll.to.arrow.keys.in.apps.like.vim",
                    "Whether alternate scroll mode is active by default (converts mouse scroll to arrow keys in apps like Vim).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.alternate_scroll"),
                    pick: |settings_content| {
                        settings_content
                            .terminal
                            .as_ref()?
                            .alternate_scroll
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .alternate_scroll = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.minimum.contrast",
                    "Minimum Contrast",
                ),
                description: lt(
                    "settings_ui.page_data.description.the.minimum.apca.perceptual.contrast.between.foreground.and.background.colors.0.106",
                    "The minimum APCA perceptual contrast between foreground and background colors (0-106).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.minimum_contrast"),
                    pick: |settings_content| {
                        settings_content
                            .terminal
                            .as_ref()?
                            .minimum_contrast
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .minimum_contrast = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn behavior_settings_section() -> [SettingsPageItem; 6] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.behavior.settings",
                "Behavior Settings",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.option.as.meta",
                    "Option As Meta",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.the.option.key.behaves.as.the.meta.key",
                    "Whether the option key behaves as the meta key.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.option_as_meta"),
                    pick: |settings_content| {
                        settings_content.terminal.as_ref()?.option_as_meta.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .option_as_meta = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.open.links.in.mouse.mode",
                    "Open Links In Mouse Mode",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.cmd.click.ctrl.click.opens.hyperlinks.when.the.terminal.application.has.enabled.mouse.reporting",
                    "Whether cmd-click (ctrl-click on Linux and Windows) opens hyperlinks even when the terminal application has enabled mouse reporting. When disabled, these clicks are forwarded to the application; links can still be opened with shift-cmd-click.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.open_links_in_mouse_mode"),
                    pick: |settings_content| {
                        settings_content
                            .terminal
                            .as_ref()?
                            .open_links_in_mouse_mode
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .open_links_in_mouse_mode = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.copy.on.select",
                    "Copy On Select",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.selecting.text.in.the.terminal.automatically.copies.to.the.system.clipboard",
                    "Whether selecting text in the terminal automatically copies to the system clipboard.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.copy_on_select"),
                    pick: |settings_content| {
                        settings_content.terminal.as_ref()?.copy_on_select.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .copy_on_select = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.keep.selection.on.copy",
                    "Keep Selection On Copy",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.keep.the.text.selection.after.copying.it.to.the.clipboard",
                    "Whether to keep the text selection after copying it to the clipboard.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.keep_selection_on_copy"),
                    pick: |settings_content| {
                        settings_content
                            .terminal
                            .as_ref()?
                            .keep_selection_on_copy
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .keep_selection_on_copy = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.audible.bell", "Audible Bell"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.play.a.sound.when.the.bel.character.a.0x07.is.printed",
                    "Whether to play a sound when the BEL character (`\\\\a`, `0x07`) is printed",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.bell"),
                    pick: |settings_content| settings_content.terminal.as_ref()?.bell.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.terminal.get_or_insert_default().bell = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn layout_settings_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.layout.settings",
                "Layout Settings",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.default.width", "Default Width"),
                description: lt(
                    "settings_ui.page_data.description.default.width.when.the.terminal.is.docked.to.the.left.or.right.in.pixels",
                    "Default width when the terminal is docked to the left or right (in pixels).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.default_width"),
                    pick: |settings_content| {
                        settings_content.terminal.as_ref()?.default_width.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .default_width = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.default.height",
                    "Default Height",
                ),
                description: lt(
                    "settings_ui.page_data.description.default.height.when.the.terminal.is.docked.to.the.bottom.in.pixels",
                    "Default height when the terminal is docked to the bottom (in pixels).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.default_height"),
                    pick: |settings_content| {
                        settings_content.terminal.as_ref()?.default_height.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .default_height = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn advanced_settings_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.advanced.settings",
                "Advanced Settings",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.max.scroll.history.lines",
                    "Max Scroll History Lines",
                ),
                description: lt(
                    "settings_ui.page_data.description.maximum.number.of.lines.to.keep.in.scrollback.history.max.100.000.0.disables.scrolling",
                    "Maximum number of lines to keep in scrollback history (max: 100,000; 0 disables scrolling).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.max_scroll_history_lines"),
                    pick: |settings_content| {
                        settings_content
                            .terminal
                            .as_ref()?
                            .max_scroll_history_lines
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .max_scroll_history_lines = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.scroll.multiplier",
                    "Scroll Multiplier",
                ),
                description: lt(
                    "settings_ui.page_data.description.the.multiplier.for.scrolling.in.the.terminal.with.the.mouse.wheel",
                    "The multiplier for scrolling in the terminal with the mouse wheel",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.scroll_multiplier"),
                    pick: |settings_content| {
                        settings_content
                            .terminal
                            .as_ref()?
                            .scroll_multiplier
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .scroll_multiplier = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn toolbar_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.toolbar", "Toolbar")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.breadcrumbs", "Breadcrumbs"),
                description: lt(
                    "settings_ui.page_data.description.display.the.terminal.title.in.breadcrumbs.inside.the.terminal.pane",
                    "Display the terminal title in breadcrumbs inside the terminal pane.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.toolbar.breadcrumbs"),
                    pick: |settings_content| {
                        settings_content
                            .terminal
                            .as_ref()?
                            .toolbar
                            .as_ref()?
                            .breadcrumbs
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .toolbar
                            .get_or_insert_default()
                            .breadcrumbs = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn scrollbar_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.scrollbar",
                "Scrollbar",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.scrollbar",
                    "Show Scrollbar",
                ),
                description: lt(
                    "settings_ui.page_data.description.when.to.show.the.scrollbar.in.the.terminal",
                    "When to show the scrollbar in the terminal.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("terminal.scrollbar.show"),
                    pick: |settings_content| {
                        show_scrollbar_or_editor(settings_content, |settings_content| {
                            settings_content
                                .terminal
                                .as_ref()?
                                .scrollbar
                                .as_ref()?
                                .show
                                .as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .terminal
                            .get_or_insert_default()
                            .scrollbar
                            .get_or_insert_default()
                            .show = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    SettingsPage {
        title: lt("settings_ui.page_data.title.terminal", "Terminal"),
        items: concat_sections![
            environment_section(),
            font_section(),
            display_settings_section(),
            behavior_settings_section(),
            layout_settings_section(),
            advanced_settings_section(),
            toolbar_section(),
            scrollbar_section(),
        ],
    }
}

fn version_control_page() -> SettingsPage {
    fn git_integration_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.git.integration",
                "Git Integration",
            )),
            SettingsPageItem::DynamicItem(DynamicItem {
                discriminant: SettingItem {
                    files: USER,
                    title: lt(
                        "settings_ui.page_data.title.disable.git.integration",
                        "Disable Git Integration",
                    ),
                    description: lt(
                        "settings_ui.page_data.description.disable.all.git.integration.features.in.zzz",
                        "Disable all Git integration features in ZZZ.",
                    ),
                    field: Box::new(SettingField::<bool> {
                        json_path: Some("git.disable_git"),
                        pick: |settings_content| {
                            settings_content
                                .git
                                .as_ref()?
                                .enabled
                                .as_ref()?
                                .disable_git
                                .as_ref()
                        },
                        write: |settings_content, value, _| {
                            settings_content
                                .git
                                .get_or_insert_default()
                                .enabled
                                .get_or_insert_default()
                                .disable_git = value;
                        },
                    }),
                    metadata: None,
                },
                pick_discriminant: |settings_content| {
                    let disabled = settings_content
                        .git
                        .as_ref()?
                        .enabled
                        .as_ref()?
                        .disable_git
                        .unwrap_or(false);
                    Some(if disabled { 0 } else { 1 })
                },
                fields: vec![
                    vec![],
                    vec![
                        SettingItem {
                            files: USER,
                            title: lt(
                                "settings_ui.page_data.title.enable.git.status",
                                "Enable Git Status",
                            ),
                            description: lt(
                                "settings_ui.page_data.description.show.git.status.information.in.the.editor",
                                "Show Git status information in the editor.",
                            ),
                            field: Box::new(SettingField::<bool> {
                                json_path: Some("git.enable_status"),
                                pick: |settings_content| {
                                    settings_content
                                        .git
                                        .as_ref()?
                                        .enabled
                                        .as_ref()?
                                        .enable_status
                                        .as_ref()
                                },
                                write: |settings_content, value, _| {
                                    settings_content
                                        .git
                                        .get_or_insert_default()
                                        .enabled
                                        .get_or_insert_default()
                                        .enable_status = value;
                                },
                            }),
                            metadata: None,
                        },
                        SettingItem {
                            files: USER,
                            title: lt(
                                "settings_ui.page_data.title.enable.git.diff",
                                "Enable Git Diff",
                            ),
                            description: lt(
                                "settings_ui.page_data.description.show.git.diff.information.in.the.editor",
                                "Show Git diff information in the editor.",
                            ),
                            field: Box::new(SettingField::<bool> {
                                json_path: Some("git.enable_diff"),
                                pick: |settings_content| {
                                    settings_content
                                        .git
                                        .as_ref()?
                                        .enabled
                                        .as_ref()?
                                        .enable_diff
                                        .as_ref()
                                },
                                write: |settings_content, value, _| {
                                    settings_content
                                        .git
                                        .get_or_insert_default()
                                        .enabled
                                        .get_or_insert_default()
                                        .enable_diff = value;
                                },
                            }),
                            metadata: None,
                        },
                    ],
                ],
            }),
        ]
    }

    fn git_gutter_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.git.gutter",
                "Git Gutter",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.visibility", "Visibility"),
                description: lt(
                    "settings_ui.page_data.description.control.whether.git.status.is.shown.in.the.editor.s.gutter",
                    "Control whether Git status is shown in the editor's gutter.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git.git_gutter"),
                    pick: |settings_content| settings_content.git.as_ref()?.git_gutter.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.git.get_or_insert_default().git_gutter = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            // todo(settings_ui): Figure out the right default for this value in default.json
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.debounce", "Debounce"),
                description: lt(
                    "settings_ui.page_data.description.debounce.threshold.in.milliseconds.after.which.changes.are.reflected.in.the.git.gutter",
                    "Debounce threshold in milliseconds after which changes are reflected in the Git gutter.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git.gutter_debounce"),
                    pick: |settings_content| {
                        settings_content.git.as_ref()?.gutter_debounce.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.git.get_or_insert_default().gutter_debounce = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn inline_git_blame_section() -> [SettingsPageItem; 6] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.inline.git.blame",
                "Inline Git Blame",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.enabled", "Enabled"),
                description: lt(
                    "settings_ui.page_data.description.whether.or.not.to.show.git.blame.data.inline.in.the.currently.focused.line",
                    "Whether or not to show Git blame data inline in the currently focused line.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git.inline_blame.enabled"),
                    pick: |settings_content| {
                        settings_content
                            .git
                            .as_ref()?
                            .inline_blame
                            .as_ref()?
                            .enabled
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git
                            .get_or_insert_default()
                            .inline_blame
                            .get_or_insert_default()
                            .enabled = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.delay", "Delay"),
                description: lt(
                    "settings_ui.page_data.description.the.delay.after.which.the.inline.blame.information.is.shown",
                    "The delay after which the inline blame information is shown.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git.inline_blame.delay_ms"),
                    pick: |settings_content| {
                        settings_content
                            .git
                            .as_ref()?
                            .inline_blame
                            .as_ref()?
                            .delay_ms
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git
                            .get_or_insert_default()
                            .inline_blame
                            .get_or_insert_default()
                            .delay_ms = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.padding", "Padding"),
                description: lt(
                    "settings_ui.page_data.description.padding.between.the.end.of.the.source.line.and.the.start.of.the.inline.blame.in.columns",
                    "Padding between the end of the source line and the start of the inline blame in columns.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git.inline_blame.padding"),
                    pick: |settings_content| {
                        settings_content
                            .git
                            .as_ref()?
                            .inline_blame
                            .as_ref()?
                            .padding
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git
                            .get_or_insert_default()
                            .inline_blame
                            .get_or_insert_default()
                            .padding = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.minimum.column",
                    "Minimum Column",
                ),
                description: lt(
                    "settings_ui.page_data.description.the.minimum.column.number.at.which.to.show.the.inline.blame.information",
                    "The minimum column number at which to show the inline blame information.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git.inline_blame.min_column"),
                    pick: |settings_content| {
                        settings_content
                            .git
                            .as_ref()?
                            .inline_blame
                            .as_ref()?
                            .min_column
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git
                            .get_or_insert_default()
                            .inline_blame
                            .get_or_insert_default()
                            .min_column = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.commit.summary",
                    "Show Commit Summary",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.commit.summary.as.part.of.the.inline.blame",
                    "Show commit summary as part of the inline blame.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git.inline_blame.show_commit_summary"),
                    pick: |settings_content| {
                        settings_content
                            .git
                            .as_ref()?
                            .inline_blame
                            .as_ref()?
                            .show_commit_summary
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git
                            .get_or_insert_default()
                            .inline_blame
                            .get_or_insert_default()
                            .show_commit_summary = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn git_avatars_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.git.avatars",
                "Git Avatars",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.show.avatars", "Show Avatars"),
                description: lt(
                    "settings_ui.page_data.description.show.avatar.images.for.commit.authors.in.git.views",
                    "Show avatar images for commit authors in git views. Avatars are loaded from the hosting provider and disclose the author's email address to it.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git.show_avatar"),
                    pick: |settings_content| settings_content.git.as_ref()?.show_avatar.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.git.get_or_insert_default().show_avatar = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn git_blame_view_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.git.blame.view",
                "Git Blame View",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.show.avatar", "Show Avatar"),
                description: lt(
                    "settings_ui.page_data.description.show.the.avatar.of.the.author.of.the.commit",
                    "Show the avatar of the author of the commit.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git.blame.show_avatar"),
                    pick: |settings_content| {
                        settings_content
                            .git
                            .as_ref()?
                            .blame
                            .as_ref()?
                            .show_avatar
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git
                            .get_or_insert_default()
                            .blame
                            .get_or_insert_default()
                            .show_avatar = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn branch_picker_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.branch.picker",
                "Branch Picker",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.author.name",
                    "Show Author Name",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.author.name.as.part.of.the.commit.information.in.branch.picker",
                    "Show author name as part of the commit information in branch picker.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git.branch_picker.show_author_name"),
                    pick: |settings_content| {
                        settings_content
                            .git
                            .as_ref()?
                            .branch_picker
                            .as_ref()?
                            .show_author_name
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git
                            .get_or_insert_default()
                            .branch_picker
                            .get_or_insert_default()
                            .show_author_name = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn git_hunks_section() -> [SettingsPageItem; 4] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.git.hunks",
                "Git Hunks",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.hunk.style", "Hunk Style"),
                description: lt(
                    "settings_ui.page_data.description.how.git.hunks.are.displayed.visually.in.the.editor",
                    "How Git hunks are displayed visually in the editor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git.hunk_style"),
                    pick: |settings_content| settings_content.git.as_ref()?.hunk_style.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.git.get_or_insert_default().hunk_style = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.path.style", "Path Style"),
                description: lt(
                    "settings_ui.page_data.description.should.the.name.or.path.be.displayed.first.in.the.git.view",
                    "Should the name or path be displayed first in the git view.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git.path_style"),
                    pick: |settings_content| settings_content.git.as_ref()?.path_style.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.git.get_or_insert_default().path_style = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.stage.restore.buttons",
                    "Show Stage/Restore Buttons",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.show.the.stage.and.restore.buttons.on.diff.hunks",
                    "Whether to show the stage and restore buttons on diff hunks.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("git.show_stage_restore_buttons"),
                    pick: |settings_content| {
                        settings_content
                            .git
                            .as_ref()?
                            .show_stage_restore_buttons
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .git
                            .get_or_insert_default()
                            .show_stage_restore_buttons = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    SettingsPage {
        title: lt(
            "settings_ui.page_data.title.version.control",
            "Version Control",
        ),
        items: concat_sections![
            git_integration_section(),
            git_gutter_section(),
            inline_git_blame_section(),
            git_avatars_section(),
            git_blame_view_section(),
            branch_picker_section(),
            git_hunks_section(),
        ],
    }
}

fn ai_page(cx: &App) -> SettingsPage {
    fn general_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.general", "General")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.disable.ai", "Disable AI"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.disable.all.ai.features.in.zzz",
                    "Whether to disable all AI features in ZZZ.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("disable_ai"),
                    pick: |settings_content| settings_content.project.disable_ai.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.project.disable_ai = value;
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.threads.sidebar.side",
                    "Threads Sidebar Side",
                ),
                description: lt(
                    "settings_ui.page_data.description.which.side.of.the.window.the.threads.sidebar.appears.on",
                    "Which side of the window the threads sidebar appears on.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("agent.sidebar_side"),
                    pick: |settings_content| settings_content.agent.as_ref()?.sidebar_side.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.agent.get_or_insert_default().sidebar_side = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn agent_configuration_section(_cx: &App) -> Box<[SettingsPageItem]> {
        let mut items = vec![
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.agent.configuration",
                "Agent Configuration",
            )),
            SettingsPageItem::SubPageLink(SubPageLink {
                title: lt(
                    "settings_ui.page_data.title.configure.providers",
                    "Configure Providers",
                ),
                r#type: Default::default(),
                search_aliases: &[
                    "ai",
                    "anthropic",
                    "api key",
                    "llama",
                    "llm",
                    "lm studio",
                    "model provider",
                    "ollama",
                    "openai",
                    "provider",
                ],
                json_path: Some("language_models"),
                description: Some(lt(
                    "settings_ui.page_data.description.configure.local.third.party.language.model.providers.for.agent.workflows",
                    "Configure local and third-party language model providers for agent workflows.",
                )),
                in_json: false,
                files: USER,
                render: render_llm_providers_page,
            }),
            SettingsPageItem::SubPageLink(SubPageLink {
                title: lt(
                    "settings_ui.page_data.title.tool.permissions",
                    "Tool Permissions",
                ),
                r#type: Default::default(),
                search_aliases: &[
                    "allow",
                    "auto allow",
                    "auto deny",
                    "deny",
                    "permission",
                    "permissions",
                    "regex",
                    "tool permission",
                ],
                json_path: Some("agent.tool_permissions"),
                description: Some(lt(
                    "settings_ui.page_data.description.set.up.regex.patterns.to.auto.allow.auto.deny.or.always.request.confirmation.for.specific.tool.inputs",
                    "Set up regex patterns to auto-allow, auto-deny, or always request confirmation, for specific tool inputs.",
                )),
                in_json: true,
                files: USER,
                render: render_tool_permissions_setup_page,
            }),
            SettingsPageItem::SubPageLink(SubPageLink {
                title: lt("settings_ui.page_data.title.mcp_servers", "MCP Servers"),
                r#type: Default::default(),
                search_aliases: &[
                    "context server",
                    "context servers",
                    "mcp",
                    "mcp server",
                    "mcp servers",
                    "timeout",
                    "tool timeout",
                ],
                json_path: Some("context_server_timeout"),
                description: Some(lt(
                    "settings_ui.page_data.description.configure.mcp.server.settings.including.default.tool.timeout",
                    "Configure MCP server settings, including the default tool timeout.",
                )),
                in_json: true,
                files: USER | PROJECT,
                render: render_mcp_servers_page,
            }),
        ];

        items.extend([
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.single.file.review", "Single File Review"),
                description: lt("settings_ui.page_data.description.when.enabled.agent.edits.will.also.be.displayed.in.single.file.buffers.for.review", "When enabled, agent edits will also be displayed in single-file buffers for review."),
                field: Box::new(SettingField {
                    json_path: Some("agent.single_file_review"),
                    pick: |settings_content| {
                        settings_content.agent.as_ref()?.single_file_review.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .agent
                            .get_or_insert_default()
                            .single_file_review = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.notify.when.agent.waiting", "Notify When Agent Waiting"),
                description: lt("settings_ui.page_data.description.where.to.show.notifications.when.the.agent.has.completed.its.response.or.needs.confirmation.before.running.a.tool.action", "Where to show notifications when the agent has completed its response or needs confirmation before running a tool action."),
                field: Box::new(SettingField {
                    json_path: Some("agent.notify_when_agent_waiting"),
                    pick: |settings_content| {
                        settings_content
                            .agent
                            .as_ref()?
                            .notify_when_agent_waiting
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .agent
                            .get_or_insert_default()
                            .notify_when_agent_waiting = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.play.sound.when.agent.done", "Play Sound When Agent Done"),
                description: lt("settings_ui.page_data.description.when.to.play.a.sound.when.the.agent.has.either.completed.its.response.or.needs.user.input", "When to play a sound when the agent has either completed its response, or needs user input."),
                field: Box::new(SettingField {
                    json_path: Some("agent.play_sound_when_agent_done"),
                    pick: |settings_content| {
                        settings_content
                            .agent
                            .as_ref()?
                            .play_sound_when_agent_done
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .agent
                            .get_or_insert_default()
                            .play_sound_when_agent_done = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.expand.edit.card", "Expand Edit Card"),
                description: lt("settings_ui.page_data.description.whether.to.have.edit.cards.in.the.agent.panel.expanded.showing.a.preview.of.the.diff", "Whether to have edit cards in the agent panel expanded, showing a Preview of the diff."),
                field: Box::new(SettingField {
                    json_path: Some("agent.expand_edit_card"),
                    pick: |settings_content| {
                        settings_content.agent.as_ref()?.expand_edit_card.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .agent
                            .get_or_insert_default()
                            .expand_edit_card = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.expand.terminal.card", "Expand Terminal Card"),
                description: lt("settings_ui.page_data.description.whether.to.have.terminal.cards.in.the.agent.panel.expanded.showing.the.whole.command.output", "Whether to have terminal cards in the agent panel expanded, showing the whole command output."),
                field: Box::new(SettingField {
                    json_path: Some("agent.expand_terminal_card"),
                    pick: |settings_content| {
                        settings_content
                            .agent
                            .as_ref()?
                            .expand_terminal_card
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .agent
                            .get_or_insert_default()
                            .expand_terminal_card = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.thinking.display", "Thinking Display"),
                description: lt("settings_ui.page_data.description.how.thinking.blocks.should.be.displayed.by.default.auto.fully.expands.during.streaming.then.auto.collapses.when.done.preview.auto.expands.with.a.height.constraint.during.streaming.always.expanded.shows.full.content.always.collapsed.keeps.them.collapsed", "How thinking blocks should be displayed by default. 'Auto' fully expands during streaming, then auto-collapses when done. 'Preview' auto-expands with a height constraint during streaming. 'Always Expanded' shows full content. 'Always Collapsed' keeps them collapsed."),
                field: Box::new(SettingField {
                    json_path: Some("agent.thinking_display"),
                    pick: |settings_content| {
                        settings_content
                            .agent
                            .as_ref()?
                            .thinking_display
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .agent
                            .get_or_insert_default()
                            .thinking_display = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.cancel.generation.on.terminal.stop", "Cancel Generation On Terminal Stop"),
                description: lt("settings_ui.page_data.description.whether.clicking.the.stop.button.on.a.running.terminal.tool.should.also.cancel.the.agent.s.generation.note.that.this.only.applies.to.the.stop.button.not.to.ctrl.c.inside.the.terminal", "Whether clicking the stop button on a running terminal tool should also cancel the agent's generation. Note that this only applies to the stop button, not to ctrl+c inside the terminal."),
                field: Box::new(SettingField {
                    json_path: Some("agent.cancel_generation_on_terminal_stop"),
                    pick: |settings_content| {
                        settings_content
                            .agent
                            .as_ref()?
                            .cancel_generation_on_terminal_stop
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .agent
                            .get_or_insert_default()
                            .cancel_generation_on_terminal_stop = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.use.modifier.to.send", "Use Modifier To Send"),
                description: lt("settings_ui.page_data.description.whether.to.always.use.cmd.enter.or.ctrl.enter.on.linux.or.windows.to.send.messages", "Whether to always use cmd-enter (or ctrl-enter on Linux or Windows) to send messages."),
                field: Box::new(SettingField {
                    json_path: Some("agent.use_modifier_to_send"),
                    pick: |settings_content| {
                        settings_content
                            .agent
                            .as_ref()?
                            .use_modifier_to_send
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .agent
                            .get_or_insert_default()
                            .use_modifier_to_send = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.message.editor.min.lines", "Message Editor Min Lines"),
                description: lt("settings_ui.page_data.description.minimum.number.of.lines.to.display.in.the.agent.message.editor", "Minimum number of lines to display in the agent message editor."),
                field: Box::new(SettingField {
                    json_path: Some("agent.message_editor_min_lines"),
                    pick: |settings_content| {
                        settings_content
                            .agent
                            .as_ref()?
                            .message_editor_min_lines
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .agent
                            .get_or_insert_default()
                            .message_editor_min_lines = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.show.turn.stats", "Show Turn Stats"),
                description: lt("settings_ui.page_data.description.whether.to.show.turn.statistics.like.elapsed.time.during.generation.and.final.turn.duration", "Whether to show turn statistics like elapsed time during generation and final turn duration."),
                field: Box::new(SettingField {
                    json_path: Some("agent.show_turn_stats"),
                    pick: |settings_content| {
                        settings_content.agent.as_ref()?.show_turn_stats.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .agent
                            .get_or_insert_default()
                            .show_turn_stats = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.show.merge.conflict.indicator", "Show Merge Conflict Indicator"),
                description: lt("settings_ui.page_data.description.whether.to.show.the.merge.conflict.indicator.in.the.status.bar.that.offers.to.resolve.conflicts.using.the.agent", "Whether to show the merge conflict indicator in the status bar that offers to resolve conflicts using the agent."),
                field: Box::new(SettingField {
                    json_path: Some("agent.show_merge_conflict_indicator"),
                    pick: |settings_content| {
                        settings_content.agent.as_ref()?.show_merge_conflict_indicator.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .agent
                            .get_or_insert_default()
                            .show_merge_conflict_indicator = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]);

        items.into_boxed_slice()
    }

    fn edit_prediction_display_sub_section() -> [SettingsPageItem; 1] {
        [SettingsPageItem::SettingItem(SettingItem {
            title: lt("settings_ui.page_data.title.display.mode", "Display Mode"),
            description: lt(
                "settings_ui.page_data.description.when.to.show.edit.predictions.previews.in.buffer.the.eager.mode.displays.them.inline.while.the.subtle.mode.displays.them.only.when.holding.a.modifier.key",
                "When to show edit predictions previews in buffer. The eager mode displays them inline, while the subtle mode displays them only when holding a modifier key.",
            ),
            field: Box::new(SettingField {
                json_path: Some("edit_prediction.display_mode"),
                pick: |settings_content| {
                    settings_content
                        .project
                        .all_languages
                        .edit_predictions
                        .as_ref()?
                        .mode
                        .as_ref()
                },
                write: |settings_content, value, _| {
                    settings_content
                        .project
                        .all_languages
                        .edit_predictions
                        .get_or_insert_default()
                        .mode = value;
                },
            }),
            metadata: None,
            files: USER,
        })]
    }

    SettingsPage {
        title: lt("settings_ui.page_data.title.ai", "AI"),
        items: concat_sections![
            general_section(),
            agent_configuration_section(cx),
            edit_prediction_language_settings_section(),
            edit_prediction_display_sub_section()
        ],
    }
}

fn network_page() -> SettingsPage {
    fn network_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.network", "Network")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.proxy", "Proxy"),
                description: lt(
                    "settings_ui.page_data.description.the.proxy.to.use.for.network.requests",
                    "The proxy to use for network requests.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("proxy"),
                    pick: |settings_content| settings_content.proxy.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.proxy = value;
                    },
                }),
                metadata: Some(Box::new(SettingsFieldMetadata {
                    placeholder: Some("socks5h://localhost:10808"),
                    ..Default::default()
                })),
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.server.url", "Server URL"),
                description: lt(
                    "settings_ui.page_data.description.the.url.of.the.zzz.server.to.connect.to",
                    "The URL of the ZZZ server to connect to.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("server_url"),
                    pick: |settings_content| settings_content.server_url.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.server_url = value;
                    },
                }),
                metadata: Some(Box::new(SettingsFieldMetadata {
                    placeholder: Some("http://127.0.0.1:7331"),
                    ..Default::default()
                })),
                files: USER,
            }),
        ]
    }

    SettingsPage {
        title: lt("settings_ui.page_data.title.network", "Network"),
        items: concat_sections![network_section()],
    }
}

fn language_settings_field<T>(
    settings_content: &SettingsContent,
    get_language_setting_field: fn(&LanguageSettingsContent) -> Option<&T>,
) -> Option<&T> {
    let all_languages = &settings_content.project.all_languages;

    active_language()
        .and_then(|current_language_name| {
            all_languages
                .languages
                .0
                .get(current_language_name.as_ref())
        })
        .and_then(get_language_setting_field)
        .or_else(|| get_language_setting_field(&all_languages.defaults))
}

fn language_settings_field_mut<T>(
    settings_content: &mut SettingsContent,
    value: Option<T>,
    write: fn(&mut LanguageSettingsContent, Option<T>),
) {
    let all_languages = &mut settings_content.project.all_languages;
    let language_content = if let Some(current_language) = active_language() {
        all_languages
            .languages
            .0
            .entry(current_language.to_string())
            .or_default()
    } else {
        &mut all_languages.defaults
    };
    write(language_content, value);
}

fn language_settings_data() -> Box<[SettingsPageItem]> {
    fn indentation_section() -> [SettingsPageItem; 5] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.indentation",
                "Indentation",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.tab.size", "Tab Size"),
                description: lt(
                    "settings_ui.page_data.description.how.many.columns.a.tab.should.occupy",
                    "How many columns a tab should occupy.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).tab_size"), // TODO(cameron): not JQ syntax because not URL-safe
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.tab_size.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.tab_size = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.hard.tabs", "Hard Tabs"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.indent.lines.using.tab.characters.as.opposed.to.multiple.spaces",
                    "Whether to indent lines using tab characters, as opposed to multiple spaces.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).hard_tabs"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.hard_tabs.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.hard_tabs = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.auto.indent", "Auto Indent"),
                description: lt(
                    "settings_ui.page_data.description.controls.automatic.indentation.behavior.when.typing",
                    "Controls automatic indentation behavior when typing.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).auto_indent"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.auto_indent.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.auto_indent = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.auto.indent.on.paste",
                    "Auto Indent On Paste",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.indentation.of.pasted.content.should.be.adjusted.based.on.the.context",
                    "Whether indentation of pasted content should be adjusted based on the context.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).auto_indent_on_paste"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.auto_indent_on_paste.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.auto_indent_on_paste = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    fn wrapping_section() -> [SettingsPageItem; 6] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.wrapping",
                "Wrapping",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.soft.wrap", "Soft Wrap"),
                description: lt(
                    "settings_ui.page_data.description.how.to.soft.wrap.long.lines.of.text",
                    "How to soft-wrap long lines of text.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).soft_wrap"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.soft_wrap.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.soft_wrap = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.wrap.guides",
                    "Show Wrap Guides",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.wrap.guides.in.the.editor",
                    "Show wrap guides in the editor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).show_wrap_guides"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.show_wrap_guides.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.show_wrap_guides = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.preferred.line.length",
                    "Preferred Line Length",
                ),
                description: lt(
                    "settings_ui.page_data.description.the.column.at.which.to.soft.wrap.lines.for.buffers.where.soft.wrap.is.enabled",
                    "The column at which to soft-wrap lines, for buffers where soft-wrap is enabled.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).preferred_line_length"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.preferred_line_length.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.preferred_line_length = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.wrap.guides", "Wrap Guides"),
                description: lt(
                    "settings_ui.page_data.description.character.counts.at.which.to.show.wrap.guides.in.the.editor",
                    "Character counts at which to show wrap guides in the editor.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("languages.$(language).wrap_guides"),
                        pick: |settings_content| {
                            language_settings_field(settings_content, |language| {
                                language.wrap_guides.as_ref()
                            })
                        },
                        write: |settings_content, value, _| {
                            language_settings_field_mut(
                                settings_content,
                                value,
                                |language, value| {
                                    language.wrap_guides = value;
                                },
                            )
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.allow.rewrap", "Allow Rewrap"),
                description: lt(
                    "settings_ui.page_data.description.controls.where.the.editor.rewrap.action.is.allowed.for.this.language",
                    "Controls where the `editor::rewrap` action is allowed for this language.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).allow_rewrap"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.allow_rewrap.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.allow_rewrap = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    fn indent_guides_section() -> [SettingsPageItem; 6] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.indent.guides",
                "Indent Guides",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.enabled", "Enabled"),
                description: lt(
                    "settings_ui.page_data.description.display.indent.guides.in.the.editor",
                    "Display indent guides in the editor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).indent_guides.enabled"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language
                                .indent_guides
                                .as_ref()
                                .and_then(|indent_guides| indent_guides.enabled.as_ref())
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.indent_guides.get_or_insert_default().enabled = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.line.width", "Line Width"),
                description: lt(
                    "settings_ui.page_data.description.the.width.of.the.indent.guides.in.pixels.between.1.and.10",
                    "The width of the indent guides in pixels, between 1 and 10.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).indent_guides.line_width"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language
                                .indent_guides
                                .as_ref()
                                .and_then(|indent_guides| indent_guides.line_width.as_ref())
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.indent_guides.get_or_insert_default().line_width = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.active.line.width",
                    "Active Line Width",
                ),
                description: lt(
                    "settings_ui.page_data.description.the.width.of.the.active.indent.guide.in.pixels.between.1.and.10",
                    "The width of the active indent guide in pixels, between 1 and 10.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).indent_guides.active_line_width"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language
                                .indent_guides
                                .as_ref()
                                .and_then(|indent_guides| indent_guides.active_line_width.as_ref())
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language
                                .indent_guides
                                .get_or_insert_default()
                                .active_line_width = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.coloring", "Coloring"),
                description: lt(
                    "settings_ui.page_data.description.determines.how.indent.guides.are.colored",
                    "Determines how indent guides are colored.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).indent_guides.coloring"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language
                                .indent_guides
                                .as_ref()
                                .and_then(|indent_guides| indent_guides.coloring.as_ref())
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.indent_guides.get_or_insert_default().coloring = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.background.coloring",
                    "Background Coloring",
                ),
                description: lt(
                    "settings_ui.page_data.description.determines.how.indent.guide.backgrounds.are.colored",
                    "Determines how indent guide backgrounds are colored.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).indent_guides.background_coloring"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.indent_guides.as_ref().and_then(|indent_guides| {
                                indent_guides.background_coloring.as_ref()
                            })
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language
                                .indent_guides
                                .get_or_insert_default()
                                .background_coloring = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    fn formatting_section() -> [SettingsPageItem; 8] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.formatting",
                "Formatting",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.format.on.save",
                    "Format On Save",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.or.not.to.perform.a.buffer.format.before.saving",
                    "Whether or not to perform a buffer format before saving.",
                ),
                field: Box::new(
                    // TODO(settings_ui): this setting should just be a bool
                    SettingField {
                        json_path: Some("languages.$(language).format_on_save"),
                        pick: |settings_content| {
                            language_settings_field(settings_content, |language| {
                                language.format_on_save.as_ref()
                            })
                        },
                        write: |settings_content, value, _| {
                            language_settings_field_mut(
                                settings_content,
                                value,
                                |language, value| {
                                    language.format_on_save = value;
                                },
                            )
                        },
                    },
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.remove.trailing.whitespace.on.save",
                    "Remove Trailing Whitespace On Save",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.or.not.to.remove.any.trailing.whitespace.from.lines.of.a.buffer.before.saving.it",
                    "Whether or not to remove any trailing whitespace from lines of a buffer before saving it.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).remove_trailing_whitespace_on_save"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.remove_trailing_whitespace_on_save.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.remove_trailing_whitespace_on_save = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.ensure.final.newline.on.save",
                    "Ensure Final Newline On Save",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.or.not.to.ensure.there.s.a.single.newline.at.the.end.of.a.buffer.when.saving.it",
                    "Whether or not to ensure there's a single newline at the end of a buffer when saving it.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).ensure_final_newline_on_save"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.ensure_final_newline_on_save.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.ensure_final_newline_on_save = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.line.ending", "Line Ending"),
                description: lt(
                    "settings_ui.page_data.description.how.line.endings.should.be.handled.for.new.files.and.during.format.and.save.operations",
                    "How line endings should be handled for new files and during format and save operations.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).line_ending"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.line_ending.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.line_ending = value;
                        })
                    },
                }),
                metadata: Some(Box::new(SettingsFieldMetadata {
                    should_do_titlecase: Some(false),
                    ..Default::default()
                })),
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.formatter", "Formatter"),
                description: lt(
                    "settings_ui.page_data.description.how.to.perform.a.buffer.format",
                    "How to perform a buffer format.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("languages.$(language).formatter"),
                        pick: |settings_content| {
                            language_settings_field(settings_content, |language| {
                                language.formatter.as_ref()
                            })
                        },
                        write: |settings_content, value, _| {
                            language_settings_field_mut(
                                settings_content,
                                value,
                                |language, value| {
                                    language.formatter = value;
                                },
                            )
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.use.on.type.format",
                    "Use On Type Format",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.use.additional.lsp.queries.to.format.and.amend.the.code.after.every.trigger.symbol.input.defined.by.lsp.server.capabilities",
                    "Whether to use additional LSP queries to format (and amend) the code after every \\\"trigger\\\" symbol input, defined by LSP server capabilities",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).use_on_type_format"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.use_on_type_format.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.use_on_type_format = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.code.actions.on.format",
                    "Code Actions On Format",
                ),
                description: lt(
                    "settings_ui.page_data.description.additional.code.actions.to.run.when.formatting",
                    "Additional code actions to run when formatting.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("languages.$(language).code_actions_on_format"),
                        pick: |settings_content| {
                            language_settings_field(settings_content, |language| {
                                language.code_actions_on_format.as_ref()
                            })
                        },
                        write: |settings_content, value, _| {
                            language_settings_field_mut(
                                settings_content,
                                value,
                                |language, value| {
                                    language.code_actions_on_format = value;
                                },
                            )
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    fn autoclose_section() -> [SettingsPageItem; 5] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.autoclose",
                "Autoclose",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.use.autoclose", "Use Autoclose"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.automatically.type.closing.characters.for.you.for.example.when.you.type.zzz.will.automatically.add.a.closing.at.the.correct.position",
                    "Whether to automatically type closing characters for you. For example, when you type '(', ZZZ will automatically add a closing ')' at the correct position.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).use_autoclose"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.use_autoclose.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.use_autoclose = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.use.auto.surround",
                    "Use Auto Surround",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.automatically.surround.text.with.characters.for.you.for.example.when.you.select.text.and.type.zzz.will.automatically.surround.text.with",
                    "Whether to automatically surround text with characters for you. For example, when you select text and type '(', ZZZ will automatically surround text with ().",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).use_auto_surround"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.use_auto_surround.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.use_auto_surround = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.always.treat.brackets.as.autoclosed",
                    "Always Treat Brackets As Autoclosed",
                ),
                description: lt(
                    "settings_ui.page_data.description.controls.whether.the.closing.characters.are.always.skipped.over.and.auto.removed.no.matter.how.they.were.inserted",
                    "Controls whether the closing characters are always skipped over and auto-removed no matter how they were inserted.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).always_treat_brackets_as_autoclosed"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.always_treat_brackets_as_autoclosed.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.always_treat_brackets_as_autoclosed = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.jsx.tag.auto.close",
                    "JSX Tag Auto Close",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.automatically.close.jsx.tags",
                    "Whether to automatically close JSX tags.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).jsx_tag_auto_close"),
                    // TODO(settings_ui): this setting should just be a bool
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.jsx_tag_auto_close.as_ref()?.enabled.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.jsx_tag_auto_close.get_or_insert_default().enabled = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    fn whitespace_section() -> [SettingsPageItem; 4] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.whitespace",
                "Whitespace",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.whitespaces",
                    "Show Whitespaces",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.show.tabs.and.spaces.in.the.editor",
                    "Whether to show tabs and spaces in the editor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).show_whitespaces"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.show_whitespaces.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.show_whitespaces = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.space.whitespace.indicator",
                    "Space Whitespace Indicator",
                ),
                description: lt(
                    "settings_ui.page_data.description.visible.character.used.to.render.space.characters.when.show.whitespaces.is.enabled.default",
                    "Visible character used to render space characters when show_whitespaces is enabled (default: \"•\")",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("languages.$(language).whitespace_map.space"),
                        pick: |settings_content| {
                            language_settings_field(settings_content, |language| {
                                language.whitespace_map.as_ref()?.space.as_ref()
                            })
                        },
                        write: |settings_content, value, _| {
                            language_settings_field_mut(
                                settings_content,
                                value,
                                |language, value| {
                                    language.whitespace_map.get_or_insert_default().space = value;
                                },
                            )
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.tab.whitespace.indicator",
                    "Tab Whitespace Indicator",
                ),
                description: lt(
                    "settings_ui.page_data.description.visible.character.used.to.render.tab.characters.when.show.whitespaces.is.enabled.default",
                    "Visible character used to render tab characters when show_whitespaces is enabled (default: \"→\")",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("languages.$(language).whitespace_map.tab"),
                        pick: |settings_content| {
                            language_settings_field(settings_content, |language| {
                                language.whitespace_map.as_ref()?.tab.as_ref()
                            })
                        },
                        write: |settings_content, value, _| {
                            language_settings_field_mut(
                                settings_content,
                                value,
                                |language, value| {
                                    language.whitespace_map.get_or_insert_default().tab = value;
                                },
                            )
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    fn completions_section() -> [SettingsPageItem; 8] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.completions",
                "Completions",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.completions.on.input",
                    "Show Completions On Input",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.pop.the.completions.menu.while.typing.in.an.editor.without.explicitly.requesting.it",
                    "Whether to pop the completions menu while typing in an editor without explicitly requesting it.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).show_completions_on_input"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.show_completions_on_input.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.show_completions_on_input = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.completion.documentation",
                    "Show Completion Documentation",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.display.inline.and.alongside.documentation.for.items.in.the.completions.menu",
                    "Whether to display inline and alongside documentation for items in the completions menu.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).show_completion_documentation"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.show_completion_documentation.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.show_completion_documentation = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.words", "Words"),
                description: lt(
                    "settings_ui.page_data.description.controls.how.words.are.completed",
                    "Controls how words are completed.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).completions.words"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.completions.as_ref()?.words.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.completions.get_or_insert_default().words = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.words.min.length",
                    "Words Min Length",
                ),
                description: lt(
                    "settings_ui.page_data.description.how.many.characters.has.to.be.in.the.completions.query.to.automatically.show.the.words.based.completions",
                    "How many characters has to be in the completions query to automatically show the words-based completions.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).completions.words_min_length"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.completions.as_ref()?.words_min_length.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language
                                .completions
                                .get_or_insert_default()
                                .words_min_length = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.completion.menu.scrollbar",
                    "Completion Menu Scrollbar",
                ),
                description: lt(
                    "settings_ui.page_data.description.when.to.show.the.scrollbar.in.the.completion.menu",
                    "When to show the scrollbar in the completion menu.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("editor.completion_menu_scrollbar"),
                    pick: |settings_content| {
                        settings_content.editor.completion_menu_scrollbar.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.completion_menu_scrollbar = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.completion.detail.alignment",
                    "Completion Detail Alignment",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.align.detail.text.in.code.completions.context.menus.left.or.right",
                    "Whether to align detail text in code completions context menus left or right.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("editor.completion_detail_alignment"),
                    pick: |settings_content| {
                        settings_content.editor.completion_detail_alignment.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.completion_detail_alignment = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.completion.menu.item.kind",
                    "Completion Menu Item Kind",
                ),
                description: lt(
                    "settings_ui.page_data.description.how.to.display.the.lsp.item.kind.function.method.variable.etc.of.each.entry.in.the.completions.menu",
                    "How to display the LSP item kind (function, method, variable, etc.) of each entry in the completions menu.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("editor.completion_menu_item_kind"),
                    pick: |settings_content| {
                        settings_content.editor.completion_menu_item_kind.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.completion_menu_item_kind = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    fn inlay_hints_section() -> [SettingsPageItem; 10] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.inlay.hints",
                "Inlay Hints",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.enabled", "Enabled"),
                description: lt(
                    "settings_ui.page_data.description.global.switch.to.toggle.hints.on.and.off",
                    "Global switch to toggle hints on and off.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).inlay_hints.enabled"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.inlay_hints.as_ref()?.enabled.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.inlay_hints.get_or_insert_default().enabled = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.value.hints",
                    "Show Value Hints",
                ),
                description: lt(
                    "settings_ui.page_data.description.global.switch.to.toggle.inline.values.on.and.off.when.debugging",
                    "Global switch to toggle inline values on and off when debugging.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).inlay_hints.show_value_hints"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.inlay_hints.as_ref()?.show_value_hints.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language
                                .inlay_hints
                                .get_or_insert_default()
                                .show_value_hints = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.type.hints",
                    "Show Type Hints",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.type.hints.should.be.shown",
                    "Whether type hints should be shown.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).inlay_hints.show_type_hints"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.inlay_hints.as_ref()?.show_type_hints.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.inlay_hints.get_or_insert_default().show_type_hints = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.parameter.hints",
                    "Show Parameter Hints",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.parameter.hints.should.be.shown",
                    "Whether parameter hints should be shown.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).inlay_hints.show_parameter_hints"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.inlay_hints.as_ref()?.show_parameter_hints.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language
                                .inlay_hints
                                .get_or_insert_default()
                                .show_parameter_hints = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.other.hints",
                    "Show Other Hints",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.other.hints.should.be.shown",
                    "Whether other hints should be shown.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).inlay_hints.show_other_hints"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.inlay_hints.as_ref()?.show_other_hints.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language
                                .inlay_hints
                                .get_or_insert_default()
                                .show_other_hints = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.show.background",
                    "Show Background",
                ),
                description: lt(
                    "settings_ui.page_data.description.show.a.background.for.inlay.hints",
                    "Show a background for inlay hints.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).inlay_hints.show_background"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.inlay_hints.as_ref()?.show_background.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.inlay_hints.get_or_insert_default().show_background = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.edit.debounce.ms",
                    "Edit Debounce Ms",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.or.not.to.debounce.inlay.hints.updates.after.buffer.edits.set.to.0.to.disable.debouncing",
                    "Whether or not to debounce inlay hints updates after buffer edits (set to 0 to disable debouncing).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).inlay_hints.edit_debounce_ms"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.inlay_hints.as_ref()?.edit_debounce_ms.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language
                                .inlay_hints
                                .get_or_insert_default()
                                .edit_debounce_ms = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.scroll.debounce.ms",
                    "Scroll Debounce Ms",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.or.not.to.debounce.inlay.hints.updates.after.buffer.scrolls.set.to.0.to.disable.debouncing",
                    "Whether or not to debounce inlay hints updates after buffer scrolls (set to 0 to disable debouncing).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).inlay_hints.scroll_debounce_ms"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.inlay_hints.as_ref()?.scroll_debounce_ms.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language
                                .inlay_hints
                                .get_or_insert_default()
                                .scroll_debounce_ms = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.toggle.on.modifiers.press",
                    "Toggle On Modifiers Press",
                ),
                description: lt(
                    "settings_ui.page_data.description.toggles.inlay.hints.hides.or.shows.when.the.user.presses.the.modifiers.specified",
                    "Toggles inlay hints (hides or shows) when the user presses the modifiers specified.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some(
                            "languages.$(language).inlay_hints.toggle_on_modifiers_press",
                        ),
                        pick: |settings_content| {
                            language_settings_field(settings_content, |language| {
                                language
                                    .inlay_hints
                                    .as_ref()?
                                    .toggle_on_modifiers_press
                                    .as_ref()
                            })
                        },
                        write: |settings_content, value, _| {
                            language_settings_field_mut(
                                settings_content,
                                value,
                                |language, value| {
                                    language
                                        .inlay_hints
                                        .get_or_insert_default()
                                        .toggle_on_modifiers_press = value;
                                },
                            )
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    fn tasks_section() -> [SettingsPageItem; 4] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.tasks", "Tasks")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.enabled", "Enabled"),
                description: lt(
                    "settings_ui.page_data.description.whether.tasks.are.enabled.for.this.language",
                    "Whether tasks are enabled for this language.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).tasks.enabled"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.tasks.as_ref()?.enabled.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.tasks.get_or_insert_default().enabled = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.variables", "Variables"),
                description: lt(
                    "settings_ui.page_data.description.extra.task.variables.to.set.for.a.particular.language",
                    "Extra task variables to set for a particular language.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("languages.$(language).tasks.variables"),
                        pick: |settings_content| {
                            language_settings_field(settings_content, |language| {
                                language.tasks.as_ref()?.variables.as_ref()
                            })
                        },
                        write: |settings_content, value, _| {
                            language_settings_field_mut(
                                settings_content,
                                value,
                                |language, value| {
                                    language.tasks.get_or_insert_default().variables = value;
                                },
                            )
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.prefer.lsp", "Prefer LSP"),
                description: lt(
                    "settings_ui.page_data.description.use.lsp.tasks.over.zzz.language.extension.tasks",
                    "Use LSP tasks over ZZZ language extension tasks.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).tasks.prefer_lsp"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.tasks.as_ref()?.prefer_lsp.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.tasks.get_or_insert_default().prefer_lsp = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    fn miscellaneous_section() -> [SettingsPageItem; 8] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.miscellaneous",
                "Miscellaneous",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.language.detection",
                    "Language Detection",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.enable.automatic.language.detection.in.unsaved.buffers",
                    "Whether to enable automatic language detection in unsaved buffers.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("language_detection"),
                    pick: |settings_content| settings_content.editor.language_detection.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.language_detection = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.word.diff.enabled",
                    "Word Diff Enabled",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.enable.word.diff.highlighting.in.the.editor.when.enabled.changed.words.within.modified.lines.are.highlighted.to.show.exactly.what.changed",
                    "Whether to enable word diff highlighting in the editor. When enabled, changed words within modified lines are highlighted to show exactly what changed.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).word_diff_enabled"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.word_diff_enabled.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.word_diff_enabled = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.debuggers", "Debuggers"),
                description: lt(
                    "settings_ui.page_data.description.preferred.debuggers.for.this.language",
                    "Preferred debuggers for this language.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("languages.$(language).debuggers"),
                        pick: |settings_content| {
                            language_settings_field(settings_content, |language| {
                                language.debuggers.as_ref()
                            })
                        },
                        write: |settings_content, value, _| {
                            language_settings_field_mut(
                                settings_content,
                                value,
                                |language, value| {
                                    language.debuggers = value;
                                },
                            )
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.middle.click.paste",
                    "Middle Click Paste",
                ),
                description: lt(
                    "settings_ui.page_data.description.enable.middle.click.paste.on.linux",
                    "Enable middle-click paste on Linux.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).editor.middle_click_paste"),
                    pick: |settings_content| settings_content.editor.middle_click_paste.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.editor.middle_click_paste = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.extend.comment.on.newline",
                    "Extend Comment On Newline",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.start.a.new.line.with.a.comment.when.a.previous.line.is.a.comment.as.well",
                    "Whether to start a new line with a comment when a previous line is a comment as well.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).extend_comment_on_newline"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.extend_comment_on_newline.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.extend_comment_on_newline = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.colorize.brackets",
                    "Colorize Brackets",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.colorize.brackets.in.the.editor",
                    "Whether to colorize brackets in the editor.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).colorize_brackets"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.colorize_brackets.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.colorize_brackets = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.vim.emacs.modeline.support",
                    "Vim/Emacs Modeline Support",
                ),
                description: lt(
                    "settings_ui.page_data.description.number.of.lines.to.search.for.modelines.set.to.0.to.disable",
                    "Number of lines to search for modelines (set to 0 to disable).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("modeline_lines"),
                    pick: |settings_content| settings_content.modeline_lines.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.modeline_lines = value;
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    fn global_only_miscellaneous_sub_section() -> [SettingsPageItem; 3] {
        [
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.image.viewer", "Image Viewer"),
                description: lt(
                    "settings_ui.page_data.description.the.unit.for.image.file.sizes",
                    "The unit for image file sizes.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("image_viewer.unit"),
                    pick: |settings_content| {
                        settings_content
                            .image_viewer
                            .as_ref()
                            .and_then(|image_viewer| image_viewer.unit.as_ref())
                    },
                    write: |settings_content, value, _| {
                        settings_content.image_viewer.get_or_insert_default().unit = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::DynamicItem(DynamicItem {
                discriminant: SettingItem {
                    files: USER,
                    title: lt(
                        "settings_ui.page_data.title.limit.markdown.preview.width",
                        "Limit Markdown Preview Width",
                    ),
                    description: lt(
                        "settings_ui.page_data.description.whether.to.constrain.the.markdown.preview.content.to.a.maximum.width.centering.it.when.the.pane.is.wider.for.optimal.readability",
                        "Whether to constrain the markdown preview content to a maximum width, centering it when the pane is wider, for optimal readability.",
                    ),
                    field: Box::new(SettingField::<bool> {
                        json_path: Some("markdown_preview.limit_content_width"),
                        pick: |settings_content| {
                            settings_content
                                .markdown_preview
                                .as_ref()?
                                .limit_content_width
                                .as_ref()
                        },
                        write: |settings_content, value, _| {
                            settings_content
                                .markdown_preview
                                .get_or_insert_default()
                                .limit_content_width = value;
                        },
                    }),
                    metadata: None,
                },
                pick_discriminant: |settings_content| {
                    let enabled = settings_content
                        .markdown_preview
                        .as_ref()?
                        .limit_content_width
                        .unwrap_or(true);
                    Some(if enabled { 1 } else { 0 })
                },
                fields: vec![
                    vec![],
                    vec![SettingItem {
                        files: USER,
                        title: lt(
                            "settings_ui.page_data.title.max.markdown.preview.width",
                            "Max Width",
                        ),
                        description: lt(
                            "settings_ui.page_data.description.maximum.markdown.preview.content.width.in.pixels.content.will.be.centered.when.the.pane.is.wider.than.this.value",
                            "Maximum content width in pixels. Content will be centered when the pane is wider than this value.",
                        ),
                        field: Box::new(SettingField {
                            json_path: Some("markdown_preview.max_width"),
                            pick: |settings_content| {
                                settings_content
                                    .markdown_preview
                                    .as_ref()?
                                    .max_width
                                    .as_ref()
                            },
                            write: |settings_content, value, _| {
                                settings_content
                                    .markdown_preview
                                    .get_or_insert_default()
                                    .max_width = value;
                            },
                        }),
                        metadata: None,
                    }],
                ],
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.drop.size.target",
                    "Drop Size Target",
                ),
                description: lt(
                    "settings_ui.page_data.description.relative.size.of.the.drop.target.in.the.editor.that.will.open.dropped.file.as.a.split.pane",
                    "Relative size of the drop target in the editor that will open dropped file as a split pane.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("drop_target_size"),
                    pick: |settings_content| settings_content.workspace.drop_target_size.as_ref(),
                    write: |settings_content, value, _| {
                        settings_content.workspace.drop_target_size = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
        ]
    }

    let is_global = active_language().is_none();

    let code_lens_item = [SettingsPageItem::SettingItem(SettingItem {
        title: lt("settings_ui.page_data.title.code.lens", "Code Lens"),
        description: lt(
            "settings_ui.page_data.description.whether.and.how.to.display.code.lenses.from.language.servers",
            "Whether and how to display code lenses from language servers.",
        ),
        field: Box::new(SettingField {
            json_path: Some("code_lens"),
            pick: |settings_content| settings_content.editor.code_lens.as_ref(),
            write: |settings_content, value, _| {
                settings_content.editor.code_lens = value;
            },
        }),
        metadata: None,
        files: USER,
    })];

    let lsp_document_colors_item = [SettingsPageItem::SettingItem(SettingItem {
        title: lt(
            "settings_ui.page_data.title.lsp.document.colors",
            "LSP Document Colors",
        ),
        description: lt(
            "settings_ui.page_data.description.how.to.render.lsp.color.previews.in.the.editor",
            "How to render LSP color previews in the editor.",
        ),
        field: Box::new(SettingField {
            json_path: Some("lsp_document_colors"),
            pick: |settings_content| settings_content.editor.lsp_document_colors.as_ref(),
            write: |settings_content, value, _| {
                settings_content.editor.lsp_document_colors = value;
            },
        }),
        metadata: None,
        files: USER,
    })];

    if is_global {
        concat_sections!(
            indentation_section(),
            wrapping_section(),
            indent_guides_section(),
            formatting_section(),
            autoclose_section(),
            whitespace_section(),
            completions_section(),
            inlay_hints_section(),
            code_lens_item,
            lsp_document_colors_item,
            tasks_section(),
            miscellaneous_section(),
            global_only_miscellaneous_sub_section(),
        )
    } else {
        concat_sections!(
            indentation_section(),
            wrapping_section(),
            indent_guides_section(),
            formatting_section(),
            autoclose_section(),
            whitespace_section(),
            completions_section(),
            inlay_hints_section(),
            code_lens_item,
            tasks_section(),
            miscellaneous_section(),
        )
    }
}

/// LanguageSettings items that should be included in the "Languages & Tools" page
/// not the "Editor" page
fn non_editor_language_settings_data() -> Box<[SettingsPageItem]> {
    fn lsp_section() -> [SettingsPageItem; 9] {
        [
            SettingsPageItem::SectionHeader(lt("settings_ui.page_data.section.lsp", "LSP")),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.enable.language.server",
                    "Enable Language Server",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.use.language.servers.to.provide.code.intelligence",
                    "Whether to use language servers to provide code intelligence.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).enable_language_server"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.enable_language_server.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.enable_language_server = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.language.servers",
                    "Language Servers",
                ),
                description: lt(
                    "settings_ui.page_data.description.the.list.of.language.servers.to.use.or.disable.for.this.language",
                    "The list of language servers to use (or disable) for this language.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("languages.$(language).language_servers"),
                        pick: |settings_content| {
                            language_settings_field(settings_content, |language| {
                                language.language_servers.as_ref()
                            })
                        },
                        write: |settings_content, value, _| {
                            language_settings_field_mut(
                                settings_content,
                                value,
                                |language, value| {
                                    language.language_servers = value;
                                },
                            )
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.linked.edits", "Linked Edits"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.perform.linked.edits.of.associated.ranges.if.the.ls.supports.it.for.example.when.editing.opening.html.tag.the.contents.of.the.closing.html.tag.will.be.edited.as.well",
                    "Whether to perform linked edits of associated ranges, if the LS supports it. For example, when editing opening <html> tag, the contents of the closing </html> tag will be edited as well.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).linked_edits"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.linked_edits.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.linked_edits = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.go.to.definition.fallback",
                    "Go To Definition Fallback",
                ),
                description: lt(
                    "settings_ui.page_data.description.whether.to.follow.up.empty.go.to.definition.responses.from.the.language.server",
                    "Whether to follow-up empty Go to definition responses from the language server.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("go_to_definition_fallback"),
                    pick: |settings_content| {
                        settings_content.editor.go_to_definition_fallback.as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.go_to_definition_fallback = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.go.to.definition.scroll.strategy",
                    "Go To Definition Scroll Strategy",
                ),
                description: lt(
                    "settings_ui.page_data.description.how.to.scroll.the.target.into.view.when.navigating.to.a.definition.or.reference",
                    "How to scroll the target into view when navigating to a definition or reference.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("go_to_definition_scroll_strategy"),
                    pick: |settings_content| {
                        settings_content
                            .editor
                            .go_to_definition_scroll_strategy
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content.editor.go_to_definition_scroll_strategy = value;
                    },
                }),
                metadata: None,
                files: USER,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.semantic.tokens",
                    "Semantic Tokens",
                ),
                description: {
                    static DESCRIPTION: OnceLock<&'static str> = OnceLock::new();
                    UiText::from(*DESCRIPTION.get_or_init(|| {
                        SemanticTokens::VARIANTS
                            .iter()
                            .filter_map(|v| {
                                v.get_documentation().map(|doc| format!("{v:?}: {doc}"))
                            })
                            .join("\n")
                            .leak()
                    }))
                },
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).semantic_tokens"),
                    pick: |settings_content| {
                        settings_content
                            .project
                            .all_languages
                            .defaults
                            .semantic_tokens
                            .as_ref()
                    },
                    write: |settings_content, value, _| {
                        settings_content
                            .project
                            .all_languages
                            .defaults
                            .semantic_tokens = value;
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.lsp.folding.ranges",
                    "LSP Folding Ranges",
                ),
                description: lt(
                    "settings_ui.page_data.description.when.enabled.use.folding.ranges.from.the.language.server.instead.of.indent.based.folding",
                    "When enabled, use folding ranges from the language server instead of indent-based folding.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).document_folding_ranges"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.document_folding_ranges.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.document_folding_ranges = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.lsp.document.symbols",
                    "LSP Document Symbols",
                ),
                description: lt(
                    "settings_ui.page_data.description.when.enabled.use.the.language.server.s.document.symbols.for.outlines.and.breadcrumbs.instead.of.tree.sitter",
                    "When enabled, use the language server's document symbols for outlines and breadcrumbs instead of tree-sitter.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).document_symbols"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.document_symbols.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.document_symbols = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    fn lsp_completions_section() -> [SettingsPageItem; 4] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.lsp.completions",
                "LSP Completions",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.enabled", "Enabled"),
                description: lt(
                    "settings_ui.page_data.description.whether.to.fetch.lsp.completions.or.not",
                    "Whether to fetch LSP completions or not.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).completions.lsp"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.completions.as_ref()?.lsp.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.completions.get_or_insert_default().lsp = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt(
                    "settings_ui.page_data.title.fetch.timeout.milliseconds",
                    "Fetch Timeout (milliseconds)",
                ),
                description: lt(
                    "settings_ui.page_data.description.when.fetching.lsp.completions.determines.how.long.to.wait.for.a.response.of.a.particular.server.set.to.0.to.wait.indefinitely",
                    "When fetching LSP completions, determines how long to wait for a response of a particular server (set to 0 to wait indefinitely).",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).completions.lsp_fetch_timeout_ms"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.completions.as_ref()?.lsp_fetch_timeout_ms.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language
                                .completions
                                .get_or_insert_default()
                                .lsp_fetch_timeout_ms = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.insert.mode", "Insert Mode"),
                description: lt(
                    "settings_ui.page_data.description.controls.how.lsp.completions.are.inserted",
                    "Controls how LSP completions are inserted.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).completions.lsp_insert_mode"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.completions.as_ref()?.lsp_insert_mode.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.completions.get_or_insert_default().lsp_insert_mode = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    fn debugger_section() -> [SettingsPageItem; 2] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.debuggers",
                "Debuggers",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.debuggers", "Debuggers"),
                description: lt(
                    "settings_ui.page_data.description.preferred.debuggers.for.this.language",
                    "Preferred debuggers for this language.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("languages.$(language).debuggers"),
                        pick: |settings_content| {
                            language_settings_field(settings_content, |language| {
                                language.debuggers.as_ref()
                            })
                        },
                        write: |settings_content, value, _| {
                            language_settings_field_mut(
                                settings_content,
                                value,
                                |language, value| {
                                    language.debuggers = value;
                                },
                            )
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    fn prettier_section() -> [SettingsPageItem; 5] {
        [
            SettingsPageItem::SectionHeader(lt(
                "settings_ui.page_data.section.prettier",
                "Prettier",
            )),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.allowed", "Allowed"),
                description: lt(
                    "settings_ui.page_data.description.enables.or.disables.formatting.with.prettier.for.a.given.language",
                    "Enables or disables formatting with Prettier for a given language.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).prettier.allowed"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.prettier.as_ref()?.allowed.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.prettier.get_or_insert_default().allowed = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.parser", "Parser"),
                description: lt(
                    "settings_ui.page_data.description.forces.prettier.integration.to.use.a.specific.parser.name.when.formatting.files.with.the.language",
                    "Forces Prettier integration to use a specific parser name when formatting files with the language.",
                ),
                field: Box::new(SettingField {
                    json_path: Some("languages.$(language).prettier.parser"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.prettier.as_ref()?.parser.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.prettier.get_or_insert_default().parser = value;
                        })
                    },
                }),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.plugins", "Plugins"),
                description: lt(
                    "settings_ui.page_data.description.forces.prettier.integration.to.use.specific.plugins.when.formatting.files.with.the.language",
                    "Forces Prettier integration to use specific plugins when formatting files with the language.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("languages.$(language).prettier.plugins"),
                        pick: |settings_content| {
                            language_settings_field(settings_content, |language| {
                                language.prettier.as_ref()?.plugins.as_ref()
                            })
                        },
                        write: |settings_content, value, _| {
                            language_settings_field_mut(
                                settings_content,
                                value,
                                |language, value| {
                                    language.prettier.get_or_insert_default().plugins = value;
                                },
                            )
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
            SettingsPageItem::SettingItem(SettingItem {
                title: lt("settings_ui.page_data.title.options", "Options"),
                description: lt(
                    "settings_ui.page_data.description.default.prettier.options.in.the.format.as.in.package.json.section.for.prettier",
                    "Default Prettier options, in the format as in package.json section for Prettier.",
                ),
                field: Box::new(
                    SettingField {
                        json_path: Some("languages.$(language).prettier.options"),
                        pick: |settings_content| {
                            language_settings_field(settings_content, |language| {
                                language.prettier.as_ref()?.options.as_ref()
                            })
                        },
                        write: |settings_content, value, _| {
                            language_settings_field_mut(
                                settings_content,
                                value,
                                |language, value| {
                                    language.prettier.get_or_insert_default().options = value;
                                },
                            )
                        },
                    }
                    .unimplemented(),
                ),
                metadata: None,
                files: USER | PROJECT,
            }),
        ]
    }

    concat_sections!(
        lsp_section(),
        lsp_completions_section(),
        debugger_section(),
        prettier_section(),
    )
}

fn edit_prediction_language_settings_section() -> [SettingsPageItem; 4] {
    [
        SettingsPageItem::SectionHeader(lt(
            "settings_ui.page_data.section.edit.predictions",
            "Edit Predictions",
        )),
        SettingsPageItem::SubPageLink(SubPageLink {
            title: lt(
                "settings_ui.page_data.title.configure.providers",
                "Configure Providers",
            ),
            r#type: Default::default(),
            search_aliases: &[],
            json_path: Some("edit_predictions.providers"),
            description: Some(lt(
                "settings_ui.page_data.description.configure.edit.prediction.providers",
                "Set up local or optional remote edit prediction providers. Prefer Ollama or llama.cpp.",
            )),
            in_json: false,
            files: USER,
            render: render_edit_prediction_setup_page,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: lt(
                "settings_ui.page_data.title.show.edit.predictions",
                "Show Edit Predictions",
            ),
            description: lt(
                "settings_ui.page_data.description.controls.whether.edit.predictions.are.shown.immediately.or.manually",
                "Controls whether edit predictions are shown immediately or manually.",
            ),
            field: Box::new(SettingField {
                json_path: Some("languages.$(language).show_edit_predictions"),
                pick: |settings_content| {
                    language_settings_field(settings_content, |language| {
                        language.show_edit_predictions.as_ref()
                    })
                },
                write: |settings_content, value, _| {
                    language_settings_field_mut(settings_content, value, |language, value| {
                        language.show_edit_predictions = value;
                    })
                },
            }),
            metadata: None,
            files: USER | PROJECT,
        }),
        SettingsPageItem::SettingItem(SettingItem {
            title: lt(
                "settings_ui.page_data.title.disable.in.language.scopes",
                "Disable in Language Scopes",
            ),
            description: lt(
                "settings_ui.page_data.description.controls.whether.edit.predictions.are.shown.in.the.given.language.scopes",
                "Disable edit predictions in these language scopes, such as \"comment\" and \"string\". Use \"...\" to add scopes without repeating the inherited list.",
            ),
            field: Box::new(
                SettingField {
                    json_path: Some("languages.$(language).edit_predictions_disabled_in"),
                    pick: |settings_content| {
                        language_settings_field(settings_content, |language| {
                            language.edit_predictions_disabled_in.as_ref()
                        })
                    },
                    write: |settings_content, value, _| {
                        language_settings_field_mut(settings_content, value, |language, value| {
                            language.edit_predictions_disabled_in = value;
                        })
                    },
                }
                .unimplemented(),
            ),
            metadata: None,
            files: USER | PROJECT,
        }),
    ]
}

fn show_scrollbar_or_editor(
    settings_content: &SettingsContent,
    show: fn(&SettingsContent) -> Option<&settings::ShowScrollbar>,
) -> Option<&settings::ShowScrollbar> {
    show(settings_content).or(settings_content
        .editor
        .scrollbar
        .as_ref()
        .and_then(|scrollbar| scrollbar.show.as_ref()))
}

fn dynamic_variants<T>() -> &'static [T::Discriminant]
where
    T: strum::IntoDiscriminant,
    T::Discriminant: strum::VariantArray,
{
    <<T as strum::IntoDiscriminant>::Discriminant as strum::VariantArray>::VARIANTS
}

/// Updates the `vim_mode` setting, disabling `helix_mode` if present and
/// `vim_mode` is being enabled.
fn write_vim_mode(settings: &mut SettingsContent, value: Option<bool>, _: &App) {
    write_vim_mode_inner(settings, value);
}

fn write_vim_mode_inner(settings: &mut SettingsContent, value: Option<bool>) {
    if value == Some(true) && settings.helix_mode == Some(true) {
        settings.helix_mode = Some(false);
    }
    settings.vim_mode = value;
}

/// Updates the `helix_mode` setting, disabling `vim_mode` if present and
/// `helix_mode` is being enabled.
fn write_helix_mode(settings: &mut SettingsContent, value: Option<bool>, _: &App) {
    write_helix_mode_inner(settings, value);
}

fn write_helix_mode_inner(settings: &mut SettingsContent, value: Option<bool>) {
    if value == Some(true) && settings.vim_mode == Some(true) {
        settings.vim_mode = Some(false);
    }
    settings.helix_mode = value;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_vim_helix_mode() {
        // Enabling vim mode while `vim_mode` and `helix_mode` are not yet set
        // should only update the `vim_mode` setting.
        let mut settings = SettingsContent::default();
        write_vim_mode_inner(&mut settings, Some(true));
        assert_eq!(settings.vim_mode, Some(true));
        assert_eq!(settings.helix_mode, None);

        // Enabling helix mode while `vim_mode` and `helix_mode` are not yet set
        // should only update the `helix_mode` setting.
        let mut settings = SettingsContent::default();
        write_helix_mode_inner(&mut settings, Some(true));
        assert_eq!(settings.helix_mode, Some(true));
        assert_eq!(settings.vim_mode, None);

        // Disabling helix mode should only touch `helix_mode` setting when
        // `vim_mode` is not set.
        write_helix_mode_inner(&mut settings, Some(false));
        assert_eq!(settings.helix_mode, Some(false));
        assert_eq!(settings.vim_mode, None);

        // Enabling vim mode should update `vim_mode` but leave `helix_mode`
        // untouched.
        write_vim_mode_inner(&mut settings, Some(true));
        assert_eq!(settings.vim_mode, Some(true));
        assert_eq!(settings.helix_mode, Some(false));

        // Enabling helix mode should update `helix_mode` and disable
        // `vim_mode`.
        write_helix_mode_inner(&mut settings, Some(true));
        assert_eq!(settings.helix_mode, Some(true));
        assert_eq!(settings.vim_mode, Some(false));

        // Enabling vim mode should update `vim_mode` and disable
        // `helix_mode`.
        write_vim_mode_inner(&mut settings, Some(true));
        assert_eq!(settings.vim_mode, Some(true));
        assert_eq!(settings.helix_mode, Some(false));
    }

    #[gpui::test]
    async fn ai_page_includes_mcp_servers_subpage_and_omits_context_servers_section(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let ai_page = ai_page(cx);

            let has_mcp_servers_subpage = ai_page.items.iter().any(|item| {
                matches!(
                    item,
                    SettingsPageItem::SubPageLink(SubPageLink { title, .. })
                        if title.fallback() == "MCP Servers"
                )
            });
            let has_context_servers_section = ai_page.items.iter().any(|item| {
                matches!(
                    item,
                    SettingsPageItem::SectionHeader(title) if title.fallback() == "Context Servers"
                )
            });

            assert!(has_mcp_servers_subpage);
            assert!(!has_context_servers_section);
        });
    }
}
