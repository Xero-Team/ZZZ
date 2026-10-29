mod extension_lsp_adapter;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use extension::{ExtensionGrammarProxy, ExtensionHostProxy, ExtensionLanguageProxy};
use gpui::{App, Entity, WeakEntity};
use language::{LanguageMatcher, LanguageName, LanguageRegistry, LoadedLanguage};
use project::LspStore;

#[derive(Clone)]
pub enum LspAccess {
    ViaLspStore(WeakEntity<LspStore>),
    ViaWorkspaces(Arc<dyn Fn(&mut App) -> Result<Vec<Entity<LspStore>>> + Send + Sync + 'static>),
    Noop,
}

pub fn init(
    lsp_access: LspAccess,
    extension_host_proxy: Arc<ExtensionHostProxy>,
    language_registry: Arc<LanguageRegistry>,
) {
    let proxy = LanguageRegistryProxy {
        language_registry,
        lsp_access,
    };
    extension_host_proxy.register_grammar_proxy(proxy.clone());
    extension_host_proxy.register_language_proxy(proxy.clone());
    extension_host_proxy.register_language_server_proxy(proxy);
}

/// Bridges the extension host proxies (grammars, languages, language servers)
/// onto the shared [`LanguageRegistry`].
#[derive(Clone)]
struct LanguageRegistryProxy {
    language_registry: Arc<LanguageRegistry>,
    lsp_access: LspAccess,
}

impl ExtensionGrammarProxy for LanguageRegistryProxy {
    #[ztracing::instrument(skip_all)]
    fn register_grammars(&self, grammars: Vec<(Arc<str>, PathBuf)>) {
        self.language_registry.register_wasm_grammars(grammars)
    }
}

impl ExtensionLanguageProxy for LanguageRegistryProxy {
    fn register_language(
        &self,
        language: LanguageName,
        grammar: Option<Arc<str>>,
        matcher: LanguageMatcher,
        hidden: bool,
        load: Arc<dyn Fn() -> Result<LoadedLanguage> + Send + Sync + 'static>,
    ) {
        self.language_registry
            .register_language(language, grammar, matcher, hidden, None, load);
    }

    fn remove_languages(
        &self,
        languages_to_remove: &[LanguageName],
        grammars_to_remove: &[Arc<str>],
    ) {
        self.language_registry
            .remove_languages(languages_to_remove, grammars_to_remove);
    }
}
