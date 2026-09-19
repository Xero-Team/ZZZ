use anyhow::Result;
use codestral::{self, CodestralEditPredictionDelegate};
use copilot::Status;
use edit_prediction::EditPredictionStore;
use edit_prediction_types::EditPredictionDelegateHandle;
use editor::{
    Editor, MultiBufferOffset, SelectionEffects, actions::ShowEditPrediction, scroll::Autoscroll,
};
use fs::Fs;
use gpui::{
    Action, Anchor, Animation, AnimationExt, App, AsyncWindowContext, Entity, FocusHandle,
    Focusable, IntoElement, ParentElement, Render, Subscription, WeakEntity, actions, div,
    ease_in_out,
};
use i18n as app_i18n;
use language::{
    EditPredictionsMode, File, Language,
    language_settings::{
        AllLanguageSettings, EditPredictionProvider, LanguageSettings, all_language_settings,
    },
};
use project::{DisableAiSettings, Project};
use regex::Regex;
use settings::{Settings, SettingsStore, update_settings_file};
use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};
use ui::{
    Clickable, ContextMenu, ContextMenuEntry, DocumentationSide, IconButton, IconButtonShape,
    Indicator, PopoverMenu, PopoverMenuHandle, Tooltip, prelude::*,
};
use util::ResultExt as _;

use workspace::{
    StatusItemView, Toast, Workspace, create_and_open_local_file, item::ItemHandle,
    notifications::NotificationId,
};
use zed_actions::{OpenBrowser, OpenSettingsAt};

fn tr(cx: &App, key: &'static str, fallback: &'static str) -> SharedString {
    app_i18n::tr(cx, key, fallback).into()
}

actions!(
    edit_prediction,
    [
        /// Toggles the edit prediction menu.
        ToggleMenu
    ]
);

const COPILOT_SETTINGS_PATH: &str = "/settings/copilot";
const COPILOT_SETTINGS_URL: &str = concat!("https://github.com", "/settings/copilot");
const PRIVACY_DOCS: &str = "https://codeberg.org/ZZZEditor/ZZZ/wiki/privacy";

struct CopilotErrorToast;

pub struct EditPredictionButton {
    editor_subscription: Option<(Subscription, usize)>,
    editor_enabled: Option<bool>,
    editor_show_predictions: bool,
    editor_focus_handle: Option<FocusHandle>,
    language: Option<Arc<Language>>,
    file: Option<Arc<dyn File>>,
    edit_prediction_provider: Option<Arc<dyn EditPredictionDelegateHandle>>,
    fs: Arc<dyn Fs>,
    popover_menu_handle: PopoverMenuHandle<ContextMenu>,
    project: WeakEntity<Project>,
}

impl Render for EditPredictionButton {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Return empty div if AI is disabled
        if DisableAiSettings::get_global(cx).disable_ai {
            return div().hidden();
        }

        let language_settings = all_language_settings(None, cx);

