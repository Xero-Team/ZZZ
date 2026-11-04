//! Modal for creating and editing a saved database connection.
//!
//! Only non-sensitive fields are written to settings. When the driver declares
//! `requires_credentials`, the password goes to the platform keychain under the
//! connection's stable id and is never placed in the settings file, a log, or an
//! error message (D-09).

use database_core::{
    Capabilities, ConnectionConfig, DataSource, DatabaseError, DatabaseErrorKind, SQLITE_DRIVER_ID,
};
use editor::{Editor, EditorElement};
use fs::Fs;
use gpui::{
    App, AppContext as _, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable,
    Render, SharedString, Task, Window,
};
use i18n::tr;
use menu;
use settings::{Settings as _, update_settings_file};
use ui::{
    Button, ButtonStyle, Label, LabelSize, Modal, ModalFooter, ModalHeader, Section, prelude::*,
};
use workspace::ModalView;

use crate::DatabaseState;
use crate::connection_settings::{DatabaseSettings, next_connection_id, persist_connection};

#[derive(Clone, Copy, PartialEq, Eq)]
enum MessageKind {
    Info,
    Error,
}

pub struct ConnectionDialog {
    focus_handle: FocusHandle,
    original: Option<ConnectionConfig>,
    driver_id: String,
    capabilities: Capabilities,
    name_editor: Entity<Editor>,
    path_editor: Entity<Editor>,
    host_editor: Entity<Editor>,
    port_editor: Entity<Editor>,
    user_editor: Entity<Editor>,
    database_editor: Entity<Editor>,
    password_editor: Entity<Editor>,
    message: Option<(SharedString, MessageKind)>,
    busy: bool,
    _task: Option<Task<()>>,
}

/// Localized, classified error text. The driver's diagnostic is appended, but
/// secrets never are: they are not part of `DatabaseError`.
pub(crate) fn describe_error(cx: &App, error: &DatabaseError) -> String {
    let key = match error.kind {
        DatabaseErrorKind::Connection => "database.error.connection",
        DatabaseErrorKind::Authentication => "database.error.authentication",
        DatabaseErrorKind::Syntax => "database.error.syntax",
        DatabaseErrorKind::NotFound => "database.error.not_found",
        DatabaseErrorKind::Permission => "database.error.permission",
        DatabaseErrorKind::Cancelled => "database.error.cancelled",
        DatabaseErrorKind::Constraint => "database.error.constraint",
        DatabaseErrorKind::Timeout => "database.error.timeout",
        DatabaseErrorKind::Driver => "database.error.driver",
        DatabaseErrorKind::Other => "database.error.other",
    };
    let fallback = match error.kind {
        DatabaseErrorKind::Connection => "Could not connect to the database.",
        DatabaseErrorKind::Authentication => "Authentication failed.",
        DatabaseErrorKind::Syntax => "Invalid SQL.",
        DatabaseErrorKind::NotFound => "Object not found.",
        DatabaseErrorKind::Permission => "Permission denied.",
        DatabaseErrorKind::Cancelled => "Cancelled.",
        DatabaseErrorKind::Constraint => "Constraint violated.",
        DatabaseErrorKind::Timeout => "Timed out.",
        DatabaseErrorKind::Driver => "The database driver failed.",
        DatabaseErrorKind::Other => "Database error.",
    };
    format!("{} ({})", tr(cx, key, fallback), error.message)
}

