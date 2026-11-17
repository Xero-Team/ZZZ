use gpui::{Action as _, App};
use itertools::Itertools as _;
use settings::{LanguageSettingsContent, SemanticTokens, SettingsContent};
use std::sync::{Arc, OnceLock};
use strum::{EnumMessage, IntoDiscriminant as _, VariantArray};
use theme::SystemAppearance;
use ui::IntoElement;
use util::ResultExt as _;

use crate::{
    ActionLink, DynamicItem, PROJECT, SettingField, SettingItem, SettingsFieldMetadata,
    SettingsPage, SettingsPageItem, SubPageLink, USER, UiText, active_language, all_language_names,
    pages::{
        render_database_connections_page, render_mcp_servers_page,
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

/// Declares one settings row: a localized title and description, the JSON path
/// the row writes to, and the reader/writer pair for the underlying content.
///
/// `files` defaults to [`USER`] because most rows are user settings; pass
/// `metadata:` or `files:` only for the rows that differ from that shape.
macro_rules! setting_item {
    (
        $title:expr,
        $description:expr,
        $json_path:literal,
        $pick:expr,
        $write:expr
        $(, metadata: $metadata:expr)?
        $(, files: $files:expr)?
        $(,)?
    ) => {
        SettingsPageItem::SettingItem(SettingItem {
            title: $title,
            description: $description,
            field: Box::new(SettingField {
                json_path: Some($json_path),
                pick: $pick,
                write: $write,
            }),
            metadata: setting_item!(@metadata $($metadata)?),
            files: setting_item!(@files $($files)?),
        })
    };
    (@metadata $metadata:expr) => {
        $metadata
    };
    (@metadata) => {
        None
    };
    (@files $files:expr) => {
        $files
    };
    (@files) => {
        USER
    };
}

/// Declares a section divider inside a settings page.
macro_rules! section_header {
    ($key:literal, $fallback:literal $(,)?) => {
        SettingsPageItem::SectionHeader(lt($key, $fallback))
    };
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
        collaboration_page(),
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
            section_header!(
                "settings_ui.page_data.section.feature.flags",
                "Feature Flags"
            ),
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
            section_header!(
                "settings_ui.page_data.section.instrumentation",
                "Instrumentation"
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.performance.profiler",
                    "Performance Profiler"
                ),
                lt(
                    "settings_ui.page_data.description.collect.timing.data.for.foreground.and.background.executor.tasks.so.they.can.be.inspected.via.zzz.open.performance.profiler.may.lead.to.increased.memory.usage",
                    "Collect timing data for foreground and background executor tasks so they can be inspected via `zzz: open performance profiler`. May lead to increased memory usage."
                ),
                "instrumentation.performance_profiler.enabled",
                |settings_content| {
                    settings_content
                        .instrumentation
                        .as_ref()
                        .and_then(|i| i.performance_profiler.as_ref())
                        .and_then(|p| p.enabled.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .instrumentation
                        .get_or_insert_default()
                        .performance_profiler
                        .get_or_insert_default()
                        .enabled = value;
                }
            ),
        ]),
    }
}

