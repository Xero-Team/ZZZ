use editor::{EditorSettings, ui_scrollbar_settings_from_raw};
use gpui::{App, Pixels};
use settings::RegisterSetting;
pub use settings::{DockSide, Settings, ShowIndentGuides};
use ui::scrollbars::{ScrollbarVisibility, ShowScrollbar};

#[derive(Debug, Clone, Copy, PartialEq, RegisterSetting)]
pub struct OutlinePanelSettings {
    pub button: bool,
    pub default_width: Pixels,
    pub dock: DockSide,
    pub file_icons: bool,
    pub folder_icons: bool,
    pub git_status: bool,
    pub indent_size: f32,
    pub indent_guides: IndentGuidesSettings,
    pub auto_reveal_entries: bool,
    pub auto_fold_dirs: bool,
    pub scrollbar: ScrollbarSettings,
    pub expand_outlines_with_depth: usize,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ScrollbarSettings {
    /// When to show the scrollbar in the project panel.
    ///
    /// Default: inherits editor scrollbar settings
    pub show: Option<ShowScrollbar>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct IndentGuidesSettings {
    pub show: ShowIndentGuides,
}

#[derive(Default)]
pub(crate) struct OutlinePanelSettingsScrollbarProxy;

impl ScrollbarVisibility for OutlinePanelSettingsScrollbarProxy {
    fn visibility(&self, cx: &App) -> ShowScrollbar {
        OutlinePanelSettings::get_global(cx)
            .scrollbar
            .show
            .unwrap_or_else(|| EditorSettings::get_global(cx).scrollbar.show)
    }
}

impl Settings for OutlinePanelSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let panel = content
            .outline_panel
            .as_ref()
            .expect("value should have the expected type");
        Self {
            button: panel.button.expect("button should be present"),
            default_width: panel
                .default_width
                .map(gpui::px)
                .expect("map should be present"),
            dock: panel.dock.expect("dock should be present"),
            file_icons: panel.file_icons.expect("file_icons should be present"),
            folder_icons: panel.folder_icons.expect("folder_icons should be present"),
            git_status: panel.git_status.expect("git_status should be present")
                && content
                    .git
                    .as_ref()
                    .expect("value should have the expected type")
                    .enabled
                    .expect("enabled should be present")
                    .is_git_status_enabled(),
            indent_size: panel.indent_size.expect("indent_size should be present"),
            indent_guides: IndentGuidesSettings {
                show: panel
                    .indent_guides
                    .expect("indent_guides should be present")
                    .show
                    .expect("show should be present"),
            },
            auto_reveal_entries: panel
                .auto_reveal_entries
                .expect("auto_reveal_entries should be present"),
            auto_fold_dirs: panel
                .auto_fold_dirs
                .expect("auto_fold_dirs should be present"),
            scrollbar: ScrollbarSettings {
                show: panel
                    .scrollbar
                    .expect("scrollbar should be present")
                    .show
                    .map(ui_scrollbar_settings_from_raw),
            },
            expand_outlines_with_depth: panel
                .expand_outlines_with_depth
                .expect("expand_outlines_with_depth should be present"),
        }
    }
}
