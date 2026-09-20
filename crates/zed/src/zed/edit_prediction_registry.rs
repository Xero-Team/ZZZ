use client::{Client, UserStore};
use collections::HashMap;
use copilot::CopilotEditPredictionDelegate;
use edit_prediction::{EditPredictionModel, ZedEditPredictionDelegate, fim};
use editor::Editor;
use gpui::{AnyWindowHandle, App, AppContext as _, Context, Entity, WeakEntity};
use language::language_settings::{
    EditPredictionPromptFormat, EditPredictionProvider, all_language_settings,
};

use settings::SettingsStore;
use std::{cell::RefCell, rc::Rc, sync::Arc};
use ui::Window;

pub fn init(client: Arc<Client>, user_store: Entity<UserStore>, cx: &mut App) {
    edit_prediction::EditPredictionStore::global(&client, &user_store, cx);

    let editors: Rc<RefCell<HashMap<WeakEntity<Editor>, AnyWindowHandle>>> = Rc::default();
    cx.observe_new({
        let editors = editors.clone();
        let client = client.clone();
        let user_store = user_store.clone();
        move |editor: &mut Editor, window, cx: &mut Context<Editor>| {
            if !editor.mode().is_full() {
                return;
            }

            register_backward_compatible_actions(editor, cx);

            let Some(window) = window else {
                return;
            };

            let editor_handle = cx.entity().downgrade();
            cx.on_release({
                let editor_handle = editor_handle.clone();
                let editors = editors.clone();
                move |_, _| {
                    editors.borrow_mut().remove(&editor_handle);
                }
            })
            .detach();

            editors
                .borrow_mut()
                .insert(editor_handle, window.window_handle());
            let provider_config = edit_prediction_provider_config_for_settings(cx);
            assign_edit_prediction_provider(
                editor,
                provider_config,
                &client,
                user_store.clone(),
                window,
                cx,
            );
        }
    })
    .detach();

    cx.on_action(clear_edit_prediction_store_edit_history);

    cx.subscribe(&user_store, {
        let editors = editors.clone();
        let client = client.clone();

        move |user_store, event, cx| {
            let client::user::Event::PrivateUserInfoUpdated = event;
            let provider_config = edit_prediction_provider_config_for_settings(cx);
            assign_edit_prediction_providers(&editors, provider_config, &client, user_store, cx);
        }
    })
    .detach();

    cx.observe_global::<SettingsStore>({
        let mut previous_config = edit_prediction_provider_config_for_settings(cx);
        move |cx| {
            let new_provider_config = edit_prediction_provider_config_for_settings(cx);

            if new_provider_config != previous_config {
                previous_config = new_provider_config;
                assign_edit_prediction_providers(
                    &editors,
                    new_provider_config,
                    &client,
                    user_store.clone(),
                    cx,
                );
            }
        }
    })
    .detach();
}

fn edit_prediction_provider_config_for_settings(cx: &App) -> Option<EditPredictionProviderConfig> {
    let settings = &all_language_settings(None, cx).edit_predictions;
    let provider = settings.provider;
    match provider {
        EditPredictionProvider::None => None,
        EditPredictionProvider::Copilot => Some(EditPredictionProviderConfig::Copilot),
        EditPredictionProvider::Ollama | EditPredictionProvider::OpenAiCompatibleApi => {
            let custom_settings = if provider == EditPredictionProvider::Ollama {
                settings.ollama.as_ref()?
            } else {
                settings.open_ai_compatible_api.as_ref()?
            };

            let mut format = custom_settings.prompt_format;
            if format == EditPredictionPromptFormat::Infer {
                if let Some(inferred_format) = fim::infer_prompt_format(&custom_settings.model) {
                    format = inferred_format;
                } else {
                    // todo: notify user that prompt format inference failed
                    return None;
                }
            }

            if matches!(format, EditPredictionPromptFormat::Zeta(_)) {
                Some(EditPredictionProviderConfig::Zed(EditPredictionModel::Zeta))
            } else {
                Some(EditPredictionProviderConfig::Zed(
                    EditPredictionModel::Fim { format },
                ))
            }
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum EditPredictionProviderConfig {
    Copilot,
    Zed(EditPredictionModel),
}

fn clear_edit_prediction_store_edit_history(_: &edit_prediction::ClearHistory, cx: &mut App) {
    if let Some(ep_store) = edit_prediction::EditPredictionStore::try_global(cx) {
        ep_store.update(cx, |ep_store, _| ep_store.clear_history());
    }
}

fn assign_edit_prediction_providers(
    editors: &Rc<RefCell<HashMap<WeakEntity<Editor>, AnyWindowHandle>>>,
    provider_config: Option<EditPredictionProviderConfig>,
    client: &Arc<Client>,
    user_store: Entity<UserStore>,
    cx: &mut App,
) {
    for (editor, window) in editors.borrow().iter() {
        _ = window.update(cx, |_window, window, cx| {
            _ = editor.update(cx, |editor, cx| {
                assign_edit_prediction_provider(
                    editor,
                    provider_config,
                    client,
                    user_store.clone(),
                    window,
                    cx,
                );
            })
        });
    }
}

fn register_backward_compatible_actions(editor: &mut Editor, cx: &mut Context<Editor>) {
    // We renamed some of these actions to not be copilot-specific, but that
    // would have not been backwards-compatible. So here we are re-registering
    // the actions with the old names to not break people's keymaps.
    editor
        .register_action(cx.listener(
            |editor, _: &copilot::Suggest, window: &mut Window, cx: &mut Context<Editor>| {
                editor.show_edit_prediction(&Default::default(), window, cx);
            },
        ))
        .detach();
}

fn assign_edit_prediction_provider(
    editor: &mut Editor,
    provider_config: Option<EditPredictionProviderConfig>,
    client: &Arc<Client>,
    user_store: Entity<UserStore>,
    window: &mut Window,
    cx: &mut Context<Editor>,
) {
    // TODO: Do we really want to collect data only for singleton buffers?
    let singleton_buffer = editor.buffer().read(cx).as_singleton();

    match provider_config {
        None => {
            editor.set_edit_prediction_provider::<ZedEditPredictionDelegate>(None, window, cx);
        }
        Some(EditPredictionProviderConfig::Copilot) => {
            let ep_store = edit_prediction::EditPredictionStore::global(client, &user_store, cx);
            let Some(project) = editor.project().cloned() else {
                return;
            };
            let copilot =
                ep_store.update(cx, |this, cx| this.start_copilot_for_project(&project, cx));

            if let Some(copilot) = copilot {
                if let Some(buffer) = singleton_buffer {
                    copilot.update(cx, |copilot, cx| {
                        copilot.register_buffer(&buffer, cx);
                    });
                }
                let provider = cx.new(|_| CopilotEditPredictionDelegate::new(copilot));
                editor.set_edit_prediction_provider(Some(provider), window, cx);
            }
        }
        Some(EditPredictionProviderConfig::Zed(model)) => {
            let ep_store = edit_prediction::EditPredictionStore::global(client, &user_store, cx);

            if let Some(project) = editor.project() {
                ep_store.update(cx, |ep_store, cx| {
                    ep_store.set_edit_prediction_model(model);
                    if let Some(buffer) = &singleton_buffer {
                        ep_store.register_buffer(buffer, project, cx);
                    }
                });

                let provider = cx.new(|cx| {
                    ZedEditPredictionDelegate::new(project.clone(), &client, &user_store, cx)
                });
                editor.set_edit_prediction_provider(Some(provider), window, cx);
            }
        }
    }
}
