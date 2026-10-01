use anyhow::{Context as _, Result};
use collections::HashMap;
use context_server::{ContextServerCommand, ContextServerId};
use editor::{Editor, EditorElement, EditorStyle};

use gpui::{
    AsyncWindowContext, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, ScrollHandle,
    Subscription, Task, TextStyle, TextStyleRefinement, UnderlineStyle, WeakEntity, prelude::*,
};
use i18n as app_i18n;
use language::{Language, LanguageRegistry};
use markdown::{Markdown, MarkdownElement, MarkdownStyle};
use notifications::status_toast::StatusToast;
use parking_lot::Mutex;
use project::{
    context_server_store::{
        ContextServerStatus, ContextServerStore, ServerStatusChangedEvent,
        registry::ContextServerDescriptorRegistry,
    },
    project_settings::{ContextServerSettings, OAuthClientSettings, ProjectSettings},
    worktree_store::WorktreeStore,
};
use serde::Deserialize;
use settings::{Settings as _, update_settings_file};
use std::sync::Arc;
use theme_settings::ThemeSettings;
use ui::{
    CommonAnimationExt, KeyBinding, Modal, ModalFooter, ModalHeader, Section, Tooltip,
    WithScrollbar, prelude::*,
};
use util::ResultExt as _;
use workspace::{ModalView, Workspace};

use crate::{AddContextServer, ContextServerType};

fn tr(cx: &App, key: &'static str, fallback: &'static str) -> SharedString {
    app_i18n::tr(cx, key, fallback).into()
}

fn template_text(cx: Option<&App>, key: &'static str, fallback: &'static str) -> String {
    cx.map_or_else(|| fallback.to_owned(), |cx| app_i18n::tr(cx, key, fallback))
}

enum ConfigurationTarget {
    New {
        server_type: ContextServerType,
    },
    Existing {
        id: ContextServerId,
        command: ContextServerCommand,
    },
    ExistingHttp {
        id: ContextServerId,
        url: String,
        headers: HashMap<String, String>,
        timeout: Option<u64>,
        oauth: Option<OAuthClientSettings>,
    },

    Extension {
        id: ContextServerId,
        repository_url: Option<SharedString>,
        installation: Option<extension::ContextServerConfiguration>,
    },
}

enum ConfigurationSource {
    New {
        editor: Entity<Editor>,
        server_type: ContextServerType,
    },
    Existing {
        editor: Entity<Editor>,
        server_type: ContextServerType,
    },
    Extension {
        id: ContextServerId,
        editor: Option<Entity<Editor>>,
        repository_url: Option<SharedString>,
        installation_instructions: Option<Entity<markdown::Markdown>>,
        settings_validator: Option<jsonschema::Validator>,
    },
}

impl ConfigurationSource {
    fn has_configuration_options(&self) -> bool {
        !matches!(self, ConfigurationSource::Extension { editor: None, .. })
    }

    fn is_new(&self) -> bool {
        matches!(self, ConfigurationSource::New { .. })
    }

    fn from_target(
        target: ConfigurationTarget,
        language_registry: Arc<LanguageRegistry>,
        jsonc_language: Option<Arc<Language>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Result<Self> {
        fn create_editor(
            json: String,
            jsonc_language: Option<Arc<Language>>,
            window: &mut Window,
            cx: &mut App,
        ) -> Entity<Editor> {
            cx.new(|cx| {
                let mut editor = Editor::auto_height(4, 16, window, cx);
                editor.set_text(json, window, cx);
                editor.set_show_gutter(false, cx);
                editor.set_soft_wrap_mode(language::language_settings::SoftWrap::None, cx);
                if let Some(buffer) = editor.buffer().read(cx).as_singleton() {
                    buffer.update(cx, |buffer, cx| buffer.set_language(jsonc_language, cx))
                }
                editor
            })
        }

        let source = match target {
            ConfigurationTarget::New { server_type } => ConfigurationSource::New {
                editor: create_editor(
                    match server_type {
                        ContextServerType::Remote => context_server_http_input(None, Some(cx))?,
                        ContextServerType::Local => context_server_input(None, Some(cx))?,
                    },
                    jsonc_language,
                    window,
                    cx,
                ),
                server_type,
            },
            ConfigurationTarget::Existing { id, command } => ConfigurationSource::Existing {
                editor: create_editor(
                    context_server_input(Some((id, command)), Some(cx))?,
                    jsonc_language,
                    window,
                    cx,
                ),
                server_type: ContextServerType::Local,
            },
            ConfigurationTarget::ExistingHttp {
                id,
                url,
                headers: auth,
                timeout,
                oauth,
            } => ConfigurationSource::Existing {
                editor: create_editor(
                    context_server_http_input(Some((id, url, auth, timeout, oauth)), Some(cx))?,
                    jsonc_language,
                    window,
                    cx,
                ),
                server_type: ContextServerType::Remote,
            },

            ConfigurationTarget::Extension {
                id,
                repository_url,
                installation,
            } => {
                let settings_validator = installation.as_ref().and_then(|installation| {
                    jsonschema::validator_for(&installation.settings_schema)
                        .context("Failed to load JSON schema for context server settings")
                        .log_err()
                });
                let installation_instructions = installation.as_ref().map(|installation| {
                    cx.new(|cx| {
                        Markdown::new(
                            installation.installation_instructions.clone().into(),
                            Some(language_registry.clone()),
                            None,
                            cx,
                        )
                    })
                });
                ConfigurationSource::Extension {
                    id,
                    repository_url,
                    installation_instructions,
                    settings_validator,
                    editor: installation.map(|installation| {
                        create_editor(installation.default_settings, jsonc_language, window, cx)
                    }),
                }
            }
        };
        Ok(source)
    }

    fn output(&self, cx: &mut App) -> Result<(ContextServerId, ContextServerSettings)> {
        match self {
            ConfigurationSource::New {
                editor,
                server_type,
            }
            | ConfigurationSource::Existing {
                editor,
                server_type,
            } => match *server_type {
                ContextServerType::Remote => parse_http_input_for_ui(&editor.read(cx).text(cx), cx)
                    .map(|(id, url, auth, timeout, oauth)| {
                        (
                            id,
                            ContextServerSettings::Http {
                                enabled: true,
                                url,
                                headers: auth,
                                timeout,
                                oauth,
                            },
                        )
                    }),
                ContextServerType::Local => {
                    parse_input_for_ui(&editor.read(cx).text(cx), cx).map(|(id, command)| {
                        (
                            id,
                            ContextServerSettings::Stdio {
                                enabled: true,
                                remote: false,
                                command,
                            },
                        )
                    })
                }
            },
            ConfigurationSource::Extension {
                id,
                editor,
                settings_validator,
                ..
            } => {
                let text = editor
                    .as_ref()
                    .context(app_i18n::tr(
                        cx,
                        "agent_ui.context_server.no_output_available",
                        "No output available",
                    ))?
                    .read(cx)
                    .text(cx);
                let settings = serde_json_lenient::from_str::<serde_json::Value>(&text)?;
                if let Some(settings_validator) = settings_validator
                    && let Err(error) = settings_validator.validate(&settings)
                {
                    return Err(anyhow::anyhow!(error.to_string()));
                }
                Ok((
                    id.clone(),
                    ContextServerSettings::Extension {
                        enabled: true,
                        remote: false,
                        settings,
                    },
                ))
            }
        }
    }
}

fn context_server_input(
    existing: Option<(ContextServerId, ContextServerCommand)>,
    cx: Option<&App>,
) -> Result<String> {
    let (name, command, args, env, timeout) = match existing {
        Some((id, cmd)) => {
            let name = serde_json::to_string(id.0.as_ref())
                .context("failed to serialize context server name")?;
            let args = serde_json::to_string(&cmd.args)
                .context("failed to serialize context server arguments")?;
            let env = serde_json::to_string(&cmd.env.unwrap_or_default())
                .context("failed to serialize context server environment")?;
            let command = serde_json::to_string(&cmd.path)
                .context("failed to serialize context server command")?;
            (name, command, args, env, cmd.timeout)
        }
        None => (
            serde_json::to_string("some-mcp-server")?,
            String::new(),
            "[]".to_owned(),
            "{}".to_owned(),
            None,
        ),
    };
    let timeout = timeout.map_or_else(String::new, |timeout| {
        format!(",\n    \"timeout\": {timeout}")
    });

    Ok(format!(
        r#"{{
  /// {}
  ///
  /// {}
  {name}: {{
    /// {}
    "command": {command},
    /// {}
    "args": {args},
    /// {}
    "env": {env}{timeout}
  }}
}}"#,
        template_text(
            cx,
            "agent_ui.context_server.template_local_configure_stdin_stdout",
            "Configure an MCP server that runs locally via stdin/stdout",
        ),
        template_text(
            cx,
            "agent_ui.context_server.template_local_server_name",
            "The name of your MCP server",
        ),
        template_text(
            cx,
            "agent_ui.context_server.template_local_command",
            "The command which runs the MCP server",
        ),
        template_text(
            cx,
            "agent_ui.context_server.template_local_args",
            "The arguments to pass to the MCP server",
        ),
        template_text(
            cx,
            "agent_ui.context_server.template_local_env",
            "The environment variables to set",
        )
    ))
}

