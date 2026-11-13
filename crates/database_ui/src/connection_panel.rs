//! The connection list `Panel`.

use std::sync::Arc;

use database_core::{ConnectionConfig, ConnectionId, ConnectionManager, ConnectionStatus};
use gpui::*;
use i18n::tr;
use settings::{Settings as _, SettingsStore};
use ui::prelude::*;
use ui::{IconButton, Tooltip};
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

use crate::connection_dialog::ConnectionDialog;
use crate::connection_settings::{DatabaseSettings, config_for_file_path, persist_connection};
use crate::{DatabaseState, result_set_view::ResultSetView};

pub struct ConnectionPanel {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    manager: Arc<ConnectionManager>,
    _settings_subscription: Subscription,
    task: Option<Task<()>>,
}

impl ConnectionPanel {
    pub fn new(workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        let settings_subscription = cx.observe_global::<SettingsStore>(|_, cx| {
            DatabaseState::sync_from_settings(cx);
            cx.notify();
        });
        Self {
            focus_handle: cx.focus_handle(),
            workspace,
            manager: DatabaseState::manager(cx),
            _settings_subscription: settings_subscription,
            task: None,
        }
    }

    /// Open a database file chosen through the platform file picker. An existing
    /// saved connection for the same path is reused; a new one is saved before
    /// the browser opens so the panel and settings stay in sync.
    pub fn open_database_file(
        _workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let options = PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        };
        let paths = cx.prompt_for_paths(options);
        cx.spawn_in(window, async move |workspace, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.first() else {
                return;
            };
            let prepared = workspace.update_in(cx, |_, _, cx| {
                let settings = DatabaseSettings::get_global(cx);
                let (config, is_new) = config_for_file_path(&settings.connections, path);
                let completion = is_new.then(|| persist_connection(&config, cx));
                (config, completion)
            });
            let Ok((config, completion)) = prepared else {
                log::error!("failed to resolve database connection for the selected path");
                return;
            };
            if let Some(completion) = completion {
                // Wait for the settings reload so the runtime sync cannot drop
                // the new connection between opening the view and the write.
                match completion.await {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => {
                        log::error!("failed to save database connection: {error:#}");
                        return;
                    }
                    Err(error) => {
                        log::error!("settings update was cancelled: {error}");
                        return;
                    }
                }
            }
            if let Err(error) = workspace.update_in(cx, |workspace, window, cx| {
                DatabaseState::manager(cx).upsert_config(config.clone());
                Self::open_view(workspace, &config, window, cx);
            }) {
                log::error!("failed to open database view: {error:#}");
            }
        })
        .detach();
    }

    pub fn add_connection(
        workspace: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        workspace.toggle_modal(window, cx, |window, cx| {
            ConnectionDialog::new(None, window, cx)
        });
    }

    fn open_view(
        workspace: &mut Workspace,
        config: &ConnectionConfig,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let manager = DatabaseState::manager(cx);
        let credentials = DatabaseState::credentials(cx);
        let config = config.clone();
        let workspace_handle = cx.entity().downgrade();
        let view = cx.new(|cx| {
            ResultSetView::new(manager, credentials, config, workspace_handle, window, cx)
        });
        workspace.add_item_to_active_pane(Box::new(view), None, true, window, cx);
    }

    fn edit_connection(
        &mut self,
        config: ConnectionConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace
            .update(cx, |workspace, cx| {
                workspace.toggle_modal(window, cx, |window, cx| {
                    ConnectionDialog::new(Some(config), window, cx)
                });
            })
            .ok();
    }

    fn connect(&mut self, config: ConnectionConfig, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
        let credentials = DatabaseState::credentials(cx);
        let key = config.credential_key();
        let id = config.id;
        let task = cx.spawn(async move |this, cx| {
            let secret = match credentials.read(&key, cx).await {
                Ok(secret) => secret,
                Err(error) => {
                    log::warn!("failed to read database credentials: {error:#}");
                    None
                }
            };
            if let Err(error) = manager.connect(&id, secret.as_deref()).await {
                log::warn!("database connection failed: {error:#}");
            }
            this.update(cx, |_, cx| cx.notify()).ok();
        });
        self.task = Some(task);
    }

    fn disconnect(&mut self, id: ConnectionId, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
        let task = cx.spawn(async move |this, cx| {
            if let Err(error) = manager.disconnect(&id).await {
                log::warn!("database disconnect failed: {error:#}");
            }
            this.update(cx, |_, cx| cx.notify()).ok();
        });
        self.task = Some(task);
    }

    fn status_text(&self, config: &ConnectionConfig, cx: &App) -> (SharedString, Hsla) {
        if self.manager.capabilities_for(&config.driver_id).is_none() {
            let text = tr(
                cx,
                "database.panel.driver_missing",
                "Driver not available: {}",
            )
            .replacen("{}", &config.driver_id, 1);
            return (text.into(), cx.theme().colors().text_muted);
        }
        match self.manager.status(&config.id) {
            Some(ConnectionStatus::Connected) => (
                tr(cx, "database.status.connected", "Connected").into(),
                cx.theme().status().success,
            ),
            Some(ConnectionStatus::Connecting) => (
                tr(cx, "database.status.connecting", "Connecting…").into(),
                cx.theme().status().info,
            ),
            Some(ConnectionStatus::Querying) => (
                tr(cx, "database.status.querying", "Running query").into(),
                cx.theme().status().info,
            ),
            _ => (
                tr(cx, "database.status.disconnected", "Disconnected").into(),
                cx.theme().colors().text_muted,
            ),
        }
    }

    fn render_row(&self, config: &ConnectionConfig, cx: &mut Context<Self>) -> AnyElement {
        let (status_text, status_color) = self.status_text(config, cx);
        let connected = matches!(
            self.manager.status(&config.id),
            Some(ConnectionStatus::Connected | ConnectionStatus::Querying)
        );
        let is_missing_driver = self.manager.capabilities_for(&config.driver_id).is_none();
        let id = config.id.0.clone();

        let connect_button = if connected {
            IconButton::new(
                ElementId::Name(format!("disconnect-{id}").into()),
                IconName::Stop,
            )
            .tooltip(Tooltip::text(tr(
                cx,
                "database.panel.disconnect",
                "Disconnect",
            )))
            .on_click(cx.listener({
                let query_id = config.id.clone();
                move |this, _, _, cx| this.disconnect(query_id.clone(), cx)
            }))
        } else {
            IconButton::new(
                ElementId::Name(format!("connect-{id}").into()),
                IconName::PlayFilled,
            )
            .tooltip(Tooltip::text(tr(cx, "database.panel.connect", "Connect")))
            .disabled(is_missing_driver)
            .on_click(cx.listener({
                let query_config = config.clone();
                move |this, _, _, cx| this.connect(query_config.clone(), cx)
            }))
        };

        h_flex()
            .id(ElementId::Name(format!("connection-{id}").into()))
            .p_1()
            .gap_1()
            .rounded_sm()
            .hover(|style| style.bg(cx.theme().colors().element_hover))
            .child(
                v_flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(div().truncate().child(config.display_name.clone()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(status_color)
                            .truncate()
                            .child(status_text),
                    ),
            )
            .child(
                IconButton::new(
                    ElementId::Name(format!("open-{id}").into()),
                    IconName::ArrowUpRight,
                )
                .tooltip(Tooltip::text(tr(
                    cx,
                    "database.panel.open",
                    "Open database",
                )))
                .disabled(is_missing_driver)
                .on_click(cx.listener({
                    let workspace = self.workspace.clone();
                    let open_config = config.clone();
                    move |_, _, window, cx| {
                        workspace
                            .update(cx, |workspace, cx| {
                                Self::open_view(workspace, &open_config, window, cx);
                            })
                            .ok();
                    }
                })),
            )
            .child(connect_button)
            .child(
                IconButton::new(
                    ElementId::Name(format!("edit-{id}").into()),
                    IconName::Pencil,
                )
                .tooltip(Tooltip::text(tr(cx, "database.panel.edit", "Edit")))
                .on_click(cx.listener({
                    let edit_config = config.clone();
                    move |this, _, window, cx| this.edit_connection(edit_config.clone(), window, cx)
                })),
            )
            .into_any_element()
    }
}