impl ConnectionDialog {
    pub fn new(
        original: Option<ConnectionConfig>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let driver_id = original.as_ref().map_or_else(
            || SQLITE_DRIVER_ID.to_owned(),
            |config| config.driver_id.clone(),
        );
        let capabilities = DatabaseState::manager(cx)
            .capabilities_for(&driver_id)
            .unwrap_or_else(Capabilities::read_only);
        let focus_handle = cx.focus_handle();

        let name_editor = single_line_editor(
            original
                .as_ref()
                .map(|config| config.display_name.as_str())
                .unwrap_or_default(),
            false,
            window,
            cx,
        );
        let path_editor = single_line_editor(
            match original.as_ref().map(|config| &config.source) {
                Some(DataSource::File { path }) => path.display().to_string(),
                _ => String::new(),
            }
            .as_str(),
            false,
            window,
            cx,
        );
        let host_editor = single_line_editor(
            original
                .as_ref()
                .and_then(|config| config.host.as_deref())
                .unwrap_or_default(),
            false,
            window,
            cx,
        );
        let port_editor = single_line_editor(
            &original
                .as_ref()
                .and_then(|config| config.port)
                .map(|port| port.to_string())
                .unwrap_or_default(),
            false,
            window,
            cx,
        );
        let user_editor = single_line_editor(
            original
                .as_ref()
                .and_then(|config| config.user.as_deref())
                .unwrap_or_default(),
            false,
            window,
            cx,
        );
        let database_editor = single_line_editor(
            original
                .as_ref()
                .and_then(|config| config.database.as_deref())
                .unwrap_or_default(),
            false,
            window,
            cx,
        );
        let password_editor = single_line_editor("", true, window, cx);

        Self {
            focus_handle,
            original,
            driver_id,
            capabilities,
            name_editor,
            path_editor,
            host_editor,
            port_editor,
            user_editor,
            database_editor,
            password_editor,
            message: None,
            busy: false,
            _task: None,
        }
    }

    fn is_new(&self) -> bool {
        self.original.is_none()
    }

    fn is_server(&self) -> bool {
        matches!(
            self.original.as_ref().map(|config| &config.source),
            Some(DataSource::Server { .. })
        )
    }

    fn build_config(&self, cx: &App) -> Result<ConnectionConfig, SharedString> {
        let name = self.name_editor.read(cx).text(cx);
        let name = name.trim();
        if name.is_empty() {
            return Err(tr(cx, "database.dialog.name_required", "Enter a name.").into());
        }
        let source = if self.is_server() {
            let host = self.host_editor.read(cx).text(cx);
            let host = host.trim();
            if host.is_empty() {
                return Err(tr(cx, "database.dialog.host_required", "Enter a host.").into());
            }
            let port_text = self.port_editor.read(cx).text(cx);
            let port = port_text.trim().parse::<u16>().map_err(|_| {
                tr(
                    cx,
                    "database.dialog.port_invalid",
                    "Port must be a number between 1 and 65535.",
                )
            })?;
            DataSource::Server {
                host: host.to_owned(),
                port,
            }
        } else {
            let path = self.path_editor.read(cx).text(cx);
            let path = path.trim();
            if path.is_empty() {
                return Err(tr(
                    cx,
                    "database.dialog.path_required",
                    "Choose a database file.",
                )
                .into());
            }
            DataSource::File { path: path.into() }
        };
        let id = self.original.as_ref().map_or_else(
            || next_connection_id(&DatabaseSettings::get_global(cx).connections),
            |config| config.id.clone(),
        );
        let (host, port) = match &source {
            DataSource::Server { host, port } => (Some(host.clone()), Some(*port)),
            _ => (None, None),
        };
        Ok(ConnectionConfig {
            id,
            display_name: name.to_owned(),
            driver_id: self.driver_id.clone(),
            source,
            host,
            port,
            user: Some(self.user_editor.read(cx).text(cx))
                .map(|user| user.trim().to_owned())
                .filter(|user| !user.is_empty()),
            database: Some(self.database_editor.read(cx).text(cx))
                .map(|database| database.trim().to_owned())
                .filter(|database| !database.is_empty()),
            options: Default::default(),
        })
    }

    fn cancel(&mut self, _: &menu::Cancel, _window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
    }

    fn confirm(&mut self, _: &menu::Confirm, _window: &mut Window, cx: &mut Context<Self>) {
        self.save(cx);
    }

