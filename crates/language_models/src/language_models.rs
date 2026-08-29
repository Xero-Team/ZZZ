use std::sync::Arc;

use ::settings::{Settings, SettingsStore};
use client::{Client, UserStore};
use collections::HashSet;
use credentials_provider::CredentialsProvider;
use gpui::{App, Context, Entity};
use language_model::ZED_CLOUD_PROVIDER_ID;
use language_model::{ConfiguredModel, LanguageModelProviderId, LanguageModelRegistry};
use provider::deepseek::DeepSeekLanguageModelProvider;

const ENVIRONMENT_FALLBACK_PROVIDER_IDS: [&str; 3] = ["ollama", "llama_cpp", "opencode"];

pub mod extension;
pub mod provider;
mod settings;

pub use crate::extension::init_proxy as init_extension_proxy;

use crate::provider::anthropic::AnthropicLanguageModelProvider;
use crate::provider::bedrock::BedrockLanguageModelProvider;
use crate::provider::cloud::CloudLanguageModelProvider;
use crate::provider::copilot_chat::CopilotChatLanguageModelProvider;
use crate::provider::google::GoogleLanguageModelProvider;
use crate::provider::llama_cpp::LlamaCppLanguageModelProvider;
use crate::provider::lmstudio::LmStudioLanguageModelProvider;
pub use crate::provider::mistral::MistralLanguageModelProvider;
use crate::provider::ollama::OllamaLanguageModelProvider;
use crate::provider::open_ai::OpenAiLanguageModelProvider;
use crate::provider::open_ai_compatible::OpenAiCompatibleLanguageModelProvider;
use crate::provider::open_router::OpenRouterLanguageModelProvider;
use crate::provider::opencode::OpenCodeLanguageModelProvider;
use crate::provider::vercel_ai_gateway::VercelAiGatewayLanguageModelProvider;
use crate::provider::x_ai::XAiLanguageModelProvider;
pub use crate::settings::*;