impl Focusable for ConnectionPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for ConnectionPanel {}

impl Render for ConnectionPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let configs = self.manager.configs();
        let mut column = v_flex().size_full().p_2().gap_1();
        column = column.child(
            h_flex()
                .justify_between()
                .child(div().font_weight(FontWeight::BOLD).child(tr(
                    cx,
                    "database.panel.title",
                    "Databases",
                )))
                .child(
                    IconButton::new("add-database-connection", IconName::Plus)
                        .tooltip(Tooltip::text(tr(
                            cx,
                            "database.panel.add",
                            "Add connection",
                        )))
                        .on_click(cx.listener(|_, _, window, cx| {
                            window.dispatch_action(Box::new(crate::AddDatabaseConnection), cx);
                        })),
                ),
        );
        if configs.is_empty() {
            column = column.child(div().text_color(cx.theme().colors().text_muted).child(tr(
                cx,
                "database.panel.empty",
                "No connections yet. Add one or open a database file.",
            )));
        }
        for config in &configs {
            column = column.child(self.render_row(config, cx));
        }
        column
    }
}

impl Panel for ConnectionPanel {
    fn persistent_name() -> &'static str {
        "DatabasePanel"
    }

    fn panel_key() -> &'static str {
        "DatabasePanel"
    }

    fn position(&self, _window: &Window, _cx: &App) -> DockPosition {
        DockPosition::Left
    }

    fn position_is_valid(&self, _position: DockPosition) -> bool {
        true
    }

    fn set_position(
        &mut self,
        _position: DockPosition,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    fn default_size(&self, _window: &Window, _cx: &App) -> Pixels {
        px(280.)
    }

    fn icon(&self, _window: &Window, _cx: &App) -> Option<ui::IconName> {
        Some(ui::IconName::DatabaseZap)
    }

    fn icon_tooltip(&self, _window: &Window, cx: &App) -> Option<SharedString> {
        Some(tr(cx, "database.panel.title", "Databases").into())
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(crate::ToggleDatabasePanel)
    }

    fn activation_priority(&self) -> u32 {
        10
    }
}