fn context_server_http_input(
    existing: Option<(
        ContextServerId,
        String,
        HashMap<String, String>,
        Option<u64>,
        Option<OAuthClientSettings>,
    )>,
    cx: Option<&App>,
) -> Result<String> {
    let (name, url, headers, timeout, oauth) = match existing {
        Some((id, url, headers, timeout, oauth)) => {
            let name = serde_json::to_string(id.0.as_ref())
                .context("failed to serialize context server name")?;
            let url =
                serde_json::to_string(&url).context("failed to serialize context server URL")?;
            let headers = if headers.is_empty() {
                r#"// "Authorization": "Bearer <token>"#.to_owned()
            } else {
                let json = serde_json::to_string_pretty(&headers)
                    .context("failed to serialize context server headers")?;
                let mut lines = json.split("\n").collect::<Vec<_>>();
                if lines.len() > 1 {
                    lines.remove(0);
                    lines.pop();
                }
                lines
                    .into_iter()
                    .map(|line| format!("  {}", line))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            (name, url, headers, timeout, oauth)
        }
        None => (
            serde_json::to_string("some-remote-server")?,
            serde_json::to_string("https://example.com/mcp")?,
            r#"// "Authorization": "Bearer <token>"#.to_owned(),
            None,
            None,
        ),
    };
    let timeout = timeout.map_or_else(String::new, |timeout| {
        format!("\n    \"timeout\": {timeout},")
    });

    let oauth = match oauth {
        None => {
            format!(
                r#"
    /// {}
    // "oauth": {{
    //   "client_id": "your-client-id",
    // }},"#,
                template_text(
                    cx,
                    "agent_ui.context_server.template_remote_oauth_comment",
                    "Uncomment to use a pre-registered OAuth client. You can include the client secret here as well, otherwise it will be prompted interactively and saved in the system keychain.",
                ),
            )
        }
        Some(oauth) => {
            let mut lines = vec![
                String::from("\n    \"oauth\": {"),
                format!(
                    "      \"client_id\": {},",
                    serde_json::to_string(&oauth.client_id)
                        .context("failed to serialize OAuth client ID")?
                ),
            ];
            if let Some(client_secret) = oauth.client_secret {
                lines.push(format!(
                    "      \"client_secret\": {}",
                    serde_json::to_string(&client_secret)
                        .context("failed to serialize OAuth client secret")?
                ));
            } else {
                lines.push(format!(
                    "      /// {}\n      // \"client_secret\": \"your-client-secret\"",
                    template_text(
                        cx,
                        "agent_ui.context_server.template_remote_optional_client_secret",
                        "Optional client secret for confidential clients",
                    ),
                ));
            }
            lines.push(String::from("    },"));

            lines.join("\n")
        }
    };

    Ok(format!(
        r#"{{
  /// {}
  ///
  /// {}
  {name}: {{
    /// {}
    "url": {url},{timeout}{oauth}
    "headers": {{
     /// {}
     {headers}
    }}
  }}
}}"#,
        template_text(
            cx,
            "agent_ui.context_server.template_remote_configure_http",
            "Configure an MCP server that you connect to over HTTP",
        ),
        template_text(
            cx,
            "agent_ui.context_server.template_remote_server_name",
            "The name of your remote MCP server",
        ),
        template_text(
            cx,
            "agent_ui.context_server.template_remote_url",
            "The URL of the remote MCP server",
        ),
        template_text(
            cx,
            "agent_ui.context_server.template_remote_headers",
            "Any headers to send along",
        )
    ))
}

