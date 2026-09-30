use std::{num::NonZeroUsize, time::Duration};

use crate::DockPosition;
use collections::HashMap;
use serde::Deserialize;
use settings::CommandAliasTarget;
pub use settings::{
    AutosaveSetting, BottomDockLayout, DisplayLanguage, EncodingDisplayOptions, InactiveOpacity,
    PaneSplitDirectionHorizontal, PaneSplitDirectionVertical, RegisterSetting,
    RestoreOnStartupBehavior, Settings, StatusBarPosition,
};

#[derive(RegisterSetting)]
pub struct WorkspaceSettings {
    pub display_language: DisplayLanguage,
    pub active_pane_modifiers: ActivePanelModifiers,
    pub bottom_dock_layout: settings::BottomDockLayout,
    pub pane_split_direction_horizontal: settings::PaneSplitDirectionHorizontal,
    pub pane_split_direction_vertical: settings::PaneSplitDirectionVertical,
    pub centered_layout: settings::CenteredLayoutSettings,
    pub confirm_quit: bool,
    pub show_call_status_icon: bool,
    pub autosave: AutosaveSetting,
    pub restore_on_startup: settings::RestoreOnStartupBehavior,
    pub cli_default_open_behavior: settings::CliDefaultOpenBehavior,
    pub default_open_behavior: settings::DefaultOpenBehavior,
    pub restore_on_file_reopen: bool,
    pub reveal_if_open: bool,
    pub drop_target_size: f32,
    pub use_system_path_prompts: bool,
    pub use_system_prompts: bool,
    pub command_aliases: HashMap<String, CommandAliasTarget>,
    pub max_tabs: Option<NonZeroUsize>,
    pub when_closing_with_no_tabs: settings::CloseWindowWhenNoItems,
    pub on_new_window: settings::OnNewWindow,
    pub on_last_window_closed: settings::OnLastWindowClosed,
    pub text_rendering_mode: settings::TextRenderingMode,
    pub resize_all_panels_in_dock: Vec<DockPosition>,
    pub close_on_file_delete: bool,
    pub close_panel_on_toggle: bool,
    pub use_system_window_tabs: bool,
    pub fullscreen_mode: settings::FullscreenMode,
    pub zoomed_padding: bool,
    pub window_decorations: settings::WindowDecorations,
    pub focus_follows_mouse: FocusFollowsMouse,
}

#[derive(Copy, Clone, Deserialize)]
pub struct FocusFollowsMouse {
    pub enabled: bool,
    pub debounce: Duration,
}

#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct ActivePanelModifiers {
    /// Size of the border surrounding the active pane.
    /// When set to 0, the active pane doesn't have any border.
    /// The border is drawn inset.
    ///
    /// Default: `0.0`
    // TODO: make this not an option, it is never None
    pub border_size: Option<f32>,
    /// Opacity of inactive panels.
    /// When set to 1.0, the inactive panes have the same opacity as the active one.
    /// If set to 0, the inactive panes content will not be visible at all.
    /// Values are clamped to the [0.0, 1.0] range.
    ///
    /// Default: `1.0`
    // TODO: make this not an option, it is never None
    pub inactive_opacity: Option<InactiveOpacity>,
}

#[derive(Deserialize, RegisterSetting)]
pub struct TabBarSettings {
    pub show: bool,
    pub show_nav_history_buttons: bool,
    pub show_tab_bar_buttons: bool,
    pub show_pinned_tabs_in_separate_row: bool,
}