    fn test(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let config = match self.build_config(cx) {
            Ok(config) => config,
            Err(message) => {
                self.message = Some((message, MessageKind::Error));
                cx.notify();
                return;
            }
        };
        let typed_password = self.password_editor.read(cx).text(cx);
        let credentials = DatabaseState::credentials(cx);
        let manager = DatabaseState::manager(cx);
        let key = config.credential_key();
        self.busy = true;
        self.message = None;
        cx.notify();
        let task = cx.spawn(async move |this, cx| {
            let secret = if typed_password.is_empty() {
                match credentials.read(&key, cx).await {
                    Ok(secret) => secret,
                    Err(error) => {
                        log::warn!("failed to read database credentials: {error:#}");
                        None
                    }
                }
            } else {
                Some(typed_password.into_bytes())
            };
            let result = manager.test_config(&config, secret.as_deref()).await;
            this.update(cx, |this, cx| {
                this.busy = false;
                this.message = Some(match result {
                    Ok(()) => (
                        tr(
                            cx,
                            "database.dialog.test_succeeded",
                            "Connection succeeded.",
                        )
                        .into(),
                        MessageKind::Info,
                    ),
                    Err(error) => (describe_error(cx, &error).into(), MessageKind::Error),
                });
                cx.notify();
            })
            .ok();
        });
        self._task = Some(task);
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let config = match self.build_config(cx) {
            Ok(config) => config,
            Err(message) => {
                self.message = Some((message, MessageKind::Error));
                cx.notify();
                return;
            }
        };
        let typed_password = self.password_editor.read(cx).text(cx);
        let credentials = DatabaseState::credentials(cx);
        let key = config.credential_key();
        self.busy = true;
        self.message = None;
        cx.notify();
        let task = cx.spawn(async move |this, cx| {
            if !typed_password.is_empty() {
                if let Err(error) = credentials.write(&key, typed_password.as_bytes(), cx).await {
                    this.update(cx, |this, cx| {
                        this.busy = false;
                        this.message = Some((error.to_string().into(), MessageKind::Error));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            }
            let completion = cx.update(|cx| persist_connection(&config, cx));
            let saved = match completion.await {
                Ok(result) => result,
                Err(error) => {
                    this.update(cx, |this, cx| {
                        this.busy = false;
                        this.message = Some((
                            format!("settings update was cancelled: {error}").into(),
                            MessageKind::Error,
                        ));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            if let Err(error) = saved {
                this.update(cx, |this, cx| {
                    this.busy = false;
                    this.message = Some((format!("{error:#}").into(), MessageKind::Error));
                    cx.notify();
                })
                .ok();
                return;
            }
            this.update(cx, |this, cx| {
                this.busy = false;
                cx.emit(DismissEvent);
            })
            .ok();
        });
        self._task = Some(task);
    }

    fn delete(&mut self, cx: &mut Context<Self>) {
        let Some(config) = self.original.clone() else {
            return;
        };
        if self.busy {
            return;
        }
        let credentials = DatabaseState::credentials(cx);
        let manager = DatabaseState::manager(cx);
        let key = config.credential_key();
        self.busy = true;
        cx.notify();
        let task = cx.spawn(async move |this, cx| {
            if let Err(error) = credentials.delete(&key, cx).await {
                log::warn!("failed to delete database credentials: {error:#}");
            }
            if let Err(error) = manager.disconnect(&config.id).await {
                log::warn!("failed to disconnect database before removal: {error:#}");
            }
            this.update(cx, |this, cx| {
                let id = config.id.clone();
                let fs = <dyn Fs>::global(cx);
                update_settings_file(fs, cx, move |settings, _cx| {
                    if let Some(database) = settings.database.as_mut() {
                        database.connections.retain(|existing| existing.id != id.0);
                    }
                });
                manager.remove_config(&config.id);
                this.busy = false;
                cx.emit(DismissEvent);
            })
            .ok();
        });
        self._task = Some(task);
    }

    fn pick_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let options = gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        };
        let paths = cx.prompt_for_paths(options);
        let task = cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.first() else {
                return;
            };
            let text = path.display().to_string();
            this.update_in(cx, |this, window, cx| {
                this.path_editor.update(cx, |editor, cx| {
                    editor.set_text(text.clone(), window, cx);
                });
                if this.name_editor.read(cx).text(cx).trim().is_empty() {
                    let name = path
                        .file_name()
                        .map_or_else(|| text.clone(), |name| name.to_string_lossy().into_owned());
                    this.name_editor.update(cx, |editor, cx| {
                        editor.set_text(name, window, cx);
                    });
                }
                cx.notify();
            })
            .ok();
        });
        self._task = Some(task);
    }
}

fn single_line_editor(
    text: &str,
    masked: bool,
    window: &mut Window,
    cx: &mut Context<ConnectionDialog>,
) -> Entity<Editor> {
    let text = text.to_owned();
    cx.new(|cx| {
        let mut editor = Editor::single_line(window, cx);
        editor.set_text(text, window, cx);
        if masked {
            editor.set_masked(true, cx);
        }
        editor
    })
}

fn field(label: SharedString, editor: Entity<Editor>, cx: &App) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(Label::new(label).size(LabelSize::Small))
        .child(
            div()
                .px_2()
                .py_1()
                .border_1()
                .rounded_md()
                .border_color(cx.theme().colors().border)
                .child(EditorElement::new(&editor, editor::EditorStyle::default())),
        )
}

impl Focusable for ConnectionDialog {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<DismissEvent> for ConnectionDialog {}

impl ModalView for ConnectionDialog {}

impl Render for ConnectionDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = if self.is_new() {
            tr(cx, "database.dialog.new_title", "New Database Connection")
        } else {
            tr(cx, "database.dialog.edit_title", "Edit Database Connection")
        };

        let mut fields = v_flex().gap_3().child(field(
            tr(cx, "database.dialog.name", "Name").into(),
            self.name_editor.clone(),
            cx,
        ));
        if self.is_server() {
            fields = fields
                .child(field(
                    tr(cx, "database.dialog.host", "Host").into(),
                    self.host_editor.clone(),
                    cx,
                ))
                .child(field(
                    tr(cx, "database.dialog.port", "Port").into(),
                    self.port_editor.clone(),
                    cx,
                ))
                .child(field(
                    tr(cx, "database.dialog.user", "User").into(),
                    self.user_editor.clone(),
                    cx,
                ))
                .child(field(
                    tr(cx, "database.dialog.database", "Database").into(),
                    self.database_editor.clone(),
                    cx,
                ));
        } else {
            let path_field = field(
                tr(cx, "database.dialog.path", "Database file").into(),
                self.path_editor.clone(),
                cx,
            );
            fields = fields.child(
                v_flex().gap_1().child(path_field).child(
                    Button::new(
                        "browse-database-file",
                        tr(cx, "database.dialog.browse", "Browse…"),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.pick_file(window, cx);
                    })),
                ),
            );
        }
        if self.capabilities.requires_credentials {
            fields = fields
                .child(field(
                    tr(cx, "database.dialog.password", "Password").into(),
                    self.password_editor.clone(),
                    cx,
                ))
                .child(
                    Label::new(tr(
                        cx,
                        "database.dialog.password_hint",
                        "Stored in the system keychain, never in settings.json.",
                    ))
                    .size(LabelSize::Small),
                );
        }
        if let Some((message, kind)) = &self.message {
            let color = match kind {
                MessageKind::Info => Color::Success,
                MessageKind::Error => Color::Error,
            };
            fields = fields.child(
                Label::new(message.clone())
                    .color(color)
                    .size(LabelSize::Small),
            );
        }