        match language_settings.edit_predictions.provider {
            EditPredictionProvider::Copilot => {
                let Some(copilot) = EditPredictionStore::try_global(cx)
                    .and_then(|store| store.read(cx).copilot_for_project(&self.project.upgrade()?))
                else {
                    return div().hidden();
                };
                let status = copilot.read(cx).status();

                let enabled = self.editor_enabled.unwrap_or(false);

                let icon = match status {
                    Status::Error(_) => IconName::CopilotError,
                    Status::Authorized => {
                        if enabled {
                            IconName::Copilot
                        } else {
                            IconName::CopilotDisabled
                        }
                    }
                    _ => IconName::CopilotInit,
                };

                if let Status::Error(e) = status {
                    return div().child(
                        IconButton::new("copilot-error", icon)
                            .icon_size(IconSize::Small)
                            .on_click(cx.listener(move |_, _, window, cx| {
                                if let Some(workspace) = Workspace::for_window(window, cx) {
                                    workspace.update(cx, |workspace, cx| {
                                        let copilot = copilot.clone();
                                        workspace.show_toast(
                                            Toast::new(
                                                NotificationId::unique::<CopilotErrorToast>(),
                                                tr(
                                                    cx,
                                                    "edit_prediction_ui.button.copilot_start_failed",
                                                    "Copilot can't be started: {}",
                                                )
                                                .replacen("{}", e.as_ref(), 1),
                                            )
                                            .on_click(
                                                app_i18n::tr(
                                                    cx,
                                                    "edit_prediction_ui.button.reinstall_copilot",
                                                    "Reinstall Copilot",
                                                ),
                                                move |window, cx| {
                                                    copilot_ui::reinstall_and_sign_in(
                                                        copilot.clone(),
                                                        window,
                                                        cx,
                                                    )
                                                },
                                            ),
                                            cx,
                                        );
                                    });
                                }
                            }))
                            .tooltip(|_window, cx| {
                                Tooltip::for_action(
                                    tr(
                                        cx,
                                        "edit_prediction_ui.button.github_copilot",
                                        "GitHub Copilot",
                                    ),
                                    &ToggleMenu,
                                    cx,
                                )
                            }),
                    );
                }
                let this = cx.weak_entity();
                let project = self.project.clone();
                div().child(
                    PopoverMenu::new("copilot")
                        .menu(move |window, cx| {
                            let current_status = EditPredictionStore::try_global(cx)
                                .and_then(|store| {
                                    store.read(cx).copilot_for_project(&project.upgrade()?)
                                })?
                                .read(cx)
                                .status();
                            match current_status {
                                Status::Authorized => this.update(cx, |this, cx| {
                                    this.build_copilot_context_menu(window, cx)
                                }),
                                _ => this.update(cx, |this, cx| {
                                    this.build_copilot_start_menu(window, cx)
                                }),
                            }
                            .ok()
                        })
                        .anchor(Anchor::BottomRight)
                        .trigger_with_tooltip(
                            IconButton::new("copilot-icon", icon),
                            |_window, cx| {
                                Tooltip::for_action(
                                    tr(
                                        cx,
                                        "edit_prediction_ui.button.github_copilot",
                                        "GitHub Copilot",
                                    ),
                                    &ToggleMenu,
                                    cx,
                                )
                            },
                        )
                        .with_handle(self.popover_menu_handle.clone()),
                )
            }
            EditPredictionProvider::Codestral => {
                let enabled = self.editor_enabled.unwrap_or(true);
                let has_api_key = codestral::codestral_api_key(cx).is_some();
                let this = cx.weak_entity();

                let tooltip_meta = if has_api_key {
                    tr(
                        cx,
                        "edit_prediction_ui.button.powered_by_codestral",
                        "Powered by Codestral",
                    )
                } else {
                    tr(
                        cx,
                        "edit_prediction_ui.button.missing_api_key.codestral",
                        "Missing API key for Codestral",
                    )
                };

                div().child(
                    PopoverMenu::new("codestral")
                        .menu(move |window, cx| {
                            this.update(cx, |this, cx| {
                                this.build_codestral_context_menu(window, cx)
                            })
                            .ok()
                        })
                        .anchor(Anchor::BottomRight)
                        .trigger_with_tooltip(
                            IconButton::new("codestral-icon", IconName::AiMistral)
                                .shape(IconButtonShape::Square)
                                .when(!has_api_key, |this| {
                                    this.indicator(Indicator::dot().color(Color::Error))
                                        .indicator_border_color(Some(
                                            cx.theme().colors().status_bar_background,
                                        ))
                                })
                                .when(has_api_key && !enabled, |this| {
                                    this.indicator(Indicator::dot().color(Color::Ignored))
                                        .indicator_border_color(Some(
                                            cx.theme().colors().status_bar_background,
                                        ))
                                }),
                            move |_window, cx| {
                                Tooltip::with_meta(
                                    tr(
                                        cx,
                                        "edit_prediction_ui.button.edit_prediction",
                                        "Edit Prediction",
                                    ),
                                    Some(&ToggleMenu),
                                    tooltip_meta.clone(),
                                    cx,
                                )
                            },
                        )
                        .with_handle(self.popover_menu_handle.clone()),
                )
            }
            EditPredictionProvider::OpenAiCompatibleApi => {
                let enabled = self.editor_enabled.unwrap_or(true);
                let this = cx.weak_entity();

                div().child(
                    PopoverMenu::new("openai-compatible-api")
                        .menu(move |window, cx| {
                            this.update(cx, |this, cx| {
                                this.build_edit_prediction_context_menu(
                                    EditPredictionProvider::OpenAiCompatibleApi,
                                    window,
                                    cx,
                                )
                            })
                            .ok()
                        })
                        .anchor(Anchor::BottomRight)
                        .trigger(
                            IconButton::new("openai-compatible-api-icon", IconName::AiOpenAiCompat)
                                .shape(IconButtonShape::Square)
                                .when(!enabled, |this| {
                                    this.indicator(Indicator::dot().color(Color::Ignored))
                                        .indicator_border_color(Some(
                                            cx.theme().colors().status_bar_background,
                                        ))
                                }),
                        )
                        .with_handle(self.popover_menu_handle.clone()),
                )
            }
            EditPredictionProvider::Ollama => {
                let enabled = self.editor_enabled.unwrap_or(true);
                let this = cx.weak_entity();

                div().child(
                    PopoverMenu::new("ollama")
                        .menu(move |window, cx| {
                            this.update(cx, |this, cx| {
                                this.build_edit_prediction_context_menu(
                                    EditPredictionProvider::Ollama,
                                    window,
                                    cx,
                                )
                            })
                            .ok()
                        })
                        .anchor(Anchor::BottomRight)
                        .trigger_with_tooltip(
                            IconButton::new("ollama-icon", IconName::AiOllama)
                                .shape(IconButtonShape::Square)
                                .when(!enabled, |this| {
                                    this.indicator(Indicator::dot().color(Color::Ignored))
                                        .indicator_border_color(Some(
                                            cx.theme().colors().status_bar_background,
                                        ))
                                }),
                            move |_window, cx| {
                                let settings = all_language_settings(None, cx);
                                let tooltip_meta = match settings.edit_predictions.ollama.as_ref() {
                                    Some(settings) if !settings.model.trim().is_empty() => {
                                        tr(
                                            cx,
                                            "edit_prediction_ui.button.powered_by_ollama",
                                            "Powered by Ollama ({})",
                                        )
                                        .replacen("{}", settings.model.as_str(), 1)
                                    }
                                    _ => tr(
                                        cx,
                                        "edit_prediction_ui.button.ollama_model_not_configured",
                                        "Ollama model not configured — configure a model before use",
                                    )
                                    .to_string(),
                                };

                                Tooltip::with_meta(
                                    tr(
                                        cx,
                                        "edit_prediction_ui.button.edit_prediction",
                                        "Edit Prediction",
                                    ),
                                    Some(&ToggleMenu),
                                    tooltip_meta,
                                    cx,
                                )
                            },
                        )
                        .with_handle(self.popover_menu_handle.clone()),
                )
            }

            EditPredictionProvider::None => div().hidden(),
        }
    }
}

