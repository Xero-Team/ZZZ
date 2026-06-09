use anyhow::Context as _;
use gpui::{App, Global, PromptButton};
use indexmap::IndexMap;
use parking_lot::RwLock;
use rust_embed::RustEmbed;
use serde::Deserialize;
use settings::Settings;
use std::sync::Arc;
use sys_locale::get_locale;
use util::asset_str;

const DEFAULT_LOCALE: &str = "en";
const CHINESE_SIMPLIFIED_LOCALE: &str = "zh-CN";

#[derive(RustEmbed)]
#[folder = "../../assets"]
#[include = "locales/*.json"]
#[exclude = "*.DS_Store"]
struct LocaleAssets;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveLocale {
    English,
    SimplifiedChinese,
}

impl ActiveLocale {
    pub fn language_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::SimplifiedChinese => "简体中文",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::English => DEFAULT_LOCALE,
            Self::SimplifiedChinese => CHINESE_SIMPLIFIED_LOCALE,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct LocaleCatalog {
    entries: IndexMap<String, String>,
}

impl LocaleCatalog {
    fn lookup(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }
}

#[derive(Deserialize)]
struct LocaleDocument {
    entries: IndexMap<String, String>,
}

pub struct I18nService {
    active_locale: ActiveLocale,
    fallback_catalog: LocaleCatalog,
    active_catalog: LocaleCatalog,
}

struct GlobalI18nService(Arc<RwLock<I18nService>>);

impl Global for GlobalI18nService {}

pub fn init(cx: &mut App) {
    let configured_locale = resolve_locale(DisplayLanguageSetting::get_global(cx));
    let fallback_catalog = load_catalog(DEFAULT_LOCALE).expect("missing default locale catalog");
    let active_catalog = if configured_locale == ActiveLocale::English {
        fallback_catalog.clone()
    } else {
        load_catalog(configured_locale.code()).unwrap_or_else(|_| fallback_catalog.clone())
    };

    cx.set_global(GlobalI18nService(Arc::new(RwLock::new(I18nService {
        active_locale: configured_locale,
        fallback_catalog,
        active_catalog,
    }))));
}

pub fn reload(cx: &mut App) -> bool {
    let configured_locale = resolve_locale(DisplayLanguageSetting::get_global(cx));
    let service = cx.global::<GlobalI18nService>().0.clone();
    let mut service = service.write();

    if service.active_locale == configured_locale {
        return false;
    }

    service.active_locale = configured_locale;
    service.active_catalog = if configured_locale == ActiveLocale::English {
        service.fallback_catalog.clone()
    } else {
        load_catalog(configured_locale.code()).unwrap_or_else(|_| service.fallback_catalog.clone())
    };

    true
}

pub fn active_locale(cx: &App) -> ActiveLocale {
    cx.global::<GlobalI18nService>().0.read().active_locale
}

pub fn t(cx: &App, key: &str) -> String {
    let service = cx.global::<GlobalI18nService>().0.read();
    service
        .active_catalog
        .lookup(key)
        .or_else(|| service.fallback_catalog.lookup(key))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| key.to_owned())
}

pub fn tr(cx: &App, key: &str, fallback: &str) -> String {
    let service = cx.global::<GlobalI18nService>().0.read();
    service
        .active_catalog
        .lookup(key)
        .or_else(|| service.fallback_catalog.lookup(key))
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| fallback.to_owned())
}

pub fn prompt_button(cx: &App, key: &str, fallback: &str) -> PromptButton {
    PromptButton::new(tr(cx, key, fallback))
}

fn resolve_locale(setting: DisplayLanguageSetting) -> ActiveLocale {
    match setting {
        DisplayLanguageSetting::English => ActiveLocale::English,
        DisplayLanguageSetting::SimplifiedChinese => ActiveLocale::SimplifiedChinese,
        DisplayLanguageSetting::Auto => resolve_system_locale(),
    }
}

fn resolve_system_locale() -> ActiveLocale {
    let locale = get_locale().unwrap_or_else(|| DEFAULT_LOCALE.to_owned());
    if locale.starts_with("zh") {
        ActiveLocale::SimplifiedChinese
    } else {
        ActiveLocale::English
    }
}

fn load_catalog(locale: &str) -> anyhow::Result<LocaleCatalog> {
    let path = format!("locales/{locale}.json");
    let content = asset_str::<LocaleAssets>(&path);
    let document: LocaleDocument =
        serde_json::from_str(content.as_ref()).with_context(|| format!("parsing {path}"))?;
    Ok(LocaleCatalog {
        entries: document.entries,
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayLanguageSetting {
    #[default]
    Auto,
    English,
    SimplifiedChinese,
}

impl DisplayLanguageSetting {
    pub fn get_global(cx: &App) -> Self {
        WorkspaceLanguageSettings::get_global(cx).display_language
    }
}

#[derive(settings::RegisterSetting)]
pub struct WorkspaceLanguageSettings {
    pub display_language: DisplayLanguageSetting,
}

impl Settings for WorkspaceLanguageSettings {
    fn from_settings(content: &settings::SettingsContent) -> Self {
        let display_language = content
            .workspace
            .display_language
            .unwrap_or(settings::DisplayLanguage::Auto);

        Self {
            display_language: match display_language {
                settings::DisplayLanguage::Auto => DisplayLanguageSetting::Auto,
                settings::DisplayLanguage::En => DisplayLanguageSetting::English,
                settings::DisplayLanguage::ZhCn => DisplayLanguageSetting::SimplifiedChinese,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{BorrowAppContext, TestAppContext};
    use settings::SettingsStore;

    #[gpui::test]
    fn reload_switches_to_simplified_chinese(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let store = SettingsStore::test(cx);
            cx.set_global(store);

            init(cx);
            assert_eq!(active_locale(cx), ActiveLocale::English);
            assert_eq!(tr(cx, "menu.about", "About ZZZ"), "About ZZZ");

            cx.update_global::<SettingsStore, _>(|store, cx| {
                store.update_user_settings(cx, |settings| {
                    settings.workspace.display_language = Some(settings::DisplayLanguage::ZhCn);
                });
            });

            assert!(reload(cx));
            assert_eq!(active_locale(cx), ActiveLocale::SimplifiedChinese);
            assert_eq!(tr(cx, "menu.about", "About ZZZ"), "关于 ZZZ");
        });
    }
}