pub fn init(user_store: Entity<UserStore>, client: Arc<Client>, cx: &mut App) {
    let credentials_provider = client.credentials_provider();
    let registry = LanguageModelRegistry::global(cx);
    registry.update(cx, |registry, cx| {
        register_language_model_providers(
            registry,
            user_store.clone(),
            client.clone(),
            credentials_provider.clone(),
            cx,
        );
    });

    let mut cloud_enabled = client::ClientSettings::get_global(cx).remote_server_enabled();
    if !cloud_enabled {
        registry.update(cx, |registry, cx| {
            registry.unregister_provider(ZED_CLOUD_PROVIDER_ID, cx);
        });
    }

    let cloud_registry = registry.clone();
    let cloud_user_store = user_store.clone();
    let cloud_client = client.clone();
    cx.observe_global::<SettingsStore>(move |cx| {
        let enabled = client::ClientSettings::get_global(cx).remote_server_enabled();
        if enabled == cloud_enabled {
            return;
        }
        cloud_registry.update(cx, |registry, cx| {
            if enabled {
                registry.register_provider(
                    Arc::new(CloudLanguageModelProvider::new(
                        cloud_user_store.clone(),
                        cloud_client.clone(),
                        cx,
                    )),
                    cx,
                );
            } else {
                registry.unregister_provider(ZED_CLOUD_PROVIDER_ID, cx);
            }
        });
        cloud_enabled = enabled;
        update_environment_fallback_model(cx);
    })
    .detach();

    // Local model discovery changes provider state asynchronously. Recompute the
    // fallback whenever any provider reports a state change, so discovered Ollama
    // or llama.cpp models become usable without restarting the app.
    cx.subscribe(&registry, |_registry, event: &language_model::Event, cx| {
        if matches!(event, language_model::Event::ProviderStateChanged(_)) {
            update_environment_fallback_model(cx);
        }
    })
    .detach();

    // Subscribe to extension store events to track LLM extension installations
    if let Some(extension_store) = extension_host::ExtensionStore::try_global(cx) {
        cx.subscribe(&extension_store, {
            let registry = registry.downgrade();
            move |extension_store, event, cx| {
                let Some(registry) = registry.upgrade() else {
                    return;
                };
                match event {
                    extension_host::Event::ExtensionInstalled(extension_id) => {
                        if let Some(manifest) = extension_store
                            .read(cx)
                            .extension_manifest_for_id(extension_id)
                        {
                            if !manifest.language_model_providers.is_empty() {
                                registry.update(cx, |registry, cx| {
                                    registry.extension_installed(extension_id.clone(), cx);
                                });
                            }
                        }
                    }
                    extension_host::Event::ExtensionUninstalled(extension_id) => {
                        registry.update(cx, |registry, cx| {
                            registry.extension_uninstalled(extension_id, cx);
                        });
                    }
                    extension_host::Event::ExtensionsUpdated => {
                        let mut new_ids = HashSet::default();
                        for (extension_id, entry) in extension_store.read(cx).installed_extensions()
                        {
                            if !entry.manifest.language_model_providers.is_empty() {
                                new_ids.insert(extension_id.clone());
                            }
                        }
                        registry.update(cx, |registry, cx| {
                            registry.sync_installed_llm_extensions(new_ids, cx);
                        });
                    }
                    _ => {}
                }
            }
        })
        .detach();

        // Initialize with currently installed extensions
        registry.update(cx, |registry, cx| {
            let mut initial_ids = HashSet::default();
            for (extension_id, entry) in extension_store.read(cx).installed_extensions() {
                if !entry.manifest.language_model_providers.is_empty() {
                    initial_ids.insert(extension_id.clone());
                }
            }
            registry.sync_installed_llm_extensions(initial_ids, cx);
        });
    }

    let mut openai_compatible_providers = AllLanguageModelSettings::get_global(cx)
        .openai_compatible
        .keys()
        .cloned()
        .collect::<HashSet<_>>();

    registry.update(cx, |registry, cx| {
        register_openai_compatible_providers(
            registry,
            &HashSet::default(),
            &openai_compatible_providers,
            client.clone(),
            credentials_provider.clone(),
            cx,
        );
    });

    let registry = registry.downgrade();
    cx.observe_global::<SettingsStore>(move |cx| {
        let Some(registry) = registry.upgrade() else {
            return;
        };
        let openai_compatible_providers_new = AllLanguageModelSettings::get_global(cx)
            .openai_compatible
            .keys()
            .cloned()
            .collect::<HashSet<_>>();
        if openai_compatible_providers_new != openai_compatible_providers {
            registry.update(cx, |registry, cx| {
                register_openai_compatible_providers(
                    registry,
                    &openai_compatible_providers,
                    &openai_compatible_providers_new,
                    client.clone(),
                    credentials_provider.clone(),
                    cx,
                );
            });
            openai_compatible_providers = openai_compatible_providers_new;
        }
    })
    .detach();

    update_environment_fallback_model(cx);
}

/// Recomputes and sets the [`LanguageModelRegistry`]'s environment fallback
/// model based on currently authenticated providers.
///
/// Prefers discovered local models (Ollama, then llama.cpp). OpenCode is only
/// considered after explicit user configuration. Hosted providers are never
/// selected by default.
pub fn update_environment_fallback_model(cx: &mut App) {
    let registry = LanguageModelRegistry::global(cx);
    let fallback_model = {
        let registry = registry.read(cx);
        ENVIRONMENT_FALLBACK_PROVIDER_IDS
            .into_iter()
            .find_map(|provider_id| {
                let provider = registry.provider(&LanguageModelProviderId::new(provider_id))?;
                if !provider.is_authenticated(cx) {
                    return None;
                }
                let model = provider
                    .default_model(cx)
                    .or_else(|| provider.recommended_models(cx).first().cloned())
                    .or_else(|| provider.provided_models(cx).into_iter().next())?;
                Some(ConfiguredModel { provider, model })
            })
    };
    registry.update(cx, |registry, cx| {
        registry.set_environment_fallback_model(fallback_model, cx);
    });
}

#[cfg(test)]
mod tests {
    use super::ENVIRONMENT_FALLBACK_PROVIDER_IDS;

    #[test]
    fn local_provider_fallback_order_prefers_ollama_then_llama_cpp_then_opencode() {
        assert_eq!(
            ENVIRONMENT_FALLBACK_PROVIDER_IDS,
            ["ollama", "llama_cpp", "opencode"]
        );
    }

