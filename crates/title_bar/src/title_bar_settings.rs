use gpui::WindowButtonLayout;
use settings::{RegisterSetting, Settings, SettingsContent};

#[derive(Copy, Clone, Debug, RegisterSetting)]
pub struct TitleBarSettings {
    pub show_branch_status_icon: bool,
    pub show_branch_name: bool,
    pub show_project_items: bool,
    pub show_user_menu: bool,
    pub show_menus: bool,
    pub button_layout: Option<WindowButtonLayout>,
}

impl Settings for TitleBarSettings {
    fn from_settings(s: &SettingsContent) -> Self {
        let content = s.title_bar.clone().expect("value should be present");
        TitleBarSettings {
            show_branch_status_icon: content.show_branch_status_icon.expect("show_branch_status_icon should be present"),
            show_branch_name: content.show_branch_name.expect("show_branch_name should be present"),
            show_project_items: content.show_project_items.expect("show_project_items should be present"),
            show_user_menu: content.show_user_menu.expect("show_user_menu should be present"),
            show_menus: content.show_menus.expect("show_menus should be present"),
            button_layout: content.button_layout.unwrap_or_default().into_layout(),
        }
    }
}