impl EditPredictionButton {
    pub fn new(
        fs: Arc<dyn Fs>,
        popover_menu_handle: PopoverMenuHandle<ContextMenu>,
        project: Entity<Project>,
        cx: &mut Context<Self>,
    ) -> Self {
        if all_language_settings(None, cx).edit_predictions.provider
            == EditPredictionProvider::Copilot
        {
            let copilot = EditPredictionStore::try_global(cx).and_then(|store| {
                store.update(cx, |this, cx| this.start_copilot_for_project(&project, cx))
            });
            if let Some(copilot) = copilot {
                cx.observe(&copilot, |_, _, cx| cx.notify()).detach()
            }
        }

        cx.observe_global::<SettingsStore>(move |this, cx| {
            if all_language_settings(None, cx).edit_predictions.provider
                == EditPredictionProvider::Copilot
            {
                if let Some(project) = this.project.upgrade() {
                    EditPredictionStore::try_global(cx).and_then(|store| {
                        store.update(cx, |store, cx| {
                            store.start_copilot_for_project(&project, cx)
                        })
                    });
                }
            }
            cx.notify();
        })
        .detach();

        cx.observe_global::<EditPredictionStore>(move |_, cx| cx.notify())
            .detach();

        let open_ai_compatible_api_token_task =
            edit_prediction::open_ai_compatible::load_open_ai_compatible_api_token(cx);

        cx.spawn(async move |this, cx| {
            open_ai_compatible_api_token_task.await.ok();
            this.update(cx, |_, cx| {
                cx.notify();
            })
            .ok();
        })
        .detach();

        CodestralEditPredictionDelegate::ensure_api_key_loaded(cx);

        Self {
            editor_subscription: None,
            editor_enabled: None,
            editor_show_predictions: true,
            editor_focus_handle: None,
            language: None,
            file: None,
            edit_prediction_provider: None,
            popover_menu_handle,
            project: project.downgrade(),
            fs,
        }
    }

    fn add_provider_switching_section(
        &self,
        mut menu: ContextMenu,
        current_provider: EditPredictionProvider,
        cx: &mut App,
    ) -> ContextMenu {
        let available_providers = get_available_providers(cx);

        let providers: Vec<_> = available_providers
            .into_iter()
            .filter(|p| *p != EditPredictionProvider::None)
            .collect();

        if !providers.is_empty() {
            menu =
                menu.separator()
                    .header(tr(cx, "edit_prediction_ui.button.providers", "Providers"));

            for provider in providers {
                let Some(name) = provider.display_name() else {
                    continue;
                };
                let is_current = provider == current_provider;
                let is_disabled_zed_provider = false;
                let fs = self.fs.clone();

                menu = menu.item(
                    ContextMenuEntry::new(name)
                        .toggleable(IconPosition::Start, is_current && !is_disabled_zed_provider)
                        .disabled(is_disabled_zed_provider)
                        .when(is_disabled_zed_provider, |item| {
                            item.documentation_aside(DocumentationSide::Left, move |cx| {
                                Label::new(tr(
                                    cx,
                                    "edit_prediction_ui.button.organization_disabled",
                                    "Edit predictions are disabled for this organization.",
                                ))
                                .into_any_element()
                            })
                        })
                        .handler(move |_, cx| {
                            set_completion_provider(fs.clone(), cx, provider);
                        }),
                )
            }
        }

        menu
    }