impl Settings for WorkspaceSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let workspace = &content.workspace;
        Self {
            display_language: workspace
                .display_language
                .expect("display_language should be present"),
            active_pane_modifiers: ActivePanelModifiers {
                border_size: Some(
                    workspace
                        .active_pane_modifiers
                        .expect("active_pane_modifiers should be present")
                        .border_size
                        .expect("border_size should be present"),
                ),
                inactive_opacity: Some(
                    workspace
                        .active_pane_modifiers
                        .expect("active_pane_modifiers should be present")
                        .inactive_opacity
                        .expect("inactive_opacity should be present"),
                ),
            },
            bottom_dock_layout: workspace
                .bottom_dock_layout
                .expect("bottom_dock_layout should be present"),
            pane_split_direction_horizontal: workspace
                .pane_split_direction_horizontal
                .expect("pane_split_direction_horizontal should be present"),
            pane_split_direction_vertical: workspace
                .pane_split_direction_vertical
                .expect("pane_split_direction_vertical should be present"),
            centered_layout: workspace
                .centered_layout
                .expect("centered_layout should be present"),
            confirm_quit: workspace
                .confirm_quit
                .expect("confirm_quit should be present"),
            show_call_status_icon: workspace
                .show_call_status_icon
                .expect("show_call_status_icon should be present"),
            autosave: workspace.autosave.expect("autosave should be present"),
            restore_on_startup: workspace
                .restore_on_startup
                .expect("restore_on_startup should be present"),
            cli_default_open_behavior: workspace
                .cli_default_open_behavior
                .expect("cli_default_open_behavior should be present"),
            default_open_behavior: workspace
                .default_open_behavior
                .expect("default_open_behavior should be present"),
            restore_on_file_reopen: workspace
                .restore_on_file_reopen
                .expect("restore_on_file_reopen should be present"),
            reveal_if_open: workspace
                .reveal_if_open
                .expect("reveal_if_open should be present"),
            drop_target_size: workspace
                .drop_target_size
                .expect("drop_target_size should be present"),
            use_system_path_prompts: workspace
                .use_system_path_prompts
                .expect("use_system_path_prompts should be present"),
            use_system_prompts: workspace
                .use_system_prompts
                .expect("use_system_prompts should be present"),
            command_aliases: workspace.command_aliases.clone(),
            max_tabs: workspace.max_tabs,
            when_closing_with_no_tabs: workspace
                .when_closing_with_no_tabs
                .expect("when_closing_with_no_tabs should be present"),
            on_new_window: workspace
                .on_new_window
                .expect("on_new_window should be present"),
            on_last_window_closed: workspace
                .on_last_window_closed
                .expect("on_last_window_closed should be present"),
            text_rendering_mode: workspace
                .text_rendering_mode
                .expect("text_rendering_mode should be present"),
            resize_all_panels_in_dock: workspace
                .resize_all_panels_in_dock
                .clone()
                .expect("value should be present")
                .into_iter()
                .map(Into::into)
                .collect(),
            close_on_file_delete: workspace
                .close_on_file_delete
                .expect("close_on_file_delete should be present"),
            close_panel_on_toggle: workspace
                .close_panel_on_toggle
                .expect("close_panel_on_toggle should be present"),
            use_system_window_tabs: workspace
                .use_system_window_tabs
                .expect("use_system_window_tabs should be present"),
            fullscreen_mode: workspace
                .fullscreen_mode
                .expect("fullscreen_mode should be present"),
            zoomed_padding: workspace
                .zoomed_padding
                .expect("zoomed_padding should be present"),
            window_decorations: workspace
                .window_decorations
                .expect("window_decorations should be present"),
            focus_follows_mouse: FocusFollowsMouse {
                enabled: workspace
                    .focus_follows_mouse
                    .expect("focus_follows_mouse should be present")
                    .enabled
                    .unwrap_or(false),
                debounce: Duration::from_millis(
                    workspace
                        .focus_follows_mouse
                        .expect("focus_follows_mouse should be present")
                        .debounce_ms
                        .unwrap_or(250),
                ),
            },
        }
    }
}

impl Settings for TabBarSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let tab_bar = content.tab_bar.clone().expect("value should be present");
        TabBarSettings {
            show: tab_bar.show.expect("show should be present"),
            show_nav_history_buttons: tab_bar
                .show_nav_history_buttons
                .expect("show_nav_history_buttons should be present"),
            show_tab_bar_buttons: tab_bar
                .show_tab_bar_buttons
                .expect("show_tab_bar_buttons should be present"),
            show_pinned_tabs_in_separate_row: tab_bar
                .show_pinned_tabs_in_separate_row
                .expect("show_pinned_tabs_in_separate_row should be present"),
        }
    }
}

#[derive(Deserialize, RegisterSetting)]
pub struct StatusBarSettings {
    pub show: bool,
    pub show_active_file: bool,
    pub active_language_button: bool,
    pub cursor_position_button: bool,
    pub line_endings_button: bool,
    pub active_encoding_button: EncodingDisplayOptions,
    pub position: StatusBarPosition,
}

impl Settings for StatusBarSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let status_bar = content.status_bar.clone().expect("value should be present");
        StatusBarSettings {
            show: status_bar.show.expect("show should be present"),
            show_active_file: status_bar
                .show_active_file
                .expect("show_active_file should be present"),
            active_language_button: status_bar
                .active_language_button
                .expect("active_language_button should be present"),
            cursor_position_button: status_bar
                .cursor_position_button
                .expect("cursor_position_button should be present"),
            line_endings_button: status_bar
                .line_endings_button
                .expect("line_endings_button should be present"),
            active_encoding_button: status_bar
                .active_encoding_button
                .expect("active_encoding_button should be present"),
            position: status_bar.position.expect("position should be present"),
        }
    }
}