fn parse_http_input(
    text: &str,
) -> Result<(
    ContextServerId,
    String,
    HashMap<String, String>,
    Option<u64>,
    Option<OAuthClientSettings>,
)> {
    #[derive(Deserialize)]
    struct Temp {
        url: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default)]
        timeout: Option<u64>,
        #[serde(default)]
        oauth: Option<OAuthClientSettings>,
    }
    let value: HashMap<String, Temp> = serde_json_lenient::from_str(text)?;
    if value.len() != 1 {
        anyhow::bail!("Expected exactly one context server configuration");
    }

    let Some((key, value)) = value.into_iter().next() else {
        anyhow::bail!("Expected exactly one context server configuration");
    };

    Ok((
        ContextServerId(key.into()),
        value.url,
        value.headers,
        value.timeout,
        value.oauth,
    ))
}

fn parse_http_input_for_ui(
    text: &str,
    cx: &App,
) -> Result<(
    ContextServerId,
    String,
    HashMap<String, String>,
    Option<u64>,
    Option<OAuthClientSettings>,
)> {
    parse_http_input(text).map_err(|error| {
        if error
            .to_string()
            .contains("Expected exactly one context server configuration")
        {
            anyhow::anyhow!(app_i18n::tr(
                cx,
                "agent_ui.context_server.expected_exactly_one_context_server_configuration",
                "Expected exactly one context server configuration",
            ))
        } else {
            error
        }
    })
}

fn resolve_context_server_extension(
    id: ContextServerId,
    worktree_store: Entity<WorktreeStore>,
    cx: &mut App,
) -> Task<Option<ConfigurationTarget>> {
    let registry = ContextServerDescriptorRegistry::default_global(cx).read(cx);

    let Some(descriptor) = registry.context_server_descriptor(&id.0) else {
        return Task::ready(None);
    };

    let extension = crate::agent_configuration::resolve_extension_for_context_server(&id, cx);
    let failed_to_resolve_configuration = app_i18n::tr(
        cx,
        "agent_ui.context_server.failed_to_resolve_configuration",
        "Failed to resolve context server configuration",
    );
    cx.spawn(async move |cx| {
        let installation = descriptor
            .configuration(worktree_store, cx)
            .await
            .context(failed_to_resolve_configuration)
            .log_err()
            .flatten();

        Some(ConfigurationTarget::Extension {
            id,
            repository_url: extension
                .and_then(|(_, manifest)| manifest.repository.clone().map(SharedString::from)),
            installation,
        })
    })
}

enum State {
    Idle,
    Waiting,
    AuthRequired {
        server_id: ContextServerId,
    },
    ClientSecretRequired {
        server_id: ContextServerId,
        error: Option<SharedString>,
    },
    Authenticating {
        server_id: ContextServerId,
    },
    Error(SharedString),
}

pub struct ConfigureContextServerModal {
    context_server_store: Entity<ContextServerStore>,
    workspace: WeakEntity<Workspace>,
    source: ConfigurationSource,
    state: State,
    original_server_id: Option<ContextServerId>,
    scroll_handle: ScrollHandle,
    secret_editor: Entity<Editor>,
    _auth_subscription: Option<Subscription>,
}

impl ConfigureContextServerModal {
    fn initial_state(
        context_server_store: &Entity<ContextServerStore>,
        target: &ConfigurationTarget,
        cx: &App,
    ) -> State {
        let Some(server_id) = (match target {
            ConfigurationTarget::Existing { id, .. }
            | ConfigurationTarget::ExistingHttp { id, .. }
            | ConfigurationTarget::Extension { id, .. } => Some(id),
            ConfigurationTarget::New { .. } => None,
        }) else {
            return State::Idle;
        };

        match context_server_store.read(cx).status_for_server(server_id) {
            Some(ContextServerStatus::AuthRequired) => State::AuthRequired {
                server_id: server_id.clone(),
            },
            Some(ContextServerStatus::ClientSecretRequired { error }) => {
                State::ClientSecretRequired {
                    server_id: server_id.clone(),
                    error: error.map(SharedString::from),
                }
            }
            Some(ContextServerStatus::Authenticating) => State::Authenticating {
                server_id: server_id.clone(),
            },
            Some(ContextServerStatus::Error(error)) => State::Error(error.into()),

            Some(
                ContextServerStatus::Starting
                | ContextServerStatus::Running
                | ContextServerStatus::Stopped,
            )
            | None => State::Idle,
        }
    }

    pub fn register(
        workspace: &mut Workspace,
        language_registry: Arc<LanguageRegistry>,
        _window: Option<&mut Window>,
        _cx: &mut Context<Workspace>,
    ) {
        workspace.register_action({
            move |_workspace, action: &AddContextServer, window, cx| {
                let workspace_handle = cx.weak_entity();
                let language_registry = language_registry.clone();
                let server_type = action.context_server_type;
                window
                    .spawn(cx, async move |cx| {
                        Self::show_modal(
                            ConfigurationTarget::New { server_type },
                            language_registry,
                            workspace_handle,
                            cx,
                        )
                        .await
                    })
                    .detach_and_log_err(cx);
            }
        });
    }