    fn add_configure_providers_item(&self, menu: ContextMenu, cx: &App) -> ContextMenu {
        menu.separator().item(
            ContextMenuEntry::new(tr(
                cx,
                "edit_prediction_ui.button.configure_providers",
                "Configure Providers",
            ))
            .icon(IconName::Settings)
            .icon_position(IconPosition::Start)
            .icon_color(Color::Muted)
            .handler(move |window, cx| {
                window.dispatch_action(
                    OpenSettingsAt {
                        path: "edit_predictions.providers".to_owned(),
                    }
                    .boxed_clone(),
                    cx,
                );
            }),
        )
    }

    pub fn build_copilot_start_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ContextMenu> {
        let fs = self.fs.clone();
        let project = self.project.clone();
        ContextMenu::build(window, cx, |menu, _, cx| {
            let menu = menu
                .entry(
                    tr(
                        cx,
                        "edit_prediction_ui.button.sign_in_to_copilot",
                        "Configure Copilot",
                    ),
                    None,
                    move |window, cx| {
                        if let Some(copilot) =
                            EditPredictionStore::try_global(cx).and_then(|store| {
                                store.update(cx, |this, cx| {
                                    this.start_copilot_for_project(&project.upgrade()?, cx)
                                })
                            })
                        {
                            copilot_ui::initiate_sign_in(copilot, window, cx);
                        }
                    },
                )
                .entry(
                    tr(
                        cx,
                        "edit_prediction_ui.button.disable_copilot",
                        "Disable Copilot",
                    ),
                    None,
                    {
                        let fs = fs.clone();
                        move |_window, cx| hide_copilot(fs.clone(), cx)
                    },
                );

            let menu =
                self.add_provider_switching_section(menu, EditPredictionProvider::Copilot, cx);
            let menu = self.add_configure_providers_item(menu, cx);
            menu
        })
    }

