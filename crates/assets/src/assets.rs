// This crate was essentially pulled out verbatim from main `zzz` crate to avoid having to run RustEmbed macro whenever zzz has to be rebuilt. It saves a second or two on an incremental build.

use anyhow::Context as _;
use gpui::{App, AssetSource, Result, SharedString};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../assets"]
#[include = "fonts/**/*"]
#[include = "icons/**/*"]
#[include = "images/**/*"]
#[include = "themes/**/*"]
#[exclude = "themes/src/*"]
#[include = "sounds/**/*"]
#[include = "prompts/**/*"]
#[include = "*.md"]
#[exclude = "*.DS_Store"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<std::borrow::Cow<'static, [u8]>>> {
        Self::get(path)
            .map(|f| Some(f.data))
            .with_context(|| format!("loading asset at path {path:?}"))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter_map(|p| {
                if p.starts_with(path) {
                    Some(p.into())
                } else {
                    None
                }
            })
            .collect())
    }
}

impl Assets {
    /// Populate the [`TextSystem`] of the given [`AppContext`] with all `.ttf` fonts in the `fonts` directory.
    pub fn load_fonts(&self, cx: &App) -> anyhow::Result<()> {
        let font_paths = self.list("fonts")?;
        let mut embedded_fonts = Vec::new();
        for font_path in font_paths {
            if font_path.ends_with(".ttf") {
                let font_bytes = cx
                    .asset_source()
                    .load(&font_path)?
                    .expect("Assets should never return None");
                embedded_fonts.push(font_bytes);
            }
        }

        cx.text_system().add_fonts(embedded_fonts)
    }

    pub fn load_test_fonts(&self, cx: &App) {
        cx.text_system()
            .add_fonts(vec![
                self.load("fonts/lilex/Lilex-Regular.ttf").unwrap().unwrap(),
            ])
            .unwrap()
    }
}

/// The built-in documentation, embedded so that it is available offline.
#[derive(RustEmbed)]
#[folder = "../../docs/src"]
#[include = "**/*.md"]
#[exclude = "*.DS_Store"]
pub struct Docs;

impl AssetSource for Docs {
    fn load(&self, path: &str) -> Result<Option<std::borrow::Cow<'static, [u8]>>> {
        Self::get(path)
            .map(|file| Some(file.data))
            .with_context(|| format!("loading docs at path {path:?}"))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter_map(|p| {
                if p.starts_with(path) {
                    Some(p.into())
                } else {
                    None
                }
            })
            .collect())
    }
}

/// Looks up an embedded documentation file, trying `path` and then `path.md`.
pub fn lookup_docs(path: &str) -> Option<rust_embed::EmbeddedFile> {
    Docs::get(path).or_else(|| Docs::get(&format!("{path}.md")))
}

/// Returns the paths of every embedded documentation file.
pub fn all_docs() -> Vec<SharedString> {
    Docs::iter().map(|path| SharedString::from(path.to_string())).collect()
}

/// Returns the contents of an embedded documentation file as UTF-8 text.
pub fn lookup_docs_text(path: &str) -> Option<String> {
    let file = lookup_docs(path)?;
    String::from_utf8(file.data.into_owned()).ok()
}

#[cfg(test)]
mod tests {
    use gpui::AssetSource as _;

    use super::Assets;

    #[test]
    fn load_returns_embedded_asset_bytes() {
        let asset = Assets
            .load("fonts/lilex/Lilex-Regular.ttf")
            .unwrap()
            .unwrap();

        assert!(!asset.is_empty());
    }

    #[test]
    fn load_returns_contextual_error_for_missing_asset() {
        let error = Assets.load("fonts/does-not-exist.ttf").unwrap_err();

        assert!(
            error
                .to_string()
                .contains("loading asset at path \"fonts/does-not-exist.ttf\"")
        );
    }

    #[test]
    fn list_filters_to_requested_prefix() {
        let fonts = Assets.list("fonts").unwrap();

        assert!(!fonts.is_empty());
        assert!(fonts.iter().all(|path| path.as_ref().starts_with("fonts/")));
        assert!(
            fonts
                .iter()
                .any(|path| path.as_ref() == "fonts/lilex/Lilex-Regular.ttf")
        );
        assert!(
            fonts
                .iter()
                .all(|path| !path.as_ref().starts_with("icons/"))
        );
    }
}