    pub fn show_modal_for_existing_server(
        server_id: ContextServerId,
        language_registry: Arc<LanguageRegistry>,
        workspace: WeakEntity<Workspace>,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<Result<()>> {
        let not_found = app_i18n::tr(
            cx,
            "agent_ui.context_server.not_found",
            "Context server not found",
        );
        let failed_to_resolve = app_i18n::tr(
            cx,
            "agent_ui.context_server.failed_to_resolve",
            "Failed to resolve context server",
        );
        let Some(settings) = ProjectSettings::get_global(cx)
            .context_servers
            .get(&server_id.0)
            .cloned()
            .or_else(|| {
                ContextServerDescriptorRegistry::default_global(cx)
                    .read(cx)
                    .context_server_descriptor(&server_id.0)
                    .map(|_| ContextServerSettings::default_extension())
            })
        else {
            return Task::ready(Err(anyhow::anyhow!(not_found)));
        };

        window.spawn(cx, async move |cx| {
            let target = match settings {
                ContextServerSettings::Stdio {
                    enabled: _,
                    command,
                    ..
                } => Some(ConfigurationTarget::Existing {
                    id: server_id,
                    command,
                }),
                ContextServerSettings::Http {
                    enabled: _,
                    url,
                    headers,
                    timeout,
                    oauth,
                } => Some(ConfigurationTarget::ExistingHttp {
                    id: server_id,
                    url,
                    headers,
                    timeout,
                    oauth,
                }),

                ContextServerSettings::Extension { .. } => {
                    match workspace
                        .update(cx, |workspace, cx| {
                            resolve_context_server_extension(
                                server_id,
                                workspace.project().read(cx).worktree_store(),
                                cx,
                            )
                        })
                        .log_err()
                    {
                        Some(task) => task.await,
                        None => None,
                    }
                }
            };

            match target {
                Some(target) => Self::show_modal(target, language_registry, workspace, cx).await,
                None => Err(anyhow::anyhow!(failed_to_resolve)),
            }
        })
    }

    fn show_modal(
        target: ConfigurationTarget,
        language_registry: Arc<LanguageRegistry>,
        workspace: WeakEntity<Workspace>,
        cx: &mut AsyncWindowContext,
    ) -> Task<Result<()>> {
        cx.spawn(async move |cx| {
            let jsonc_language = language_registry.language_for_name("jsonc").await.log_err();
            workspace.update_in(cx, |workspace, window, cx| -> Result<()> {
                let workspace_handle = cx.weak_entity();
                let context_server_store = workspace.project().read(cx).context_server_store();
                let original_server_id = match &target {
                    ConfigurationTarget::Existing { id, .. } => Some(id.clone()),
                    ConfigurationTarget::ExistingHttp { id, .. } => Some(id.clone()),
                    ConfigurationTarget::Extension { id, .. } => Some(id.clone()),
                    ConfigurationTarget::New { .. } => None,
                };
                let state = Self::initial_state(&context_server_store, &target, cx);
                let source = ConfigurationSource::from_target(
                    target,
                    language_registry,
                    jsonc_language,
                    window,
                    cx,
                )?;
                workspace.toggle_modal(window, cx, |window, cx| Self {
                    context_server_store: context_server_store.clone(),
                    workspace: workspace_handle,
                    state,
                    original_server_id,
                    source,
                    scroll_handle: ScrollHandle::new(),
                    secret_editor: cx.new(|cx| {
                        let mut editor = Editor::single_line(window, cx);
                        editor.set_placeholder_text(
                            &app_i18n::tr(
                                cx,
                                "agent_ui.context_server.enter_client_secret_public_clients",
                                "Enter client secret (leave empty for public clients)",
                            ),
                            window,
                            cx,
                        );
                        editor.set_masked(true, cx);
                        editor
                    }),
                    _auth_subscription: None,
                });
                Ok(())
            })??;
            Ok(())
        })
    }

    fn set_error(&mut self, err: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.state = State::Error(err.into());
        cx.notify();
    }

    fn confirm(&mut self, _: &menu::Confirm, cx: &mut Context<Self>) {
        if matches!(self.state, State::Waiting | State::Authenticating { .. }) {
            return;
        }

        self._auth_subscription = None;

        self.state = State::Idle;
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };

        let (id, settings) = match self.source.output(cx) {
            Ok(val) => val,
            Err(error) => {
                self.set_error(error.to_string(), cx);
                return;
            }
        };

        self.state = State::Waiting;

        let existing_server = self.context_server_store.read(cx).get_server(&id);
        if existing_server.is_some() {
            self.context_server_store.update(cx, |store, cx| {
                store.stop_server(&id, cx).log_err();
            });
        }

        let wait_for_context_server_task = wait_for_context_server(
            &self.context_server_store,
            id.clone(),
            tr(
                cx,
                "agent_ui.context_server.stopped_running",
                "Context server stopped running",
            )
            .into(),
            tr(
                cx,
                "agent_ui.context_server.store_dropped",
                "Context server store was dropped",
            )
            .into(),
            app_i18n::tr(
                cx,
                "agent_ui.context_server.timed_out_waiting_to_start",
                "Timed out waiting for context server `{}` to start. Check the ZZZ log for details.",
            ),
            cx,
        );
        cx.spawn({
            let id = id.clone();
            async move |this, cx| {
                let result = wait_for_context_server_task.await;
                this.update(cx, |this, cx| match result {
                    Ok(ContextServerStatus::Running) => {
                        this.state = State::Idle;
                        this.show_configured_context_server_toast(id, cx);
                        cx.emit(DismissEvent);
                    }
                    Ok(ContextServerStatus::AuthRequired) => {
                        this.state = State::AuthRequired { server_id: id };
                        cx.notify();
                    }
                    Ok(ContextServerStatus::ClientSecretRequired { error }) => {
                        this.state = State::ClientSecretRequired {
                            server_id: id,
                            error: error.map(SharedString::from),
                        };
                        cx.notify();
                    }
                    Err(err) => {
                        this.set_error(err, cx);
                    }
                    Ok(_) => {}
                })
            }
        })
        .detach();

        let settings_changed =
            ProjectSettings::get_global(cx).context_servers.get(&id.0) != Some(&settings);

        if settings_changed {
            // When we write the settings to the file, the context server will be restarted.
            workspace.update(cx, |workspace, cx| {
                let fs = workspace.app_state().fs.clone();
                let original_server_id = self.original_server_id.clone();
                update_settings_file(fs.clone(), cx, move |current, _| {
                    if let Some(original_id) = original_server_id {
                        if original_id != id {
                            current.project.context_servers.remove(&original_id.0);
                        }
                    }
                    current
                        .project
                        .context_servers
                        .insert(id.0, settings.into());
                });
            });
        } else if let Some(existing_server) = existing_server {
            self.context_server_store
                .update(cx, |store, cx| store.start_server(existing_server, cx));
        }
    }