    pub fn build_language_settings_menu(
        &self,
        mut menu: ContextMenu,
        window: &Window,
        cx: &mut App,
    ) -> ContextMenu {
        let fs = self.fs.clone();
        let _line_height = window.line_height();

        menu = menu.header(tr(
            cx,
            "edit_prediction_ui.button.show_edit_predictions_for",
            "Show Edit Predictions For",
        ));

        let language_state = self.language.as_ref().map(|language| {
            (
                language.clone(),
                LanguageSettings::resolve(None, Some(&language.name()), cx).show_edit_predictions,
            )
        });

        if let Some(editor_focus_handle) = self.editor_focus_handle.clone() {
            let entry = ContextMenuEntry::new(tr(
                cx,
                "edit_prediction_ui.button.this_buffer",
                "This Buffer",
            ))
            .toggleable(IconPosition::Start, self.editor_show_predictions)
            .action(Box::new(editor::actions::ToggleEditPrediction))
            .handler(move |window, cx| {
                editor_focus_handle.dispatch_action(
                    &editor::actions::ToggleEditPrediction,
                    window,
                    cx,
                );
            });

            match language_state.clone() {
                Some((language, false)) => {
                    menu = menu.item(entry.disabled(true).documentation_aside(
                        DocumentationSide::Left,
                        move |cx| {
                            Label::new(
                                tr(
                                    cx,
                                    "edit_prediction_ui.button.disabled_for_language",
                                    "Edit predictions are disabled for {}",
                                )
                                .replacen(
                                    "{}",
                                    language.name().as_ref(),
                                    1,
                                ),
                            )
                            .into_any_element()
                        },
                    ));
                }
                Some(_) | None => menu = menu.item(entry),
            }
        }

        if let Some((language, language_enabled)) = language_state {
            let fs = fs.clone();
            let language_name = language.name();

            menu = menu.toggleable_entry(
                language_name,
                language_enabled,
                IconPosition::Start,
                None,
                move |_, cx| {
                    toggle_show_edit_predictions_for_language(language.clone(), fs.clone(), cx)
                },
            );
        }

        let settings = AllLanguageSettings::get_global(cx);

        let globally_enabled = settings.show_edit_predictions(None, cx);
        let entry =
            ContextMenuEntry::new(tr(cx, "edit_prediction_ui.button.all_files", "All Files"))
                .toggleable(IconPosition::Start, globally_enabled)
                .action(workspace::ToggleEditPrediction.boxed_clone())
                .handler(|window, cx| {
                    window.dispatch_action(workspace::ToggleEditPrediction.boxed_clone(), cx)
                });
        menu = menu.item(entry);

        let _provider = settings.edit_predictions.provider;
        let current_mode = settings.edit_predictions_mode();
        let subtle_mode = matches!(current_mode, EditPredictionsMode::Subtle);
        let eager_mode = matches!(current_mode, EditPredictionsMode::Eager);

        menu = menu
                .separator()
                .header(tr(
                    cx,
                    "edit_prediction_ui.button.display_modes",
                    "Display Modes",
                ))
                .item(
                    ContextMenuEntry::new(tr(
                        cx,
                        "edit_prediction_ui.button.eager",
                        "Eager",
                    ))
                        .toggleable(IconPosition::Start, eager_mode)
                        .documentation_aside(DocumentationSide::Left, move |cx| {
                            Label::new(tr(
                                cx,
                                "edit_prediction_ui.button.eager_description",
                                "Display predictions inline when there are no language server completions available.",
                            ))
                            .into_any_element()
                        })
                        .handler({
                            let fs = fs.clone();
                            move |_, cx| {
                                toggle_edit_prediction_mode(fs.clone(), EditPredictionsMode::Eager, cx)
                            }
                        }),
                )
                .item(
                    ContextMenuEntry::new(tr(
                        cx,
                        "edit_prediction_ui.button.subtle",
                        "Subtle",
                    ))
                        .toggleable(IconPosition::Start, subtle_mode)
                        .documentation_aside(DocumentationSide::Left, move |cx| {
                            Label::new(tr(
                                cx,
                                "edit_prediction_ui.button.subtle_description",
                                concat!(
                                    "Display predictions inline only when holding a modifier key (",
                                    ui::alt_key_name!(),
                                    " by default)."
                                ),
                            ))
                            .into_any_element()
                        })
                        .handler({
                            let fs = fs.clone();
                            move |_, cx| {
                                toggle_edit_prediction_mode(fs.clone(), EditPredictionsMode::Subtle, cx)
                            }
                        }),
                );

        menu = menu
            .separator()
            .header(tr(cx, "edit_prediction_ui.button.privacy", "Privacy"));

        menu = menu.item(
            ContextMenuEntry::new(tr(
                cx,
                "edit_prediction_ui.button.configure_excluded_files",
                "Configure Excluded Files",
            ))
                .icon(IconName::LockOutlined)
                .icon_color(Color::Muted)
                .documentation_aside(DocumentationSide::Left, |cx| {
                    Label::new(tr(
                        cx,
                        "edit_prediction_ui.button.configure_excluded_files_description",
                        "Open your settings to add sensitive paths for which ZZZ will never predict edits.",
                    ))
                    .into_any_element()
                })
                .handler(move |window, cx| {
                    if let Some(workspace) = Workspace::for_window(window, cx) {
                        let workspace = workspace.downgrade();
                        window
                            .spawn(cx, async |cx| {
                                open_disabled_globs_setting_in_editor(
                                    workspace,
                                    cx,
                                ).await
                            })
                            .detach_and_log_err(cx);
                    }
                }),
        ).item(
            ContextMenuEntry::new(tr(
                cx,
                "edit_prediction_ui.button.view_docs",
                "View Docs",
            ))
                .icon(IconName::FileGeneric)
                .icon_color(Color::Muted)
                .handler(move |_, cx| {
                    cx.open_url(PRIVACY_DOCS);
                })
        );

        if !self.editor_enabled.unwrap_or(true) {
            let icons = self
                .edit_prediction_provider
                .as_ref()
                .map(|p| p.icons(cx))
                .unwrap_or_else(|| {
                    edit_prediction_types::EditPredictionIconSet::new(IconName::ZedPredict)
                });
            menu = menu.item(
                ContextMenuEntry::new(tr(
                    cx,
                    "edit_prediction_ui.button.this_file_is_excluded",
                    "This file is excluded.",
                ))
                .disabled(true)
                .icon(icons.disabled)
                .icon_size(IconSize::Small),
            );
        }

        if let Some(editor_focus_handle) = self.editor_focus_handle.clone() {
            menu = menu
                .separator()
                .header(tr(cx, "edit_prediction_ui.button.actions", "Actions"))
                .entry(
                    tr(
                        cx,
                        "edit_prediction_ui.button.predict_edit_at_cursor",
                        "Predict Edit at Cursor",
                    ),
                    Some(Box::new(ShowEditPrediction)),
                    {
                        let editor_focus_handle = editor_focus_handle.clone();
                        move |window, cx| {
                            editor_focus_handle.dispatch_action(&ShowEditPrediction, window, cx);
                        }
                    },
                )
                .context(editor_focus_handle);
        }

        menu
    }