fn general_page(cx: &App) -> SettingsPage {
    fn general_settings_section(_cx: &App) -> Vec<SettingsPageItem> {
        vec![
            section_header!(
                "settings_ui.page_data.section.general.settings",
                "General Settings"
            ),
            setting_item!(
            lt("settings_ui.page_data.title.display.language", "Display Language"),
            lt("settings_ui.page_data.description.choose.the.language.used.for.the.zzz.user.interface", "Choose the language used for the ZZZ user interface."),
            "display_language",
            |settings_content| settings_content.workspace.display_language.as_ref(),
            |settings_content, value, _| { settings_content.workspace.display_language = value; },
            metadata: Some(Box::new(SettingsFieldMetadata {
                should_do_titlecase: Some(false),
                ..Default::default()
            }))),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.when.closing.with.no.tabs",
                    "When Closing With No Tabs"
                ),
                lt(
                    "settings_ui.page_data.description.what.to.do.when.using.the.close.active.item.action.with.no.tabs",
                    "What to do when using the 'close active item' action with no tabs."
                ),
                "when_closing_with_no_tabs",
                |settings_content| {
                    settings_content
                        .workspace
                        .when_closing_with_no_tabs
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content.workspace.when_closing_with_no_tabs = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.on.new.window", "On New Window"),
                lt(
                    "settings_ui.page_data.description.what.to.show.when.opening.a.new.window",
                    "What to show when opening a new window; Readme opens a project's README when no items are open, leaving the pane empty if none is found."
                ),
                "on_new_window",
                |settings_content| settings_content.workspace.on_new_window.as_ref(),
                |settings_content, value, _| {
                    settings_content.workspace.on_new_window = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.on.last.window.closed",
                    "On Last Window Closed"
                ),
                lt(
                    "settings_ui.page_data.description.what.to.do.when.the.last.window.is.closed",
                    "What to do when the last window is closed."
                ),
                "on_last_window_closed",
                |settings_content| { settings_content.workspace.on_last_window_closed.as_ref() },
                |settings_content, value, _| {
                    settings_content.workspace.on_last_window_closed = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.use.system.path.prompts",
                    "Use System Path Prompts"
                ),
                lt(
                    "settings_ui.page_data.description.use.native.os.dialogs.for.open.and.save.as",
                    "Use native OS dialogs for 'Open' and 'Save As'."
                ),
                "use_system_path_prompts",
                |settings_content| { settings_content.workspace.use_system_path_prompts.as_ref() },
                |settings_content, value, _| {
                    settings_content.workspace.use_system_path_prompts = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.use.system.prompts",
                    "Use System Prompts"
                ),
                lt(
                    "settings_ui.page_data.description.use.native.os.dialogs.for.confirmations",
                    "Use native OS dialogs for confirmations."
                ),
                "use_system_prompts",
                |settings_content| settings_content.workspace.use_system_prompts.as_ref(),
                |settings_content, value, _| {
                    settings_content.workspace.use_system_prompts = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.redact.private.values",
                    "Redact Private Values"
                ),
                lt(
                    "settings_ui.page_data.description.hide.the.values.of.variables.in.private.files",
                    "Hide the values of variables in private files."
                ),
                "redact_private_values",
                |settings_content| settings_content.editor.redact_private_values.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.redact_private_values = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.private.files", "Private Files"),
                lt(
                    "settings_ui.page_data.description.globs.to.match.against.file.paths.to.determine.if.a.file.is.private",
                    "Globs to match against file paths to determine if a file is private."
                ),
                "worktree.private_files",
                |settings_content| { settings_content.project.worktree.private_files.as_ref() },
                |settings_content, value, _| {
                    settings_content.project.worktree.private_files = value;
                }
            ),
            setting_item!(
            lt("settings_ui.page_data.title.cli.default.open.behavior", "CLI Default Open Behavior"),
            lt("settings_ui.page_data.description.how.zzz.path.opens.directories.when.no.flag.is.specified", "How `zzz <path>` opens directories when no flag is specified."),
            "cli_default_open_behavior",
            |settings_content| { settings_content.workspace.cli_default_open_behavior.as_ref() },
            |settings_content, value, _| { settings_content.workspace.cli_default_open_behavior = value; },
            metadata: Some(Box::new(SettingsFieldMetadata {
                should_do_titlecase: Some(false),
                ..Default::default()
            }))),
            setting_item!(
            lt("settings_ui.page_data.title.default.open.behavior", "Default Open Behavior"),
            lt("settings_ui.page_data.description.how.projects.open.from.the.ui.by.default", "How projects open from the UI by default."),
            "default_open_behavior",
            |settings_content| { settings_content.workspace.default_open_behavior.as_ref() },
            |settings_content, value, _| { settings_content.workspace.default_open_behavior = value; },
            metadata: Some(Box::new(SettingsFieldMetadata {
                should_do_titlecase: Some(false),
                ..Default::default()
            }))),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.reveal.if.open",
                    "Reveal If Open"
                ),
                lt(
                    "settings_ui.page_data.description.when.enabled.zzz.prefers.an.already.open.file.in.another.pane",
                    "When enabled, ZZZ prefers an already-open file in another pane."
                ),
                "reveal_if_open",
                |settings_content| settings_content.workspace.reveal_if_open.as_ref(),
                |settings_content, value, _| {
                    settings_content.workspace.reveal_if_open = value;
                }
            ),
        ]
    }
    fn security_section() -> [SettingsPageItem; 2] {
        [
            section_header!("settings_ui.page_data.section.security", "Security"),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.trust.all.projects.by.default",
                    "Trust All Projects By Default"
                ),
                lt(
                    "settings_ui.page_data.description.when.opening.zzz.avoid.restricted.mode.by.auto.trusting.all.projects.enabling.use.of.all.features.without.having.to.give.permission.to.each.new.project",
                    "When opening ZZZ, avoid Restricted Mode by auto-trusting all projects, enabling use of all features without having to give permission to each new project."
                ),
                "session.trust_all_projects",
                |settings_content| {
                    settings_content
                        .session
                        .as_ref()
                        .and_then(|session| session.trust_all_worktrees.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .session
                        .get_or_insert_default()
                        .trust_all_worktrees = value;
                }
            ),
        ]
    }

    fn workspace_restoration_section() -> [SettingsPageItem; 3] {
        [
            section_header!(
                "settings_ui.page_data.section.workspace.restoration",
                "Workspace Restoration"
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.restore.unsaved.buffers",
                    "Restore Unsaved Buffers"
                ),
                lt(
                    "settings_ui.page_data.description.whether.or.not.to.restore.unsaved.buffers.on.restart",
                    "Whether or not to restore unsaved buffers on restart."
                ),
                "session.restore_unsaved_buffers",
                |settings_content| {
                    settings_content
                        .session
                        .as_ref()
                        .and_then(|session| session.restore_unsaved_buffers.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .session
                        .get_or_insert_default()
                        .restore_unsaved_buffers = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.restore.on.startup",
                    "Restore On Startup"
                ),
                lt(
                    "settings_ui.page_data.description.what.to.restore.from.the.previous.session.when.opening.zzz",
                    "What to restore from the previous session when opening ZZZ."
                ),
                "restore_on_startup",
                |settings_content| settings_content.workspace.restore_on_startup.as_ref(),
                |settings_content, value, _| {
                    settings_content.workspace.restore_on_startup = value;
                }
            ),
        ]
    }

    fn scoped_settings_section() -> [SettingsPageItem; 2] {
        [
            section_header!(
                "settings_ui.page_data.section.scoped.settings",
                "Scoped Settings"
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.settings.profiles",
                    "Settings Profiles"
                ),
                lt(
                    "settings_ui.page_data.description.any.number.of.settings.profiles.that.are.temporarily.applied.on.top.of.your.existing.user.settings",
                    "Any number of settings profiles that are temporarily applied on top of your existing user settings."
                ),
                "settings_profiles",
                |settings_content| Some(settings_content),
                |_settings_content, _value, _| {}
            ),
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
    // Rows that switch on a variant discriminant (`DynamicItem`) stay literal: their pick/write
    // closures match on every variant, which `setting_item!` cannot express.
    fn theme_section() -> [SettingsPageItem; 3] {
        [
            section_header!("settings_ui.page_data.section.theme", "Theme"),
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
            section_header!("settings_ui.page_data.section.buffer.font", "Buffer Font"),
            setting_item!(
                lt("settings_ui.page_data.title.font.family", "Font Family"),
                lt("settings_ui.page_data.description.font.family.for.editor.text", "Font family for editor text."),
                "buffer_font_family",
                |settings_content| settings_content.theme.buffer_font_family.as_ref(),
                |settings_content, value, _| { settings_content.theme.buffer_font_family = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.font.size", "Font Size"),
                lt("settings_ui.page_data.description.font.size.for.editor.text", "Font size for editor text."),
                "buffer_font_size",
                |settings_content| settings_content.theme.buffer_font_size.as_ref(),
                |settings_content, value, _| { settings_content.theme.buffer_font_size = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.font.weight", "Font Weight"),
                lt("settings_ui.page_data.description.font.weight.for.editor.text.100.900", "Font weight for editor text (100-900)."),
                "buffer_font_weight",
                |settings_content| settings_content.theme.buffer_font_weight.as_ref(),
                |settings_content, value, _| { settings_content.theme.buffer_font_weight = value; }),
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
            setting_item!(
                lt("settings_ui.page_data.title.font.features", "Font Features"),
                lt("settings_ui.page_data.description.the.opentype.features.to.enable.for.rendering.in.text.buffers", "The OpenType features to enable for rendering in text buffers."),
                "buffer_font_features",
                |settings_content| { settings_content.theme.buffer_font_features.as_ref() },
                |settings_content, value, _| { settings_content.theme.buffer_font_features = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.font.fallbacks", "Font Fallbacks"),
                lt("settings_ui.page_data.description.the.font.fallbacks.to.use.for.rendering.in.text.buffers", "The font fallbacks to use for rendering in text buffers."),
                "buffer_font_fallbacks",
                |settings_content| { settings_content.theme.buffer_font_fallbacks.as_ref() },
                |settings_content, value, _| { settings_content.theme.buffer_font_fallbacks = value; }),
        ]
    }

    fn ui_font_section() -> [SettingsPageItem; 6] {
        [
            section_header!("settings_ui.page_data.section.ui.font", "UI Font"),
            setting_item!(
                lt("settings_ui.page_data.title.font.family", "Font Family"),
                lt(
                    "settings_ui.page_data.description.font.family.for.ui.elements",
                    "Font family for UI elements."
                ),
                "ui_font_family",
                |settings_content| settings_content.theme.ui_font_family.as_ref(),
                |settings_content, value, _| {
                    settings_content.theme.ui_font_family = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.font.size", "Font Size"),
                lt(
                    "settings_ui.page_data.description.font.size.for.ui.elements",
                    "Font size for UI elements."
                ),
                "ui_font_size",
                |settings_content| settings_content.theme.ui_font_size.as_ref(),
                |settings_content, value, _| {
                    settings_content.theme.ui_font_size = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.font.weight", "Font Weight"),
                lt(
                    "settings_ui.page_data.description.font.weight.for.ui.elements.100.900",
                    "Font weight for UI elements (100-900)."
                ),
                "ui_font_weight",
                |settings_content| settings_content.theme.ui_font_weight.as_ref(),
                |settings_content, value, _| {
                    settings_content.theme.ui_font_weight = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.font.features", "Font Features"),
                lt(
                    "settings_ui.page_data.description.the.opentype.features.to.enable.for.rendering.in.ui.elements",
                    "The OpenType features to enable for rendering in UI elements."
                ),
                "ui_font_features",
                |settings_content| settings_content.theme.ui_font_features.as_ref(),
                |settings_content, value, _| {
                    settings_content.theme.ui_font_features = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.font.fallbacks",
                    "Font Fallbacks"
                ),
                lt(
                    "settings_ui.page_data.description.the.font.fallbacks.to.use.for.rendering.in.the.ui",
                    "The font fallbacks to use for rendering in the UI."
                ),
                "ui_font_fallbacks",
                |settings_content| settings_content.theme.ui_font_fallbacks.as_ref(),
                |settings_content, value, _| {
                    settings_content.theme.ui_font_fallbacks = value;
                }
            ),
        ]
    }

    fn agent_panel_font_section() -> [SettingsPageItem; 5] {
        [
            section_header!(
                "settings_ui.page_data.section.agent.panel.font",
                "Agent Panel Font"
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.ui.font.family",
                    "UI Font Family"
                ),
                lt(
                    "settings_ui.page_data.description.font.family.for.agent.response.text.in.the.agent.panel.falls.back.to.the.regular.ui.font.family",
                    "Font family for agent response text in the agent panel. Falls back to the regular UI font family."
                ),
                "agent_ui_font_family",
                |settings_content| {
                    settings_content
                        .theme
                        .agent_ui_font_family
                        .as_ref()
                        .or(settings_content.theme.ui_font_family.as_ref())
                },
                |settings_content, value, _| {
                    settings_content.theme.agent_ui_font_family = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.ui.font.size", "UI Font Size"),
                lt(
                    "settings_ui.page_data.description.font.size.for.agent.response.text.in.the.agent.panel.falls.back.to.the.regular.ui.font.size",
                    "Font size for agent response text in the agent panel. Falls back to the regular UI font size."
                ),
                "agent_ui_font_size",
                |settings_content| {
                    settings_content
                        .theme
                        .agent_ui_font_size
                        .as_ref()
                        .or(settings_content.theme.ui_font_size.as_ref())
                },
                |settings_content, value, _| {
                    settings_content.theme.agent_ui_font_size = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.buffer.font.family",
                    "Buffer Font Family"
                ),
                lt(
                    "settings_ui.page_data.description.font.family.for.user.messages.in.the.agent.panel.falls.back.to.the.regular.buffer.font.family",
                    "Font family for user messages in the agent panel. Falls back to the regular buffer font family."
                ),
                "agent_buffer_font_family",
                |settings_content| {
                    settings_content
                        .theme
                        .agent_buffer_font_family
                        .as_ref()
                        .or(settings_content.theme.buffer_font_family.as_ref())
                },
                |settings_content, value, _| {
                    settings_content.theme.agent_buffer_font_family = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.buffer.font.size",
                    "Buffer Font Size"
                ),
                lt(
                    "settings_ui.page_data.description.font.size.for.user.messages.text.in.the.agent.panel",
                    "Font size for user messages text in the agent panel."
                ),
                "agent_buffer_font_size",
                |settings_content| {
                    settings_content
                        .theme
                        .agent_buffer_font_size
                        .as_ref()
                        .or(settings_content.theme.buffer_font_size.as_ref())
                },
                |settings_content, value, _| {
                    settings_content.theme.agent_buffer_font_size = value;
                }
            ),
        ]
    }

    fn mermaid_font_section() -> [SettingsPageItem; 2] {
        [
            section_header!("settings_ui.page_data.section.mermaid.font", "Mermaid Font"),
            setting_item!(
                lt("settings_ui.page_data.title.font.family", "Font Family"),
                lt(
                    "settings_ui.page_data.description.font.family.for.mermaid.diagrams.in.the.agent.panel.and.markdown.preview.falls.back.to.the.ui.font.family",
                    "Font family for Mermaid diagrams in the agent panel and markdown preview. Falls back to the UI font family."
                ),
                "mermaid_font_family",
                |settings_content| {
                    settings_content
                        .theme
                        .mermaid_font_family
                        .as_ref()
                        .or(settings_content.theme.ui_font_family.as_ref())
                },
                |settings_content, value, _| {
                    settings_content.theme.mermaid_font_family = value;
                }
            ),
        ]
    }

    fn text_rendering_section() -> [SettingsPageItem; 3] {
        [
            section_header!(
                "settings_ui.page_data.section.text.rendering",
                "Text Rendering"
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.text.rendering.mode",
                    "Text Rendering Mode"
                ),
                lt(
                    "settings_ui.page_data.description.the.text.rendering.mode.to.use",
                    "The text rendering mode to use."
                ),
                "text_rendering_mode",
                |settings_content| { settings_content.workspace.text_rendering_mode.as_ref() },
                |settings_content, value, _| {
                    settings_content.workspace.text_rendering_mode = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.display.p3", "Display P3"),
                lt(
                    "settings_ui.page_data.description.display.p3",
                    "(macOS only) render windows using the Display P3 color space."
                ),
                "use_display_p3",
                |settings_content| settings_content.workspace.use_display_p3.as_ref(),
                |settings_content, value, _| {
                    settings_content.workspace.use_display_p3 = value;
                }
            ),
        ]
    }

    fn cursor_section() -> [SettingsPageItem; 7] {
        [
            section_header!("settings_ui.page_data.section.cursor", "Cursor"),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.multi.cursor.modifier",
                    "Multi Cursor Modifier"
                ),
                lt(
                    "settings_ui.page_data.description.modifier.key.for.adding.multiple.cursors",
                    "Modifier key for adding multiple cursors."
                ),
                "multi_cursor_modifier",
                |settings_content| settings_content.editor.multi_cursor_modifier.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.multi_cursor_modifier = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.cursor.blink", "Cursor Blink"),
                lt(
                    "settings_ui.page_data.description.whether.the.cursor.blinks.in.the.editor",
                    "Whether the cursor blinks in the editor."
                ),
                "cursor_blink",
                |settings_content| settings_content.editor.cursor_blink.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.cursor_blink = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.cursor.shape", "Cursor Shape"),
                lt(
                    "settings_ui.page_data.description.cursor.shape.for.the.editor",
                    "Cursor shape for the editor."
                ),
                "cursor_shape",
                |settings_content| settings_content.editor.cursor_shape.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.cursor_shape = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.cursor.animation",
                    "Cursor Animation"
                ),
                lt(
                    "settings_ui.page_data.description.whether.the.cursor.smoothly.animates",
                    "Whether the cursor smoothly animates when moving around the editor."
                ),
                "cursor_animation.enabled",
                |settings_content| {
                    settings_content
                        .editor
                        .cursor_animation
                        .as_ref()?
                        .enabled
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .cursor_animation
                        .get_or_insert_default()
                        .enabled = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.smooth.scrolling",
                    "Smooth Scrolling"
                ),
                lt(
                    "settings_ui.page_data.description.whether.scrolling.animates.smoothly",
                    "Whether scrolling animates smoothly."
                ),
                "smooth_scroll.enabled",
                |settings_content| {
                    settings_content
                        .editor
                        .smooth_scroll
                        .as_ref()?
                        .enabled
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .smooth_scroll
                        .get_or_insert_default()
                        .enabled = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.hide.mouse", "Hide Mouse"),
                lt(
                    "settings_ui.page_data.description.when.to.hide.the.mouse.cursor",
                    "When to hide the mouse cursor."
                ),
                "hide_mouse",
                |settings_content| settings_content.hide_mouse.as_ref(),
                |settings_content, value, _| {
                    settings_content.hide_mouse = value;
                }
            ),
        ]
    }

    fn highlighting_section() -> [SettingsPageItem; 6] {
        [
            section_header!("settings_ui.page_data.section.highlighting", "Highlighting"),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.unnecessary.code.fade",
                    "Unnecessary Code Fade"
                ),
                lt(
                    "settings_ui.page_data.description.how.much.to.fade.out.unused.code.0.0.0.9",
                    "How much to fade out unused code (0.0 - 0.9)."
                ),
                "unnecessary_code_fade",
                |settings_content| settings_content.theme.unnecessary_code_fade.as_ref(),
                |settings_content, value, _| {
                    settings_content.theme.unnecessary_code_fade = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.current.line.highlight",
                    "Current Line Highlight"
                ),
                lt(
                    "settings_ui.page_data.description.how.to.highlight.the.current.line",
                    "How to highlight the current line."
                ),
                "current_line_highlight",
                |settings_content| { settings_content.editor.current_line_highlight.as_ref() },
                |settings_content, value, _| {
                    settings_content.editor.current_line_highlight = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.selection.highlight",
                    "Selection Highlight"
                ),
                lt(
                    "settings_ui.page_data.description.highlight.all.occurrences.of.selected.text",
                    "Highlight all occurrences of selected text."
                ),
                "selection_highlight",
                |settings_content| settings_content.editor.selection_highlight.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.selection_highlight = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.rounded.selection",
                    "Rounded Selection"
                ),
                lt(
                    "settings_ui.page_data.description.whether.the.text.selection.should.have.rounded.corners",
                    "Whether the text selection should have rounded corners."
                ),
                "rounded_selection",
                |settings_content| settings_content.editor.rounded_selection.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.rounded_selection = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.minimum.contrast.for.highlights",
                    "Minimum Contrast For Highlights"
                ),
                lt(
                    "settings_ui.page_data.description.the.minimum.apca.perceptual.contrast.to.maintain.when.rendering.text.over.highlight.backgrounds",
                    "The minimum APCA perceptual contrast to maintain when rendering text over highlight backgrounds."
                ),
                "minimum_contrast_for_highlights",
                |settings_content| {
                    settings_content
                        .editor
                        .minimum_contrast_for_highlights
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content.editor.minimum_contrast_for_highlights = value;
                }
            ),
        ]
    }

    fn guides_section() -> [SettingsPageItem; 3] {
        [
            section_header!("settings_ui.page_data.section.guides", "Guides"),
            setting_item!(
                lt("settings_ui.page_data.title.show.wrap.guides", "Show Wrap Guides"),
                lt("settings_ui.page_data.description.show.wrap.guides.vertical.rulers", "Show wrap guides (vertical rulers)."),
                "show_wrap_guides",
                |settings_content| { settings_content.project.all_languages.defaults.show_wrap_guides.as_ref() },
                |settings_content, value, _| { settings_content.project.all_languages.defaults.show_wrap_guides = value; },
                files: USER | PROJECT),
            // todo(settings_ui): This needs a custom component
            setting_item!(
                lt("settings_ui.page_data.title.wrap.guides", "Wrap Guides"),
                lt("settings_ui.page_data.description.character.counts.at.which.to.show.wrap.guides", "Character counts at which to show wrap guides."),
                "wrap_guides",
                |settings_content| { settings_content.project.all_languages.defaults.wrap_guides.as_ref() },
                |settings_content, value, _| { settings_content.project.all_languages.defaults.wrap_guides = value; },
                files: USER | PROJECT),
        ]
    }

    let items: Box<[SettingsPageItem]> = concat_sections!(
        theme_section(),
        buffer_font_section(),
        ui_font_section(),
        agent_panel_font_section(),
        mermaid_font_section(),
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
            section_header!("settings_ui.page_data.section.keybindings", "Keybindings"),
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
                        .log_err();
                    window.remove_window();
                }),
                files: USER,
            }),
        ]
    }

    fn base_keymap_section() -> [SettingsPageItem; 2] {
        [
            section_header!("settings_ui.page_data.section.base.keymap", "Base Keymap"),
            setting_item!(
            lt("settings_ui.page_data.title.base.keymap", "Base Keymap"),
            lt("settings_ui.page_data.description.the.name.of.a.base.set.of.key.bindings.to.use", "The name of a base set of key bindings to use."),
            "base_keymap",
            |settings_content| settings_content.base_keymap.as_ref(),
            |settings_content, value, _| { settings_content.base_keymap = value; },
            metadata: Some(Box::new(SettingsFieldMetadata {
                should_do_titlecase: Some(false),
                ..Default::default()
            }))),
        ]
    }

    fn modal_editing_section() -> [SettingsPageItem; 3] {
        [
            section_header!(
                "settings_ui.page_data.section.modal.editing",
                "Modal Editing"
            ),
            setting_item!(
                lt("settings_ui.page_data.title.vim.mode", "Vim Mode"),
                lt(
                    "settings_ui.page_data.description.enable.vim.mode.and.key.bindings",
                    "Enable Vim mode and key bindings."
                ),
                "vim_mode",
                |settings_content| settings_content.vim_mode.as_ref(),
                write_vim_mode
            ),
            setting_item!(
                lt("settings_ui.page_data.title.helix.mode", "Helix Mode"),
                lt(
                    "settings_ui.page_data.description.enable.helix.mode.and.key.bindings",
                    "Enable Helix mode and key bindings."
                ),
                "helix_mode",
                |settings_content| settings_content.helix_mode.as_ref(),
                write_helix_mode
            ),
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
    // Rows that switch on a variant discriminant (`DynamicItem`) stay literal: their pick/write
    // closures match on every variant, which `setting_item!` cannot express.
    fn auto_save_section() -> [SettingsPageItem; 2] {
        [
            section_header!("settings_ui.page_data.section.auto.save", "Auto Save"),
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
                                .get_or_insert(settings::AutosaveSetting::Off);
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
            section_header!(
                "settings_ui.page_data.section.which.key.menu",
                "Which-key Menu"
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.which.key.menu",
                    "Show Which-key Menu"
                ),
                lt(
                    "settings_ui.page_data.description.display.the.which.key.menu.with.matching.bindings.while.a.multi.stroke.binding.is.pending",
                    "Display the which-key menu with matching bindings while a multi-stroke binding is pending."
                ),
                "which_key.enabled",
                |settings_content| {
                    settings_content
                        .which_key
                        .as_ref()
                        .and_then(|settings| settings.enabled.as_ref())
                },
                |settings_content, value, _| {
                    settings_content.which_key.get_or_insert_default().enabled = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.menu.delay", "Menu Delay"),
                lt(
                    "settings_ui.page_data.description.delay.in.milliseconds.before.the.which.key.menu.appears",
                    "Delay in milliseconds before the which-key menu appears."
                ),
                "which_key.delay_ms",
                |settings_content| {
                    settings_content
                        .which_key
                        .as_ref()
                        .and_then(|settings| settings.delay_ms.as_ref())
                },
                |settings_content, value, _| {
                    settings_content.which_key.get_or_insert_default().delay_ms = value;
                }
            ),
        ]
    }

    fn multibuffer_section() -> [SettingsPageItem; 7] {
        [
            section_header!("settings_ui.page_data.section.multibuffer", "Multibuffer"),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.double.click.in.multibuffer",
                    "Double Click In Multibuffer"
                ),
                lt(
                    "settings_ui.page_data.description.what.to.do.when.multibuffer.is.double.clicked.in.some.of.its.excerpts",
                    "What to do when multibuffer is double-clicked in some of its excerpts."
                ),
                "double_click_in_multibuffer",
                |settings_content| { settings_content.editor.double_click_in_multibuffer.as_ref() },
                |settings_content, value, _| {
                    settings_content.editor.double_click_in_multibuffer = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.expand.excerpt.lines",
                    "Expand Excerpt Lines"
                ),
                lt(
                    "settings_ui.page_data.description.how.many.lines.to.expand.the.multibuffer.excerpts.by.default",
                    "How many lines to expand the multibuffer excerpts by default."
                ),
                "expand_excerpt_lines",
                |settings_content| settings_content.editor.expand_excerpt_lines.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.expand_excerpt_lines = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.excerpt.context.lines",
                    "Excerpt Context Lines"
                ),
                lt(
                    "settings_ui.page_data.description.how.many.lines.of.context.to.provide.in.multibuffer.excerpts.by.default",
                    "How many lines of context to provide in multibuffer excerpts by default."
                ),
                "excerpt_context_lines",
                |settings_content| settings_content.editor.excerpt_context_lines.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.excerpt_context_lines = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.expand.outlines.with.depth",
                    "Expand Outlines With Depth"
                ),
                lt(
                    "settings_ui.page_data.description.default.depth.to.expand.outline.items.in.the.current.file",
                    "Default depth to expand outline items in the current file."
                ),
                "outline_panel.expand_outlines_with_depth",
                |settings_content| {
                    settings_content
                        .outline_panel
                        .as_ref()
                        .and_then(|outline_panel| outline_panel.expand_outlines_with_depth.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .expand_outlines_with_depth = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.diff.view.style",
                    "Diff View Style"
                ),
                lt(
                    "settings_ui.page_data.description.how.to.display.diffs.in.the.editor",
                    "How to display diffs in the editor."
                ),
                "diff_view_style",
                |settings_content| settings_content.editor.diff_view_style.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.diff_view_style = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.minimum.split.diff.width",
                    "Minimum Split Diff Width"
                ),
                lt(
                    "settings_ui.page_data.description.the.minimum.width.in.columns.at.which.the.split.diff.view.is.used.when.the.editor.is.narrower.the.diff.view.automatically.switches.to.unified.mode.set.to.0.to.disable",
                    "The minimum width (in columns) at which the split diff view is used. When the editor is narrower, the diff view automatically switches to unified mode. Set to 0 to disable."
                ),
                "minimum_split_diff_width",
                |settings_content| { settings_content.editor.minimum_split_diff_width.as_ref() },
                |settings_content, value, _| {
                    settings_content.editor.minimum_split_diff_width = value;
                }
            ),
        ]
    }

    fn scrolling_section() -> [SettingsPageItem; 9] {
        [
            section_header!("settings_ui.page_data.section.scrolling", "Scrolling"),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.scroll.beyond.last.line",
                    "Scroll Beyond Last Line"
                ),
                lt(
                    "settings_ui.page_data.description.whether.the.editor.will.scroll.beyond.the.last.line",
                    "Whether the editor will scroll beyond the last line."
                ),
                "scroll_beyond_last_line",
                |settings_content| { settings_content.editor.scroll_beyond_last_line.as_ref() },
                |settings_content, value, _| {
                    settings_content.editor.scroll_beyond_last_line = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.vertical.scroll.margin",
                    "Vertical Scroll Margin"
                ),
                lt(
                    "settings_ui.page_data.description.the.number.of.lines.to.keep.above.below.the.cursor.when.auto.scrolling",
                    "The number of lines to keep above/below the cursor when auto-scrolling."
                ),
                "vertical_scroll_margin",
                |settings_content| { settings_content.editor.vertical_scroll_margin.as_ref() },
                |settings_content, value, _| {
                    settings_content.editor.vertical_scroll_margin = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.horizontal.scroll.margin",
                    "Horizontal Scroll Margin"
                ),
                lt(
                    "settings_ui.page_data.description.the.number.of.characters.to.keep.on.either.side.when.scrolling.with.the.mouse",
                    "The number of characters to keep on either side when scrolling with the mouse."
                ),
                "horizontal_scroll_margin",
                |settings_content| { settings_content.editor.horizontal_scroll_margin.as_ref() },
                |settings_content, value, _| {
                    settings_content.editor.horizontal_scroll_margin = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.scroll.sensitivity",
                    "Scroll Sensitivity"
                ),
                lt(
                    "settings_ui.page_data.description.scroll.sensitivity.multiplier.for.both.horizontal.and.vertical.scrolling",
                    "Scroll sensitivity multiplier for both horizontal and vertical scrolling."
                ),
                "scroll_sensitivity",
                |settings_content| settings_content.editor.scroll_sensitivity.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.scroll_sensitivity = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.mouse.wheel.zoom",
                    "Mouse Wheel Zoom"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.zoom.the.editor.font.size.with.the.mouse.wheel.while.holding.the.primary.modifier.key",
                    "Whether to zoom the editor font size with the mouse wheel while holding the primary modifier key."
                ),
                "mouse_wheel_zoom",
                |settings_content| settings_content.editor.mouse_wheel_zoom.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.mouse_wheel_zoom = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.fast.scroll.sensitivity",
                    "Fast Scroll Sensitivity"
                ),
                lt(
                    "settings_ui.page_data.description.fast.scroll.sensitivity.multiplier.for.both.horizontal.and.vertical.scrolling",
                    "Fast scroll sensitivity multiplier for both horizontal and vertical scrolling."
                ),
                "fast_scroll_sensitivity",
                |settings_content| { settings_content.editor.fast_scroll_sensitivity.as_ref() },
                |settings_content, value, _| {
                    settings_content.editor.fast_scroll_sensitivity = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.autoscroll.on.clicks",
                    "Autoscroll On Clicks"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.scroll.when.clicking.near.the.edge.of.the.visible.text.area",
                    "Whether to scroll when clicking near the edge of the visible text area."
                ),
                "autoscroll_on_clicks",
                |settings_content| settings_content.editor.autoscroll_on_clicks.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.autoscroll_on_clicks = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.sticky.scroll", "Sticky Scroll"),
                lt(
                    "settings_ui.page_data.description.whether.to.stick.scopes.to.the.top.of.the.editor",
                    "Whether to stick scopes to the top of the editor"
                ),
                "sticky_scroll.enabled",
                |settings_content| {
                    settings_content
                        .editor
                        .sticky_scroll
                        .as_ref()
                        .and_then(|sticky_scroll| sticky_scroll.enabled.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .sticky_scroll
                        .get_or_insert_default()
                        .enabled = value;
                }
            ),
        ]
    }

    fn signature_help_section() -> [SettingsPageItem; 4] {
        [
            section_header!(
                "settings_ui.page_data.section.signature.help",
                "Signature Help"
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.auto.signature.help",
                    "Auto Signature Help"
                ),
                lt(
                    "settings_ui.page_data.description.automatically.show.a.signature.help.pop.up",
                    "Automatically show a signature help pop-up."
                ),
                "auto_signature_help",
                |settings_content| settings_content.editor.auto_signature_help.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.auto_signature_help = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.signature.help.after.edits",
                    "Show Signature Help After Edits"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.signature.help.pop.up.after.completions.or.bracket.pairs.are.inserted",
                    "Show the signature help pop-up after completions or bracket pairs are inserted."
                ),
                "show_signature_help_after_edits",
                |settings_content| {
                    settings_content
                        .editor
                        .show_signature_help_after_edits
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content.editor.show_signature_help_after_edits = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.snippet.sort.order",
                    "Snippet Sort Order"
                ),
                lt(
                    "settings_ui.page_data.description.determines.how.snippets.are.sorted.relative.to.other.completion.items",
                    "Determines how snippets are sorted relative to other completion items."
                ),
                "snippet_sort_order",
                |settings_content| settings_content.editor.snippet_sort_order.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.snippet_sort_order = value;
                }
            ),
        ]
    }

    fn hover_popover_section() -> [SettingsPageItem; 5] {
        [
            section_header!(
                "settings_ui.page_data.section.hover.popover",
                "Hover Popover"
            ),
            setting_item!(
                lt("settings_ui.page_data.title.enabled", "Enabled"),
                lt(
                    "settings_ui.page_data.description.show.the.informational.hover.box.when.moving.the.mouse.over.symbols.in.the.editor",
                    "Show the informational hover box when moving the mouse over symbols in the editor."
                ),
                "hover_popover_enabled",
                |settings_content| settings_content.editor.hover_popover_enabled.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.hover_popover_enabled = value;
                }
            ),
            // todo(settings ui): add units to this number input
            setting_item!(
                lt("settings_ui.page_data.title.delay", "Delay"),
                lt(
                    "settings_ui.page_data.description.time.to.wait.in.milliseconds.before.showing.the.informational.hover.box",
                    "Time to wait in milliseconds before showing the informational hover box."
                ),
                "hover_popover_delay",
                |settings_content| settings_content.editor.hover_popover_delay.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.hover_popover_delay = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.sticky", "Sticky"),
                lt(
                    "settings_ui.page_data.description.whether.the.hover.popover.sticks.when.the.mouse.moves.toward.it.allowing.interaction.with.its.contents",
                    "Whether the hover popover sticks when the mouse moves toward it, allowing interaction with its contents."
                ),
                "hover_popover_sticky",
                |settings_content| settings_content.editor.hover_popover_sticky.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.hover_popover_sticky = value;
                }
            ),
            // todo(settings ui): add units to this number input
            setting_item!(
                lt("settings_ui.page_data.title.hiding.delay", "Hiding Delay"),
                lt(
                    "settings_ui.page_data.description.time.to.wait.in.milliseconds.before.hiding.the.hover.popover.after.the.mouse.moves.away",
                    "Time to wait in milliseconds before hiding the hover popover after the mouse moves away."
                ),
                "hover_popover_hiding_delay",
                |settings_content| { settings_content.editor.hover_popover_hiding_delay.as_ref() },
                |settings_content, value, _| {
                    settings_content.editor.hover_popover_hiding_delay = value;
                }
            ),
        ]
    }

    fn drag_and_drop_selection_section() -> [SettingsPageItem; 3] {
        [
            section_header!(
                "settings_ui.page_data.section.drag.and.drop.selection",
                "Drag And Drop Selection"
            ),
            setting_item!(
                lt("settings_ui.page_data.title.enabled", "Enabled"),
                lt(
                    "settings_ui.page_data.description.enable.drag.and.drop.selection",
                    "Enable drag and drop selection."
                ),
                "drag_and_drop_selection.enabled",
                |settings_content| {
                    settings_content
                        .editor
                        .drag_and_drop_selection
                        .as_ref()
                        .and_then(|drag_and_drop| drag_and_drop.enabled.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .drag_and_drop_selection
                        .get_or_insert_default()
                        .enabled = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.delay", "Delay"),
                lt(
                    "settings_ui.page_data.description.delay.in.milliseconds.before.drag.and.drop.selection.starts",
                    "Delay in milliseconds before drag and drop selection starts."
                ),
                "drag_and_drop_selection.delay",
                |settings_content| {
                    settings_content
                        .editor
                        .drag_and_drop_selection
                        .as_ref()
                        .and_then(|drag_and_drop| drag_and_drop.delay.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .drag_and_drop_selection
                        .get_or_insert_default()
                        .delay = value;
                }
            ),
        ]
    }

    fn gutter_section() -> [SettingsPageItem; 11] {
        [
            section_header!("settings_ui.page_data.section.gutter", "Gutter"),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.git.gutter.width",
                    "Git Gutter Width"
                ),
                lt(
                    "settings_ui.page_data.description.git.gutter.width",
                    "Width, in pixels, of the git diff indicators in the gutter. When unset, the width scales with the buffer font size."
                ),
                "gutter.git_gutter_width",
                |settings_content| {
                    settings_content
                        .editor
                        .gutter
                        .as_ref()
                        .and_then(|gutter| gutter.git_gutter_width.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .gutter
                        .get_or_insert_default()
                        .git_gutter_width = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.line.numbers",
                    "Show Line Numbers"
                ),
                lt(
                    "settings_ui.page_data.description.show.line.numbers.in.the.gutter",
                    "Show line numbers in the gutter."
                ),
                "gutter.line_numbers",
                |settings_content| {
                    settings_content
                        .editor
                        .gutter
                        .as_ref()
                        .and_then(|gutter| gutter.line_numbers.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .gutter
                        .get_or_insert_default()
                        .line_numbers = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.relative.line.numbers",
                    "Relative Line Numbers"
                ),
                lt(
                    "settings_ui.page_data.description.controls.line.number.display.in.the.editor.s.gutter.disabled.shows.absolute.line.numbers.enabled.shows.relative.line.numbers.for.each.absolute.line.and.wrapped.shows.relative.line.numbers.for.every.line.absolute.or.wrapped",
                    "Controls line number display in the editor's gutter. \\\"disabled\\\" shows absolute line numbers, \\\"enabled\\\" shows relative line numbers for each absolute line, and \\\"wrapped\\\" shows relative line numbers for every line, absolute or wrapped."
                ),
                "relative_line_numbers",
                |settings_content| settings_content.editor.relative_line_numbers.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.relative_line_numbers = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.line.number.scale",
                    "Line Number Scale"
                ),
                lt(
                    "settings_ui.page_data.description.scale.of.the.line.number.font.size.relative.to.the.buffer.font.size",
                    "Scale of the line number font size, relative to the buffer font size."
                ),
                "line_number_scale",
                |settings_content| settings_content.editor.line_number_scale.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.line_number_scale = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.runnables",
                    "Show Runnables"
                ),
                lt(
                    "settings_ui.page_data.description.show.runnable.buttons.in.the.gutter",
                    "Show runnable buttons in the gutter."
                ),
                "gutter.runnables",
                |settings_content| {
                    settings_content
                        .editor
                        .gutter
                        .as_ref()
                        .and_then(|gutter| gutter.runnables.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .gutter
                        .get_or_insert_default()
                        .runnables = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.breakpoints",
                    "Show Breakpoints"
                ),
                lt(
                    "settings_ui.page_data.description.show.breakpoints.in.the.gutter",
                    "Show breakpoints in the gutter."
                ),
                "gutter.breakpoints",
                |settings_content| {
                    settings_content
                        .editor
                        .gutter
                        .as_ref()
                        .and_then(|gutter| gutter.breakpoints.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .gutter
                        .get_or_insert_default()
                        .breakpoints = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.bookmarks",
                    "Show Bookmarks"
                ),
                lt(
                    "settings_ui.page_data.description.show.bookmarks.in.the.gutter",
                    "Show bookmarks in the gutter."
                ),
                "gutter.bookmarks",
                |settings_content| {
                    settings_content
                        .editor
                        .gutter
                        .as_ref()
                        .and_then(|gutter| gutter.bookmarks.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .gutter
                        .get_or_insert_default()
                        .bookmarks = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.show.folds", "Show Folds"),
                lt(
                    "settings_ui.page_data.description.show.code.folding.controls.in.the.gutter",
                    "Show code folding controls in the gutter."
                ),
                "gutter.folds",
                |settings_content| {
                    settings_content
                        .editor
                        .gutter
                        .as_ref()
                        .and_then(|gutter| gutter.folds.as_ref())
                },
                |settings_content, value, _| {
                    settings_content.editor.gutter.get_or_insert_default().folds = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.min.line.number.digits",
                    "Min Line Number Digits"
                ),
                lt(
                    "settings_ui.page_data.description.minimum.number.of.characters.to.reserve.space.for.in.the.gutter",
                    "Minimum number of characters to reserve space for in the gutter."
                ),
                "gutter.min_line_number_digits",
                |settings_content| {
                    settings_content
                        .editor
                        .gutter
                        .as_ref()
                        .and_then(|gutter| gutter.min_line_number_digits.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .gutter
                        .get_or_insert_default()
                        .min_line_number_digits = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.inline.code.actions",
                    "Inline Code Actions"
                ),
                lt(
                    "settings_ui.page_data.description.show.code.action.button.at.start.of.buffer.line",
                    "Show code action button at start of buffer line."
                ),
                "inline_code_actions",
                |settings_content| settings_content.editor.inline_code_actions.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.inline_code_actions = value;
                }
            ),
        ]
    }

    fn scrollbar_section() -> [SettingsPageItem; 10] {
        [
            section_header!("settings_ui.page_data.section.scrollbar", "Scrollbar"),
            setting_item!(
                lt("settings_ui.page_data.title.show", "Show"),
                lt(
                    "settings_ui.page_data.description.when.to.show.the.scrollbar.in.the.editor",
                    "When to show the scrollbar in the editor."
                ),
                "scrollbar",
                |settings_content| { settings_content.editor.scrollbar.as_ref()?.show.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .show = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.cursors", "Cursors"),
                lt(
                    "settings_ui.page_data.description.show.cursor.positions.in.the.scrollbar",
                    "Show cursor positions in the scrollbar."
                ),
                "scrollbar.cursors",
                |settings_content| { settings_content.editor.scrollbar.as_ref()?.cursors.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .cursors = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.git.diff", "Git Diff"),
                lt(
                    "settings_ui.page_data.description.show.git.diff.indicators.in.the.scrollbar",
                    "Show Git diff indicators in the scrollbar."
                ),
                "scrollbar.git_diff",
                |settings_content| {
                    settings_content
                        .editor
                        .scrollbar
                        .as_ref()?
                        .git_diff
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .git_diff = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.search.results",
                    "Search Results"
                ),
                lt(
                    "settings_ui.page_data.description.show.buffer.search.result.indicators.in.the.scrollbar",
                    "Show buffer search result indicators in the scrollbar."
                ),
                "scrollbar.search_results",
                |settings_content| {
                    settings_content
                        .editor
                        .scrollbar
                        .as_ref()?
                        .search_results
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .search_results = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.selected.text", "Selected Text"),
                lt(
                    "settings_ui.page_data.description.show.selected.text.occurrences.in.the.scrollbar",
                    "Show selected text occurrences in the scrollbar."
                ),
                "scrollbar.selected_text",
                |settings_content| {
                    settings_content
                        .editor
                        .scrollbar
                        .as_ref()?
                        .selected_text
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .selected_text = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.selected.symbol",
                    "Selected Symbol"
                ),
                lt(
                    "settings_ui.page_data.description.show.selected.symbol.occurrences.in.the.scrollbar",
                    "Show selected symbol occurrences in the scrollbar."
                ),
                "scrollbar.selected_symbol",
                |settings_content| {
                    settings_content
                        .editor
                        .scrollbar
                        .as_ref()?
                        .selected_symbol
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .selected_symbol = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.diagnostics", "Diagnostics"),
                lt(
                    "settings_ui.page_data.description.which.diagnostic.indicators.to.show.in.the.scrollbar",
                    "Which diagnostic indicators to show in the scrollbar."
                ),
                "scrollbar.diagnostics",
                |settings_content| {
                    settings_content
                        .editor
                        .scrollbar
                        .as_ref()?
                        .diagnostics
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .diagnostics = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.horizontal.scrollbar",
                    "Horizontal Scrollbar"
                ),
                lt(
                    "settings_ui.page_data.description.when.false.forcefully.disables.the.horizontal.scrollbar",
                    "When false, forcefully disables the horizontal scrollbar."
                ),
                "scrollbar.axes.horizontal",
                |settings_content| {
                    settings_content
                        .editor
                        .scrollbar
                        .as_ref()?
                        .axes
                        .as_ref()?
                        .horizontal
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .axes
                        .get_or_insert_default()
                        .horizontal = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.vertical.scrollbar",
                    "Vertical Scrollbar"
                ),
                lt(
                    "settings_ui.page_data.description.when.false.forcefully.disables.the.vertical.scrollbar",
                    "When false, forcefully disables the vertical scrollbar."
                ),
                "scrollbar.axes.vertical",
                |settings_content| {
                    settings_content
                        .editor
                        .scrollbar
                        .as_ref()?
                        .axes
                        .as_ref()?
                        .vertical
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .scrollbar
                        .get_or_insert_default()
                        .axes
                        .get_or_insert_default()
                        .vertical = value;
                }
            ),
        ]
    }

    fn minimap_section() -> [SettingsPageItem; 7] {
        [
            section_header!("settings_ui.page_data.section.minimap", "Minimap"),
            setting_item!(
                lt("settings_ui.page_data.title.show", "Show"),
                lt(
                    "settings_ui.page_data.description.when.to.show.the.minimap.in.the.editor",
                    "When to show the minimap in the editor."
                ),
                "minimap.show",
                |settings_content| { settings_content.editor.minimap.as_ref()?.show.as_ref() },
                |settings_content, value, _| {
                    settings_content.editor.minimap.get_or_insert_default().show = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.display.in", "Display In"),
                lt(
                    "settings_ui.page_data.description.where.to.show.the.minimap.in.the.editor",
                    "Where to show the minimap in the editor."
                ),
                "minimap.display_in",
                |settings_content| {
                    settings_content
                        .editor
                        .minimap
                        .as_ref()?
                        .display_in
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .minimap
                        .get_or_insert_default()
                        .display_in = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.thumb", "Thumb"),
                lt(
                    "settings_ui.page_data.description.when.to.show.the.minimap.thumb",
                    "When to show the minimap thumb."
                ),
                "minimap.thumb",
                |settings_content| { settings_content.editor.minimap.as_ref()?.thumb.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .minimap
                        .get_or_insert_default()
                        .thumb = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.thumb.border", "Thumb Border"),
                lt(
                    "settings_ui.page_data.description.border.style.for.the.minimap.s.scrollbar.thumb",
                    "Border style for the minimap's scrollbar thumb."
                ),
                "minimap.thumb_border",
                |settings_content| {
                    settings_content
                        .editor
                        .minimap
                        .as_ref()?
                        .thumb_border
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .minimap
                        .get_or_insert_default()
                        .thumb_border = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.current.line.highlight",
                    "Current Line Highlight"
                ),
                lt(
                    "settings_ui.page_data.description.how.to.highlight.the.current.line.in.the.minimap",
                    "How to highlight the current line in the minimap."
                ),
                "minimap.current_line_highlight",
                |settings_content| {
                    settings_content
                        .editor
                        .minimap
                        .as_ref()
                        .and_then(|minimap| minimap.current_line_highlight.as_ref())
                        .or(settings_content.editor.current_line_highlight.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .minimap
                        .get_or_insert_default()
                        .current_line_highlight = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.max.width.columns",
                    "Max Width Columns"
                ),
                lt(
                    "settings_ui.page_data.description.maximum.number.of.columns.to.display.in.the.minimap",
                    "Maximum number of columns to display in the minimap."
                ),
                "minimap.max_width_columns",
                |settings_content| {
                    settings_content
                        .editor
                        .minimap
                        .as_ref()?
                        .max_width_columns
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .minimap
                        .get_or_insert_default()
                        .max_width_columns = value;
                }
            ),
        ]
    }

    fn toolbar_section() -> [SettingsPageItem; 6] {
        [
            section_header!("settings_ui.page_data.section.toolbar", "Toolbar"),
            setting_item!(
                lt("settings_ui.page_data.title.breadcrumbs", "Breadcrumbs"),
                lt(
                    "settings_ui.page_data.description.show.breadcrumbs",
                    "Show breadcrumbs."
                ),
                "toolbar.breadcrumbs",
                |settings_content| {
                    settings_content
                        .editor
                        .toolbar
                        .as_ref()?
                        .breadcrumbs
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .toolbar
                        .get_or_insert_default()
                        .breadcrumbs = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.quick.actions", "Quick Actions"),
                lt(
                    "settings_ui.page_data.description.show.quick.action.buttons.e.g.search.selection.editor.controls.etc",
                    "Show quick action buttons (e.g., search, selection, editor controls, etc.)."
                ),
                "toolbar.quick_actions",
                |settings_content| {
                    settings_content
                        .editor
                        .toolbar
                        .as_ref()?
                        .quick_actions
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .toolbar
                        .get_or_insert_default()
                        .quick_actions = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.selections.menu",
                    "Selections Menu"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.selections.menu.in.the.editor.toolbar",
                    "Show the selections menu in the editor toolbar."
                ),
                "toolbar.selections_menu",
                |settings_content| {
                    settings_content
                        .editor
                        .toolbar
                        .as_ref()?
                        .selections_menu
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .toolbar
                        .get_or_insert_default()
                        .selections_menu = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.agent.review", "Agent Review"),
                lt(
                    "settings_ui.page_data.description.show.agent.review.buttons.in.the.editor.toolbar",
                    "Show agent review buttons in the editor toolbar."
                ),
                "toolbar.agent_review",
                |settings_content| {
                    settings_content
                        .editor
                        .toolbar
                        .as_ref()?
                        .agent_review
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .toolbar
                        .get_or_insert_default()
                        .agent_review = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.code.actions", "Code Actions"),
                lt(
                    "settings_ui.page_data.description.show.code.action.buttons.in.the.editor.toolbar",
                    "Show code action buttons in the editor toolbar."
                ),
                "toolbar.code_actions",
                |settings_content| {
                    settings_content
                        .editor
                        .toolbar
                        .as_ref()?
                        .code_actions
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .toolbar
                        .get_or_insert_default()
                        .code_actions = value;
                }
            ),
        ]
    }

    fn vim_settings_section() -> [SettingsPageItem; 13] {
        [
            section_header!("settings_ui.page_data.section.vim", "Vim"),
            setting_item!(
                lt("settings_ui.page_data.title.default.mode", "Default Mode"),
                lt(
                    "settings_ui.page_data.description.the.default.mode.when.vim.starts",
                    "The default mode when Vim starts."
                ),
                "vim.default_mode",
                |settings_content| settings_content.vim.as_ref()?.default_mode.as_ref(),
                |settings_content, value, _| {
                    settings_content.vim.get_or_insert_default().default_mode = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.toggle.relative.line.numbers",
                    "Toggle Relative Line Numbers"
                ),
                lt(
                    "settings_ui.page_data.description.toggle.relative.line.numbers.in.vim.mode",
                    "Toggle relative line numbers in Vim mode."
                ),
                "vim.toggle_relative_line_numbers",
                |settings_content| {
                    settings_content
                        .vim
                        .as_ref()?
                        .toggle_relative_line_numbers
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .toggle_relative_line_numbers = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.use.system.clipboard",
                    "Use System Clipboard"
                ),
                lt(
                    "settings_ui.page_data.description.controls.when.to.use.system.clipboard.in.vim.mode",
                    "Controls when to use system clipboard in Vim mode."
                ),
                "vim.use_system_clipboard",
                |settings_content| { settings_content.vim.as_ref()?.use_system_clipboard.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .use_system_clipboard = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.use.smartcase.find",
                    "Use Smartcase Find"
                ),
                lt(
                    "settings_ui.page_data.description.enable.smartcase.searching.in.vim.mode",
                    "Enable smartcase searching in Vim mode."
                ),
                "vim.use_smartcase_find",
                |settings_content| { settings_content.vim.as_ref()?.use_smartcase_find.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .use_smartcase_find = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.global.substitution.default",
                    "Global Substitution Default"
                ),
                lt(
                    "settings_ui.page_data.description.when.enabled.the.substitute.command.replaces.all.matches.in.a.line.by.default.the.g.flag.then.toggles.this.behavior",
                    "When enabled, the :substitute command replaces all matches in a line by default. The 'g' flag then toggles this behavior."
                ),
                "vim.gdefault",
                |settings_content| settings_content.vim.as_ref()?.gdefault.as_ref(),
                |settings_content, value, _| {
                    settings_content.vim.get_or_insert_default().gdefault = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.highlight.on.yank.duration",
                    "Highlight on Yank Duration"
                ),
                lt(
                    "settings_ui.page_data.description.duration.in.milliseconds.to.highlight.yanked.text.in.vim.mode",
                    "Duration in milliseconds to highlight yanked text in Vim mode."
                ),
                "vim.highlight_on_yank_duration",
                |settings_content| {
                    settings_content
                        .vim
                        .as_ref()?
                        .highlight_on_yank_duration
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .highlight_on_yank_duration = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.regex.search", "Regex Search"),
                lt(
                    "settings_ui.page_data.description.use.regex.search.by.default.in.vim.search",
                    "Use regex search by default in Vim search."
                ),
                "vim.use_regex_search",
                |settings_content| { settings_content.vim.as_ref()?.use_regex_search.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .use_regex_search = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.cursor.shape.normal.mode",
                    "Cursor Shape - Normal Mode"
                ),
                lt(
                    "settings_ui.page_data.description.cursor.shape.for.normal.mode",
                    "Cursor shape for normal mode."
                ),
                "vim.cursor_shape.normal",
                |settings_content| {
                    settings_content
                        .vim
                        .as_ref()?
                        .cursor_shape
                        .as_ref()?
                        .normal
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .cursor_shape
                        .get_or_insert_default()
                        .normal = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.cursor.shape.insert.mode",
                    "Cursor Shape - Insert Mode"
                ),
                lt(
                    "settings_ui.page_data.description.cursor.shape.for.insert.mode.inherit.uses.the.editor.s.cursor.shape",
                    "Cursor shape for insert mode. Inherit uses the editor's cursor shape."
                ),
                "vim.cursor_shape.insert",
                |settings_content| {
                    settings_content
                        .vim
                        .as_ref()?
                        .cursor_shape
                        .as_ref()?
                        .insert
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .cursor_shape
                        .get_or_insert_default()
                        .insert = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.cursor.shape.replace.mode",
                    "Cursor Shape - Replace Mode"
                ),
                lt(
                    "settings_ui.page_data.description.cursor.shape.for.replace.mode",
                    "Cursor shape for replace mode."
                ),
                "vim.cursor_shape.replace",
                |settings_content| {
                    settings_content
                        .vim
                        .as_ref()?
                        .cursor_shape
                        .as_ref()?
                        .replace
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .cursor_shape
                        .get_or_insert_default()
                        .replace = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.cursor.shape.visual.mode",
                    "Cursor Shape - Visual Mode"
                ),
                lt(
                    "settings_ui.page_data.description.cursor.shape.for.visual.mode",
                    "Cursor shape for visual mode."
                ),
                "vim.cursor_shape.visual",
                |settings_content| {
                    settings_content
                        .vim
                        .as_ref()?
                        .cursor_shape
                        .as_ref()?
                        .visual
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .vim
                        .get_or_insert_default()
                        .cursor_shape
                        .get_or_insert_default()
                        .visual = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.custom.digraphs",
                    "Custom Digraphs"
                ),
                lt(
                    "settings_ui.page_data.description.custom.digraph.mappings.for.vim.mode",
                    "Custom digraph mappings for Vim mode."
                ),
                "vim.custom_digraphs",
                |settings_content| { settings_content.vim.as_ref()?.custom_digraphs.as_ref() },
                |settings_content, value, _| {
                    settings_content.vim.get_or_insert_default().custom_digraphs = value;
                }
            ),
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
            section_header!("settings_ui.page_data.section.file.types", "File Types"),
            setting_item!(
                lt("settings_ui.page_data.title.file.type.associations", "File Type Associations"),
                lt("settings_ui.page_data.description.a.mapping.from.languages.to.files.and.file.extensions.that.should.be.treated.as.that.language", "A mapping from languages to files and file extensions that should be treated as that language."),
                "file_type_associations",
                |settings_content| { settings_content.project.all_languages.file_types.as_ref() },
                |settings_content, value, _| { settings_content.project.all_languages.file_types = value; },
                files: USER | PROJECT),
        ]
    }

    fn diagnostics_section() -> [SettingsPageItem; 3] {
        [
            section_header!("settings_ui.page_data.section.diagnostics", "Diagnostics"),
            setting_item!(
                lt("settings_ui.page_data.title.max.severity", "Max Severity"),
                lt(
                    "settings_ui.page_data.description.which.level.to.use.to.filter.out.diagnostics.displayed.in.the.editor",
                    "Which level to use to filter out diagnostics displayed in the editor."
                ),
                "diagnostics_max_severity",
                |settings_content| { settings_content.editor.diagnostics_max_severity.as_ref() },
                |settings_content, value, _| {
                    settings_content.editor.diagnostics_max_severity = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.include.warnings",
                    "Include Warnings"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.show.warnings.or.not.by.default",
                    "Whether to show warnings or not by default."
                ),
                "diagnostics.include_warnings",
                |settings_content| {
                    settings_content
                        .diagnostics
                        .as_ref()?
                        .include_warnings
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .diagnostics
                        .get_or_insert_default()
                        .include_warnings = value;
                }
            ),
        ]
    }

    fn inline_diagnostics_section() -> [SettingsPageItem; 5] {
        [
            section_header!(
                "settings_ui.page_data.section.inline.diagnostics",
                "Inline Diagnostics"
            ),
            setting_item!(
                lt("settings_ui.page_data.title.enabled", "Enabled"),
                lt(
                    "settings_ui.page_data.description.whether.to.show.diagnostics.inline.or.not",
                    "Whether to show diagnostics inline or not."
                ),
                "diagnostics.inline.enabled",
                |settings_content| {
                    settings_content
                        .diagnostics
                        .as_ref()?
                        .inline
                        .as_ref()?
                        .enabled
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .diagnostics
                        .get_or_insert_default()
                        .inline
                        .get_or_insert_default()
                        .enabled = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.update.debounce",
                    "Update Debounce"
                ),
                lt(
                    "settings_ui.page_data.description.the.delay.in.milliseconds.to.show.inline.diagnostics.after.the.last.diagnostic.update",
                    "The delay in milliseconds to show inline diagnostics after the last diagnostic update."
                ),
                "diagnostics.inline.update_debounce_ms",
                |settings_content| {
                    settings_content
                        .diagnostics
                        .as_ref()?
                        .inline
                        .as_ref()?
                        .update_debounce_ms
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .diagnostics
                        .get_or_insert_default()
                        .inline
                        .get_or_insert_default()
                        .update_debounce_ms = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.padding", "Padding"),
                lt(
                    "settings_ui.page_data.description.the.amount.of.padding.between.the.end.of.the.source.line.and.the.start.of.the.inline.diagnostic",
                    "The amount of padding between the end of the source line and the start of the inline diagnostic."
                ),
                "diagnostics.inline.padding",
                |settings_content| {
                    settings_content
                        .diagnostics
                        .as_ref()?
                        .inline
                        .as_ref()?
                        .padding
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .diagnostics
                        .get_or_insert_default()
                        .inline
                        .get_or_insert_default()
                        .padding = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.minimum.column",
                    "Minimum Column"
                ),
                lt(
                    "settings_ui.page_data.description.the.minimum.column.at.which.to.display.inline.diagnostics",
                    "The minimum column at which to display inline diagnostics."
                ),
                "diagnostics.inline.min_column",
                |settings_content| {
                    settings_content
                        .diagnostics
                        .as_ref()?
                        .inline
                        .as_ref()?
                        .min_column
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .diagnostics
                        .get_or_insert_default()
                        .inline
                        .get_or_insert_default()
                        .min_column = value;
                }
            ),
        ]
    }

    fn lsp_pull_diagnostics_section() -> [SettingsPageItem; 3] {
        [
            section_header!(
                "settings_ui.page_data.section.lsp.pull.diagnostics",
                "LSP Pull Diagnostics"
            ),
            setting_item!(
                lt("settings_ui.page_data.title.enabled", "Enabled"),
                lt(
                    "settings_ui.page_data.description.whether.to.pull.for.language.server.powered.diagnostics.or.not",
                    "Whether to pull for language server-powered diagnostics or not."
                ),
                "diagnostics.lsp_pull_diagnostics.enabled",
                |settings_content| {
                    settings_content
                        .diagnostics
                        .as_ref()?
                        .lsp_pull_diagnostics
                        .as_ref()?
                        .enabled
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .diagnostics
                        .get_or_insert_default()
                        .lsp_pull_diagnostics
                        .get_or_insert_default()
                        .enabled = value;
                }
            ),
            // todo(settings_ui): Needs unit
            setting_item!(
                lt("settings_ui.page_data.title.debounce", "Debounce"),
                lt(
                    "settings_ui.page_data.description.minimum.time.to.wait.before.pulling.diagnostics.from.the.language.server.s",
                    "Minimum time to wait before pulling diagnostics from the language server(s)."
                ),
                "diagnostics.lsp_pull_diagnostics.debounce_ms",
                |settings_content| {
                    settings_content
                        .diagnostics
                        .as_ref()?
                        .lsp_pull_diagnostics
                        .as_ref()?
                        .debounce_ms
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .diagnostics
                        .get_or_insert_default()
                        .lsp_pull_diagnostics
                        .get_or_insert_default()
                        .debounce_ms = value;
                }
            ),
        ]
    }

    fn lsp_highlights_section() -> [SettingsPageItem; 2] {
        [
            section_header!(
                "settings_ui.page_data.section.lsp.highlights",
                "LSP Highlights"
            ),
            setting_item!(
                lt("settings_ui.page_data.title.debounce", "Debounce"),
                lt(
                    "settings_ui.page_data.description.the.debounce.delay.before.querying.highlights.from.the.language",
                    "The debounce delay before querying highlights from the language."
                ),
                "lsp_highlight_debounce",
                |settings_content| { settings_content.editor.lsp_highlight_debounce.as_ref() },
                |settings_content, value, _| {
                    settings_content.editor.lsp_highlight_debounce = value;
                }
            ),
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
                        non_editor_language_settings_data()
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
            section_header!("settings_ui.page_data.section.search", "Search"),
            setting_item!(
                lt("settings_ui.page_data.title.whole.word", "Whole Word"),
                lt(
                    "settings_ui.page_data.description.search.for.whole.words.by.default",
                    "Search for whole words by default."
                ),
                "search.whole_word",
                |settings_content| { settings_content.editor.search.as_ref()?.whole_word.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .search
                        .get_or_insert_default()
                        .whole_word = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.case.sensitive",
                    "Case Sensitive"
                ),
                lt(
                    "settings_ui.page_data.description.search.case.sensitively.by.default",
                    "Search case-sensitively by default."
                ),
                "search.case_sensitive",
                |settings_content| {
                    settings_content
                        .editor
                        .search
                        .as_ref()?
                        .case_sensitive
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .search
                        .get_or_insert_default()
                        .case_sensitive = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.use.smartcase.search",
                    "Use Smartcase Search"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.automatically.enable.case.sensitive.search.based.on.the.search.query",
                    "Whether to automatically enable case-sensitive search based on the search query."
                ),
                "use_smartcase_search",
                |settings_content| settings_content.editor.use_smartcase_search.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.use_smartcase_search = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.include.ignored",
                    "Include Ignored"
                ),
                lt(
                    "settings_ui.page_data.description.include.ignored.files.in.search.results.by.default",
                    "Include ignored files in search results by default."
                ),
                "search.include_ignored",
                |settings_content| {
                    settings_content
                        .editor
                        .search
                        .as_ref()?
                        .include_ignored
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .search
                        .get_or_insert_default()
                        .include_ignored = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.regex", "Regex"),
                lt(
                    "settings_ui.page_data.description.use.regex.search.by.default",
                    "Use regex search by default."
                ),
                "search.regex",
                |settings_content| { settings_content.editor.search.as_ref()?.regex.as_ref() },
                |settings_content, value, _| {
                    settings_content.editor.search.get_or_insert_default().regex = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.search.wrap", "Search Wrap"),
                lt(
                    "settings_ui.page_data.description.whether.the.editor.search.results.will.loop",
                    "Whether the editor search results will loop."
                ),
                "search_wrap",
                |settings_content| settings_content.editor.search_wrap.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.search_wrap = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.center.on.match",
                    "Center on Match"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.center.the.current.match.in.the.editor",
                    "Whether to center the current match in the editor"
                ),
                "editor.search.center_on_match",
                |settings_content| {
                    settings_content
                        .editor
                        .search
                        .as_ref()
                        .and_then(|search| search.center_on_match.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .search
                        .get_or_insert_default()
                        .center_on_match = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.seed.search.query.from.cursor",
                    "Seed Search Query From Cursor"
                ),
                lt(
                    "settings_ui.page_data.description.when.to.populate.a.new.search.s.query.based.on.the.text.under.the.cursor",
                    "When to populate a new search's query based on the text under the cursor."
                ),
                "seed_search_query_from_cursor",
                |settings_content| {
                    settings_content
                        .editor
                        .seed_search_query_from_cursor
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content.editor.seed_search_query_from_cursor = value;
                }
            ),
        ]
    }

    fn file_finder_section() -> [SettingsPageItem; 6] {
        [
            section_header!("settings_ui.page_data.section.file.finder", "File Finder"),
            // todo: null by default
            setting_item!(
                lt(
                    "settings_ui.page_data.title.include.ignored.in.search",
                    "Include Ignored in Search"
                ),
                lt(
                    "settings_ui.page_data.description.use.gitignored.files.when.searching",
                    "Use gitignored files when searching."
                ),
                "file_finder.include_ignored",
                |settings_content| {
                    settings_content
                        .file_finder
                        .as_ref()?
                        .include_ignored
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .file_finder
                        .get_or_insert_default()
                        .include_ignored = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.file.icons", "File Icons"),
                lt(
                    "settings_ui.page_data.description.show.file.icons.in.the.file.finder",
                    "Show file icons in the file finder."
                ),
                "file_finder.file_icons",
                |settings_content| { settings_content.file_finder.as_ref()?.file_icons.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .file_finder
                        .get_or_insert_default()
                        .file_icons = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.modal.max.width",
                    "Modal Max Width"
                ),
                lt(
                    "settings_ui.page_data.description.determines.how.much.space.the.file.finder.can.take.up.in.relation.to.the.available.window.width",
                    "Determines how much space the file finder can take up in relation to the available window width."
                ),
                "file_finder.modal_max_width",
                |settings_content| {
                    settings_content
                        .file_finder
                        .as_ref()?
                        .modal_max_width
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .file_finder
                        .get_or_insert_default()
                        .modal_max_width = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.skip.focus.for.active.in.search",
                    "Skip Focus For Active In Search"
                ),
                lt(
                    "settings_ui.page_data.description.whether.the.file.finder.should.skip.focus.for.the.active.file.in.search.results",
                    "Whether the file finder should skip focus for the active file in search results."
                ),
                "file_finder.skip_focus_for_active_in_search",
                |settings_content| {
                    settings_content
                        .file_finder
                        .as_ref()?
                        .skip_focus_for_active_in_search
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .file_finder
                        .get_or_insert_default()
                        .skip_focus_for_active_in_search = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.prefill.query.from.selection",
                    "Prefill Query From Selection"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.pre.fill.the.file.finders.query.with.text.selected.in.the.focused.item",
                    "Whether to pre-fill the file finder's query with text selected in the focused item."
                ),
                "file_finder.prefill_query_from_selection",
                |settings_content| {
                    settings_content
                        .file_finder
                        .as_ref()?
                        .prefill_query_from_selection
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .file_finder
                        .get_or_insert_default()
                        .prefill_query_from_selection = value;
                }
            ),
        ]
    }

    fn file_scan_section() -> [SettingsPageItem; 5] {
        [
            section_header!("settings_ui.page_data.section.file.scan", "File Scan"),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.file.scan.exclusions",
                    "File Scan Exclusions"
                ),
                lt(
                    "settings_ui.page_data.description.files.or.globs.of.files.that.will.be.excluded.by.zzz.entirely.they.will.be.skipped.during.file.scans.file.searches.and.not.be.displayed.in.the.project.file.tree.takes.precedence.over.file.scan.inclusions",
                    "Files or globs of files that will be excluded by ZZZ entirely. They will be skipped during file scans, file searches, and not be displayed in the project file tree. Takes precedence over \\\"File Scan Inclusions\\\""
                ),
                "file_scan_exclusions",
                |settings_content| {
                    settings_content
                        .project
                        .worktree
                        .file_scan_exclusions
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content.project.worktree.file_scan_exclusions = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.file.scan.inclusions",
                    "File Scan Inclusions"
                ),
                lt(
                    "settings_ui.page_data.description.files.or.globs.of.files.that.will.be.included.by.zzz.even.when.ignored.by.git.this.is.useful.for.files.that.are.not.tracked.by.git.but.are.still.important.to.your.project.note.that.globs.that.are.overly.broad.can.slow.down.zzz.s.file.scanning.file.scan.exclusions.takes.precedence.over.these.inclusions",
                    "Files or globs of files that will be included by ZZZ, even when ignored by git. This is useful for files that are not tracked by git, but are still important to your project. Note that globs that are overly broad can slow down ZZZ's file scanning. \\\"File Scan Exclusions\\\" takes precedence over these inclusions"
                ),
                "file_scan_inclusions",
                |settings_content| {
                    settings_content
                        .project
                        .worktree
                        .file_scan_inclusions
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content.project.worktree.file_scan_inclusions = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.restore.file.state",
                    "Restore File State"
                ),
                lt(
                    "settings_ui.page_data.description.restore.previous.file.state.when.reopening",
                    "Restore previous file state when reopening."
                ),
                "restore_on_file_reopen",
                |settings_content| { settings_content.workspace.restore_on_file_reopen.as_ref() },
                |settings_content, value, _| {
                    settings_content.workspace.restore_on_file_reopen = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.close.on.file.delete",
                    "Close on File Delete"
                ),
                lt(
                    "settings_ui.page_data.description.automatically.close.files.that.have.been.deleted",
                    "Automatically close files that have been deleted."
                ),
                "close_on_file_delete",
                |settings_content| { settings_content.workspace.close_on_file_delete.as_ref() },
                |settings_content, value, _| {
                    settings_content.workspace.close_on_file_delete = value;
                }
            ),
        ]
    }

    SettingsPage {
        title: lt("settings_ui.page_data.title.search.files", "Search & Files"),
        items: concat_sections![search_section(), file_finder_section(), file_scan_section()],
    }
}

fn window_and_layout_page() -> SettingsPage {
    // Rows that switch on a variant discriminant (`DynamicItem`) stay literal: their pick/write
    // closures match on every variant, which `setting_item!` cannot express.
    fn status_bar_section() -> [SettingsPageItem; 12] {
        [
            section_header!("settings_ui.page_data.section.status.bar", "Status Bar"),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.project.panel.button",
                    "Project Panel Button"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.project.panel.button.in.the.status.bar",
                    "Show the project panel button in the status bar."
                ),
                "project_panel.button",
                |settings_content| { settings_content.project_panel.as_ref()?.button.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .button = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.active.language.button",
                    "Active Language Button"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.active.language.button.in.the.status.bar",
                    "Show the active language button in the status bar."
                ),
                "status_bar.active_language_button",
                |settings_content| {
                    settings_content
                        .status_bar
                        .as_ref()?
                        .active_language_button
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .status_bar
                        .get_or_insert_default()
                        .active_language_button = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.active.encoding.button",
                    "Active Encoding Button"
                ),
                lt(
                    "settings_ui.page_data.description.control.when.to.show.the.active.encoding.in.the.status.bar",
                    "Control when to show the active encoding in the status bar."
                ),
                "status_bar.active_encoding_button",
                |settings_content| {
                    settings_content
                        .status_bar
                        .as_ref()?
                        .active_encoding_button
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .status_bar
                        .get_or_insert_default()
                        .active_encoding_button = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.position", "Position"),
                lt(
                    "settings_ui.page_data.description.where.to.show.the.status.bar.in.the.workspace",
                    "Where to show the status bar in the workspace."
                ),
                "status_bar.position",
                |settings_content| { settings_content.status_bar.as_ref()?.position.as_ref() },
                |settings_content, value, _| {
                    settings_content.status_bar.get_or_insert_default().position = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.cursor.position.button",
                    "Cursor Position Button"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.cursor.position.button.in.the.status.bar",
                    "Show the cursor position button in the status bar."
                ),
                "status_bar.cursor_position_button",
                |settings_content| {
                    settings_content
                        .status_bar
                        .as_ref()?
                        .cursor_position_button
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .status_bar
                        .get_or_insert_default()
                        .cursor_position_button = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.line.endings.button",
                    "Line Endings Button"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.active.line.endings.button.in.the.status.bar",
                    "Show the active line endings button in the status bar."
                ),
                "status_bar.line_endings_button",
                |settings_content| {
                    settings_content
                        .status_bar
                        .as_ref()?
                        .line_endings_button
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .status_bar
                        .get_or_insert_default()
                        .line_endings_button = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.terminal.button",
                    "Terminal Button"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.terminal.button.in.the.status.bar",
                    "Show the terminal button in the status bar."
                ),
                "terminal.button",
                |settings_content| settings_content.terminal.as_ref()?.button.as_ref(),
                |settings_content, value, _| {
                    settings_content.terminal.get_or_insert_default().button = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.diagnostics.button",
                    "Diagnostics Button"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.project.diagnostics.button.in.the.status.bar",
                    "Show the project diagnostics button in the status bar."
                ),
                "diagnostics.button",
                |settings_content| settings_content.diagnostics.as_ref()?.button.as_ref(),
                |settings_content, value, _| {
                    settings_content.diagnostics.get_or_insert_default().button = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.project.search.button",
                    "Project Search Button"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.project.search.button.in.the.status.bar",
                    "Show the project search button in the status bar."
                ),
                "search.button",
                |settings_content| { settings_content.editor.search.as_ref()?.button.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .editor
                        .search
                        .get_or_insert_default()
                        .button = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.debugger.button",
                    "Debugger Button"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.debugger.button.in.the.status.bar",
                    "Show the debugger button in the status bar."
                ),
                "debugger.button",
                |settings_content| settings_content.debugger.as_ref()?.button.as_ref(),
                |settings_content, value, _| {
                    settings_content.debugger.get_or_insert_default().button = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.active.file.name",
                    "Active File Name"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.name.of.the.active.file.in.the.status.bar",
                    "Show the name of the active file in the status bar."
                ),
                "status_bar.show_active_file",
                |settings_content| {
                    settings_content
                        .status_bar
                        .as_ref()?
                        .show_active_file
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .status_bar
                        .get_or_insert_default()
                        .show_active_file = value;
                }
            ),
        ]
    }

    fn title_bar_section() -> [SettingsPageItem; 8] {
        [
            section_header!("settings_ui.page_data.section.title.bar", "Title Bar"),
            setting_item!(
                lt("settings_ui.page_data.title.show.branch.status.icon", "Show Branch Status Icon"),
                lt("settings_ui.page_data.description.show.git.status.indicators.on.the.branch.icon.in.the.titlebar", "Show git status indicators on the branch icon in the titlebar."),
                "title_bar.show_branch_status_icon",
                |settings_content| { settings_content.title_bar.as_ref()?.show_branch_status_icon.as_ref() },
                |settings_content, value, _| { settings_content.title_bar.get_or_insert_default().show_branch_status_icon = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.show.branch.name", "Show Branch Name"),
                lt("settings_ui.page_data.description.show.the.branch.name.button.in.the.titlebar", "Show the branch name button in the titlebar."),
                "title_bar.show_branch_name",
                |settings_content| { settings_content.title_bar.as_ref()?.show_branch_name.as_ref() },
                |settings_content, value, _| { settings_content.title_bar.get_or_insert_default().show_branch_name = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.show.project.items", "Show Project Items"),
                lt("settings_ui.page_data.description.show.the.project.host.and.name.in.the.titlebar", "Show the project host and name in the titlebar."),
                "title_bar.show_project_items",
                |settings_content| { settings_content.title_bar.as_ref()?.show_project_items.as_ref() },
                |settings_content, value, _| { settings_content.title_bar.get_or_insert_default().show_project_items = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.show.user.menu", "Show User Menu"),
                lt("settings_ui.page_data.description.show.the.user.menu.button.in.the.titlebar", "Show the user menu button in the titlebar."),
                "title_bar.show_user_menu",
                |settings_content| { settings_content.title_bar.as_ref()?.show_user_menu.as_ref() },
                |settings_content, value, _| { settings_content.title_bar.get_or_insert_default().show_user_menu = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.show.menus", "Show Menus"),
                lt("settings_ui.page_data.description.show.the.menus.in.the.titlebar", "Show the menus in the titlebar."),
                "title_bar.show_menus",
                |settings_content| { settings_content.title_bar.as_ref()?.show_menus.as_ref() },
                |settings_content, value, _| { settings_content.title_bar.get_or_insert_default().show_menus = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.open.menus.on.hover", "Open Menus on Hover"),
                lt("settings_ui.page_data.description.automatically.open.menus.in.the.titlebar.on.hover", "Automatically open menus in the titlebar on hover."),
                "title_bar.open_menus_on_hover",
                |settings_content| { settings_content.title_bar.as_ref()?.open_menus_on_hover.as_ref() },
                |settings_content, value, _| { settings_content.title_bar.get_or_insert_default().open_menus_on_hover = value; }),
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
            section_header!("settings_ui.page_data.section.tab.bar", "Tab Bar"),
            setting_item!(
                lt("settings_ui.page_data.title.show.tab.bar", "Show Tab Bar"),
                lt(
                    "settings_ui.page_data.description.show.the.tab.bar.in.the.editor",
                    "Show the tab bar in the editor."
                ),
                "tab_bar.show",
                |settings_content| settings_content.tab_bar.as_ref()?.show.as_ref(),
                |settings_content, value, _| {
                    settings_content.tab_bar.get_or_insert_default().show = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.git.status.in.tabs",
                    "Show Git Status In Tabs"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.git.file.status.on.a.tab.item",
                    "Show the Git file status on a tab item."
                ),
                "tabs.git_status",
                |settings_content| settings_content.tabs.as_ref()?.git_status.as_ref(),
                |settings_content, value, _| {
                    settings_content.tabs.get_or_insert_default().git_status = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.file.icons.in.tabs",
                    "Show File Icons In Tabs"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.file.icon.for.a.tab",
                    "Show the file icon for a tab."
                ),
                "tabs.file_icons",
                |settings_content| settings_content.tabs.as_ref()?.file_icons.as_ref(),
                |settings_content, value, _| {
                    settings_content.tabs.get_or_insert_default().file_icons = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.tab.close.position",
                    "Tab Close Position"
                ),
                lt(
                    "settings_ui.page_data.description.position.of.the.close.button.in.a.tab",
                    "Position of the close button in a tab."
                ),
                "tabs.close_position",
                |settings_content| { settings_content.tabs.as_ref()?.close_position.as_ref() },
                |settings_content, value, _| {
                    settings_content.tabs.get_or_insert_default().close_position = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.maximum.tabs", "Maximum Tabs"),
                lt(
                    "settings_ui.page_data.description.maximum.open.tabs.in.a.pane.will.not.close.an.unsaved.tab",
                    "Maximum open tabs in a pane. Will not close an unsaved tab.",
                ),
                // todo(settings_ui): The default for this value is null and it's use in code
                // is complex, so I'm going to come back to this later,
                "max_tabs",
                |settings_content| settings_content.workspace.max_tabs.as_ref(),
                |settings_content, value, _| {
                    settings_content.workspace.max_tabs = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.navigation.history.buttons",
                    "Show Navigation History Buttons"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.navigation.history.buttons.in.the.tab.bar",
                    "Show the navigation history buttons in the tab bar."
                ),
                "tab_bar.show_nav_history_buttons",
                |settings_content| {
                    settings_content
                        .tab_bar
                        .as_ref()?
                        .show_nav_history_buttons
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .tab_bar
                        .get_or_insert_default()
                        .show_nav_history_buttons = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.tab.bar.buttons",
                    "Show Tab Bar Buttons"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.tab.bar.buttons.new.split.pane.zoom",
                    "Show the tab bar buttons (New, Split Pane, Zoom)."
                ),
                "tab_bar.show_tab_bar_buttons",
                |settings_content| {
                    settings_content
                        .tab_bar
                        .as_ref()?
                        .show_tab_bar_buttons
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .tab_bar
                        .get_or_insert_default()
                        .show_tab_bar_buttons = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.pinned.tabs.layout",
                    "Pinned Tabs Layout"
                ),
                lt(
                    "settings_ui.page_data.description.show.pinned.tabs.in.a.separate.row.above.unpinned.tabs",
                    "Show pinned tabs in a separate row above unpinned tabs."
                ),
                "tab_bar.show_pinned_tabs_in_separate_row",
                |settings_content| {
                    settings_content
                        .tab_bar
                        .as_ref()?
                        .show_pinned_tabs_in_separate_row
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .tab_bar
                        .get_or_insert_default()
                        .show_pinned_tabs_in_separate_row = value;
                }
            ),
        ]
    }

    fn tab_settings_section() -> [SettingsPageItem; 5] {
        [
            section_header!("settings_ui.page_data.section.tab.settings", "Tab Settings"),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.activate.on.close",
                    "Activate On Close"
                ),
                lt(
                    "settings_ui.page_data.description.what.to.do.after.closing.the.current.tab",
                    "What to do after closing the current tab."
                ),
                "tabs.activate_on_close",
                |settings_content| { settings_content.tabs.as_ref()?.activate_on_close.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .tabs
                        .get_or_insert_default()
                        .activate_on_close = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.tab.show.diagnostics",
                    "Tab Show Diagnostics"
                ),
                lt(
                    "settings_ui.page_data.description.which.files.containing.diagnostic.errors.warnings.to.mark.in.the.tabs",
                    "Which files containing diagnostic errors/warnings to mark in the tabs."
                ),
                "tabs.show_diagnostics",
                |settings_content| { settings_content.tabs.as_ref()?.show_diagnostics.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .tabs
                        .get_or_insert_default()
                        .show_diagnostics = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.close.button",
                    "Show Close Button"
                ),
                lt(
                    "settings_ui.page_data.description.controls.the.appearance.behavior.of.the.tab.s.close.button",
                    "Controls the appearance behavior of the tab's close button."
                ),
                "tabs.show_close_button",
                |settings_content| { settings_content.tabs.as_ref()?.show_close_button.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .tabs
                        .get_or_insert_default()
                        .show_close_button = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.unsaved.indicator",
                    "Show Unsaved Indicator"
                ),
                lt(
                    "settings_ui.page_data.description.show.an.indicator.on.tabs.with.unsaved.changes",
                    "Show an indicator on tabs with unsaved changes."
                ),
                "tabs.show_unsaved_indicator",
                |settings_content| {
                    settings_content
                        .tabs
                        .as_ref()?
                        .show_unsaved_indicator
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .tabs
                        .get_or_insert_default()
                        .show_unsaved_indicator = value;
                }
            ),
        ]
    }

    fn preview_tabs_section() -> [SettingsPageItem; 8] {
        [
            section_header!("settings_ui.page_data.section.preview.tabs", "Preview Tabs"),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.preview.tabs.enabled",
                    "Preview Tabs Enabled"
                ),
                lt(
                    "settings_ui.page_data.description.show.opened.editors.as.preview.tabs",
                    "Show opened editors as preview tabs."
                ),
                "preview_tabs.enabled",
                |settings_content| { settings_content.preview_tabs.as_ref()?.enabled.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .preview_tabs
                        .get_or_insert_default()
                        .enabled = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.enable.preview.from.project.panel",
                    "Enable Preview From Project Panel"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.open.tabs.in.preview.mode.when.opened.from.the.project.panel.with.a.single.click",
                    "Whether to open tabs in preview mode when opened from the project panel with a single click or the Open action."
                ),
                "preview_tabs.enable_preview_from_project_panel",
                |settings_content| {
                    settings_content
                        .preview_tabs
                        .as_ref()?
                        .enable_preview_from_project_panel
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .preview_tabs
                        .get_or_insert_default()
                        .enable_preview_from_project_panel = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.enable.preview.from.file.finder",
                    "Enable Preview From File Finder"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.open.tabs.in.preview.mode.when.selected.from.the.file.finder",
                    "Whether to open tabs in preview mode when selected from the file finder."
                ),
                "preview_tabs.enable_preview_from_file_finder",
                |settings_content| {
                    settings_content
                        .preview_tabs
                        .as_ref()?
                        .enable_preview_from_file_finder
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .preview_tabs
                        .get_or_insert_default()
                        .enable_preview_from_file_finder = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.enable.preview.from.multibuffer",
                    "Enable Preview From Multibuffer"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.open.tabs.in.preview.mode.when.opened.from.a.multibuffer",
                    "Whether to open tabs in preview mode when opened from a multibuffer."
                ),
                "preview_tabs.enable_preview_from_multibuffer",
                |settings_content| {
                    settings_content
                        .preview_tabs
                        .as_ref()?
                        .enable_preview_from_multibuffer
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .preview_tabs
                        .get_or_insert_default()
                        .enable_preview_from_multibuffer = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.enable.preview.multibuffer.from.code.navigation",
                    "Enable Preview Multibuffer From Code Navigation"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.open.tabs.in.preview.mode.when.code.navigation.is.used.to.open.a.multibuffer",
                    "Whether to open tabs in preview mode when code navigation is used to open a multibuffer."
                ),
                "preview_tabs.enable_preview_multibuffer_from_code_navigation",
                |settings_content| {
                    settings_content
                        .preview_tabs
                        .as_ref()?
                        .enable_preview_multibuffer_from_code_navigation
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .preview_tabs
                        .get_or_insert_default()
                        .enable_preview_multibuffer_from_code_navigation = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.enable.preview.file.from.code.navigation",
                    "Enable Preview File From Code Navigation"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.open.tabs.in.preview.mode.when.code.navigation.is.used.to.open.a.single.file",
                    "Whether to open tabs in preview mode when code navigation is used to open a single file."
                ),
                "preview_tabs.enable_preview_file_from_code_navigation",
                |settings_content| {
                    settings_content
                        .preview_tabs
                        .as_ref()?
                        .enable_preview_file_from_code_navigation
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .preview_tabs
                        .get_or_insert_default()
                        .enable_preview_file_from_code_navigation = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.enable.keep.preview.on.code.navigation",
                    "Enable Keep Preview On Code Navigation"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.keep.tabs.in.preview.mode.when.code.navigation.is.used.to.navigate.away.from.them.if.enable.preview.file.from.code.navigation.or.enable.preview.multibuffer.from.code.navigation.is.also.true.the.new.tab.may.replace.the.existing.one",
                    "Whether to keep tabs in preview mode when code navigation is used to navigate away from them. If `enable_preview_file_from_code_navigation` or `enable_preview_multibuffer_from_code_navigation` is also true, the new tab may replace the existing one."
                ),
                "preview_tabs.enable_keep_preview_on_code_navigation",
                |settings_content| {
                    settings_content
                        .preview_tabs
                        .as_ref()?
                        .enable_keep_preview_on_code_navigation
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .preview_tabs
                        .get_or_insert_default()
                        .enable_keep_preview_on_code_navigation = value;
                }
            ),
        ]
    }

    fn layout_section() -> [SettingsPageItem; 6] {
        [
            section_header!("settings_ui.page_data.section.layout", "Layout"),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.bottom.dock.layout",
                    "Bottom Dock Layout"
                ),
                lt(
                    "settings_ui.page_data.description.layout.mode.for.the.bottom.dock",
                    "Layout mode for the bottom dock."
                ),
                "bottom_dock_layout",
                |settings_content| settings_content.workspace.bottom_dock_layout.as_ref(),
                |settings_content, value, _| {
                    settings_content.workspace.bottom_dock_layout = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.centered.layout.left.padding",
                    "Centered Layout Left Padding"
                ),
                lt(
                    "settings_ui.page_data.description.left.padding.for.centered.layout",
                    "Left padding for centered layout."
                ),
                "centered_layout.left_padding",
                |settings_content| {
                    settings_content
                        .workspace
                        .centered_layout
                        .as_ref()?
                        .left_padding
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .workspace
                        .centered_layout
                        .get_or_insert_default()
                        .left_padding = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.centered.layout.right.padding",
                    "Centered Layout Right Padding"
                ),
                lt(
                    "settings_ui.page_data.description.right.padding.for.centered.layout",
                    "Right padding for centered layout."
                ),
                "centered_layout.right_padding",
                |settings_content| {
                    settings_content
                        .workspace
                        .centered_layout
                        .as_ref()?
                        .right_padding
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .workspace
                        .centered_layout
                        .get_or_insert_default()
                        .right_padding = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.focus.follows.mouse",
                    "Focus Follows Mouse"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.change.focus.to.a.pane.when.the.mouse.hovers.over.it",
                    "Whether to change focus to a pane when the mouse hovers over it."
                ),
                "focus_follows_mouse.enabled",
                |settings_content| {
                    settings_content
                        .workspace
                        .focus_follows_mouse
                        .as_ref()
                        .and_then(|s| s.enabled.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .workspace
                        .focus_follows_mouse
                        .get_or_insert_default()
                        .enabled = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.focus.follows.mouse.debounce.ms",
                    "Focus Follows Mouse Debounce ms"
                ),
                lt(
                    "settings_ui.page_data.description.amount.of.time.to.wait.before.changing.focus",
                    "Amount of time to wait before changing focus."
                ),
                "focus_follows_mouse.debounce_ms",
                |settings_content| {
                    settings_content
                        .workspace
                        .focus_follows_mouse
                        .as_ref()
                        .and_then(|s| s.debounce_ms.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .workspace
                        .focus_follows_mouse
                        .get_or_insert_default()
                        .debounce_ms = value;
                }
            ),
        ]
    }

    fn window_section() -> [SettingsPageItem; 3] {
        [
            section_header!("settings_ui.page_data.section.window", "Window"),
            // todo(settings_ui): Should we filter by platform.as_ref()?
            setting_item!(
                lt(
                    "settings_ui.page_data.title.use.system.window.tabs",
                    "Use System Window Tabs"
                ),
                lt(
                    "settings_ui.page_data.description.macos.only.whether.to.allow.windows.to.tab.together",
                    "(macOS only) whether to allow Windows to tab together."
                ),
                "use_system_window_tabs",
                |settings_content| { settings_content.workspace.use_system_window_tabs.as_ref() },
                |settings_content, value, _| {
                    settings_content.workspace.use_system_window_tabs = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.window.decorations",
                    "Window Decorations"
                ),
                lt(
                    "settings_ui.page_data.description.linux.only.whether.zzz.or.your.compositor.should.draw.window.decorations",
                    "(Linux only) whether ZZZ or your compositor should draw window decorations."
                ),
                "window_decorations",
                |settings_content| settings_content.workspace.window_decorations.as_ref(),
                |settings_content, value, _| {
                    settings_content.workspace.window_decorations = value;
                }
            ),
        ]
    }

    fn pane_modifiers_section() -> [SettingsPageItem; 5] {
        [
            section_header!(
                "settings_ui.page_data.section.pane.modifiers",
                "Pane Modifiers"
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.inactive.opacity",
                    "Inactive Opacity"
                ),
                lt(
                    "settings_ui.page_data.description.opacity.of.inactive.panels.0.0.1.0",
                    "Opacity of inactive panels (0.0 - 1.0)."
                ),
                "active_pane_modifiers.inactive_opacity",
                |settings_content| {
                    settings_content
                        .workspace
                        .active_pane_modifiers
                        .as_ref()?
                        .inactive_opacity
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .workspace
                        .active_pane_modifiers
                        .get_or_insert_default()
                        .inactive_opacity = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.border.size", "Border Size"),
                lt(
                    "settings_ui.page_data.description.size.of.the.border.surrounding.the.active.pane",
                    "Size of the border surrounding the active pane."
                ),
                "active_pane_modifiers.border_size",
                |settings_content| {
                    settings_content
                        .workspace
                        .active_pane_modifiers
                        .as_ref()?
                        .border_size
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .workspace
                        .active_pane_modifiers
                        .get_or_insert_default()
                        .border_size = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.zoomed.padding",
                    "Zoomed Padding"
                ),
                lt(
                    "settings_ui.page_data.description.show.padding.for.zoomed.panes",
                    "Show padding for zoomed panes."
                ),
                "zoomed_padding",
                |settings_content| settings_content.workspace.zoomed_padding.as_ref(),
                |settings_content, value, _| {
                    settings_content.workspace.zoomed_padding = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.close.panel.on.toggle",
                    "Close Panel on Toggle"
                ),
                lt(
                    "settings_ui.page_data.description.whether.invoking.a.panel.s.togglefocus.action.while.it.s.already.focused.closes.the.panel.instead.of.just.moving.focus.back.to.the.editor",
                    "Whether invoking a panel's ToggleFocus action while it's already focused closes the panel, instead of just moving focus back to the editor."
                ),
                "close_panel_on_toggle",
                |settings_content| { settings_content.workspace.close_panel_on_toggle.as_ref() },
                |settings_content, value, _| {
                    settings_content.workspace.close_panel_on_toggle = value;
                }
            ),
        ]
    }

    fn pane_split_direction_section() -> [SettingsPageItem; 3] {
        [
            section_header!(
                "settings_ui.page_data.section.pane.split.direction",
                "Pane Split Direction"
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.vertical.split.direction",
                    "Vertical Split Direction"
                ),
                lt(
                    "settings_ui.page_data.description.direction.to.split.vertically",
                    "Direction to split vertically."
                ),
                "pane_split_direction_vertical",
                |settings_content| {
                    settings_content
                        .workspace
                        .pane_split_direction_vertical
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content.workspace.pane_split_direction_vertical = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.horizontal.split.direction",
                    "Horizontal Split Direction"
                ),
                lt(
                    "settings_ui.page_data.description.direction.to.split.horizontally",
                    "Direction to split horizontally."
                ),
                "pane_split_direction_horizontal",
                |settings_content| {
                    settings_content
                        .workspace
                        .pane_split_direction_horizontal
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content.workspace.pane_split_direction_horizontal = value;
                }
            ),
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
    // Rows that switch on a variant discriminant (`DynamicItem`) stay literal: their pick/write
    // closures match on every variant, which `setting_item!` cannot express.
    fn project_panel_section() -> [SettingsPageItem; 32] {
        [
            section_header!(
                "settings_ui.page_data.section.project.panel",
                "Project Panel"
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.project.panel.dock",
                    "Project Panel Dock"
                ),
                lt(
                    "settings_ui.page_data.description.where.to.dock.the.project.panel",
                    "Where to dock the project panel."
                ),
                "project_panel.dock",
                |settings_content| settings_content.project_panel.as_ref()?.dock.as_ref(),
                |settings_content, value, _| {
                    settings_content.project_panel.get_or_insert_default().dock = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.project.panel.title.tooltip.delay",
                    "Path Tooltip Delay"
                ),
                lt(
                    "settings_ui.page_data.description.delay.before.showing.a.path.tooltip",
                    "Delay before showing a path tooltip when hovering a project panel entry."
                ),
                "project_panel.title_tooltip_delay",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .title_tooltip_delay
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .title_tooltip_delay = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.project.panel.default.width",
                    "Project Panel Default Width"
                ),
                lt(
                    "settings_ui.page_data.description.default.width.of.the.project.panel.in.pixels",
                    "Default width of the project panel in pixels."
                ),
                "project_panel.default_width",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .default_width
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .default_width = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.hide.gitignore",
                    "Hide .gitignore"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.hide.the.gitignore.entries.in.the.project.panel",
                    "Whether to hide the gitignore entries in the project panel."
                ),
                "project_panel.hide_gitignore",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .hide_gitignore
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .hide_gitignore = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.entry.spacing", "Entry Spacing"),
                lt(
                    "settings_ui.page_data.description.spacing.between.worktree.entries.in.the.project.panel",
                    "Spacing between worktree entries in the project panel."
                ),
                "project_panel.entry_spacing",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .entry_spacing
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .entry_spacing = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.file.icons", "File Icons"),
                lt(
                    "settings_ui.page_data.description.show.file.icons.in.the.project.panel",
                    "Show file icons in the project panel."
                ),
                "project_panel.file_icons",
                |settings_content| { settings_content.project_panel.as_ref()?.file_icons.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .file_icons = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.folder.icons", "Folder Icons"),
                lt(
                    "settings_ui.page_data.description.whether.to.show.folder.icons.or.chevrons.for.directories.in.the.project.panel",
                    "Whether to show folder icons or chevrons for directories in the project panel."
                ),
                "project_panel.folder_icons",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .folder_icons
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .folder_icons = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.git.status", "Git Status"),
                lt(
                    "settings_ui.page_data.description.show.the.git.status.in.the.project.panel",
                    "Show the Git status in the project panel."
                ),
                "project_panel.git_status",
                |settings_content| { settings_content.project_panel.as_ref()?.git_status.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .git_status = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.indent.size", "Indent Size"),
                lt(
                    "settings_ui.page_data.description.amount.of.indentation.for.nested.items",
                    "Amount of indentation for nested items."
                ),
                "project_panel.indent_size",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .indent_size
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .indent_size = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.auto.reveal.entries",
                    "Auto Reveal Entries"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.reveal.entries.in.the.project.panel.automatically.when.a.corresponding.project.entry.becomes.active",
                    "Whether to reveal entries in the project panel automatically when a corresponding project entry becomes active."
                ),
                "project_panel.auto_reveal_entries",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .auto_reveal_entries
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .auto_reveal_entries = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.starts.open", "Starts Open"),
                lt(
                    "settings_ui.page_data.description.whether.the.project.panel.should.open.on.startup",
                    "Whether the project panel should open on startup."
                ),
                "project_panel.starts_open",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .starts_open
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .starts_open = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.auto.fold.directories",
                    "Auto Fold Directories"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.fold.directories.automatically.and.show.compact.folders.when.a.directory.has.only.one.subdirectory.inside",
                    "Whether to fold directories automatically and show compact folders when a directory has only one subdirectory inside."
                ),
                "project_panel.auto_fold_dirs",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .auto_fold_dirs
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .auto_fold_dirs = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.bold.folder.labels",
                    "Bold Folder Labels"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.show.folder.names.with.bold.text.in.the.project.panel",
                    "Whether to show folder names with bold text in the project panel."
                ),
                "project_panel.bold_folder_labels",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .bold_folder_labels
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .bold_folder_labels = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.scrollbar",
                    "Show Scrollbar"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.scrollbar.in.the.project.panel",
                    "Show the scrollbar in the project panel."
                ),
                "project_panel.scrollbar.show",
                |settings_content| {
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
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .scrollbar
                        .get_or_insert_default()
                        .show = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.horizontal.scroll",
                    "Horizontal Scroll"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.allow.horizontal.scrolling.in.the.project.panel.when.disabled.the.view.is.always.locked.to.the.leftmost.position.and.long.file.names.are.clipped",
                    "Whether to allow horizontal scrolling in the project panel. When disabled, the view is always locked to the leftmost position and long file names are clipped."
                ),
                "project_panel.scrollbar.horizontal_scroll",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .scrollbar
                        .as_ref()?
                        .horizontal_scroll
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .scrollbar
                        .get_or_insert_default()
                        .horizontal_scroll = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.diagnostics",
                    "Show Diagnostics"
                ),
                lt(
                    "settings_ui.page_data.description.which.files.containing.diagnostic.errors.warnings.to.mark.in.the.project.panel",
                    "Which files containing diagnostic errors/warnings to mark in the project panel."
                ),
                "project_panel.show_diagnostics",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .show_diagnostics
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .show_diagnostics = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.diagnostic.badges",
                    "Diagnostic Badges"
                ),
                lt(
                    "settings_ui.page_data.description.show.error.and.warning.count.badges.next.to.file.names.in.the.project.panel",
                    "Show error and warning count badges next to file names in the project panel."
                ),
                "project_panel.diagnostic_badges",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .diagnostic_badges
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .diagnostic_badges = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.git.status.indicator",
                    "Git Status Indicator"
                ),
                lt(
                    "settings_ui.page_data.description.show.a.git.status.indicator.next.to.file.names.in.the.project.panel",
                    "Show a git status indicator next to file names in the project panel."
                ),
                "project_panel.git_status_indicator",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .git_status_indicator
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .git_status_indicator = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.sticky.scroll", "Sticky Scroll"),
                lt(
                    "settings_ui.page_data.description.whether.to.stick.parent.directories.at.top.of.the.project.panel",
                    "Whether to stick parent directories at top of the project panel."
                ),
                "project_panel.sticky_scroll",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .sticky_scroll
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .sticky_scroll = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.indent.guides",
                    "Show Indent Guides"
                ),
                lt(
                    "settings_ui.page_data.description.show.indent.guides.in.the.project.panel",
                    "Show indent guides in the project panel."
                ),
                "project_panel.indent_guides.show",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .indent_guides
                        .as_ref()?
                        .show
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .indent_guides
                        .get_or_insert_default()
                        .show = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.drag.and.drop", "Drag and Drop"),
                lt(
                    "settings_ui.page_data.description.whether.to.enable.drag.and.drop.operations.in.the.project.panel",
                    "Whether to enable drag-and-drop operations in the project panel."
                ),
                "project_panel.drag_and_drop",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .drag_and_drop
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .drag_and_drop = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.hide.root", "Hide Root"),
                lt(
                    "settings_ui.page_data.description.whether.to.hide.the.root.entry.when.only.one.folder.is.open.in.the.window",
                    "Whether to hide the root entry when only one folder is open in the window."
                ),
                "project_panel.hide_root",
                |settings_content| { settings_content.project_panel.as_ref()?.hide_root.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .hide_root = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.hide.hidden", "Hide Hidden"),
                lt(
                    "settings_ui.page_data.description.whether.to.hide.the.hidden.entries.in.the.project.panel",
                    "Whether to hide the hidden entries in the project panel."
                ),
                "project_panel.hide_hidden",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .hide_hidden
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .hide_hidden = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.sort.mode", "Sort Mode"),
                lt(
                    "settings_ui.page_data.description.sort.order.for.entries.in.the.project.panel",
                    "Sort order for entries in the project panel."
                ),
                "project_panel.sort_mode",
                |settings_content| { settings_content.project_panel.as_ref()?.sort_mode.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .sort_mode = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.sort.order", "Sort Order"),
                lt(
                    "settings_ui.page_data.description.whether.to.sort.file.and.folder.names.case.sensitively.in.the.project.panel",
                    "Whether to sort file and folder names case-sensitively in the project panel."
                ),
                "project_panel.sort_order",
                |settings_content| { settings_content.project_panel.as_ref()?.sort_order.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .sort_order = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.auto.open.files.on.create",
                    "Auto Open Files On Create"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.automatically.open.newly.created.files.in.the.editor",
                    "Whether to automatically open newly created files in the editor."
                ),
                "project_panel.auto_open.on_create",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .auto_open
                        .as_ref()?
                        .on_create
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .auto_open
                        .get_or_insert_default()
                        .on_create = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.auto.open.files.on.paste",
                    "Auto Open Files On Paste"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.automatically.open.files.after.pasting.or.duplicating.them",
                    "Whether to automatically open files after pasting or duplicating them."
                ),
                "project_panel.auto_open.on_paste",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .auto_open
                        .as_ref()?
                        .on_paste
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .auto_open
                        .get_or_insert_default()
                        .on_paste = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.auto.open.files.on.drop",
                    "Auto Open Files On Drop"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.automatically.open.files.dropped.from.external.sources",
                    "Whether to automatically open files dropped from external sources."
                ),
                "project_panel.auto_open.on_drop",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .auto_open
                        .as_ref()?
                        .on_drop
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .auto_open
                        .get_or_insert_default()
                        .on_drop = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.should.focus", "Should Focus"),
                lt(
                    "settings_ui.page_data.description.whether.to.focus.on.files.automatically.opened",
                    "Whether to focus on files automatically opened."
                ),
                "project_panel.auto_open.should_focus",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .auto_open
                        .as_ref()?
                        .should_focus
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .auto_open
                        .get_or_insert_default()
                        .should_focus = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.should.focus", "Should Focus"),
                lt(
                    "settings_ui.page_data.description.whether.to.focus.on.files.automatically.opened",
                    "Whether to focus on files automatically opened."
                ),
                "project_panel.auto_open.should_focus",
                |settings_content| {
                    settings_content
                        .project_panel
                        .as_ref()?
                        .auto_open
                        .as_ref()?
                        .should_focus
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .project_panel
                        .get_or_insert_default()
                        .auto_open
                        .get_or_insert_default()
                        .should_focus = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.hidden.files", "Hidden Files"),
                lt(
                    "settings_ui.page_data.description.globs.to.match.files.that.will.be.considered.hidden.and.can.be.hidden.from.the.project.panel",
                    "Globs to match files that will be considered \\\"hidden\\\" and can be hidden from the project panel."
                ),
                "worktree.hidden_files",
                |settings_content| { settings_content.project.worktree.hidden_files.as_ref() },
                |settings_content, value, _| {
                    settings_content.project.worktree.hidden_files = value;
                }
            ),
        ]
    }

    fn terminal_panel_section() -> [SettingsPageItem; 4] {
        [
            section_header!(
                "settings_ui.page_data.section.terminal.panel",
                "Terminal Panel"
            ),
            setting_item!(
                lt("settings_ui.page_data.title.terminal.dock", "Terminal Dock"),
                lt(
                    "settings_ui.page_data.description.where.to.dock.the.terminal.panel",
                    "Where to dock the terminal panel."
                ),
                "terminal.dock",
                |settings_content| settings_content.terminal.as_ref()?.dock.as_ref(),
                |settings_content, value, _| {
                    settings_content.terminal.get_or_insert_default().dock = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.terminal.panel.flexible.sizing",
                    "Terminal Panel Flexible Sizing"
                ),
                lt(
                    "settings_ui.page_data.description.whether.the.terminal.panel.should.use.flexible.proportional.sizing.when.docked.to.the.left.or.right",
                    "Whether the terminal panel should use flexible (proportional) sizing when docked to the left or right."
                ),
                "terminal.flexible",
                |settings_content| settings_content.terminal.as_ref()?.flexible.as_ref(),
                |settings_content, value, _| {
                    settings_content.terminal.get_or_insert_default().flexible = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.count.badge",
                    "Show Count Badge"
                ),
                lt(
                    "settings_ui.page_data.description.show.a.badge.on.the.terminal.panel.icon.with.the.count.of.open.terminals",
                    "Show a badge on the terminal panel icon with the count of open terminals."
                ),
                "terminal.show_count_badge",
                |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()?
                        .show_count_badge
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .show_count_badge = value;
                }
            ),
        ]
    }

    fn outline_panel_section() -> [SettingsPageItem; 11] {
        [
            section_header!(
                "settings_ui.page_data.section.outline.panel",
                "Outline Panel"
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.outline.panel.button",
                    "Outline Panel Button"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.outline.panel.button.in.the.status.bar",
                    "Show the outline panel button in the status bar."
                ),
                "outline_panel.button",
                |settings_content| { settings_content.outline_panel.as_ref()?.button.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .button = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.outline.panel.dock",
                    "Outline Panel Dock"
                ),
                lt(
                    "settings_ui.page_data.description.where.to.dock.the.outline.panel",
                    "Where to dock the outline panel."
                ),
                "outline_panel.dock",
                |settings_content| settings_content.outline_panel.as_ref()?.dock.as_ref(),
                |settings_content, value, _| {
                    settings_content.outline_panel.get_or_insert_default().dock = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.outline.panel.default.width",
                    "Outline Panel Default Width"
                ),
                lt(
                    "settings_ui.page_data.description.default.width.of.the.outline.panel.in.pixels",
                    "Default width of the outline panel in pixels."
                ),
                "outline_panel.default_width",
                |settings_content| {
                    settings_content
                        .outline_panel
                        .as_ref()?
                        .default_width
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .default_width = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.file.icons", "File Icons"),
                lt(
                    "settings_ui.page_data.description.show.file.icons.in.the.outline.panel",
                    "Show file icons in the outline panel."
                ),
                "outline_panel.file_icons",
                |settings_content| { settings_content.outline_panel.as_ref()?.file_icons.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .file_icons = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.folder.icons", "Folder Icons"),
                lt(
                    "settings_ui.page_data.description.whether.to.show.folder.icons.or.chevrons.for.directories.in.the.outline.panel",
                    "Whether to show folder icons or chevrons for directories in the outline panel."
                ),
                "outline_panel.folder_icons",
                |settings_content| {
                    settings_content
                        .outline_panel
                        .as_ref()?
                        .folder_icons
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .folder_icons = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.git.status", "Git Status"),
                lt(
                    "settings_ui.page_data.description.show.the.git.status.in.the.outline.panel",
                    "Show the Git status in the outline panel."
                ),
                "outline_panel.git_status",
                |settings_content| { settings_content.outline_panel.as_ref()?.git_status.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .git_status = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.indent.size", "Indent Size"),
                lt(
                    "settings_ui.page_data.description.amount.of.indentation.for.nested.items",
                    "Amount of indentation for nested items."
                ),
                "outline_panel.indent_size",
                |settings_content| {
                    settings_content
                        .outline_panel
                        .as_ref()?
                        .indent_size
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .indent_size = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.auto.reveal.entries",
                    "Auto Reveal Entries"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.reveal.when.a.corresponding.outline.entry.becomes.active",
                    "Whether to reveal when a corresponding outline entry becomes active."
                ),
                "outline_panel.auto_reveal_entries",
                |settings_content| {
                    settings_content
                        .outline_panel
                        .as_ref()?
                        .auto_reveal_entries
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .auto_reveal_entries = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.auto.fold.directories",
                    "Auto Fold Directories"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.fold.directories.automatically.when.a.directory.contains.only.one.subdirectory",
                    "Whether to fold directories automatically when a directory contains only one subdirectory."
                ),
                "outline_panel.auto_fold_dirs",
                |settings_content| {
                    settings_content
                        .outline_panel
                        .as_ref()?
                        .auto_fold_dirs
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .auto_fold_dirs = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.indent.guides",
                    "Show Indent Guides"
                ),
                lt(
                    "settings_ui.page_data.description.when.to.show.indent.guides.in.the.outline.panel",
                    "When to show indent guides in the outline panel."
                ),
                "outline_panel.indent_guides.show",
                |settings_content| {
                    settings_content
                        .outline_panel
                        .as_ref()?
                        .indent_guides
                        .as_ref()?
                        .show
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .outline_panel
                        .get_or_insert_default()
                        .indent_guides
                        .get_or_insert_default()
                        .show = value;
                }
            ),
        ]
    }

    fn git_panel_section() -> [SettingsPageItem; 17] {
        [
            section_header!("settings_ui.page_data.section.git.panel", "Git Panel"),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.git.panel.button",
                    "Git Panel Button"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.git.panel.button.in.the.status.bar",
                    "Show the Git panel button in the status bar."
                ),
                "git_panel.button",
                |settings_content| settings_content.git_panel.as_ref()?.button.as_ref(),
                |settings_content, value, _| {
                    settings_content.git_panel.get_or_insert_default().button = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.git.panel.dock",
                    "Git Panel Dock"
                ),
                lt(
                    "settings_ui.page_data.description.where.to.dock.the.git.panel",
                    "Where to dock the Git panel."
                ),
                "git_panel.dock",
                |settings_content| settings_content.git_panel.as_ref()?.dock.as_ref(),
                |settings_content, value, _| {
                    settings_content.git_panel.get_or_insert_default().dock = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.git.panel.default.width",
                    "Git Panel Default Width"
                ),
                lt(
                    "settings_ui.page_data.description.default.width.of.the.git.panel.in.pixels",
                    "Default width of the Git panel in pixels."
                ),
                "git_panel.default_width",
                |settings_content| { settings_content.git_panel.as_ref()?.default_width.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .default_width = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.git.panel.status.style",
                    "Git Panel Status Style"
                ),
                lt(
                    "settings_ui.page_data.description.how.entry.statuses.are.displayed",
                    "How entry statuses are displayed."
                ),
                "git_panel.status_style",
                |settings_content| { settings_content.git_panel.as_ref()?.status_style.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .status_style = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.fallback.branch.name",
                    "Fallback Branch Name"
                ),
                lt(
                    "settings_ui.page_data.description.default.branch.name.will.be.when.init.defaultbranch.is.not.set.in.git",
                    "Default branch name will be when init.defaultbranch is not set in Git."
                ),
                "git_panel.fallback_branch_name",
                |settings_content| {
                    settings_content
                        .git_panel
                        .as_ref()?
                        .fallback_branch_name
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .fallback_branch_name = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.sort.by.path", "Sort By Path"),
                lt(
                    "settings_ui.page_data.description.enable.to.sort.entries.in.the.panel.by.path.disable.to.sort.by.status",
                    "Enable to sort entries in the panel by path, disable to sort by status."
                ),
                "git_panel.sort_by_path",
                |settings_content| { settings_content.git_panel.as_ref()?.sort_by_path.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .sort_by_path = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.collapse.untracked.diff",
                    "Collapse Untracked Diff"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.collapse.untracked.files.in.the.diff.panel",
                    "Whether to collapse untracked files in the diff panel."
                ),
                "git_panel.collapse_untracked_diff",
                |settings_content| {
                    settings_content
                        .git_panel
                        .as_ref()?
                        .collapse_untracked_diff
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .collapse_untracked_diff = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.tree.view", "Tree View"),
                lt(
                    "settings_ui.page_data.description.enable.to.show.entries.in.tree.view.list.disable.to.show.in.flat.view.list",
                    "Enable to show entries in tree view list, disable to show in flat view list."
                ),
                "git_panel.tree_view",
                |settings_content| { settings_content.git_panel.as_ref()?.tree_view.as_ref() },
                |settings_content, value, _| {
                    settings_content.git_panel.get_or_insert_default().tree_view = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.file.icons", "File Icons"),
                lt(
                    "settings_ui.page_data.description.show.file.icons.next.to.the.git.status.icon",
                    "Show file icons next to the Git status icon."
                ),
                "git_panel.file_icons",
                |settings_content| { settings_content.git_panel.as_ref()?.file_icons.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .file_icons = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.folder.icons", "Folder Icons"),
                lt(
                    "settings_ui.page_data.description.whether.to.show.folder.icons.or.chevrons.for.directories.in.the.git.panel",
                    "Whether to show folder icons or chevrons for directories in the git panel."
                ),
                "git_panel.folder_icons",
                |settings_content| { settings_content.git_panel.as_ref()?.folder_icons.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .folder_icons = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.diff.stats", "Diff Stats"),
                lt(
                    "settings_ui.page_data.description.whether.to.show.the.addition.deletion.change.count.next.to.each.file.in.the.git.panel",
                    "Whether to show the addition/deletion change count next to each file in the Git panel."
                ),
                "git_panel.diff_stats",
                |settings_content| { settings_content.git_panel.as_ref()?.diff_stats.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .diff_stats = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.primary.click.behavior",
                    "Primary Click Behavior"
                ),
                lt(
                    "settings_ui.page_data.description.default.action.when.clicking.a.changed.file.in.the.git.panel",
                    "Default action when clicking a changed file in the Git panel."
                ),
                "git_panel.entry_primary_click_action",
                |settings_content| {
                    settings_content
                        .git_panel
                        .as_ref()?
                        .entry_primary_click_action
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .entry_primary_click_action = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.commit.editor", "Commit Editor"),
                lt(
                    "settings_ui.page_data.description.whether.the.commit.message.editor.is.shown.in.the.git.panel.by.default",
                    "Whether the commit message editor is shown in the Git panel by default."
                ),
                "git_panel.commit_editor",
                |settings_content| { settings_content.git_panel.as_ref()?.commit_editor.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .commit_editor = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.count.badge",
                    "Show Count Badge"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.show.a.badge.on.the.git.panel.icon.with.the.count.of.uncommitted.changes",
                    "Whether to show a badge on the git panel icon with the count of uncommitted changes."
                ),
                "git_panel.show_count_badge",
                |settings_content| {
                    settings_content
                        .git_panel
                        .as_ref()?
                        .show_count_badge
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .show_count_badge = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.commit.title.max.length",
                    "Commit Title Max Length"
                ),
                lt(
                    "settings_ui.page_data.description.maximum.length.of.the.commit.message.title.before.a.warning.is.shown.set.to.0.to.disable",
                    "Maximum length of the commit message title before a warning is shown. Set to 0 to disable."
                ),
                "git_panel.commit_title_max_length",
                |settings_content| {
                    settings_content
                        .git_panel
                        .as_ref()?
                        .commit_title_max_length
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .commit_title_max_length = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.scroll.bar", "Scroll Bar"),
                lt(
                    "settings_ui.page_data.description.how.and.when.the.scrollbar.should.be.displayed",
                    "How and when the scrollbar should be displayed."
                ),
                "git_panel.scrollbar.show",
                |settings_content| {
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
                |settings_content, value, _| {
                    settings_content
                        .git_panel
                        .get_or_insert_default()
                        .scrollbar
                        .get_or_insert_default()
                        .show = value;
                }
            ),
        ]
    }

    fn debugger_panel_section() -> [SettingsPageItem; 4] {
        [
            section_header!(
                "settings_ui.page_data.section.debugger.panel",
                "Debugger Panel"
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.debugger.panel.dock",
                    "Debugger Panel Dock"
                ),
                lt(
                    "settings_ui.page_data.description.the.dock.position.of.the.debug.panel",
                    "The dock position of the debug panel."
                ),
                "debugger.dock",
                |settings_content| settings_content.debugger.as_ref()?.dock.as_ref(),
                |settings_content, value, _| {
                    settings_content.debugger.get_or_insert_default().dock = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.debugger.panel.default.width",
                    "Debugger Panel Default Width"
                ),
                lt(
                    "settings_ui.page_data.description.default.width.in.pixels.when.the.debug.panel.is.docked.to.the.left.or.right",
                    "Default width in pixels when the debug panel is docked to the left or right."
                ),
                "debugger.default_width",
                |settings_content| { settings_content.debugger.as_ref()?.default_width.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .debugger
                        .get_or_insert_default()
                        .default_width = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.debugger.panel.default.height",
                    "Debugger Panel Default Height"
                ),
                lt(
                    "settings_ui.page_data.description.default.height.in.pixels.when.the.debug.panel.is.docked.to.the.bottom",
                    "Default height in pixels when the debug panel is docked to the bottom."
                ),
                "debugger.default_height",
                |settings_content| { settings_content.debugger.as_ref()?.default_height.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .debugger
                        .get_or_insert_default()
                        .default_height = value;
                }
            ),
        ]
    }

    fn agent_panel_section() -> [SettingsPageItem; 7] {
        [
            section_header!("settings_ui.page_data.section.agent.panel", "Agent Panel"),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.agent.panel.button",
                    "Agent Panel Button"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.show.the.agent.panel.button.in.the.status.bar",
                    "Whether to show the agent panel button in the status bar."
                ),
                "agent.button",
                |settings_content| settings_content.agent.as_ref()?.button.as_ref(),
                |settings_content, value, _| {
                    settings_content.agent.get_or_insert_default().button = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.agent.panel.dock",
                    "Agent Panel Dock"
                ),
                lt(
                    "settings_ui.page_data.description.where.to.dock.the.agent.panel",
                    "Where to dock the agent panel."
                ),
                "agent.dock",
                |settings_content| settings_content.agent.as_ref()?.dock.as_ref(),
                |settings_content, value, _| {
                    settings_content.agent.get_or_insert_default().dock = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.agent.panel.flexible.sizing",
                    "Agent Panel Flexible Sizing"
                ),
                lt(
                    "settings_ui.page_data.description.whether.the.agent.panel.should.use.flexible.proportional.sizing.when.docked.to.the.left.or.right",
                    "Whether the agent panel should use flexible (proportional) sizing when docked to the left or right. When enabled, the default width does not control the panel width, and resetting the panel restores the default proportion."
                ),
                "agent.flexible",
                |settings_content| settings_content.agent.as_ref()?.flexible.as_ref(),
                |settings_content, value, _| {
                    settings_content.agent.get_or_insert_default().flexible = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.agent.panel.default.width",
                    "Agent Panel Default Width"
                ),
                lt(
                    "settings_ui.page_data.description.default.width.when.the.agent.panel.is.docked.to.the.left.or.right",
                    "Default fixed width when the agent panel is docked to the left or right and flexible sizing is disabled."
                ),
                "agent.default_width",
                |settings_content| { settings_content.agent.as_ref()?.default_width.as_ref() },
                |settings_content, value, _| {
                    settings_content.agent.get_or_insert_default().default_width = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.agent.panel.default.height",
                    "Agent Panel Default Height"
                ),
                lt(
                    "settings_ui.page_data.description.default.height.when.the.agent.panel.is.docked.to.the.bottom",
                    "Default height when the agent panel is docked to the bottom."
                ),
                "agent.default_height",
                |settings_content| { settings_content.agent.as_ref()?.default_height.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .agent
                        .get_or_insert_default()
                        .default_height = value;
                }
            ),
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
                    Some(usize::from(enabled))
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

    fn telegram_panel_section() -> [SettingsPageItem; 11] {
        [
            section_header!(
                "settings_ui.page_data.section.telegram.panel",
                "Telegram Panel"
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.telegram.panel.button",
                    "Telegram Panel Button"
                ),
                lt(
                    "settings_ui.page_data.description.show.the.telegram.panel.button.in.the.status.bar",
                    "Show the Telegram panel button in the status bar."
                ),
                "telegram_panel.button",
                |settings_content| { settings_content.telegram_panel.as_ref()?.button.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .telegram_panel
                        .get_or_insert_default()
                        .button = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.telegram.panel.dock",
                    "Telegram Panel Dock"
                ),
                lt(
                    "settings_ui.page_data.description.where.to.dock.the.telegram.panel",
                    "Where to dock the Telegram panel."
                ),
                "telegram_panel.dock",
                |settings_content| { settings_content.telegram_panel.as_ref()?.dock.as_ref() },
                |settings_content, value, _| {
                    settings_content.telegram_panel.get_or_insert_default().dock = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.telegram.panel.default.width",
                    "Telegram Panel Default Width"
                ),
                lt(
                    "settings_ui.page_data.description.default.width.of.the.telegram.panel.in.pixels",
                    "Default width of the Telegram panel in pixels."
                ),
                "telegram_panel.default_width",
                |settings_content| {
                    settings_content
                        .telegram_panel
                        .as_ref()?
                        .default_width
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .telegram_panel
                        .get_or_insert_default()
                        .default_width = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.telegram.panel.default.height",
                    "Telegram Panel Default Height"
                ),
                lt(
                    "settings_ui.page_data.description.default.height.of.the.telegram.panel.in.pixels",
                    "Default height of the Telegram panel in pixels."
                ),
                "telegram_panel.default_height",
                |settings_content| {
                    settings_content
                        .telegram_panel
                        .as_ref()?
                        .default_height
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .telegram_panel
                        .get_or_insert_default()
                        .default_height = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.telegram.panel.flexible",
                    "Telegram Panel Flexible Sizing"
                ),
                lt(
                    "settings_ui.page_data.description.allow.the.telegram.panel.to.stretch.to.fill.its.dock",
                    "Allow the Telegram panel to stretch to fill its dock."
                ),
                "telegram_panel.flexible",
                |settings_content| { settings_content.telegram_panel.as_ref()?.flexible.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .telegram_panel
                        .get_or_insert_default()
                        .flexible = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.telegram.panel.starts.open",
                    "Telegram Panel Opens on Startup"
                ),
                lt(
                    "settings_ui.page_data.description.open.the.telegram.panel.on.startup",
                    "Open the Telegram panel on startup."
                ),
                "telegram_panel.starts_open",
                |settings_content| {
                    settings_content
                        .telegram_panel
                        .as_ref()?
                        .starts_open
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .telegram_panel
                        .get_or_insert_default()
                        .starts_open = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.telegram.panel.show.unread.badge",
                    "Telegram Panel Unread Badge"
                ),
                lt(
                    "settings_ui.page_data.description.show.a.badge.with.the.number.of.unread.messages.muted.chats.do.not.count",
                    "Show a badge with the number of unread messages on the Telegram panel icon. Muted chats do not count."
                ),
                "telegram_panel.show_unread_badge",
                |settings_content| {
                    settings_content
                        .telegram_panel
                        .as_ref()?
                        .show_unread_badge
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .telegram_panel
                        .get_or_insert_default()
                        .show_unread_badge = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.telegram.panel.limit.content.width",
                    "Telegram Panel Limit Content Width"
                ),
                lt(
                    "settings_ui.page_data.description.limit.the.width.of.telegram.messages.the.chat.header.and.the.composer",
                    "Limit the width of messages, the chat header, and the composer."
                ),
                "telegram_panel.limit_content_width",
                |settings_content| {
                    settings_content
                        .telegram_panel
                        .as_ref()?
                        .limit_content_width
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .telegram_panel
                        .get_or_insert_default()
                        .limit_content_width = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.telegram.panel.max.content.width",
                    "Telegram Panel Max Content Width"
                ),
                lt(
                    "settings_ui.page_data.description.maximum.content.width.in.pixels.for.the.telegram.panel",
                    "Maximum content width in pixels. Content is centered when the panel is wider than this value."
                ),
                "telegram_panel.max_content_width",
                |settings_content| {
                    settings_content
                        .telegram_panel
                        .as_ref()?
                        .max_content_width
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .telegram_panel
                        .get_or_insert_default()
                        .max_content_width = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.telegram.composer.minimum.lines",
                    "Telegram Composer Minimum Lines"
                ),
                lt(
                    "settings_ui.page_data.description.minimum.number.of.lines.the.telegram.composer.shows.before.it.grows",
                    "Minimum number of lines the Telegram composer shows before it grows."
                ),
                "telegram_panel.composer_min_lines",
                |settings_content| {
                    settings_content
                        .telegram_panel
                        .as_ref()?
                        .composer_min_lines
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .telegram_panel
                        .get_or_insert_default()
                        .composer_min_lines = value;
                }
            ),
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
            telegram_panel_section(),
        ],
    }
}

fn collaboration_page() -> SettingsPage {
    SettingsPage {
        title: lt("settings_ui.page_data.title.collaboration", "Collaboration"),
        items: Box::new([
            section_header!("settings_ui.page_data.section.telegram", "Telegram"),
            setting_item!(
            lt("settings_ui.page_data.title.telegram.proxy", "Telegram Proxy"),
            lt("settings_ui.page_data.description.a.dedicated.socks5.proxy.for.telegram", "A dedicated SOCKS5 proxy for Telegram. Telegram cannot use HTTP or HTTPS proxies."),
            "telegram.proxy",
            |settings_content| settings_content.telegram.as_ref()?.proxy.as_ref(),
            |settings_content, value, _| { settings_content.telegram.get_or_insert_default().proxy = value; },
            metadata: Some(Box::new(SettingsFieldMetadata {
                placeholder: Some("socks5://127.0.0.1:1080"),
                ..Default::default()
            }))),
        ]),
    }
}

fn debugger_page() -> SettingsPage {
    fn general_section() -> [SettingsPageItem; 6] {
        [
            section_header!("settings_ui.page_data.section.general", "General"),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.stepping.granularity",
                    "Stepping Granularity"
                ),
                lt(
                    "settings_ui.page_data.description.determines.the.stepping.granularity.for.debug.operations",
                    "Determines the stepping granularity for debug operations."
                ),
                "debugger.stepping_granularity",
                |settings_content| {
                    settings_content
                        .debugger
                        .as_ref()?
                        .stepping_granularity
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .debugger
                        .get_or_insert_default()
                        .stepping_granularity = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.save.breakpoints",
                    "Save Breakpoints"
                ),
                lt(
                    "settings_ui.page_data.description.whether.breakpoints.should.be.reused.across.zzz.sessions",
                    "Whether breakpoints should be reused across ZZZ sessions."
                ),
                "debugger.save_breakpoints",
                |settings_content| {
                    settings_content
                        .debugger
                        .as_ref()?
                        .save_breakpoints
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .debugger
                        .get_or_insert_default()
                        .save_breakpoints = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.timeout", "Timeout"),
                lt(
                    "settings_ui.page_data.description.time.in.milliseconds.until.timeout.error.when.connecting.to.a.tcp.debug.adapter",
                    "Time in milliseconds until timeout error when connecting to a TCP debug adapter."
                ),
                "debugger.timeout",
                |settings_content| settings_content.debugger.as_ref()?.timeout.as_ref(),
                |settings_content, value, _| {
                    settings_content.debugger.get_or_insert_default().timeout = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.log.dap.communications",
                    "Log DAP Communications"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.log.messages.between.active.debug.adapters.and.zzz",
                    "Whether to log messages between active debug adapters and ZZZ."
                ),
                "debugger.log_dap_communications",
                |settings_content| {
                    settings_content
                        .debugger
                        .as_ref()?
                        .log_dap_communications
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .debugger
                        .get_or_insert_default()
                        .log_dap_communications = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.format.dap.log.messages",
                    "Format DAP Log Messages"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.format.dap.messages.when.adding.them.to.debug.adapter.logger",
                    "Whether to format DAP messages when adding them to debug adapter logger."
                ),
                "debugger.format_dap_log_messages",
                |settings_content| {
                    settings_content
                        .debugger
                        .as_ref()?
                        .format_dap_log_messages
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .debugger
                        .get_or_insert_default()
                        .format_dap_log_messages = value;
                }
            ),
        ]
    }

    SettingsPage {
        title: lt("settings_ui.page_data.title.debugger", "Debugger"),
        items: concat_sections![general_section()],
    }
}

fn terminal_page() -> SettingsPage {
    // Rows that switch on a variant discriminant (`DynamicItem`) stay literal: their pick/write
    // closures match on every variant, which `setting_item!` cannot express.
    fn environment_section() -> [SettingsPageItem; 5] {
        [
                section_header!("settings_ui.page_data.section.environment", "Environment"),
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
                                    .get_or_insert(settings::WorkingDirectory::CurrentProjectDirectory);
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
                setting_item!(
                    lt("settings_ui.page_data.title.environment.variables", "Environment Variables"),
                    lt("settings_ui.page_data.description.key.value.pairs.to.add.to.the.terminal.s.environment", "Key-value pairs to add to the terminal's environment."),
                    "terminal.env",
                    |settings_content| settings_content.terminal.as_ref()?.project.env.as_ref(),
                    |settings_content, value, _| { settings_content.terminal.get_or_insert_default().project.env = value; },
                    files: USER | PROJECT),
                setting_item!(
                    lt("settings_ui.page_data.title.detect.virtual.environment", "Detect Virtual Environment"),
                    lt("settings_ui.page_data.description.activates.the.python.virtual.environment.if.one.is.found.in.the.terminal.s.working.directory", "Activates the Python virtual environment, if one is found, in the terminal's working directory."),
                    "terminal.detect_venv",
                    |settings_content| settings_content.terminal.as_ref()?.project.detect_venv.as_ref(),
                    |settings_content, value, _| { settings_content.terminal.get_or_insert_default().project.detect_venv = value; },
                    files: USER | PROJECT),
            ]
    }

    fn font_section() -> [SettingsPageItem; 6] {
        [
            section_header!("settings_ui.page_data.section.font", "Font"),
            setting_item!(
                lt("settings_ui.page_data.title.font.size", "Font Size"),
                lt(
                    "settings_ui.page_data.description.font.size.for.terminal.text.if.not.set.defaults.to.buffer.font.size",
                    "Font size for terminal text. If not set, defaults to buffer font size."
                ),
                "terminal.font_size",
                |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()
                        .and_then(|terminal| terminal.font_size.as_ref())
                        .or(settings_content.theme.buffer_font_size.as_ref())
                },
                |settings_content, value, _| {
                    settings_content.terminal.get_or_insert_default().font_size = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.font.family", "Font Family"),
                lt(
                    "settings_ui.page_data.description.font.family.for.terminal.text.if.not.set.defaults.to.buffer.font.family",
                    "Font family for terminal text. If not set, defaults to buffer font family."
                ),
                "terminal.font_family",
                |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()
                        .and_then(|terminal| terminal.font_family.as_ref())
                        .or(settings_content.theme.buffer_font_family.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .font_family = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.font.fallbacks",
                    "Font Fallbacks"
                ),
                lt(
                    "settings_ui.page_data.description.font.fallbacks.for.terminal.text.if.not.set.defaults.to.buffer.font.fallbacks",
                    "Font fallbacks for terminal text. If not set, defaults to buffer font fallbacks."
                ),
                "terminal.font_fallbacks",
                |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()
                        .and_then(|terminal| terminal.font_fallbacks.as_ref())
                        .or(settings_content.theme.buffer_font_fallbacks.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .font_fallbacks = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.font.weight", "Font Weight"),
                lt(
                    "settings_ui.page_data.description.font.weight.for.terminal.text.in.css.weight.units.100.900",
                    "Font weight for terminal text in CSS weight units (100-900)."
                ),
                "terminal.font_weight",
                |settings_content| { settings_content.terminal.as_ref()?.font_weight.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .font_weight = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.font.features", "Font Features"),
                lt(
                    "settings_ui.page_data.description.font.features.for.terminal.text",
                    "Font features for terminal text."
                ),
                "terminal.font_features",
                |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()
                        .and_then(|terminal| terminal.font_features.as_ref())
                        .or(settings_content.theme.buffer_font_features.as_ref())
                },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .font_features = value;
                }
            ),
        ]
    }

    fn display_settings_section() -> [SettingsPageItem; 6] {
        [
            section_header!(
                "settings_ui.page_data.section.display.settings",
                "Display Settings"
            ),
            setting_item!(
                lt("settings_ui.page_data.title.line.height", "Line Height"),
                lt(
                    "settings_ui.page_data.description.line.height.for.terminal.text",
                    "Line height for terminal text."
                ),
                "terminal.line_height",
                |settings_content| { settings_content.terminal.as_ref()?.line_height.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .line_height = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.cursor.shape", "Cursor Shape"),
                lt(
                    "settings_ui.page_data.description.default.cursor.shape.for.the.terminal.bar.block.underline.or.hollow",
                    "Default cursor shape for the terminal (bar, block, underline, or hollow)."
                ),
                "terminal.cursor_shape",
                |settings_content| { settings_content.terminal.as_ref()?.cursor_shape.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .cursor_shape = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.cursor.blinking",
                    "Cursor Blinking"
                ),
                lt(
                    "settings_ui.page_data.description.sets.the.cursor.blinking.behavior.in.the.terminal",
                    "Sets the cursor blinking behavior in the terminal."
                ),
                "terminal.blinking",
                |settings_content| settings_content.terminal.as_ref()?.blinking.as_ref(),
                |settings_content, value, _| {
                    settings_content.terminal.get_or_insert_default().blinking = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.alternate.scroll",
                    "Alternate Scroll"
                ),
                lt(
                    "settings_ui.page_data.description.whether.alternate.scroll.mode.is.active.by.default.converts.mouse.scroll.to.arrow.keys.in.apps.like.vim",
                    "Whether alternate scroll mode is active by default (converts mouse scroll to arrow keys in apps like Vim)."
                ),
                "terminal.alternate_scroll",
                |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()?
                        .alternate_scroll
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .alternate_scroll = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.minimum.contrast",
                    "Minimum Contrast"
                ),
                lt(
                    "settings_ui.page_data.description.the.minimum.apca.perceptual.contrast.between.foreground.and.background.colors.0.106",
                    "The minimum APCA perceptual contrast between foreground and background colors (0-106)."
                ),
                "terminal.minimum_contrast",
                |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()?
                        .minimum_contrast
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .minimum_contrast = value;
                }
            ),
        ]
    }

    fn behavior_settings_section() -> [SettingsPageItem; 6] {
        [
            section_header!(
                "settings_ui.page_data.section.behavior.settings",
                "Behavior Settings"
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.option.as.meta",
                    "Option As Meta"
                ),
                lt(
                    "settings_ui.page_data.description.whether.the.option.key.behaves.as.the.meta.key",
                    "Whether the option key behaves as the meta key."
                ),
                "terminal.option_as_meta",
                |settings_content| { settings_content.terminal.as_ref()?.option_as_meta.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .option_as_meta = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.open.links.in.mouse.mode",
                    "Open Links In Mouse Mode"
                ),
                lt(
                    "settings_ui.page_data.description.whether.cmd.click.ctrl.click.opens.hyperlinks.when.the.terminal.application.has.enabled.mouse.reporting",
                    "Whether cmd-click (ctrl-click on Linux and Windows) opens hyperlinks even when the terminal application has enabled mouse reporting. When disabled, these clicks are forwarded to the application; links can still be opened with shift-cmd-click."
                ),
                "terminal.open_links_in_mouse_mode",
                |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()?
                        .open_links_in_mouse_mode
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .open_links_in_mouse_mode = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.copy.on.select",
                    "Copy On Select"
                ),
                lt(
                    "settings_ui.page_data.description.whether.selecting.text.in.the.terminal.automatically.copies.to.the.system.clipboard",
                    "Whether selecting text in the terminal automatically copies to the system clipboard."
                ),
                "terminal.copy_on_select",
                |settings_content| { settings_content.terminal.as_ref()?.copy_on_select.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .copy_on_select = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.keep.selection.on.copy",
                    "Keep Selection On Copy"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.keep.the.text.selection.after.copying.it.to.the.clipboard",
                    "Whether to keep the text selection after copying it to the clipboard."
                ),
                "terminal.keep_selection_on_copy",
                |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()?
                        .keep_selection_on_copy
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .keep_selection_on_copy = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.audible.bell", "Audible Bell"),
                lt(
                    "settings_ui.page_data.description.whether.to.play.a.sound.when.the.bel.character.a.0x07.is.printed",
                    "Whether to play a sound when the BEL character (`\\\\a`, `0x07`) is printed"
                ),
                "terminal.bell",
                |settings_content| settings_content.terminal.as_ref()?.bell.as_ref(),
                |settings_content, value, _| {
                    settings_content.terminal.get_or_insert_default().bell = value;
                }
            ),
        ]
    }

    fn layout_settings_section() -> [SettingsPageItem; 3] {
        [
            section_header!(
                "settings_ui.page_data.section.layout.settings",
                "Layout Settings"
            ),
            setting_item!(
                lt("settings_ui.page_data.title.default.width", "Default Width"),
                lt(
                    "settings_ui.page_data.description.default.width.when.the.terminal.is.docked.to.the.left.or.right.in.pixels",
                    "Default width when the terminal is docked to the left or right (in pixels)."
                ),
                "terminal.default_width",
                |settings_content| { settings_content.terminal.as_ref()?.default_width.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .default_width = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.default.height",
                    "Default Height"
                ),
                lt(
                    "settings_ui.page_data.description.default.height.when.the.terminal.is.docked.to.the.bottom.in.pixels",
                    "Default height when the terminal is docked to the bottom (in pixels)."
                ),
                "terminal.default_height",
                |settings_content| { settings_content.terminal.as_ref()?.default_height.as_ref() },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .default_height = value;
                }
            ),
        ]
    }

    fn advanced_settings_section() -> [SettingsPageItem; 3] {
        [
            section_header!(
                "settings_ui.page_data.section.advanced.settings",
                "Advanced Settings"
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.max.scroll.history.lines",
                    "Max Scroll History Lines"
                ),
                lt(
                    "settings_ui.page_data.description.maximum.number.of.lines.to.keep.in.scrollback.history.max.100.000.0.disables.scrolling",
                    "Maximum number of lines to keep in scrollback history (max: 100,000; 0 disables scrolling)."
                ),
                "terminal.max_scroll_history_lines",
                |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()?
                        .max_scroll_history_lines
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .max_scroll_history_lines = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.scroll.multiplier",
                    "Scroll Multiplier"
                ),
                lt(
                    "settings_ui.page_data.description.the.multiplier.for.scrolling.in.the.terminal.with.the.mouse.wheel",
                    "The multiplier for scrolling in the terminal with the mouse wheel"
                ),
                "terminal.scroll_multiplier",
                |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()?
                        .scroll_multiplier
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .scroll_multiplier = value;
                }
            ),
        ]
    }

    fn toolbar_section() -> [SettingsPageItem; 2] {
        [
            section_header!("settings_ui.page_data.section.toolbar", "Toolbar"),
            setting_item!(
                lt("settings_ui.page_data.title.breadcrumbs", "Breadcrumbs"),
                lt(
                    "settings_ui.page_data.description.display.the.terminal.title.in.breadcrumbs.inside.the.terminal.pane",
                    "Display the terminal title in breadcrumbs inside the terminal pane."
                ),
                "terminal.toolbar.breadcrumbs",
                |settings_content| {
                    settings_content
                        .terminal
                        .as_ref()?
                        .toolbar
                        .as_ref()?
                        .breadcrumbs
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .toolbar
                        .get_or_insert_default()
                        .breadcrumbs = value;
                }
            ),
        ]
    }

    fn scrollbar_section() -> [SettingsPageItem; 2] {
        [
            section_header!("settings_ui.page_data.section.scrollbar", "Scrollbar"),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.scrollbar",
                    "Show Scrollbar"
                ),
                lt(
                    "settings_ui.page_data.description.when.to.show.the.scrollbar.in.the.terminal",
                    "When to show the scrollbar in the terminal."
                ),
                "terminal.scrollbar.show",
                |settings_content| {
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
                |settings_content, value, _| {
                    settings_content
                        .terminal
                        .get_or_insert_default()
                        .scrollbar
                        .get_or_insert_default()
                        .show = value;
                }
            ),
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
    // Rows that switch on a variant discriminant (`DynamicItem`) stay literal: their pick/write
    // closures match on every variant, which `setting_item!` cannot express.
    fn git_integration_section() -> [SettingsPageItem; 3] {
        [
            section_header!(
                "settings_ui.page_data.section.git.integration",
                "Git Integration"
            ),
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
                    Some(usize::from(!disabled))
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
            SettingsPageItem::DynamicItem(DynamicItem {
                discriminant: SettingItem {
                    files: USER,
                    title: lt("settings_ui.page_data.title.auto.fetch", "Auto Fetch"),
                    description: lt(
                        "settings_ui.page_data.description.automatically.fetch.the.default.branch.s.remote.and.the.current.branch.s.upstream.remote",
                        "Automatically fetch the default branch's remote and the current branch's upstream remote.",
                    ),
                    field: Box::new(SettingField::<bool> {
                        json_path: Some("git.auto_fetch.enabled"),
                        pick: |settings_content| {
                            settings_content
                                .git
                                .as_ref()?
                                .auto_fetch
                                .as_ref()?
                                .enabled
                                .as_ref()
                        },
                        write: |settings_content, value, _| {
                            settings_content
                                .git
                                .get_or_insert_default()
                                .auto_fetch
                                .get_or_insert_default()
                                .enabled = value;
                        },
                    }),
                    metadata: None,
                },
                pick_discriminant: |settings_content| {
                    Some(
                        *settings_content
                            .git
                            .as_ref()?
                            .auto_fetch
                            .as_ref()?
                            .enabled
                            .as_ref()? as usize,
                    )
                },
                fields: vec![
                    vec![],
                    vec![SettingItem {
                        files: USER,
                        title: lt(
                            "settings_ui.page_data.title.auto.fetch.interval",
                            "Auto Fetch Interval",
                        ),
                        description: lt(
                            "settings_ui.page_data.description.how.long.to.wait.between.automatic.fetches.in.seconds.min.15",
                            "How long to wait between automatic fetches, in seconds. (min: 15)",
                        ),
                        field: Box::new(SettingField::<u64> {
                            json_path: Some("git.auto_fetch.interval_secs"),
                            pick: |settings_content| {
                                settings_content
                                    .git
                                    .as_ref()?
                                    .auto_fetch
                                    .as_ref()?
                                    .interval_secs
                                    .as_ref()
                            },
                            write: |settings_content, value, _| {
                                settings_content
                                    .git
                                    .get_or_insert_default()
                                    .auto_fetch
                                    .get_or_insert_default()
                                    .interval_secs = value;
                            },
                        }),
                        metadata: None,
                    }],
                ],
            }),
        ]
    }

    fn git_gutter_section() -> [SettingsPageItem; 3] {
        [
            section_header!("settings_ui.page_data.section.git.gutter", "Git Gutter"),
            setting_item!(
                lt("settings_ui.page_data.title.visibility", "Visibility"),
                lt(
                    "settings_ui.page_data.description.control.whether.git.status.is.shown.in.the.editor.s.gutter",
                    "Control whether Git status is shown in the editor's gutter."
                ),
                "git.git_gutter",
                |settings_content| settings_content.git.as_ref()?.git_gutter.as_ref(),
                |settings_content, value, _| {
                    settings_content.git.get_or_insert_default().git_gutter = value;
                }
            ),
            // todo(settings_ui): Figure out the right default for this value in default.json
            setting_item!(
                lt("settings_ui.page_data.title.debounce", "Debounce"),
                lt(
                    "settings_ui.page_data.description.debounce.threshold.in.milliseconds.after.which.changes.are.reflected.in.the.git.gutter",
                    "Debounce threshold in milliseconds after which changes are reflected in the Git gutter."
                ),
                "git.gutter_debounce",
                |settings_content| { settings_content.git.as_ref()?.gutter_debounce.as_ref() },
                |settings_content, value, _| {
                    settings_content.git.get_or_insert_default().gutter_debounce = value;
                }
            ),
        ]
    }

    fn inline_git_blame_section() -> [SettingsPageItem; 6] {
        [
            section_header!(
                "settings_ui.page_data.section.inline.git.blame",
                "Inline Git Blame"
            ),
            setting_item!(
                lt("settings_ui.page_data.title.enabled", "Enabled"),
                lt(
                    "settings_ui.page_data.description.whether.or.not.to.show.git.blame.data.inline.in.the.currently.focused.line",
                    "Whether or not to show Git blame data inline in the currently focused line."
                ),
                "git.inline_blame.enabled",
                |settings_content| {
                    settings_content
                        .git
                        .as_ref()?
                        .inline_blame
                        .as_ref()?
                        .enabled
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .git
                        .get_or_insert_default()
                        .inline_blame
                        .get_or_insert_default()
                        .enabled = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.delay", "Delay"),
                lt(
                    "settings_ui.page_data.description.the.delay.after.which.the.inline.blame.information.is.shown",
                    "The delay after which the inline blame information is shown."
                ),
                "git.inline_blame.delay_ms",
                |settings_content| {
                    settings_content
                        .git
                        .as_ref()?
                        .inline_blame
                        .as_ref()?
                        .delay_ms
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .git
                        .get_or_insert_default()
                        .inline_blame
                        .get_or_insert_default()
                        .delay_ms = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.padding", "Padding"),
                lt(
                    "settings_ui.page_data.description.padding.between.the.end.of.the.source.line.and.the.start.of.the.inline.blame.in.columns",
                    "Padding between the end of the source line and the start of the inline blame in columns."
                ),
                "git.inline_blame.padding",
                |settings_content| {
                    settings_content
                        .git
                        .as_ref()?
                        .inline_blame
                        .as_ref()?
                        .padding
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .git
                        .get_or_insert_default()
                        .inline_blame
                        .get_or_insert_default()
                        .padding = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.minimum.column",
                    "Minimum Column"
                ),
                lt(
                    "settings_ui.page_data.description.the.minimum.column.number.at.which.to.show.the.inline.blame.information",
                    "The minimum column number at which to show the inline blame information."
                ),
                "git.inline_blame.min_column",
                |settings_content| {
                    settings_content
                        .git
                        .as_ref()?
                        .inline_blame
                        .as_ref()?
                        .min_column
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .git
                        .get_or_insert_default()
                        .inline_blame
                        .get_or_insert_default()
                        .min_column = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.commit.summary",
                    "Show Commit Summary"
                ),
                lt(
                    "settings_ui.page_data.description.show.commit.summary.as.part.of.the.inline.blame",
                    "Show commit summary as part of the inline blame."
                ),
                "git.inline_blame.show_commit_summary",
                |settings_content| {
                    settings_content
                        .git
                        .as_ref()?
                        .inline_blame
                        .as_ref()?
                        .show_commit_summary
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .git
                        .get_or_insert_default()
                        .inline_blame
                        .get_or_insert_default()
                        .show_commit_summary = value;
                }
            ),
        ]
    }

    fn git_avatars_section() -> [SettingsPageItem; 2] {
        [
            section_header!("settings_ui.page_data.section.git.avatars", "Git Avatars"),
            setting_item!(
                lt("settings_ui.page_data.title.show.avatars", "Show Avatars"),
                lt(
                    "settings_ui.page_data.description.show.avatar.images.for.commit.authors.in.git.views",
                    "Show avatar images for commit authors in git views. Avatars are loaded from the hosting provider and disclose the author's email address to it."
                ),
                "git.show_avatar",
                |settings_content| settings_content.git.as_ref()?.show_avatar.as_ref(),
                |settings_content, value, _| {
                    settings_content.git.get_or_insert_default().show_avatar = value;
                }
            ),
        ]
    }

    fn git_blame_view_section() -> [SettingsPageItem; 2] {
        [
            section_header!(
                "settings_ui.page_data.section.git.blame.view",
                "Git Blame View"
            ),
            setting_item!(
                lt("settings_ui.page_data.title.show.avatar", "Show Avatar"),
                lt(
                    "settings_ui.page_data.description.show.the.avatar.of.the.author.of.the.commit",
                    "Show the avatar of the author of the commit."
                ),
                "git.blame.show_avatar",
                |settings_content| {
                    settings_content
                        .git
                        .as_ref()?
                        .blame
                        .as_ref()?
                        .show_avatar
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .git
                        .get_or_insert_default()
                        .blame
                        .get_or_insert_default()
                        .show_avatar = value;
                }
            ),
        ]
    }

    fn branch_picker_section() -> [SettingsPageItem; 2] {
        [
            section_header!(
                "settings_ui.page_data.section.branch.picker",
                "Branch Picker"
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.author.name",
                    "Show Author Name"
                ),
                lt(
                    "settings_ui.page_data.description.show.author.name.as.part.of.the.commit.information.in.branch.picker",
                    "Show author name as part of the commit information in branch picker."
                ),
                "git.branch_picker.show_author_name",
                |settings_content| {
                    settings_content
                        .git
                        .as_ref()?
                        .branch_picker
                        .as_ref()?
                        .show_author_name
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .git
                        .get_or_insert_default()
                        .branch_picker
                        .get_or_insert_default()
                        .show_author_name = value;
                }
            ),
        ]
    }

    fn git_hunks_section() -> [SettingsPageItem; 4] {
        [
            section_header!("settings_ui.page_data.section.git.hunks", "Git Hunks"),
            setting_item!(
                lt("settings_ui.page_data.title.hunk.style", "Hunk Style"),
                lt(
                    "settings_ui.page_data.description.how.git.hunks.are.displayed.visually.in.the.editor",
                    "How Git hunks are displayed visually in the editor."
                ),
                "git.hunk_style",
                |settings_content| settings_content.git.as_ref()?.hunk_style.as_ref(),
                |settings_content, value, _| {
                    settings_content.git.get_or_insert_default().hunk_style = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.path.style", "Path Style"),
                lt(
                    "settings_ui.page_data.description.should.the.name.or.path.be.displayed.first.in.the.git.view",
                    "Should the name or path be displayed first in the git view."
                ),
                "git.path_style",
                |settings_content| settings_content.git.as_ref()?.path_style.as_ref(),
                |settings_content, value, _| {
                    settings_content.git.get_or_insert_default().path_style = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.show.stage.restore.buttons",
                    "Show Stage/Restore Buttons"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.show.the.stage.and.restore.buttons.on.diff.hunks",
                    "Whether to show the stage and restore buttons on diff hunks."
                ),
                "git.show_stage_restore_buttons",
                |settings_content| {
                    settings_content
                        .git
                        .as_ref()?
                        .show_stage_restore_buttons
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content
                        .git
                        .get_or_insert_default()
                        .show_stage_restore_buttons = value;
                }
            ),
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
            section_header!("settings_ui.page_data.section.general", "General"),
            setting_item!(
                lt("settings_ui.page_data.title.disable.ai", "Disable AI"),
                lt("settings_ui.page_data.description.whether.to.disable.all.ai.features.in.zzz", "Whether to disable all AI features in ZZZ."),
                "disable_ai",
                |settings_content| settings_content.project.disable_ai.as_ref(),
                |settings_content, value, _| { settings_content.project.disable_ai = value; },
                files: USER | PROJECT),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.threads.sidebar.side",
                    "Threads Sidebar Side"
                ),
                lt(
                    "settings_ui.page_data.description.which.side.of.the.window.the.threads.sidebar.appears.on",
                    "Which side of the window the threads sidebar appears on."
                ),
                "agent.sidebar_side",
                |settings_content| settings_content.agent.as_ref()?.sidebar_side.as_ref(),
                |settings_content, value, _| {
                    settings_content.agent.get_or_insert_default().sidebar_side = value;
                }
            ),
        ]
    }

    fn agent_configuration_section(_cx: &App) -> Box<[SettingsPageItem]> {
        let mut items = vec![
            section_header!(
                "settings_ui.page_data.section.agent.configuration",
                "Agent Configuration"
            ),
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
            setting_item!(
                lt("settings_ui.page_data.title.single.file.review", "Single File Review"),
                lt("settings_ui.page_data.description.when.enabled.agent.edits.will.also.be.displayed.in.single.file.buffers.for.review", "When enabled, agent edits will also be displayed in single-file buffers for review."),
                "agent.single_file_review",
                |settings_content| { settings_content.agent.as_ref()?.single_file_review.as_ref() },
                |settings_content, value, _| { settings_content.agent.get_or_insert_default().single_file_review = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.notify.when.agent.waiting", "Notify When Agent Waiting"),
                lt("settings_ui.page_data.description.where.to.show.notifications.when.the.agent.has.completed.its.response.or.needs.confirmation.before.running.a.tool.action", "Where to show notifications when the agent has completed its response or needs confirmation before running a tool action."),
                "agent.notify_when_agent_waiting",
                |settings_content| { settings_content.agent.as_ref()?.notify_when_agent_waiting.as_ref() },
                |settings_content, value, _| { settings_content.agent.get_or_insert_default().notify_when_agent_waiting = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.play.sound.when.agent.done", "Play Sound When Agent Done"),
                lt("settings_ui.page_data.description.when.to.play.a.sound.when.the.agent.has.either.completed.its.response.or.needs.user.input", "When to play a sound when the agent has either completed its response, or needs user input."),
                "agent.play_sound_when_agent_done",
                |settings_content| { settings_content.agent.as_ref()?.play_sound_when_agent_done.as_ref() },
                |settings_content, value, _| { settings_content.agent.get_or_insert_default().play_sound_when_agent_done = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.expand.edit.card", "Expand Edit Card"),
                lt("settings_ui.page_data.description.whether.to.have.edit.cards.in.the.agent.panel.expanded.showing.a.preview.of.the.diff", "Whether to have edit cards in the agent panel expanded, showing a Preview of the diff."),
                "agent.expand_edit_card",
                |settings_content| { settings_content.agent.as_ref()?.expand_edit_card.as_ref() },
                |settings_content, value, _| { settings_content.agent.get_or_insert_default().expand_edit_card = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.expand.terminal.card", "Expand Terminal Card"),
                lt("settings_ui.page_data.description.whether.to.have.terminal.cards.in.the.agent.panel.expanded.showing.the.whole.command.output", "Whether to have terminal cards in the agent panel expanded, showing the whole command output."),
                "agent.expand_terminal_card",
                |settings_content| { settings_content.agent.as_ref()?.expand_terminal_card.as_ref() },
                |settings_content, value, _| { settings_content.agent.get_or_insert_default().expand_terminal_card = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.thinking.display", "Thinking Display"),
                lt("settings_ui.page_data.description.how.thinking.blocks.should.be.displayed.by.default.auto.fully.expands.during.streaming.then.auto.collapses.when.done.preview.auto.expands.with.a.height.constraint.during.streaming.always.expanded.shows.full.content.always.collapsed.keeps.them.collapsed", "How thinking blocks should be displayed by default. 'Auto' fully expands during streaming, then auto-collapses when done. 'Preview' auto-expands with a height constraint during streaming. 'Always Expanded' shows full content. 'Always Collapsed' keeps them collapsed."),
                "agent.thinking_display",
                |settings_content| { settings_content.agent.as_ref()?.thinking_display.as_ref() },
                |settings_content, value, _| { settings_content.agent.get_or_insert_default().thinking_display = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.cancel.generation.on.terminal.stop", "Cancel Generation On Terminal Stop"),
                lt("settings_ui.page_data.description.whether.clicking.the.stop.button.on.a.running.terminal.tool.should.also.cancel.the.agent.s.generation.note.that.this.only.applies.to.the.stop.button.not.to.ctrl.c.inside.the.terminal", "Whether clicking the stop button on a running terminal tool should also cancel the agent's generation. Note that this only applies to the stop button, not to ctrl+c inside the terminal."),
                "agent.cancel_generation_on_terminal_stop",
                |settings_content| { settings_content.agent.as_ref()?.cancel_generation_on_terminal_stop.as_ref() },
                |settings_content, value, _| { settings_content.agent.get_or_insert_default().cancel_generation_on_terminal_stop = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.use.modifier.to.send", "Use Modifier To Send"),
                lt("settings_ui.page_data.description.whether.to.always.use.cmd.enter.or.ctrl.enter.on.linux.or.windows.to.send.messages", "Whether to always use cmd-enter (or ctrl-enter on Linux or Windows) to send messages."),
                "agent.use_modifier_to_send",
                |settings_content| { settings_content.agent.as_ref()?.use_modifier_to_send.as_ref() },
                |settings_content, value, _| { settings_content.agent.get_or_insert_default().use_modifier_to_send = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.message.editor.min.lines", "Message Editor Min Lines"),
                lt("settings_ui.page_data.description.minimum.number.of.lines.to.display.in.the.agent.message.editor", "Minimum number of lines to display in the agent message editor."),
                "agent.message_editor_min_lines",
                |settings_content| { settings_content.agent.as_ref()?.message_editor_min_lines.as_ref() },
                |settings_content, value, _| { settings_content.agent.get_or_insert_default().message_editor_min_lines = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.show.turn.stats", "Show Turn Stats"),
                lt("settings_ui.page_data.description.whether.to.show.turn.statistics.like.elapsed.time.during.generation.and.final.turn.duration", "Whether to show turn statistics like elapsed time during generation and final turn duration."),
                "agent.show_turn_stats",
                |settings_content| { settings_content.agent.as_ref()?.show_turn_stats.as_ref() },
                |settings_content, value, _| { settings_content.agent.get_or_insert_default().show_turn_stats = value; }),
            setting_item!(
                lt("settings_ui.page_data.title.show.merge.conflict.indicator", "Show Merge Conflict Indicator"),
                lt("settings_ui.page_data.description.whether.to.show.the.merge.conflict.indicator.in.the.status.bar.that.offers.to.resolve.conflicts.using.the.agent", "Whether to show the merge conflict indicator in the status bar that offers to resolve conflicts using the agent."),
                "agent.show_merge_conflict_indicator",
                |settings_content| { settings_content.agent.as_ref()?.show_merge_conflict_indicator.as_ref() },
                |settings_content, value, _| { settings_content.agent.get_or_insert_default().show_merge_conflict_indicator = value; }),
        ]);

        items.into_boxed_slice()
    }

    SettingsPage {
        title: lt("settings_ui.page_data.title.ai", "AI"),
        items: concat_sections![general_section(), agent_configuration_section(cx)],
    }
}

fn network_page() -> SettingsPage {
    fn network_section() -> [SettingsPageItem; 4] {
        [
            section_header!("settings_ui.page_data.section.network", "Network"),
            setting_item!(
            lt("settings_ui.page_data.title.proxy", "Proxy"),
            lt("settings_ui.page_data.description.the.proxy.to.use.for.network.requests", "The proxy to use for network requests."),
            "proxy",
            |settings_content| settings_content.proxy.as_ref(),
            |settings_content, value, _| { settings_content.proxy = value; },
            metadata: Some(Box::new(SettingsFieldMetadata {
                placeholder: Some("socks5h://localhost:10808"),
                ..Default::default()
            }))),
            setting_item!(
            lt("settings_ui.page_data.title.server.url", "Server URL"),
            lt("settings_ui.page_data.description.the.url.of.the.zzz.server.to.connect.to", "The URL of the ZZZ server to connect to."),
            "server_url",
            |settings_content| settings_content.server_url.as_ref(),
            |settings_content, value, _| { settings_content.server_url = value; },
            metadata: Some(Box::new(SettingsFieldMetadata {
                placeholder: Some("http://127.0.0.1:7331"),
                ..Default::default()
            }))),
            SettingsPageItem::SubPageLink(SubPageLink {
                title: lt("database.settings.title", "Database Connections"),
                r#type: Default::default(),
                search_aliases: &[
                    "database",
                    "databases",
                    "sqlite",
                    "connection",
                    "connections",
                ],
                json_path: Some("database.connections"),
                description: Some(lt(
                    "database.settings.description",
                    "Manage saved database connections. Passwords are stored in the system keychain; the connection list can be edited in JSON.",
                )),
                in_json: true,
                files: USER,
                render: render_database_connections_page,
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
    // Rows that switch on a variant discriminant (`DynamicItem`) stay literal: their pick/write
    // closures match on every variant, which `setting_item!` cannot express.
    fn indentation_section() -> [SettingsPageItem; 5] {
        [
            section_header!("settings_ui.page_data.section.indentation", "Indentation"),
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
            setting_item!(
                lt("settings_ui.page_data.title.hard.tabs", "Hard Tabs"),
                lt("settings_ui.page_data.description.whether.to.indent.lines.using.tab.characters.as.opposed.to.multiple.spaces", "Whether to indent lines using tab characters, as opposed to multiple spaces."),
                "languages.$(language).hard_tabs",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.hard_tabs.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.hard_tabs = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.auto.indent", "Auto Indent"),
                lt("settings_ui.page_data.description.controls.automatic.indentation.behavior.when.typing", "Controls automatic indentation behavior when typing."),
                "languages.$(language).auto_indent",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.auto_indent.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.auto_indent = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.auto.indent.on.paste", "Auto Indent On Paste"),
                lt("settings_ui.page_data.description.whether.indentation.of.pasted.content.should.be.adjusted.based.on.the.context", "Whether indentation of pasted content should be adjusted based on the context."),
                "languages.$(language).auto_indent_on_paste",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.auto_indent_on_paste.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.auto_indent_on_paste = value;
                                        })
                                    },
                files: USER | PROJECT),
        ]
    }

    fn wrapping_section() -> [SettingsPageItem; 7] {
        [
            section_header!("settings_ui.page_data.section.wrapping", "Wrapping"),
            setting_item!(
                lt("settings_ui.page_data.title.soft.wrap", "Soft Wrap"),
                lt("settings_ui.page_data.description.how.to.soft.wrap.long.lines.of.text", "How to soft-wrap long lines of text."),
                "languages.$(language).soft_wrap",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.soft_wrap.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.soft_wrap = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.soft.wrap.indent", "Soft Wrap Indent"),
                lt("settings_ui.page_data.description.how.to.indent.soft.wrapped.lines", "How to indent soft-wrapped lines."),
                "languages.$(language).soft_wrap_indent",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.soft_wrap_indent.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.soft_wrap_indent = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.show.wrap.guides", "Show Wrap Guides"),
                lt("settings_ui.page_data.description.show.wrap.guides.in.the.editor", "Show wrap guides in the editor."),
                "languages.$(language).show_wrap_guides",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.show_wrap_guides.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.show_wrap_guides = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.preferred.line.length", "Preferred Line Length"),
                lt("settings_ui.page_data.description.the.column.at.which.to.soft.wrap.lines.for.buffers.where.soft.wrap.is.enabled", "The column at which to soft-wrap lines, for buffers where soft-wrap is enabled."),
                "languages.$(language).preferred_line_length",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.preferred_line_length.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.preferred_line_length = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.wrap.guides", "Wrap Guides"),
                lt("settings_ui.page_data.description.character.counts.at.which.to.show.wrap.guides.in.the.editor", "Character counts at which to show wrap guides in the editor."),
                "languages.$(language).wrap_guides",
                |settings_content| {
                                            language_settings_field(settings_content, |language| {
                                                language.wrap_guides.as_ref()
                                            })
                                        },
                |settings_content, value, _| {
                                            language_settings_field_mut(
                                                settings_content,
                                                value,
                                                |language, value| {
                                                    language.wrap_guides = value;
                                                },
                                            )
                                        },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.allow.rewrap", "Allow Rewrap"),
                lt("settings_ui.page_data.description.controls.where.the.editor.rewrap.action.is.allowed.for.this.language", "Controls where the `editor::rewrap` action is allowed for this language."),
                "languages.$(language).allow_rewrap",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.allow_rewrap.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.allow_rewrap = value;
                                        })
                                    },
                files: USER | PROJECT),
        ]
    }

    fn indent_guides_section() -> [SettingsPageItem; 6] {
        [
            section_header!(
                "settings_ui.page_data.section.indent.guides",
                "Indent Guides"
            ),
            setting_item!(
                lt("settings_ui.page_data.title.enabled", "Enabled"),
                lt("settings_ui.page_data.description.display.indent.guides.in.the.editor", "Display indent guides in the editor."),
                "languages.$(language).indent_guides.enabled",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language
                                                .indent_guides
                                                .as_ref()
                                                .and_then(|indent_guides| indent_guides.enabled.as_ref())
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.indent_guides.get_or_insert_default().enabled = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.line.width", "Line Width"),
                lt("settings_ui.page_data.description.the.width.of.the.indent.guides.in.pixels.between.1.and.10", "The width of the indent guides in pixels, between 1 and 10."),
                "languages.$(language).indent_guides.line_width",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language
                                                .indent_guides
                                                .as_ref()
                                                .and_then(|indent_guides| indent_guides.line_width.as_ref())
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.indent_guides.get_or_insert_default().line_width = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.active.line.width", "Active Line Width"),
                lt("settings_ui.page_data.description.the.width.of.the.active.indent.guide.in.pixels.between.1.and.10", "The width of the active indent guide in pixels, between 1 and 10."),
                "languages.$(language).indent_guides.active_line_width",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language
                                                .indent_guides
                                                .as_ref()
                                                .and_then(|indent_guides| indent_guides.active_line_width.as_ref())
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language
                                                .indent_guides
                                                .get_or_insert_default()
                                                .active_line_width = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.coloring", "Coloring"),
                lt("settings_ui.page_data.description.determines.how.indent.guides.are.colored", "Determines how indent guides are colored."),
                "languages.$(language).indent_guides.coloring",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language
                                                .indent_guides
                                                .as_ref()
                                                .and_then(|indent_guides| indent_guides.coloring.as_ref())
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.indent_guides.get_or_insert_default().coloring = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.background.coloring", "Background Coloring"),
                lt("settings_ui.page_data.description.determines.how.indent.guide.backgrounds.are.colored", "Determines how indent guide backgrounds are colored."),
                "languages.$(language).indent_guides.background_coloring",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.indent_guides.as_ref().and_then(|indent_guides| {
                                                indent_guides.background_coloring.as_ref()
                                            })
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language
                                                .indent_guides
                                                .get_or_insert_default()
                                                .background_coloring = value;
                                        })
                                    },
                files: USER | PROJECT),
        ]
    }

    fn formatting_section() -> [SettingsPageItem; 8] {
        [
            section_header!("settings_ui.page_data.section.formatting", "Formatting"),
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
            setting_item!(
                lt("settings_ui.page_data.title.remove.trailing.whitespace.on.save", "Remove Trailing Whitespace On Save"),
                lt("settings_ui.page_data.description.whether.or.not.to.remove.any.trailing.whitespace.from.lines.of.a.buffer.before.saving.it", "Whether or not to remove any trailing whitespace from lines of a buffer before saving it."),
                "languages.$(language).remove_trailing_whitespace_on_save",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.remove_trailing_whitespace_on_save.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.remove_trailing_whitespace_on_save = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.ensure.final.newline.on.save", "Ensure Final Newline On Save"),
                lt("settings_ui.page_data.description.whether.or.not.to.ensure.there.s.a.single.newline.at.the.end.of.a.buffer.when.saving.it", "Whether or not to ensure there's a single newline at the end of a buffer when saving it."),
                "languages.$(language).ensure_final_newline_on_save",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.ensure_final_newline_on_save.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.ensure_final_newline_on_save = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.line.ending", "Line Ending"),
                lt("settings_ui.page_data.description.how.line.endings.should.be.handled.for.new.files.and.during.format.and.save.operations", "How line endings should be handled for new files and during format and save operations."),
                "languages.$(language).line_ending",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.line_ending.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.line_ending = value;
                                        })
                                    },
                metadata: Some(Box::new(SettingsFieldMetadata {
                    should_do_titlecase: Some(false),
                    ..Default::default()
                })),
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.formatter", "Formatter"),
                lt("settings_ui.page_data.description.how.to.perform.a.buffer.format", "How to perform a buffer format."),
                "languages.$(language).formatter",
                |settings_content| {
                                            language_settings_field(settings_content, |language| {
                                                language.formatter.as_ref()
                                            })
                                        },
                |settings_content, value, _| {
                                            language_settings_field_mut(
                                                settings_content,
                                                value,
                                                |language, value| {
                                                    language.formatter = value;
                                                },
                                            )
                                        },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.use.on.type.format", "Use On Type Format"),
                lt("settings_ui.page_data.description.whether.to.use.additional.lsp.queries.to.format.and.amend.the.code.after.every.trigger.symbol.input.defined.by.lsp.server.capabilities", "Whether to use additional LSP queries to format (and amend) the code after every \\\"trigger\\\" symbol input, defined by LSP server capabilities"),
                "languages.$(language).use_on_type_format",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.use_on_type_format.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.use_on_type_format = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.code.actions.on.format", "Code Actions On Format"),
                lt("settings_ui.page_data.description.additional.code.actions.to.run.when.formatting", "Additional code actions to run when formatting."),
                "languages.$(language).code_actions_on_format",
                |settings_content| {
                                            language_settings_field(settings_content, |language| {
                                                language.code_actions_on_format.as_ref()
                                            })
                                        },
                |settings_content, value, _| {
                                            language_settings_field_mut(
                                                settings_content,
                                                value,
                                                |language, value| {
                                                    language.code_actions_on_format = value;
                                                },
                                            )
                                        },
                files: USER | PROJECT),
        ]
    }

    fn autoclose_section() -> [SettingsPageItem; 5] {
        [
            section_header!("settings_ui.page_data.section.autoclose", "Autoclose"),
            setting_item!(
                lt("settings_ui.page_data.title.use.autoclose", "Use Autoclose"),
                lt("settings_ui.page_data.description.whether.to.automatically.type.closing.characters.for.you.for.example.when.you.type.zzz.will.automatically.add.a.closing.at.the.correct.position", "Whether to automatically type closing characters for you. For example, when you type '(', ZZZ will automatically add a closing ')' at the correct position."),
                "languages.$(language).use_autoclose",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.use_autoclose.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.use_autoclose = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.use.auto.surround", "Use Auto Surround"),
                lt("settings_ui.page_data.description.whether.to.automatically.surround.text.with.characters.for.you.for.example.when.you.select.text.and.type.zzz.will.automatically.surround.text.with", "Whether to automatically surround text with characters for you. For example, when you select text and type '(', ZZZ will automatically surround text with ()."),
                "languages.$(language).use_auto_surround",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.use_auto_surround.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.use_auto_surround = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.always.treat.brackets.as.autoclosed", "Always Treat Brackets As Autoclosed"),
                lt("settings_ui.page_data.description.controls.whether.the.closing.characters.are.always.skipped.over.and.auto.removed.no.matter.how.they.were.inserted", "Controls whether the closing characters are always skipped over and auto-removed no matter how they were inserted."),
                "languages.$(language).always_treat_brackets_as_autoclosed",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.always_treat_brackets_as_autoclosed.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.always_treat_brackets_as_autoclosed = value;
                                        })
                                    },
                files: USER | PROJECT),
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
            section_header!("settings_ui.page_data.section.whitespace", "Whitespace"),
            setting_item!(
                lt("settings_ui.page_data.title.show.whitespaces", "Show Whitespaces"),
                lt("settings_ui.page_data.description.whether.to.show.tabs.and.spaces.in.the.editor", "Whether to show tabs and spaces in the editor."),
                "languages.$(language).show_whitespaces",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.show_whitespaces.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.show_whitespaces = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.space.whitespace.indicator", "Space Whitespace Indicator"),
                lt("settings_ui.page_data.description.visible.character.used.to.render.space.characters.when.show.whitespaces.is.enabled.default", "Visible character used to render space characters when show_whitespaces is enabled (default: \"•\")"),
                "languages.$(language).whitespace_map.space",
                |settings_content| {
                                            language_settings_field(settings_content, |language| {
                                                language.whitespace_map.as_ref()?.space.as_ref()
                                            })
                                        },
                |settings_content, value, _| {
                                            language_settings_field_mut(
                                                settings_content,
                                                value,
                                                |language, value| {
                                                    language.whitespace_map.get_or_insert_default().space = value;
                                                },
                                            )
                                        },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.tab.whitespace.indicator", "Tab Whitespace Indicator"),
                lt("settings_ui.page_data.description.visible.character.used.to.render.tab.characters.when.show.whitespaces.is.enabled.default", "Visible character used to render tab characters when show_whitespaces is enabled (default: \"→\")"),
                "languages.$(language).whitespace_map.tab",
                |settings_content| {
                                            language_settings_field(settings_content, |language| {
                                                language.whitespace_map.as_ref()?.tab.as_ref()
                                            })
                                        },
                |settings_content, value, _| {
                                            language_settings_field_mut(
                                                settings_content,
                                                value,
                                                |language, value| {
                                                    language.whitespace_map.get_or_insert_default().tab = value;
                                                },
                                            )
                                        },
                files: USER | PROJECT),
        ]
    }

    fn completions_section() -> [SettingsPageItem; 8] {
        [
            section_header!("settings_ui.page_data.section.completions", "Completions"),
            setting_item!(
                lt("settings_ui.page_data.title.show.completions.on.input", "Show Completions On Input"),
                lt("settings_ui.page_data.description.whether.to.pop.the.completions.menu.while.typing.in.an.editor.without.explicitly.requesting.it", "Whether to pop the completions menu while typing in an editor without explicitly requesting it."),
                "languages.$(language).show_completions_on_input",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.show_completions_on_input.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.show_completions_on_input = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.show.completion.documentation", "Show Completion Documentation"),
                lt("settings_ui.page_data.description.whether.to.display.inline.and.alongside.documentation.for.items.in.the.completions.menu", "Whether to display inline and alongside documentation for items in the completions menu."),
                "languages.$(language).show_completion_documentation",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.show_completion_documentation.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.show_completion_documentation = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.words", "Words"),
                lt("settings_ui.page_data.description.controls.how.words.are.completed", "Controls how words are completed."),
                "languages.$(language).completions.words",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.completions.as_ref()?.words.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.completions.get_or_insert_default().words = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.words.min.length", "Words Min Length"),
                lt("settings_ui.page_data.description.how.many.characters.has.to.be.in.the.completions.query.to.automatically.show.the.words.based.completions", "How many characters has to be in the completions query to automatically show the words-based completions."),
                "languages.$(language).completions.words_min_length",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.completions.as_ref()?.words_min_length.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language
                                                .completions
                                                .get_or_insert_default()
                                                .words_min_length = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.completion.menu.scrollbar",
                    "Completion Menu Scrollbar"
                ),
                lt(
                    "settings_ui.page_data.description.when.to.show.the.scrollbar.in.the.completion.menu",
                    "When to show the scrollbar in the completion menu."
                ),
                "editor.completion_menu_scrollbar",
                |settings_content| { settings_content.editor.completion_menu_scrollbar.as_ref() },
                |settings_content, value, _| {
                    settings_content.editor.completion_menu_scrollbar = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.completion.detail.alignment",
                    "Completion Detail Alignment"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.align.detail.text.in.code.completions.context.menus.left.or.right",
                    "Whether to align detail text in code completions context menus left or right."
                ),
                "editor.completion_detail_alignment",
                |settings_content| { settings_content.editor.completion_detail_alignment.as_ref() },
                |settings_content, value, _| {
                    settings_content.editor.completion_detail_alignment = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.completion.menu.item.kind",
                    "Completion Menu Item Kind"
                ),
                lt(
                    "settings_ui.page_data.description.how.to.display.the.lsp.item.kind.function.method.variable.etc.of.each.entry.in.the.completions.menu",
                    "How to display the LSP item kind (function, method, variable, etc.) of each entry in the completions menu."
                ),
                "editor.completion_menu_item_kind",
                |settings_content| { settings_content.editor.completion_menu_item_kind.as_ref() },
                |settings_content, value, _| {
                    settings_content.editor.completion_menu_item_kind = value;
                }
            ),
        ]
    }

    fn inlay_hints_section() -> [SettingsPageItem; 10] {
        [
            section_header!("settings_ui.page_data.section.inlay.hints", "Inlay Hints"),
            setting_item!(
                lt("settings_ui.page_data.title.enabled", "Enabled"),
                lt("settings_ui.page_data.description.global.switch.to.toggle.hints.on.and.off", "Global switch to toggle hints on and off."),
                "languages.$(language).inlay_hints.enabled",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.inlay_hints.as_ref()?.enabled.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.inlay_hints.get_or_insert_default().enabled = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.show.value.hints", "Show Value Hints"),
                lt("settings_ui.page_data.description.global.switch.to.toggle.inline.values.on.and.off.when.debugging", "Global switch to toggle inline values on and off when debugging."),
                "languages.$(language).inlay_hints.show_value_hints",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.inlay_hints.as_ref()?.show_value_hints.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language
                                                .inlay_hints
                                                .get_or_insert_default()
                                                .show_value_hints = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.show.type.hints", "Show Type Hints"),
                lt("settings_ui.page_data.description.whether.type.hints.should.be.shown", "Whether type hints should be shown."),
                "languages.$(language).inlay_hints.show_type_hints",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.inlay_hints.as_ref()?.show_type_hints.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.inlay_hints.get_or_insert_default().show_type_hints = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.show.parameter.hints", "Show Parameter Hints"),
                lt("settings_ui.page_data.description.whether.parameter.hints.should.be.shown", "Whether parameter hints should be shown."),
                "languages.$(language).inlay_hints.show_parameter_hints",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.inlay_hints.as_ref()?.show_parameter_hints.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language
                                                .inlay_hints
                                                .get_or_insert_default()
                                                .show_parameter_hints = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.show.other.hints", "Show Other Hints"),
                lt("settings_ui.page_data.description.whether.other.hints.should.be.shown", "Whether other hints should be shown."),
                "languages.$(language).inlay_hints.show_other_hints",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.inlay_hints.as_ref()?.show_other_hints.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language
                                                .inlay_hints
                                                .get_or_insert_default()
                                                .show_other_hints = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.show.background", "Show Background"),
                lt("settings_ui.page_data.description.show.a.background.for.inlay.hints", "Show a background for inlay hints."),
                "languages.$(language).inlay_hints.show_background",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.inlay_hints.as_ref()?.show_background.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.inlay_hints.get_or_insert_default().show_background = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.edit.debounce.ms", "Edit Debounce Ms"),
                lt("settings_ui.page_data.description.whether.or.not.to.debounce.inlay.hints.updates.after.buffer.edits.set.to.0.to.disable.debouncing", "Whether or not to debounce inlay hints updates after buffer edits (set to 0 to disable debouncing)."),
                "languages.$(language).inlay_hints.edit_debounce_ms",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.inlay_hints.as_ref()?.edit_debounce_ms.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language
                                                .inlay_hints
                                                .get_or_insert_default()
                                                .edit_debounce_ms = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.scroll.debounce.ms", "Scroll Debounce Ms"),
                lt("settings_ui.page_data.description.whether.or.not.to.debounce.inlay.hints.updates.after.buffer.scrolls.set.to.0.to.disable.debouncing", "Whether or not to debounce inlay hints updates after buffer scrolls (set to 0 to disable debouncing)."),
                "languages.$(language).inlay_hints.scroll_debounce_ms",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.inlay_hints.as_ref()?.scroll_debounce_ms.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language
                                                .inlay_hints
                                                .get_or_insert_default()
                                                .scroll_debounce_ms = value;
                                        })
                                    },
                files: USER | PROJECT),
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
            section_header!("settings_ui.page_data.section.tasks", "Tasks"),
            setting_item!(
                lt("settings_ui.page_data.title.enabled", "Enabled"),
                lt("settings_ui.page_data.description.whether.tasks.are.enabled.for.this.language", "Whether tasks are enabled for this language."),
                "languages.$(language).tasks.enabled",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.tasks.as_ref()?.enabled.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.tasks.get_or_insert_default().enabled = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.variables", "Variables"),
                lt("settings_ui.page_data.description.extra.task.variables.to.set.for.a.particular.language", "Extra task variables to set for a particular language."),
                "languages.$(language).tasks.variables",
                |settings_content| {
                                            language_settings_field(settings_content, |language| {
                                                language.tasks.as_ref()?.variables.as_ref()
                                            })
                                        },
                |settings_content, value, _| {
                                            language_settings_field_mut(
                                                settings_content,
                                                value,
                                                |language, value| {
                                                    language.tasks.get_or_insert_default().variables = value;
                                                },
                                            )
                                        },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.prefer.lsp", "Prefer LSP"),
                lt("settings_ui.page_data.description.use.lsp.tasks.over.zzz.language.extension.tasks", "Use LSP tasks over ZZZ language extension tasks."),
                "languages.$(language).tasks.prefer_lsp",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.tasks.as_ref()?.prefer_lsp.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.tasks.get_or_insert_default().prefer_lsp = value;
                                        })
                                    },
                files: USER | PROJECT),
        ]
    }

    fn miscellaneous_section() -> [SettingsPageItem; 8] {
        [
            section_header!(
                "settings_ui.page_data.section.miscellaneous",
                "Miscellaneous"
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.language.detection",
                    "Language Detection"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.enable.automatic.language.detection.in.unsaved.buffers",
                    "Whether to enable automatic language detection in unsaved buffers."
                ),
                "language_detection",
                |settings_content| settings_content.editor.language_detection.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.language_detection = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.word.diff.enabled", "Word Diff Enabled"),
                lt("settings_ui.page_data.description.whether.to.enable.word.diff.highlighting.in.the.editor.when.enabled.changed.words.within.modified.lines.are.highlighted.to.show.exactly.what.changed", "Whether to enable word diff highlighting in the editor. When enabled, changed words within modified lines are highlighted to show exactly what changed."),
                "languages.$(language).word_diff_enabled",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.word_diff_enabled.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.word_diff_enabled = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.debuggers", "Debuggers"),
                lt("settings_ui.page_data.description.preferred.debuggers.for.this.language", "Preferred debuggers for this language."),
                "languages.$(language).debuggers",
                |settings_content| {
                                            language_settings_field(settings_content, |language| {
                                                language.debuggers.as_ref()
                                            })
                                        },
                |settings_content, value, _| {
                                            language_settings_field_mut(
                                                settings_content,
                                                value,
                                                |language, value| {
                                                    language.debuggers = value;
                                                },
                                            )
                                        },
                files: USER | PROJECT),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.middle.click.paste",
                    "Middle Click Paste"
                ),
                lt(
                    "settings_ui.page_data.description.enable.middle.click.paste.on.linux",
                    "Enable middle-click paste on Linux."
                ),
                "languages.$(language).editor.middle_click_paste",
                |settings_content| settings_content.editor.middle_click_paste.as_ref(),
                |settings_content, value, _| {
                    settings_content.editor.middle_click_paste = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.extend.comment.on.newline", "Extend Comment On Newline"),
                lt("settings_ui.page_data.description.whether.to.start.a.new.line.with.a.comment.when.a.previous.line.is.a.comment.as.well", "Whether to start a new line with a comment when a previous line is a comment as well."),
                "languages.$(language).extend_comment_on_newline",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.extend_comment_on_newline.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.extend_comment_on_newline = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.colorize.brackets", "Colorize Brackets"),
                lt("settings_ui.page_data.description.whether.to.colorize.brackets.in.the.editor", "Whether to colorize brackets in the editor."),
                "languages.$(language).colorize_brackets",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.colorize_brackets.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.colorize_brackets = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.vim.emacs.modeline.support", "Vim/Emacs Modeline Support"),
                lt("settings_ui.page_data.description.number.of.lines.to.search.for.modelines.set.to.0.to.disable", "Number of lines to search for modelines (set to 0 to disable)."),
                "modeline_lines",
                |settings_content| settings_content.modeline_lines.as_ref(),
                |settings_content, value, _| { settings_content.modeline_lines = value; },
                files: USER | PROJECT),
        ]
    }

    fn global_only_miscellaneous_sub_section() -> [SettingsPageItem; 3] {
        [
            setting_item!(
                lt("settings_ui.page_data.title.image.viewer", "Image Viewer"),
                lt(
                    "settings_ui.page_data.description.the.unit.for.image.file.sizes",
                    "The unit for image file sizes."
                ),
                "image_viewer.unit",
                |settings_content| {
                    settings_content
                        .image_viewer
                        .as_ref()
                        .and_then(|image_viewer| image_viewer.unit.as_ref())
                },
                |settings_content, value, _| {
                    settings_content.image_viewer.get_or_insert_default().unit = value;
                }
            ),
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
                    Some(usize::from(enabled))
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
            setting_item!(
                lt(
                    "settings_ui.page_data.title.drop.size.target",
                    "Drop Size Target"
                ),
                lt(
                    "settings_ui.page_data.description.relative.size.of.the.drop.target.in.the.editor.that.will.open.dropped.file.as.a.split.pane",
                    "Relative size of the drop target in the editor that will open dropped file as a split pane."
                ),
                "drop_target_size",
                |settings_content| settings_content.workspace.drop_target_size.as_ref(),
                |settings_content, value, _| {
                    settings_content.workspace.drop_target_size = value;
                }
            ),
        ]
    }

    let is_global = active_language().is_none();

    let code_lens_item = [setting_item!(
        lt("settings_ui.page_data.title.code.lens", "Code Lens"),
        lt(
            "settings_ui.page_data.description.whether.and.how.to.display.code.lenses.from.language.servers",
            "Whether and how to display code lenses from language servers."
        ),
        "code_lens",
        |settings_content| settings_content.editor.code_lens.as_ref(),
        |settings_content, value, _| {
            settings_content.editor.code_lens = value;
        }
    )];

    let lsp_document_colors_item = [setting_item!(
        lt(
            "settings_ui.page_data.title.lsp.document.colors",
            "LSP Document Colors"
        ),
        lt(
            "settings_ui.page_data.description.how.to.render.lsp.color.previews.in.the.editor",
            "How to render LSP color previews in the editor."
        ),
        "lsp_document_colors",
        |settings_content| settings_content.editor.lsp_document_colors.as_ref(),
        |settings_content, value, _| {
            settings_content.editor.lsp_document_colors = value;
        }
    )];

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
            section_header!("settings_ui.page_data.section.lsp", "LSP"),
            setting_item!(
                lt("settings_ui.page_data.title.enable.language.server", "Enable Language Server"),
                lt("settings_ui.page_data.description.whether.to.use.language.servers.to.provide.code.intelligence", "Whether to use language servers to provide code intelligence."),
                "languages.$(language).enable_language_server",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.enable_language_server.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.enable_language_server = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.language.servers", "Language Servers"),
                lt("settings_ui.page_data.description.the.list.of.language.servers.to.use.or.disable.for.this.language", "The list of language servers to use (or disable) for this language."),
                "languages.$(language).language_servers",
                |settings_content| {
                                            language_settings_field(settings_content, |language| {
                                                language.language_servers.as_ref()
                                            })
                                        },
                |settings_content, value, _| {
                                            language_settings_field_mut(
                                                settings_content,
                                                value,
                                                |language, value| {
                                                    language.language_servers = value;
                                                },
                                            )
                                        },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.linked.edits", "Linked Edits"),
                lt("settings_ui.page_data.description.whether.to.perform.linked.edits.of.associated.ranges.if.the.ls.supports.it.for.example.when.editing.opening.html.tag.the.contents.of.the.closing.html.tag.will.be.edited.as.well", "Whether to perform linked edits of associated ranges, if the LS supports it. For example, when editing opening <html> tag, the contents of the closing </html> tag will be edited as well."),
                "languages.$(language).linked_edits",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.linked_edits.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.linked_edits = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.go.to.definition.fallback",
                    "Go To Definition Fallback"
                ),
                lt(
                    "settings_ui.page_data.description.whether.to.follow.up.empty.go.to.definition.responses.from.the.language.server",
                    "Whether to follow-up empty Go to definition responses from the language server."
                ),
                "go_to_definition_fallback",
                |settings_content| { settings_content.editor.go_to_definition_fallback.as_ref() },
                |settings_content, value, _| {
                    settings_content.editor.go_to_definition_fallback = value;
                }
            ),
            setting_item!(
                lt(
                    "settings_ui.page_data.title.go.to.definition.scroll.strategy",
                    "Go To Definition Scroll Strategy"
                ),
                lt(
                    "settings_ui.page_data.description.how.to.scroll.the.target.into.view.when.navigating.to.a.definition.or.reference",
                    "How to scroll the target into view when navigating to a definition or reference."
                ),
                "go_to_definition_scroll_strategy",
                |settings_content| {
                    settings_content
                        .editor
                        .go_to_definition_scroll_strategy
                        .as_ref()
                },
                |settings_content, value, _| {
                    settings_content.editor.go_to_definition_scroll_strategy = value;
                }
            ),
            setting_item!(
                lt("settings_ui.page_data.title.semantic.tokens", "Semantic Tokens"),
                {
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
                "languages.$(language).semantic_tokens",
                |settings_content| { settings_content.project.all_languages.defaults.semantic_tokens.as_ref() },
                |settings_content, value, _| { settings_content.project.all_languages.defaults.semantic_tokens = value; },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.lsp.folding.ranges", "LSP Folding Ranges"),
                lt("settings_ui.page_data.description.when.enabled.use.folding.ranges.from.the.language.server.instead.of.indent.based.folding", "When enabled, use folding ranges from the language server instead of indent-based folding."),
                "languages.$(language).document_folding_ranges",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.document_folding_ranges.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.document_folding_ranges = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.lsp.document.symbols", "LSP Document Symbols"),
                lt("settings_ui.page_data.description.when.enabled.use.the.language.server.s.document.symbols.for.outlines.and.breadcrumbs.instead.of.tree.sitter", "When enabled, use the language server's document symbols for outlines and breadcrumbs instead of tree-sitter."),
                "languages.$(language).document_symbols",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.document_symbols.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.document_symbols = value;
                                        })
                                    },
                files: USER | PROJECT),
        ]
    }

    fn lsp_completions_section() -> [SettingsPageItem; 4] {
        [
            section_header!(
                "settings_ui.page_data.section.lsp.completions",
                "LSP Completions"
            ),
            setting_item!(
                lt("settings_ui.page_data.title.enabled", "Enabled"),
                lt("settings_ui.page_data.description.whether.to.fetch.lsp.completions.or.not", "Whether to fetch LSP completions or not."),
                "languages.$(language).completions.lsp",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.completions.as_ref()?.lsp.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.completions.get_or_insert_default().lsp = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.fetch.timeout.milliseconds", "Fetch Timeout (milliseconds)"),
                lt("settings_ui.page_data.description.when.fetching.lsp.completions.determines.how.long.to.wait.for.a.response.of.a.particular.server.set.to.0.to.wait.indefinitely", "When fetching LSP completions, determines how long to wait for a response of a particular server (set to 0 to wait indefinitely)."),
                "languages.$(language).completions.lsp_fetch_timeout_ms",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.completions.as_ref()?.lsp_fetch_timeout_ms.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language
                                                .completions
                                                .get_or_insert_default()
                                                .lsp_fetch_timeout_ms = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.insert.mode", "Insert Mode"),
                lt("settings_ui.page_data.description.controls.how.lsp.completions.are.inserted", "Controls how LSP completions are inserted."),
                "languages.$(language).completions.lsp_insert_mode",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.completions.as_ref()?.lsp_insert_mode.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.completions.get_or_insert_default().lsp_insert_mode = value;
                                        })
                                    },
                files: USER | PROJECT),
        ]
    }

    fn debugger_section() -> [SettingsPageItem; 2] {
        [
            section_header!("settings_ui.page_data.section.debuggers", "Debuggers"),
            setting_item!(
                lt("settings_ui.page_data.title.debuggers", "Debuggers"),
                lt("settings_ui.page_data.description.preferred.debuggers.for.this.language", "Preferred debuggers for this language."),
                "languages.$(language).debuggers",
                |settings_content| {
                                            language_settings_field(settings_content, |language| {
                                                language.debuggers.as_ref()
                                            })
                                        },
                |settings_content, value, _| {
                                            language_settings_field_mut(
                                                settings_content,
                                                value,
                                                |language, value| {
                                                    language.debuggers = value;
                                                },
                                            )
                                        },
                files: USER | PROJECT),
        ]
    }

    fn prettier_section() -> [SettingsPageItem; 5] {
        [
            section_header!("settings_ui.page_data.section.prettier", "Prettier"),
            setting_item!(
                lt("settings_ui.page_data.title.allowed", "Allowed"),
                lt("settings_ui.page_data.description.enables.or.disables.formatting.with.prettier.for.a.given.language", "Enables or disables formatting with Prettier for a given language."),
                "languages.$(language).prettier.allowed",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.prettier.as_ref()?.allowed.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.prettier.get_or_insert_default().allowed = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.parser", "Parser"),
                lt("settings_ui.page_data.description.forces.prettier.integration.to.use.a.specific.parser.name.when.formatting.files.with.the.language", "Forces Prettier integration to use a specific parser name when formatting files with the language."),
                "languages.$(language).prettier.parser",
                |settings_content| {
                                        language_settings_field(settings_content, |language| {
                                            language.prettier.as_ref()?.parser.as_ref()
                                        })
                                    },
                |settings_content, value, _| {
                                        language_settings_field_mut(settings_content, value, |language, value| {
                                            language.prettier.get_or_insert_default().parser = value;
                                        })
                                    },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.plugins", "Plugins"),
                lt("settings_ui.page_data.description.forces.prettier.integration.to.use.specific.plugins.when.formatting.files.with.the.language", "Forces Prettier integration to use specific plugins when formatting files with the language."),
                "languages.$(language).prettier.plugins",
                |settings_content| {
                                            language_settings_field(settings_content, |language| {
                                                language.prettier.as_ref()?.plugins.as_ref()
                                            })
                                        },
                |settings_content, value, _| {
                                            language_settings_field_mut(
                                                settings_content,
                                                value,
                                                |language, value| {
                                                    language.prettier.get_or_insert_default().plugins = value;
                                                },
                                            )
                                        },
                files: USER | PROJECT),
            setting_item!(
                lt("settings_ui.page_data.title.options", "Options"),
                lt("settings_ui.page_data.description.default.prettier.options.in.the.format.as.in.package.json.section.for.prettier", "Default Prettier options, in the format as in package.json section for Prettier."),
                "languages.$(language).prettier.options",
                |settings_content| {
                                            language_settings_field(settings_content, |language| {
                                                language.prettier.as_ref()?.options.as_ref()
                                            })
                                        },
                |settings_content, value, _| {
                                            language_settings_field_mut(
                                                settings_content,
                                                value,
                                                |language, value| {
                                                    language.prettier.get_or_insert_default().options = value;
                                                },
                                            )
                                        },
                files: USER | PROJECT),
        ]
    }

    concat_sections!(
        lsp_section(),
        lsp_completions_section(),
        debugger_section(),
        prettier_section(),
    )
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

    #[test]
    fn telegram_connection_settings_are_on_collaboration_page() {
        let has_telegram_proxy = |page: &SettingsPage| {
            page.items.iter().any(|item| {
                matches!(
                    item,
                    SettingsPageItem::SettingItem(item)
                        if item.field.json_path() == Some("telegram.proxy")
                )
            })
        };

        assert!(!has_telegram_proxy(&panels_page()));
        assert!(has_telegram_proxy(&collaboration_page()));
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