    fn cancel(&mut self, _: &menu::Cancel, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn cancel_authentication(&mut self, server_id: &ContextServerId, cx: &mut Context<Self>) {
        self._auth_subscription = None;
        self.context_server_store.update(cx, |store, cx| {
            store.stop_server(server_id, cx).log_err();
        });
        self.state = State::Idle;
        cx.notify();
    }

    fn authenticate(&mut self, server_id: ContextServerId, cx: &mut Context<Self>) {
        self.context_server_store.update(cx, |store, cx| {
            store.authenticate_server(&server_id, cx).log_err();
        });
        self.await_auth_outcome(server_id, cx);
    }

    fn submit_client_secret(&mut self, server_id: ContextServerId, cx: &mut Context<Self>) {
        let secret = self.secret_editor.read(cx).text(cx);
        self.context_server_store.update(cx, |store, cx| {
            store.submit_client_secret(&server_id, secret, cx).log_err();
        });
        self.await_auth_outcome(server_id, cx);
    }

    fn await_auth_outcome(&mut self, server_id: ContextServerId, cx: &mut Context<Self>) {
        self.state = State::Authenticating {
            server_id: server_id.clone(),
        };

        self._auth_subscription = Some(cx.subscribe(
            &self.context_server_store,
            move |this, _, event: &ServerStatusChangedEvent, cx| {
                if event.server_id != server_id {
                    return;
                }
                match &event.status {
                    ContextServerStatus::Running => {
                        this._auth_subscription = None;
                        this.state = State::Idle;
                        this.show_configured_context_server_toast(event.server_id.clone(), cx);
                        cx.emit(DismissEvent);
                    }
                    ContextServerStatus::AuthRequired => {
                        this._auth_subscription = None;
                        this.state = State::AuthRequired {
                            server_id: event.server_id.clone(),
                        };
                        cx.notify();
                    }
                    ContextServerStatus::ClientSecretRequired { error } => {
                        this._auth_subscription = None;
                        this.state = State::ClientSecretRequired {
                            server_id: event.server_id.clone(),
                            error: error.clone().map(SharedString::from),
                        };
                        cx.notify();
                    }
                    ContextServerStatus::Error(error) => {
                        this._auth_subscription = None;
                        this.set_error(error.clone(), cx);
                    }
                    ContextServerStatus::Authenticating
                    | ContextServerStatus::Starting
                    | ContextServerStatus::Stopped => {}
                }
            },
        ));

        cx.notify();
    }

    fn show_configured_context_server_toast(&self, id: ContextServerId, cx: &mut App) {
        self.workspace
            .update(cx, {
                |workspace, cx| {
                    let status_toast = StatusToast::new(
                        app_i18n::tr(
                            cx,
                            "agent_ui.context_server.configured_successfully",
                            "{} configured successfully.",
                        )
                        .replacen("{}", id.0.as_ref(), 1),
                        cx,
                        |this, cx| {
                            this.icon(
                                Icon::new(IconName::ToolHammer)
                                    .size(IconSize::Small)
                                    .color(Color::Muted),
                            )
                            .action(
                                app_i18n::tr(cx, "agent_ui.context_server.dismiss", "Dismiss"),
                                |_, _| {},
                            )
                        },
                    );

                    workspace.toggle_status_toast(status_toast, cx);
                }
            })
            .log_err();
    }
}

fn parse_input(text: &str) -> Result<(ContextServerId, ContextServerCommand)> {
    let value: serde_json::Value = serde_json_lenient::from_str(text)?;
    let object = value.as_object().context("Expected object")?;
    anyhow::ensure!(object.len() == 1, "Expected exactly one key-value pair");
    let Some((context_server_name, value)) = object.iter().next() else {
        anyhow::bail!("Expected exactly one key-value pair");
    };
    let command: ContextServerCommand = serde_json::from_value(value.clone())?;
    Ok((ContextServerId(context_server_name.clone().into()), command))
}

fn parse_input_for_ui(text: &str, cx: &App) -> Result<(ContextServerId, ContextServerCommand)> {
    parse_input(text).map_err(|error| {
        let message = error.to_string();
        if message.contains("Expected object") {
            anyhow::anyhow!(app_i18n::tr(
                cx,
                "agent_ui.context_server.expected_object",
                "Expected object",
            ))
        } else if message.contains("Expected exactly one key-value pair") {
            anyhow::anyhow!(app_i18n::tr(
                cx,
                "agent_ui.context_server.expected_exactly_one_key_value_pair",
                "Expected exactly one key-value pair",
            ))
        } else {
            error
        }
    })
}

impl ModalView for ConfigureContextServerModal {}

impl Focusable for ConfigureContextServerModal {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match &self.source {
            ConfigurationSource::New { editor, .. } => editor.focus_handle(cx),
            ConfigurationSource::Existing { editor, .. } => editor.focus_handle(cx),
            ConfigurationSource::Extension { editor, .. } => editor
                .as_ref()
                .map_or_else(|| cx.focus_handle(), |editor| editor.focus_handle(cx)),
        }
    }
}

impl EventEmitter<DismissEvent> for ConfigureContextServerModal {}

impl ConfigureContextServerModal {
    fn render_modal_header(&self, cx: &App) -> ModalHeader {
        let text: SharedString = match &self.source {
            ConfigurationSource::New { .. } => tr(
                cx,
                "agent_ui.context_server.add_mcp_server",
                "Add MCP Server",
            ),
            ConfigurationSource::Existing { .. } => tr(
                cx,
                "agent_ui.context_server.configure_mcp_server",
                "Configure MCP Server",
            ),
            ConfigurationSource::Extension { id, .. } => app_i18n::tr(
                cx,
                "agent_ui.context_server.configure_server",
                "Configure {}",
            )
            .replacen("{}", id.0.as_ref(), 1)
            .into(),
        };
        ModalHeader::new().headline(text)
    }

    fn render_modal_description(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        const MODAL_DESCRIPTION: &str =
            "Check the server docs for required arguments and environment variables.";

        if let ConfigurationSource::Extension {
            installation_instructions: Some(installation_instructions),
            ..
        } = &self.source
        {
            div()
                .pb_2()
                .text_sm()
                .child(MarkdownElement::new(
                    installation_instructions.clone(),
                    default_markdown_style(window, cx),
                ))
                .into_any_element()
        } else {
            Label::new(tr(
                cx,
                "agent_ui.context_server.modal_description",
                MODAL_DESCRIPTION,
            ))
            .color(Color::Muted)
            .into_any_element()
        }
    }

    fn render_tab_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let is_http = match &self.source {
            ConfigurationSource::New { server_type, .. } => {
                *server_type == ContextServerType::Remote
            }
            _ => return None,
        };

