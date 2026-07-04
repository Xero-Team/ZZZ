use std::sync::Arc;

use anyhow::Result;
use cloud_llm_client::WebSearchResponse;
use collections::HashMap;
use gpui::{App, AppContext as _, Context, Entity, Global, SharedString, Task};

pub fn init(cx: &mut App) {
    let registry = cx.new(|_cx| WebSearchRegistry::default());
    cx.set_global(GlobalWebSearchRegistry(registry));
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Ord, PartialOrd)]
pub struct WebSearchProviderId(pub SharedString);

pub trait WebSearchProvider {
    fn id(&self) -> WebSearchProviderId;
    fn search(&self, query: String, cx: &mut App) -> Task<Result<WebSearchResponse>>;
}

struct GlobalWebSearchRegistry(Entity<WebSearchRegistry>);

impl Global for GlobalWebSearchRegistry {}

#[derive(Default)]
pub struct WebSearchRegistry {
    providers: HashMap<WebSearchProviderId, Arc<dyn WebSearchProvider>>,
    active_provider: Option<Arc<dyn WebSearchProvider>>,
}

impl WebSearchRegistry {
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<GlobalWebSearchRegistry>().0.clone()
    }

    pub fn read_global(cx: &App) -> &Self {
        cx.global::<GlobalWebSearchRegistry>().0.read(cx)
    }

    pub fn providers(&self) -> impl Iterator<Item = &Arc<dyn WebSearchProvider>> {
        self.providers.values()
    }

    pub fn active_provider(&self) -> Option<Arc<dyn WebSearchProvider>> {
        self.active_provider.clone()
    }

    pub fn set_active_provider(&mut self, provider: Arc<dyn WebSearchProvider>) {
        self.active_provider = Some(provider.clone());
        self.providers.insert(provider.id(), provider);
    }

    pub fn register_provider<T: WebSearchProvider + 'static>(
        &mut self,
        provider: T,
        _cx: &mut Context<Self>,
    ) {
        let id = provider.id();
        let provider = Arc::new(provider);
        self.providers.insert(id, provider.clone());
        if self.active_provider.is_none() {
            self.active_provider = Some(provider);
        }
    }

    pub fn unregister_provider(&mut self, id: WebSearchProviderId) {
        self.providers.remove(&id);
        if self.active_provider.as_ref().map(|provider| provider.id()) == Some(id) {
            self.active_provider = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{WebSearchProvider, WebSearchProviderId, WebSearchRegistry};
    use cloud_llm_client::WebSearchResponse;
    use gpui::{App, SharedString, Task};

    struct TestProvider(&'static str);

    impl WebSearchProvider for TestProvider {
        fn id(&self) -> WebSearchProviderId {
            WebSearchProviderId(SharedString::from(self.0))
        }

        fn search(&self, _query: String, _cx: &mut App) -> Task<anyhow::Result<WebSearchResponse>> {
            panic!("search should not be called in registry tests")
        }
    }

    #[test]
    fn set_active_provider_inserts_and_marks_active() {
        let mut registry = WebSearchRegistry::default();
        let provider = Arc::new(TestProvider("provider-a"));

        registry.set_active_provider(provider.clone());

        assert_eq!(registry.providers().count(), 1);
        assert_eq!(registry.active_provider().unwrap().id(), provider.id());
        assert!(
            registry
                .providers()
                .any(|candidate| candidate.id() == provider.id())
        );
    }

    #[test]
    fn unregistering_active_provider_clears_active_provider() {
        let mut registry = WebSearchRegistry::default();
        let provider = Arc::new(TestProvider("provider-a"));
        let provider_id = provider.id();
        registry.set_active_provider(provider);

        registry.unregister_provider(provider_id.clone());

        assert!(registry.active_provider().is_none());
        assert!(
            !registry
                .providers()
                .any(|candidate| candidate.id() == provider_id)
        );
    }

    #[test]
    fn unregistering_inactive_provider_keeps_active_provider() {
        let mut registry = WebSearchRegistry::default();
        let active_provider = Arc::new(TestProvider("provider-a"));
        let inactive_provider = Arc::new(TestProvider("provider-b"));
        let inactive_id = inactive_provider.id();

        registry.set_active_provider(active_provider.clone());
        registry
            .providers
            .insert(inactive_provider.id(), inactive_provider);

        registry.unregister_provider(inactive_id.clone());

        assert_eq!(
            registry.active_provider().unwrap().id(),
            active_provider.id()
        );
        assert_eq!(registry.providers().count(), 1);
        assert!(
            !registry
                .providers()
                .any(|candidate| candidate.id() == inactive_id)
        );
    }
}
