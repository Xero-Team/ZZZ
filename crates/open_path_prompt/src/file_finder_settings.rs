use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use settings::{RegisterSetting, Settings};

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, RegisterSetting)]
pub struct FileFinderSettings {
    pub file_icons: bool,
    pub modal_max_width: FileFinderWidth,
    pub skip_focus_for_active_in_search: bool,
    pub include_ignored: Option<bool>,
}

impl Settings for FileFinderSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let file_finder = content.file_finder.as_ref().expect("value should have the expected type");

        Self {
            file_icons: file_finder.file_icons.expect("file_icons should be present"),
            modal_max_width: file_finder.modal_max_width.expect("modal_max_width should be present").into(),
            skip_focus_for_active_in_search: file_finder.skip_focus_for_active_in_search.expect("skip_focus_for_active_in_search should be present"),
            include_ignored: match file_finder.include_ignored.expect("include_ignored should be present") {
                settings::IncludeIgnoredContent::All => Some(true),
                settings::IncludeIgnoredContent::Indexed => Some(false),
                settings::IncludeIgnoredContent::Smart => None,
            },
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FileFinderWidth {
    #[default]
    Small,
    Medium,
    Large,
    XLarge,
    Full,
}

impl From<settings::FileFinderWidthContent> for FileFinderWidth {
    fn from(content: settings::FileFinderWidthContent) -> Self {
        match content {
            settings::FileFinderWidthContent::Small => FileFinderWidth::Small,
            settings::FileFinderWidthContent::Medium => FileFinderWidth::Medium,
            settings::FileFinderWidthContent::Large => FileFinderWidth::Large,
            settings::FileFinderWidthContent::XLarge => FileFinderWidth::XLarge,
            settings::FileFinderWidthContent::Full => FileFinderWidth::Full,
        }
    }
}