        let tab = |label: SharedString, active: bool| {
            div()
                .id(label.clone())
                .cursor_pointer()
                .p_1()
                .text_sm()
                .border_b_1()
                .when_else(
                    active,
                    |this| this.border_color(cx.theme().colors().border_focused),
                    |this| {
                        this.border_color(gpui::transparent_black())
                            .text_color(cx.theme().colors().text_muted)
                            .hover(|s| s.text_color(cx.theme().colors().text))
                    },
                )
                .child(label)
        };

        Some(
            h_flex()
                .pt_1()
                .mb_2p5()
                .gap_1()
                .border_b_1()
                .border_color(cx.theme().colors().border.opacity(0.5))
                .child(
                    tab(tr(cx, "agent_ui.context_server.local", "Local"), !is_http).on_click(
                        cx.listener(|this, _, window, cx| {
                            if let ConfigurationSource::New {
                                editor,
                                server_type,
                            } = &mut this.source
                                && *server_type != ContextServerType::Local
                            {
                                if let Some(new_text) =
                                    context_server_input(None, Some(cx)).log_err()
                                {
                                    *server_type = ContextServerType::Local;
                                    editor.update(cx, |editor, cx| {
                                        editor.set_text(new_text, window, cx);
                                    });
                                }
                            }
                        }),
                    ),
                )
                .child(
                    tab(tr(cx, "agent_ui.context_server.remote", "Remote"), is_http).on_click(
                        cx.listener(|this, _, window, cx| {
                            if let ConfigurationSource::New {
                                editor,
                                server_type,
                            } = &mut this.source
                                && *server_type != ContextServerType::Remote
                            {
                                if let Some(new_text) =
                                    context_server_http_input(None, Some(cx)).log_err()
                                {
                                    *server_type = ContextServerType::Remote;
                                    editor.update(cx, |editor, cx| {
                                        editor.set_text(new_text, window, cx);
                                    });
                                }
                            }
                        }),
                    ),
                )
                .into_any_element(),
        )
    }

    fn render_modal_content(&self, cx: &App) -> AnyElement {
        let editor = match &self.source {
            ConfigurationSource::New { editor, .. } => editor,
            ConfigurationSource::Existing { editor, .. } => editor,
            ConfigurationSource::Extension { editor, .. } => {
                let Some(editor) = editor else {
                    return div().into_any_element();
                };
                editor
            }
        };

        div()
            .p_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().colors().border_variant)
            .bg(cx.theme().colors().editor_background)
            .child({
                let settings = ThemeSettings::get_global(cx);
                let text_style = TextStyle {
                    color: cx.theme().colors().text,
                    font_family: settings.buffer_font.family.clone(),
                    font_fallbacks: settings.buffer_font.fallbacks.clone(),
                    font_size: settings.buffer_font_size(cx).into(),
                    font_weight: settings.buffer_font.weight,
                    line_height: relative(settings.buffer_line_height.value()),
                    ..Default::default()
                };
                EditorElement::new(
                    editor,
                    EditorStyle {
                        background: cx.theme().colors().editor_background,
                        local_player: cx.theme().players().local(),
                        text: text_style,
                        syntax: cx.theme().syntax().clone(),
                        ..Default::default()
                    },
                )
            })
            .into_any_element()
    }

    fn render_modal_footer(&self, cx: &mut Context<Self>) -> ModalFooter {
        let focus_handle = self.focus_handle(cx);
        let is_busy = matches!(self.state, State::Waiting | State::Authenticating { .. });

        ModalFooter::new()
            .start_slot::<Button>(
                if let ConfigurationSource::Extension {
                    repository_url: Some(repository_url),
                    ..
                } = &self.source
                {
                    Some(
                        Button::new(
                            "open-repository",
                            tr(
                                cx,
                                "agent_ui.context_server.open_repository",
                                "Open Repository",
                            ),
                        )
                        .end_icon(
                            Icon::new(IconName::ArrowUpRight)
                                .size(IconSize::Small)
                                .color(Color::Muted),
                        )
                        .tooltip({
                            let repository_url = repository_url.clone();
                            move |_window, cx| {
                                Tooltip::with_meta(
                                    tr(
                                        cx,
                                        "agent_ui.context_server.open_repository",
                                        "Open Repository",
                                    ),
                                    None,
                                    repository_url.clone(),
                                    cx,
                                )
                            }
                        })
                        .on_click({
                            let repository_url = repository_url.clone();
                            move |_, _, cx| cx.open_url(&repository_url)
                        }),
                    )
                } else {
                    None
                },
            )
            .end_slot(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new(
                            "cancel",
                            if self.source.has_configuration_options() {
                                tr(cx, "agent_ui.context_server.cancel", "Cancel")
                            } else {
                                tr(cx, "agent_ui.context_server.dismiss", "Dismiss")
                            },
                        )
                        .key_binding(
                            KeyBinding::for_action_in(&menu::Cancel, &focus_handle, cx)
                                .map(|kb| kb.size(rems_from_px(12.))),
                        )
                        .on_click(
                            cx.listener(|this, _event, _window, cx| this.cancel(&menu::Cancel, cx)),
                        ),
                    )
                    .children(self.source.has_configuration_options().then(|| {
                        Button::new(
                            "add-server",
                            if self.source.is_new() {
                                tr(cx, "agent_ui.context_server.add_server", "Add Server")
                            } else {
                                tr(
                                    cx,
                                    "agent_ui.context_server.configure_server_button",
                                    "Configure Server",
                                )
                            },
                        )
                        .disabled(is_busy)
                        .key_binding(
                            KeyBinding::for_action_in(&menu::Confirm, &focus_handle, cx)
                                .map(|kb| kb.size(rems_from_px(12.))),
                        )
                        .on_click(
                            cx.listener(|this, _event, _window, cx| {
                                this.confirm(&menu::Confirm, cx)
                            }),
                        )
                    })),
            )
    }

    fn render_loading(&self, label: impl Into<SharedString>) -> Div {
        h_flex()
            .h_8()
            .gap_1p5()
            .justify_center()
            .child(
                Icon::new(IconName::LoadCircle)
                    .size(IconSize::XSmall)
                    .color(Color::Muted)
                    .with_rotate_animation(3),
            )
            .child(Label::new(label).size(LabelSize::Small).color(Color::Muted))
    }

    fn render_auth_required(&self, server_id: &ContextServerId, cx: &mut Context<Self>) -> Div {
        h_flex()
            .h_8()
            .min_w_0()
            .w_full()
            .gap_2()
            .justify_center()
            .child(
                h_flex()
                    .gap_1p5()
                    .child(
                        Icon::new(IconName::Info)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new(tr(
                            cx,
                            "agent_ui.context_server.authenticate_to_connect_this_server",
                            "Authenticate to connect this server",
                        ))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                    ),
            )
            .child(
                Button::new(
                    "authenticate-server",
                    tr(cx, "agent_ui.context_server.authenticate", "Authenticate"),
                )
                .style(ButtonStyle::Outlined)
                .label_size(LabelSize::Small)
                .on_click({
                    let server_id = server_id.clone();
                    cx.listener(move |this, _event, _window, cx| {
                        this.authenticate(server_id.clone(), cx);
                    })
                }),
            )
    }

    fn render_client_secret_required(
        &self,
        server_id: &ContextServerId,
        error: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> Div {
        let settings = ThemeSettings::get_global(cx);
        let text_style = TextStyle {
            color: cx.theme().colors().text,
            font_family: settings.buffer_font.family.clone(),
            font_fallbacks: settings.buffer_font.fallbacks.clone(),
            font_size: settings.buffer_font_size(cx).into(),
            font_weight: settings.buffer_font.weight,
            line_height: relative(settings.buffer_line_height.value()),
            ..Default::default()
        };

        v_flex()
            .w_full()
            .gap_2()
            .when_some(error, |this, error| {
                this.child(Self::render_modal_error(error))
            })
            .child(
                h_flex()
                    .gap_1p5()
                    .child(
                        Icon::new(IconName::Info)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new(tr(
                            cx,
                            "agent_ui.context_server.enter_oauth_client_secret",
                            "Enter your OAuth client secret, or leave empty for public clients",
                        ))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .capture_action({
                        let server_id = server_id.clone();
                        cx.listener(move |this, _: &editor::actions::Newline, _window, cx| {
                            this.submit_client_secret(server_id.clone(), cx);
                        })
                    })
                    .child(div().flex_1().child(EditorElement::new(
                        &self.secret_editor,
                        EditorStyle {
                            background: cx.theme().colors().editor_background,
                            local_player: cx.theme().players().local(),
                            text: text_style,
                            syntax: cx.theme().syntax().clone(),
                            ..Default::default()
                        },
                    )))
                    .child(
                        Button::new(
                            "submit-client-secret",
                            tr(cx, "agent_ui.context_server.submit", "Submit"),
                        )
                        .style(ButtonStyle::Outlined)
                        .label_size(LabelSize::Small)
                        .on_click({
                            let server_id = server_id.clone();
                            cx.listener(move |this, _event, _window, cx| {
                                this.submit_client_secret(server_id.clone(), cx);
                            })
                        }),
                    ),
            )
    }

    fn render_authenticating(&self, server_id: &ContextServerId, cx: &mut Context<Self>) -> Div {
        h_flex()
            .h_8()
            .gap_2()
            .justify_center()
            .child(
                h_flex()
                    .gap_1p5()
                    .child(
                        Icon::new(IconName::LoadCircle)
                            .size(IconSize::XSmall)
                            .color(Color::Muted)
                            .with_rotate_animation(3),
                    )
                    .child(
                        Label::new(tr(
                            cx,
                            "agent_ui.context_server.authenticating",
                            "Authenticating...",
                        ))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                    ),
            )
            .child(
                Button::new(
                    "cancel-authentication",
                    tr(cx, "agent_ui.context_server.cancel", "Cancel"),
                )
                .style(ButtonStyle::Outlined)
                .label_size(LabelSize::Small)
                .on_click({
                    let server_id = server_id.clone();
                    cx.listener(move |this, _event, _window, cx| {
                        this.cancel_authentication(&server_id, cx);
                    })
                }),
            )
    }

    fn render_modal_error(error: SharedString) -> Div {
        h_flex()
            .h_8()
            .gap_1p5()
            .justify_center()
            .child(
                Icon::new(IconName::Warning)
                    .size(IconSize::Small)
                    .color(Color::Warning),
            )
            .child(
                div()
                    .w_full()
                    .child(Label::new(error).size(LabelSize::Small).color(Color::Muted)),
            )
    }
}

impl Render for ConfigureContextServerModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .elevation_3(cx)
            .w(rems(40.))
            .key_context("ConfigureContextServerModal")
            .on_action(
                cx.listener(|this, _: &menu::Cancel, _window, cx| this.cancel(&menu::Cancel, cx)),
            )
            .on_action(
                cx.listener(|this, _: &menu::Confirm, _window, cx| {
                    this.confirm(&menu::Confirm, cx)
                }),
            )
            .capture_any_mouse_down(cx.listener(|this, _, window, cx| {
                this.focus_handle(cx).focus(window, cx);
            }))
            .child(
                Modal::new("configure-context-server", None)
                    .header(self.render_modal_header(cx))
                    .section(
                        Section::new().child(
                            div()
                                .size_full()
                                .child(
                                    div()
                                        .id("modal-content")
                                        .max_h(vh(0.7, window))
                                        .overflow_y_scroll()
                                        .track_scroll(&self.scroll_handle)
                                        .child(self.render_modal_description(window, cx))
                                        .children(self.render_tab_bar(cx))
                                        .child(self.render_modal_content(cx))
                                        .child(match &self.state {
                                            State::Idle => div(),
                                            State::Waiting => self.render_loading(tr(
                                                cx,
                                                "agent_ui.context_server.connecting_server",
                                                "Connecting Server...",
                                            )),
                                            State::AuthRequired { server_id } => {
                                                self.render_auth_required(&server_id.clone(), cx)
                                            }
                                            State::ClientSecretRequired { server_id, error } => {
                                                self.render_client_secret_required(
                                                    &server_id.clone(),
                                                    error.clone(),
                                                    cx,
                                                )
                                            }
                                            State::Authenticating { server_id } => {
                                                self.render_authenticating(&server_id.clone(), cx)
                                            }
                                            State::Error(error) => {
                                                Self::render_modal_error(error.clone())
                                            }
                                        }),
                                )
                                .vertical_scrollbar_for(&self.scroll_handle, window, cx),
                        ),
                    )
                    .footer(self.render_modal_footer(cx)),
            )
    }
}