    fn build_copilot_context_menu(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ContextMenu> {
        let all_language_settings = all_language_settings(None, cx);
        let next_edit_suggestions = all_language_settings
            .edit_predictions
            .copilot
            .enable_next_edit_suggestions
            .unwrap_or(true);
        let copilot_config = copilot_chat::CopilotChatConfiguration {
            enterprise_uri: all_language_settings
                .edit_predictions
                .copilot
                .enterprise_uri
                .clone(),
        };
        let settings_url = copilot_settings_url(copilot_config.enterprise_uri.as_deref());

        ContextMenu::build(window, cx, |menu, window, cx| {
            let menu = self.build_language_settings_menu(menu, window, cx);
            let menu =
                self.add_provider_switching_section(menu, EditPredictionProvider::Copilot, cx);

            let menu = self.add_configure_providers_item(menu, cx);
            let menu = menu
                .separator()
                .item(
                    ContextMenuEntry::new(tr(
                        cx,
                        "edit_prediction_ui.button.copilot_next_edit_suggestions",
                        "Copilot: Next Edit Suggestions",
                    ))
                    .toggleable(IconPosition::Start, next_edit_suggestions)
                    .handler({
                        let fs = self.fs.clone();
                        move |_, cx| {
                            update_settings_file(fs.clone(), cx, move |settings, _| {
                                settings
                                    .project
                                    .all_languages
                                    .edit_predictions
                                    .get_or_insert_default()
                                    .copilot
                                    .get_or_insert_default()
                                    .enable_next_edit_suggestions = Some(!next_edit_suggestions);
                            });
                        }
                    }),
                )
                .separator()
                .link(
                    tr(
                        cx,
                        "edit_prediction_ui.button.go_to_copilot_settings",
                        "Go to Copilot Settings",
                    ),
                    OpenBrowser { url: settings_url }.boxed_clone(),
                )
                .action(
                    tr(cx, "edit_prediction_ui.button.sign_out", "Sign Out"),
                    copilot::SignOut.boxed_clone(),
                );
            menu
        })
    }

    fn build_codestral_context_menu(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ContextMenu> {
        ContextMenu::build(window, cx, |menu, window, cx| {
            let menu = self.build_language_settings_menu(menu, window, cx);
            let menu =
                self.add_provider_switching_section(menu, EditPredictionProvider::Codestral, cx);

            let menu = self.add_configure_providers_item(menu, cx);
            menu
        })
    }

    fn build_edit_prediction_context_menu(
        &self,
        provider: EditPredictionProvider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ContextMenu> {
        ContextMenu::build(window, cx, |mut menu, window, cx| {
            let needs_provider = matches!(provider, EditPredictionProvider::None);

            if needs_provider {
                menu = menu
                    .custom_row(move |_window, cx| {
                        v_flex()
                            .max_w_64()
                            .h(rems_from_px(148.))
                            .child(render_zeta_tab_animation(cx))
                            .child(Label::new(tr(
                                cx,
                                "edit_prediction_ui.button.edit_prediction",
                                "Edit Prediction",
                            )))
                            .child(
                                Label::new(tr(
                                    cx,
                                    "edit_prediction_ui.button.provider_description",
                                    "Configure a local provider to enable edit predictions.",
                                ))
                                .color(Color::Muted)
                                .size(LabelSize::Small),
                            )
                            .into_any_element()
                    })
                    .separator()
                    .entry(
                        tr(
                            cx,
                            "edit_prediction_ui.button.configure_provider",
                            "Configure Provider",
                        ),
                        None,
                        |_window, _cx| {},
                    )
                    .separator();
            }

            if !needs_provider {
                menu = self.build_language_settings_menu(menu, window, cx);
            }
            menu = self.add_provider_switching_section(menu, provider, cx);

            let menu = self.add_configure_providers_item(menu, cx);
            menu
        })
    }

    pub fn update_enabled(&mut self, editor: Entity<Editor>, cx: &mut Context<Self>) {
        let editor = editor.read(cx);
        let snapshot = editor.buffer().read(cx).snapshot(cx);
        let suggestion_anchor = editor.selections.newest_anchor().start;
        let language = snapshot.language_at(suggestion_anchor);
        let file = snapshot.file_at(suggestion_anchor).cloned();
        self.editor_enabled = {
            let file = file.as_ref();
            Some(
                file.map(|file| {
                    all_language_settings(Some(file), cx)
                        .edit_predictions_enabled_for_file(file, cx)
                })
                .unwrap_or(true),
            )
        };
        self.editor_show_predictions = editor.edit_predictions_enabled();
        self.edit_prediction_provider = editor.edit_prediction_provider();
        self.language = language.cloned();
        self.file = file;
        self.editor_focus_handle = Some(editor.focus_handle(cx));

        cx.notify();
    }
}

impl StatusItemView for EditPredictionButton {
    fn set_active_pane_item(
        &mut self,
        item: Option<&dyn ItemHandle>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(editor) = item.and_then(|item| item.act_as::<Editor>(cx)) {
            self.editor_subscription = Some((
                cx.observe(&editor, Self::update_enabled),
                editor.entity_id().as_u64() as usize,
            ));
            self.update_enabled(editor, cx);
        } else {
            self.language = None;
            self.editor_subscription = None;
            self.editor_enabled = None;
        }
        cx.notify();
    }
}

async fn open_disabled_globs_setting_in_editor(
    workspace: WeakEntity<Workspace>,
    cx: &mut AsyncWindowContext,
) -> Result<()> {
    let settings_editor = workspace
        .update_in(cx, |_, window, cx| {
            create_and_open_local_file(paths::settings_file(), window, cx, || {
                settings::initial_user_settings_content().as_ref().into()
            })
        })?
        .await?
        .downcast::<Editor>()
        .unwrap();

    settings_editor
        .downgrade()
        .update_in(cx, |item, window, cx| {
            let text = item.buffer().read(cx).snapshot(cx).text();

            let settings = cx.global::<SettingsStore>();

            // Ensure that we always have "edit_predictions { "disabled_globs": [] }"
            let Some(edits) = settings
                .edits_for_update(&text, |file| {
                    file.project
                        .all_languages
                        .edit_predictions
                        .get_or_insert_with(Default::default)
                        .disabled_globs
                        .get_or_insert_with(Vec::new);
                })
                .log_err()
            else {
                return;
            };

            if !edits.is_empty() {
                item.edit(
                    edits
                        .into_iter()
                        .map(|(r, s)| (MultiBufferOffset(r.start)..MultiBufferOffset(r.end), s)),
                    cx,
                );
            }

            let text = item.buffer().read(cx).snapshot(cx).text();

            static DISABLED_GLOBS_REGEX: LazyLock<Regex> = LazyLock::new(|| {
                Regex::new(r#""disabled_globs":\s*\[\s*(?P<content>(?:.|\n)*?)\s*\]"#).unwrap()
            });
            // Only capture [...]
            let range = DISABLED_GLOBS_REGEX.captures(&text).and_then(|captures| {
                captures
                    .name("content")
                    .map(|inner_match| inner_match.start()..inner_match.end())
            });
            if let Some(range) = range {
                let range = MultiBufferOffset(range.start)..MultiBufferOffset(range.end);
                item.change_selections(
                    SelectionEffects::scroll(Autoscroll::newest()),
                    window,
                    cx,
                    |selections| {
                        selections.select_ranges(vec![range]);
                    },
                );
            }
        })?;

    anyhow::Ok(())
}

pub fn set_completion_provider(fs: Arc<dyn Fs>, cx: &mut App, provider: EditPredictionProvider) {
    update_settings_file(fs, cx, move |settings, _| {
        settings
            .project
            .all_languages
            .edit_predictions
            .get_or_insert_default()
            .provider = Some(provider);
    });
}

pub fn get_available_providers(cx: &mut App) -> Vec<EditPredictionProvider> {
    let mut providers = Vec::new();

    if let Some(copilot) = copilot::GlobalCopilotAuth::try_global(cx).cloned()
        && copilot.0.read(cx).is_authenticated()
    {
        providers.push(EditPredictionProvider::Copilot);
    }

    if codestral::codestral_api_key(cx).is_some() {
        providers.push(EditPredictionProvider::Codestral);
    }

    if all_language_settings(None, cx)
        .edit_predictions
        .ollama
        .is_some()
    {
        providers.push(EditPredictionProvider::Ollama);
    }

    if all_language_settings(None, cx)
        .edit_predictions
        .open_ai_compatible_api
        .is_some()
    {
        providers.push(EditPredictionProvider::OpenAiCompatibleApi);
    }

    providers
}

fn toggle_show_edit_predictions_for_language(
    language: Arc<Language>,
    fs: Arc<dyn Fs>,
    cx: &mut App,
) {
    let show_edit_predictions =
        all_language_settings(None, cx).show_edit_predictions(Some(&language), cx);
    update_settings_file(fs, cx, move |settings, _| {
        settings
            .project
            .all_languages
            .languages
            .0
            .entry(language.name().0.to_string())
            .or_default()
            .show_edit_predictions = Some(!show_edit_predictions);
    });
}

fn hide_copilot(fs: Arc<dyn Fs>, cx: &mut App) {
    update_settings_file(fs, cx, move |settings, _| {
        settings
            .project
            .all_languages
            .edit_predictions
            .get_or_insert(Default::default())
            .provider = Some(EditPredictionProvider::None);
    });
}

fn toggle_edit_prediction_mode(fs: Arc<dyn Fs>, mode: EditPredictionsMode, cx: &mut App) {
    let settings = AllLanguageSettings::get_global(cx);
    let current_mode = settings.edit_predictions_mode();

    if current_mode != mode {
        update_settings_file(fs, cx, move |settings, _cx| {
            if let Some(edit_predictions) = settings.project.all_languages.edit_predictions.as_mut()
            {
                edit_predictions.mode = Some(mode);
            } else {
                settings.project.all_languages.edit_predictions =
                    Some(settings::EditPredictionSettingsContent {
                        mode: Some(mode),
                        ..Default::default()
                    });
            }
        });
    }
}

fn render_zeta_tab_animation(cx: &App) -> impl IntoElement {
    let tab = |n: u64, inverted: bool| {
        let text_color = cx.theme().colors().text;

        h_flex().child(
            h_flex()
                .text_size(TextSize::XSmall.rems(cx))
                .text_color(text_color)
                .child("tab")
                .with_animation(
                    ElementId::Integer(n),
                    Animation::new(Duration::from_secs(3)).repeat(),
                    move |tab, delta| {
                        let n_f32 = n as f32;

                        let offset = if inverted {
                            0.2 * (4.0 - n_f32)
                        } else {
                            0.2 * n_f32
                        };

                        let phase = (delta - offset + 1.0) % 1.0;
                        let pulse = if phase < 0.6 {
                            let t = phase / 0.6;
                            1.0 - (0.5 - t).abs() * 2.0
                        } else {
                            0.0
                        };

                        let eased = ease_in_out(pulse);
                        let opacity = 0.1 + 0.5 * eased;

                        tab.text_color(text_color.opacity(opacity))
                    },
                ),
        )
    };

    let tab_sequence = |inverted: bool| {
        h_flex()
            .gap_1()
            .child(tab(0, inverted))
            .child(tab(1, inverted))
            .child(tab(2, inverted))
            .child(tab(3, inverted))
            .child(tab(4, inverted))
    };

    h_flex()
        .my_1p5()
        .p_4()
        .justify_center()
        .gap_2()
        .rounded_xs()
        .border_1()
        .border_dashed()
        .border_color(cx.theme().colors().border)
        .bg(gpui::pattern_slash(
            cx.theme().colors().border.opacity(0.5),
            1.,
            8.,
        ))
        .child(tab_sequence(true))
        .child(Icon::new(IconName::ZedPredict))
        .child(tab_sequence(false))
}

fn copilot_settings_url(enterprise_uri: Option<&str>) -> String {
    match enterprise_uri {
        Some(uri) => {
            format!("{}{}", uri.trim_end_matches('/'), COPILOT_SETTINGS_PATH)
        }
        None => COPILOT_SETTINGS_URL.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    async fn test_copilot_settings_url_with_enterprise_uri(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        });

        cx.update_global(|settings_store: &mut SettingsStore, cx| {
            settings_store
                .set_user_settings(
                    r#"{"edit_predictions":{"copilot":{"enterprise_uri":"https://my-company.ghe.com"}}}"#,
                    cx,
                )
                .unwrap();
        });

        let url = cx.update(|cx| {
            let all_language_settings = all_language_settings(None, cx);
            copilot_settings_url(
                all_language_settings
                    .edit_predictions
                    .copilot
                    .enterprise_uri
                    .as_deref(),
            )
        });

        assert_eq!(url, "https://my-company.ghe.com/settings/copilot");
    }

    #[gpui::test]
    async fn test_copilot_settings_url_with_enterprise_uri_trailing_slash(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        });

        cx.update_global(|settings_store: &mut SettingsStore, cx| {
            settings_store
                .set_user_settings(
                    r#"{"edit_predictions":{"copilot":{"enterprise_uri":"https://my-company.ghe.com/"}}}"#,
                    cx,
                )
                .unwrap();
        });

        let url = cx.update(|cx| {
            let all_language_settings = all_language_settings(None, cx);
            copilot_settings_url(
                all_language_settings
                    .edit_predictions
                    .copilot
                    .enterprise_uri
                    .as_deref(),
            )
        });

        assert_eq!(url, "https://my-company.ghe.com/settings/copilot");
    }

    #[gpui::test]
    async fn test_copilot_settings_url_without_enterprise_uri(cx: &mut TestAppContext) {
        cx.update(|cx| {
            let settings_store = SettingsStore::test(cx);
            cx.set_global(settings_store);
        });

        let url = cx.update(|cx| {
            let all_language_settings = all_language_settings(None, cx);
            copilot_settings_url(
                all_language_settings
                    .edit_predictions
                    .copilot
                    .enterprise_uri
                    .as_deref(),
            )
        });

        assert_eq!(url, "https://github.com/settings/copilot");
    }
}
