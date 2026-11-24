use gpui::{Pixels, px};
use settings::{RegisterSetting, Settings};

/// The settings for the markdown preview.
#[derive(Clone, Copy, Debug, Default, RegisterSetting)]
pub struct MarkdownPreviewSettings {
    /// The maximum width of the rendered markdown content, or `None` to render
    /// content edge to edge.
    pub max_width: Option<Pixels>,
    /// Whether to render LaTeX math (`$...$`, `$$...$$`, `\(...\)`, `\[...\]`
    /// and fenced `math` blocks) as typeset formulas.
    pub render_math: bool,
    /// Whether the preview may fetch `http(s)` images. When disabled, a remote
    /// image renders as a placeholder and no request is made.
    pub allow_remote_images: bool,
}

impl Settings for MarkdownPreviewSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let content = content.markdown_preview.clone().unwrap_or_default();
        let max_width = if content.limit_content_width.unwrap_or(true) {
            content.max_width.map(px)
        } else {
            None
        };
        Self {
            max_width,
            render_math: content.render_math.unwrap_or(true),
            allow_remote_images: content.allow_remote_images.unwrap_or(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_images_are_disabled_by_default() {
        let content = settings::SettingsContent::default();
        assert!(!MarkdownPreviewSettings::from_settings(&content).allow_remote_images);
    }

    #[test]
    fn remote_images_can_be_enabled() {
        let content = settings::SettingsContent {
            markdown_preview: Some(settings::MarkdownPreviewSettingsContent {
                allow_remote_images: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(MarkdownPreviewSettings::from_settings(&content).allow_remote_images);
    }
}