fn wait_for_context_server(
    context_server_store: &Entity<ContextServerStore>,
    context_server_id: ContextServerId,
    stopped_running_message: Arc<str>,
    store_dropped_message: Arc<str>,
    timeout_message: String,
    cx: &mut App,
) -> Task<Result<ContextServerStatus, Arc<str>>> {
    use std::time::Duration;

    const WAIT_TIMEOUT: Duration = Duration::from_secs(120);

    let (tx, rx) = futures::channel::oneshot::channel();
    let tx = Arc::new(Mutex::new(Some(tx)));

    let context_server_id_for_timeout = context_server_id.clone();
    let subscription = cx.subscribe(context_server_store, move |_, event, _cx| {
        let ServerStatusChangedEvent { server_id, status } = event;

        if server_id != &context_server_id {
            return;
        }

        let send_result = |result| {
            if let Some(tx) = tx.lock().take()
                && tx.send(result).is_err()
            {
                log::debug!("context server status waiter was already dropped");
            }
        };

        match status {
            ContextServerStatus::Running
            | ContextServerStatus::AuthRequired
            | ContextServerStatus::ClientSecretRequired { .. } => {
                send_result(Ok(status.clone()));
            }
            ContextServerStatus::Stopped => {
                send_result(Err(stopped_running_message.clone()));
            }
            ContextServerStatus::Error(error) => {
                send_result(Err(error.clone()));
            }
            ContextServerStatus::Starting | ContextServerStatus::Authenticating => {}
        }
    });

    cx.spawn(async move |cx| {
        let timeout = cx.background_executor().timer(WAIT_TIMEOUT);
        let result = futures::future::select(rx, timeout).await;
        drop(subscription);
        match result {
            futures::future::Either::Left((Ok(inner), _)) => inner,
            futures::future::Either::Left((Err(_), _)) => Err(store_dropped_message),
            futures::future::Either::Right(_) => Err(Arc::from(timeout_message.replacen(
                "{}",
                context_server_id_for_timeout.0.as_ref(),
                1,
            ))),
        }
    })
}

