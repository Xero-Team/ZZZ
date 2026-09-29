use editor::{EditorSettings, ui_scrollbar_settings_from_raw};
use gpui::Pixels;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use settings::{
    DockSide, ProjectPanelEntrySpacing, ProjectPanelSortMode, ProjectPanelSortOrder,
    ProjectPanelTitleTooltipDelay, RegisterSetting, Settings, ShowDiagnostics, ShowIndentGuides,
};
use ui::{
    px,
    scrollbars::{ScrollbarVisibility, ShowScrollbar},
};

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, RegisterSetting)]
pub struct ProjectPanelSettings {
    pub button: bool,
    pub hide_gitignore: bool,
    pub default_width: Pixels,
    pub title_tooltip_delay: ProjectPanelTitleTooltipDelay,
    pub dock: DockSide,
    pub entry_spacing: ProjectPanelEntrySpacing,
    pub file_icons: bool,
    pub folder_icons: bool,
    pub git_status: bool,
    pub indent_size: f32,
    pub indent_guides: IndentGuidesSettings,
    pub sticky_scroll: bool,
    pub auto_reveal_entries: bool,
    pub auto_fold_dirs: bool,
    pub bold_folder_labels: bool,
    pub starts_open: bool,
    pub scrollbar: ScrollbarSettings,
    pub show_diagnostics: ShowDiagnostics,
    pub hide_root: bool,
    pub hide_hidden: bool,
    pub drag_and_drop: bool,
    pub auto_open: AutoOpenSettings,
    pub sort_mode: ProjectPanelSortMode,
    pub sort_order: ProjectPanelSortOrder,
    pub diagnostic_badges: bool,
    pub git_status_indicator: bool,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct IndentGuidesSettings {
    pub show: ShowIndentGuides,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ScrollbarSettings {
    /// When to show the scrollbar in the project panel.
    ///
    /// Default: inherits editor scrollbar settings
    pub show: Option<ShowScrollbar>,
    /// Whether to allow horizontal scrolling in the project panel.
    /// When false, the view is locked to the leftmost position and long file names are clipped.
    ///
    /// Default: true
    pub horizontal_scroll: bool,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct AutoOpenSettings {
    pub on_create: bool,
    pub on_paste: bool,
    pub on_drop: bool,
    pub should_focus: bool,
}

impl AutoOpenSettings {
    #[inline]
    pub fn should_open_on_create(self) -> bool {
        self.on_create
    }

    #[inline]
    pub fn should_open_on_paste(self) -> bool {
        self.on_paste
    }

    #[inline]
    pub fn should_open_on_drop(self) -> bool {
        self.on_drop
    }

    #[inline]
    pub fn should_focus_on_open(self) -> bool {
        self.should_focus
    }
}

#[derive(Default)]
pub(crate) struct ProjectPanelScrollbarProxy;

impl ScrollbarVisibility for ProjectPanelScrollbarProxy {
    fn visibility(&self, cx: &ui::App) -> ShowScrollbar {
        ProjectPanelSettings::get_global(cx)
            .scrollbar
            .show
            .unwrap_or_else(|| EditorSettings::get_global(cx).scrollbar.show)
    }
}

impl Settings for ProjectPanelSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let project_panel = content.project_panel.clone().expect("value should be present");
        Self {
            button: project_panel.button.expect("button should be present"),
            hide_gitignore: project_panel.hide_gitignore.expect("hide_gitignore should be present"),
            default_width: px(project_panel.default_width.expect("default_width should be present")),
            dock: project_panel.dock.expect("dock should be present"),
            title_tooltip_delay: project_panel.title_tooltip_delay.expect("title_tooltip_delay should be present"),
            entry_spacing: project_panel.entry_spacing.expect("entry_spacing should be present"),
            file_icons: project_panel.file_icons.expect("file_icons should be present"),
            folder_icons: project_panel.folder_icons.expect("folder_icons should be present"),
            git_status: project_panel.git_status.expect("git_status should be present")
                && content
                    .git
                    .as_ref()
                    .expect("value should have the expected type")
                    .enabled
                    .expect("enabled should be present")
                    .is_git_status_enabled(),
            indent_size: project_panel.indent_size.expect("indent_size should be present"),
            indent_guides: IndentGuidesSettings {
                show: project_panel.indent_guides.expect("indent_guides should be present").show.expect("show should be present"),
            },
            sticky_scroll: project_panel.sticky_scroll.expect("sticky_scroll should be present"),
            auto_reveal_entries: project_panel.auto_reveal_entries.expect("auto_reveal_entries should be present"),
            auto_fold_dirs: project_panel.auto_fold_dirs.expect("auto_fold_dirs should be present"),
            bold_folder_labels: project_panel.bold_folder_labels.expect("bold_folder_labels should be present"),
            starts_open: project_panel.starts_open.expect("starts_open should be present"),
            scrollbar: {
                let scrollbar = project_panel.scrollbar.expect("scrollbar should be present");
                ScrollbarSettings {
                    show: scrollbar.show.map(ui_scrollbar_settings_from_raw),
                    horizontal_scroll: scrollbar.horizontal_scroll.expect("horizontal_scroll should be present"),
                }
            },
            show_diagnostics: project_panel.show_diagnostics.expect("show_diagnostics should be present"),
            hide_root: project_panel.hide_root.expect("hide_root should be present"),
            hide_hidden: project_panel.hide_hidden.expect("hide_hidden should be present"),
            drag_and_drop: project_panel.drag_and_drop.expect("drag_and_drop should be present"),
            auto_open: {
                let auto_open = project_panel.auto_open.expect("auto_open should be present");
                AutoOpenSettings {
                    on_create: auto_open.on_create.expect("on_create should be present"),
                    on_paste: auto_open.on_paste.expect("on_paste should be present"),
                    on_drop: auto_open.on_drop.expect("on_drop should be present"),
                    should_focus: auto_open.should_focus.expect("should_focus should be present"),
                }
            },
            sort_mode: project_panel.sort_mode.expect("sort_mode should be present"),
            sort_order: project_panel.sort_order.expect("sort_order should be present"),
            diagnostic_badges: project_panel.diagnostic_badges.expect("diagnostic_badges should be present"),
            git_status_indicator: project_panel.git_status_indicator.expect("git_status_indicator should be present"),
        }
    }
}