    #[test]
    fn hosted_provider_is_not_an_environment_fallback() {
        let hosted_id = language_model::ZED_CLOUD_PROVIDER_ID.to_string();
        assert!(!ENVIRONMENT_FALLBACK_PROVIDER_IDS.contains(&hosted_id.as_str()));
    }
}

fn register_openai_compatible_providers(
    registry: &mut LanguageModelRegistry,
    old: &HashSet<Arc<str>>,
    new: &HashSet<Arc<str>>,
    client: Arc<Client>,
    credentials_provider: Arc<dyn CredentialsProvider>,
    cx: &mut Context<LanguageModelRegistry>,
) {
    for provider_id in old {
        if !new.contains(provider_id) {
            registry.unregister_provider(LanguageModelProviderId::from(provider_id.clone()), cx);
        }
    }

    for provider_id in new {
        if !old.contains(provider_id) {
            registry.register_provider(
                Arc::new(OpenAiCompatibleLanguageModelProvider::new(
                    provider_id.clone(),
                    client.http_client(),
                    credentials_provider.clone(),
                    cx,
                )),
                cx,
            );
        }
    }
}

fn register_language_model_providers(
    registry: &mut LanguageModelRegistry,
    user_store: Entity<UserStore>,
    client: Arc<Client>,
    credentials_provider: Arc<dyn CredentialsProvider>,
    cx: &mut Context<LanguageModelRegistry>,
) {
    // Do not even register the hosted provider on a fresh install. This keeps
    // cloud models out of provider listings and avoids account discovery work.
    if client::ClientSettings::get_global(cx).remote_server_enabled() {
        registry.register_provider(
            Arc::new(CloudLanguageModelProvider::new(
                user_store,
                client.clone(),
                cx,
            )),
            cx,
        );
    }
    registry.register_provider(
        Arc::new(AnthropicLanguageModelProvider::new(
            client.http_client(),
            credentials_provider.clone(),
            cx,
        )),
        cx,
    );
    registry.register_provider(
        Arc::new(OpenAiLanguageModelProvider::new(
            client.http_client(),
            credentials_provider.clone(),
            cx,
        )),
        cx,
    );
    registry.register_provider(
        Arc::new(OllamaLanguageModelProvider::new(
            client.http_client(),
            credentials_provider.clone(),
            cx,
        )),
        cx,
    );
    registry.register_provider(
        Arc::new(LmStudioLanguageModelProvider::new(
            client.http_client(),
            credentials_provider.clone(),
            cx,
        )),
        cx,
    );
    registry.register_provider(
        Arc::new(LlamaCppLanguageModelProvider::new(
            client.http_client(),
            credentials_provider.clone(),
            cx,
        )),
        cx,
    );
    registry.register_provider(
        Arc::new(DeepSeekLanguageModelProvider::new(
            client.http_client(),
            credentials_provider.clone(),
            cx,
        )),
        cx,
    );
    registry.register_provider(
        Arc::new(GoogleLanguageModelProvider::new(
            client.http_client(),
            credentials_provider.clone(),
            cx,
        )),
        cx,
    );
    registry.register_provider(
        MistralLanguageModelProvider::global(
            client.http_client(),
            credentials_provider.clone(),
            cx,
        ),
        cx,
    );
    registry.register_provider(
        Arc::new(BedrockLanguageModelProvider::new(
            client.http_client(),
            credentials_provider.clone(),
            cx,
        )),
        cx,
    );
    registry.register_provider(
        Arc::new(OpenRouterLanguageModelProvider::new(
            client.http_client(),
            credentials_provider.clone(),
            cx,
        )),
        cx,
    );
    registry.register_provider(
        Arc::new(VercelAiGatewayLanguageModelProvider::new(
            client.http_client(),
            credentials_provider.clone(),
            cx,
        )),
        cx,
    );
    registry.register_provider(
        Arc::new(XAiLanguageModelProvider::new(
            client.http_client(),
            credentials_provider.clone(),
            cx,
        )),
        cx,
    );
    registry.register_provider(
        Arc::new(OpenCodeLanguageModelProvider::new(
            client.http_client(),
            credentials_provider,
            cx,
        )),
        cx,
    );
    registry.register_provider(Arc::new(CopilotChatLanguageModelProvider::new(cx)), cx);
}