        let footer = ModalFooter::new()
            .when(!self.is_new(), |this| {
                this.start_slot(
                    Button::new(
                        "delete-database-connection",
                        tr(cx, "database.dialog.delete", "Delete Connection"),
                    )
                    .style(ButtonStyle::Tinted(ui::TintColor::Error))
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.delete(cx))),
                )
            })
            .end_slot(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new(
                            "cancel-database-connection",
                            tr(cx, "database.dialog.cancel", "Cancel"),
                        )
                        .disabled(self.busy)
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent))),
                    )
                    .child(
                        Button::new(
                            "test-database-connection",
                            tr(cx, "database.dialog.test", "Test Connection"),
                        )
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.test(cx))),
                    )
                    .child(
                        Button::new(
                            "save-database-connection",
                            tr(cx, "database.dialog.save", "Save"),
                        )
                        .style(ButtonStyle::Filled)
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    ),
            );

        v_flex()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::confirm))
            .child(
                Modal::new("database-connection-dialog", None)
                    .header(
                        ModalHeader::new()
                            .headline(title)
                            .description(format!(
                                "{} {}",
                                tr(cx, "database.dialog.driver", "Driver:"),
                                self.driver_id
                            ))
                            .show_dismiss_button(true),
                    )
                    .section(Section::new().child(fields))
                    .footer(footer),
            )
    }
}