pub(crate) fn default_markdown_style(window: &Window, cx: &App) -> MarkdownStyle {
    let theme_settings = ThemeSettings::get_global(cx);
    let colors = cx.theme().colors();
    let mut text_style = window.text_style();
    text_style.refine(&TextStyleRefinement {
        font_family: Some(theme_settings.ui_font.family.clone()),
        font_fallbacks: theme_settings.ui_font.fallbacks.clone(),
        font_features: Some(theme_settings.ui_font.features.clone()),
        font_size: Some(TextSize::XSmall.rems(cx).into()),
        color: Some(colors.text_muted),
        ..Default::default()
    });

    MarkdownStyle {
        base_text_style: text_style.clone(),
        selection_background_color: colors.element_selection_background,
        link: TextStyleRefinement {
            background_color: Some(colors.editor_foreground.opacity(0.025)),
            underline: Some(UnderlineStyle {
                color: Some(colors.text_accent.opacity(0.5)),
                thickness: px(1.),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_http_input_reads_oauth_settings() {
        let (id, url, headers, timeout, oauth) = parse_http_input(
            r#"{
  "figma": {
    "url": "https://mcp.figma.com/mcp",
    "oauth": {
      "client_id": "client-id",
      "client_secret": "client-secret"
    },
    "headers": {
      "X-Test": "test"
    }
  }
}"#,
        )
        .unwrap();

        assert_eq!(id, ContextServerId("figma".into()));
        assert_eq!(url, "https://mcp.figma.com/mcp");
        assert_eq!(headers.get("X-Test"), Some(&String::from("test")));
        assert_eq!(timeout, None);
        let oauth = oauth.expect("oauth should be present");
        assert_eq!(oauth.client_id, "client-id");
        assert_eq!(oauth.client_secret.as_deref(), Some("client-secret"));
    }

    #[test]
    fn context_server_http_input_preserves_existing_oauth_settings() {
        let text = context_server_http_input(
            Some((
                ContextServerId("figma".into()),
                String::from("https://mcp.figma.com/mcp"),
                HashMap::default(),
                Some(90),
                Some(OAuthClientSettings {
                    client_id: String::from("client-id"),
                    client_secret: Some(String::from("client-secret")),
                }),
            )),
            None,
        )
        .expect("context server input should serialize");

        let (_, _, _, timeout, oauth) = parse_http_input(&text).unwrap();
        assert_eq!(timeout, Some(90));
        let oauth = oauth.expect("oauth should be present");
        assert_eq!(oauth.client_id, "client-id");
        assert_eq!(oauth.client_secret.as_deref(), Some("client-secret"));
    }

    #[test]
    fn context_server_inputs_round_trip_escaped_values_and_timeouts() {
        let local_id = ContextServerId("local\"server".into());
        let local_command = ContextServerCommand {
            path: "path\\with\"quotes".into(),
            args: vec![String::from("--value=\"quoted\"")],
            env: Some(
                [(String::from("KEY"), String::from("a\\b"))]
                    .into_iter()
                    .collect(),
            ),
            timeout: Some(45),
        };
        let local_text =
            context_server_input(Some((local_id.clone(), local_command.clone())), None)
                .expect("local context server input should serialize");
        let (parsed_local_id, parsed_local_command) =
            parse_input(&local_text).expect("local context server input should parse");
        assert_eq!(parsed_local_id, local_id);
        assert_eq!(parsed_local_command, local_command);

        let remote_id = ContextServerId("remote\"server".into());
        let remote_url = String::from("https://example.com/a\\b?value=\"quoted\"");
        let remote_headers = [
            (String::from("Authorization"), String::from("Bearer token")),
            (String::from("X-Test"), String::from("a\\b")),
        ]
        .into_iter()
        .collect::<HashMap<_, _>>();
        let remote_text = context_server_http_input(
            Some((
                remote_id.clone(),
                remote_url.clone(),
                remote_headers.clone(),
                Some(75),
                None,
            )),
            None,
        )
        .expect("remote context server input should serialize");
        let (parsed_remote_id, parsed_remote_url, parsed_headers, timeout, _) =
            parse_http_input(&remote_text).expect("remote context server input should parse");
        assert_eq!(parsed_remote_id, remote_id);
        assert_eq!(parsed_remote_url, remote_url);
        assert_eq!(parsed_headers, remote_headers);
        assert_eq!(timeout, Some(75));
    }
}
