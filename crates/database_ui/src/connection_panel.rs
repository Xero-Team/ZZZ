//! The connection list `Panel`.

use std::sync::Arc;

use database_core::{ConnectionConfig, ConnectionId, ConnectionManager, ConnectionStatus};
use fs::Fs;
use gpui::*;
use i18n::tr;
use settings::{DockSide, Settings as _, SettingsStore, update_settings_file};
use ui::prelude::*;
use ui::{ContextMenu, ContextMenuEntry, ListItem, ListItemSpacing, Tab, Tooltip};
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

use crate::connection_dialog::ConnectionDialog;
use crate::connection_settings::{DatabaseSettings, config_for_file_path, persist_connection};
use crate::{DatabaseState, OpenSelectedConnection, result_set_view::ResultSetView};

pub struct ConnectionPanel {
    focus_handle: FocusHandle,
    workspace: WeakEntity<Workspace>,
    manager: Arc<ConnectionManager>,
    selected: Option<ConnectionId>,
    context_menu: Option<(Entity<ContextMenu>, Point<Pixels>, Subscription)>,
    _settings_subscription: Subscription,
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
            selected: None,
            context_menu: None,
            _settings_subscription: settings_subscription,
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

    fn open_connection(
        &mut self,
        config: &ConnectionConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let config = config.clone();
        self.workspace
            .update(cx, |workspace, cx| {
                Self::open_view(workspace, &config, window, cx);
            })
            .ok();
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

    fn select(&mut self, id: ConnectionId, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = Some(id);
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn selected_config(&self) -> Option<ConnectionConfig> {
        let selected = self.selected.as_ref()?;
        self.manager
            .configs()
            .into_iter()
            .find(|config| &config.id == selected)
    }

    fn open_selected(
        &mut self,
        _: &OpenSelectedConnection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(config) = self.selected_config() else {
            return;
        };
        self.open_connection(&config, window, cx);
    }

    /// Connection tasks detach on purpose: a task stored in a single field is
    /// cancelled when the next one replaces it, which would leave a connection
    /// stuck in `Connecting` when the user starts a second one.
    fn connect(&mut self, config: ConnectionConfig, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
        let credentials = DatabaseState::credentials(cx);
        let key = config.credential_key();
        let id = config.id;
        cx.spawn(async move |this, cx| {
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
        })
        .detach();
    }

    fn disconnect(&mut self, id: ConnectionId, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
        cx.spawn(async move |this, cx| {
            if let Err(error) = manager.disconnect(&id).await {
                log::warn!("database disconnect failed: {error:#}");
            }
            this.update(cx, |_, cx| cx.notify()).ok();
        })
        .detach();
    }

    fn delete_connection(&mut self, config: ConnectionConfig, cx: &mut Context<Self>) {
        let credentials = DatabaseState::credentials(cx);
        let manager = self.manager.clone();
        let key = config.credential_key();
        if self.selected.as_ref() == Some(&config.id) {
            self.selected = None;
        }
        let id = config.id;
        cx.spawn(async move |this, cx| {
            if let Err(error) = credentials.delete(&key, cx).await {
                log::warn!("failed to delete database credentials: {error:#}");
            }
            if let Err(error) = manager.disconnect(&id).await {
                log::warn!("failed to disconnect database before removal: {error:#}");
            }
            this.update(cx, |_, cx| {
                let settings_id = id.0.clone();
                let fs = <dyn Fs>::global(cx);
                update_settings_file(fs, cx, move |settings, _cx| {
                    if let Some(database) = settings.database.as_mut() {
                        database
                            .connections
                            .retain(|existing| existing.id != settings_id);
                    }
                });
                manager.remove_config(&id);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn status_text(&self, config: &ConnectionConfig, cx: &App) -> (SharedString, Color) {
        if self.manager.capabilities_for(&config.driver_id).is_none() {
            let text = tr(
                cx,
                "database.panel.driver_missing",
                "Driver not available: {}",
            )
            .replacen("{}", &config.driver_id, 1);
            return (text.into(), Color::Muted);
        }
        match self.manager.status(&config.id) {
            Some(ConnectionStatus::Connected) => (
                tr(cx, "database.status.connected", "Connected").into(),
                Color::Success,
            ),
            Some(ConnectionStatus::Connecting) => (
                tr(cx, "database.status.connecting", "Connecting…").into(),
                Color::Info,
            ),
            Some(ConnectionStatus::Querying) => (
                tr(cx, "database.status.querying", "Running query").into(),
                Color::Info,
            ),
            _ => (
                tr(cx, "database.status.disconnected", "Disconnected").into(),
                Color::Muted,
            ),
        }
    }

    fn deploy_context_menu(
        &mut self,
        position: Point<Pixels>,
        config: ConnectionConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected = Some(config.id.clone());
        let connected = matches!(
            self.manager.status(&config.id),
            Some(ConnectionStatus::Connected | ConnectionStatus::Querying)
        );
        let is_missing_driver = self.manager.capabilities_for(&config.driver_id).is_none();
        let toggle_label = if connected {
            tr(cx, "database.panel.disconnect", "Disconnect")
        } else {
            tr(cx, "database.panel.connect", "Connect")
        };
        let toggle_icon = if connected {
            IconName::Stop
        } else {
            IconName::PlayFilled
        };
        let open_label = tr(cx, "database.panel.open", "Open database");
        let edit_label = tr(cx, "database.panel.edit", "Edit connection");
        let delete_label = tr(cx, "database.panel.delete", "Delete connection");
        let focus_handle = self.focus_handle.clone();
        let panel = cx.weak_entity();

        let context_menu = ContextMenu::build(window, cx, move |menu, _, _| {
            let panel_for_toggle = panel.clone();
            let toggle_config = config.clone();
            let panel_for_open = panel.clone();
            let open_config = config.clone();
            let panel_for_edit = panel.clone();
            let edit_config = config.clone();
            let panel_for_delete = panel.clone();
            let delete_config = config;
            menu.context(focus_handle)
                .item(
                    ContextMenuEntry::new(toggle_label)
                        .icon(toggle_icon)
                        .icon_position(IconPosition::Start)
                        .disabled(is_missing_driver)
                        .handler(move |_window, cx| {
                            panel_for_toggle
                                .update(cx, |panel, cx| {
                                    if connected {
                                        panel.disconnect(toggle_config.id.clone(), cx);
                                    } else {
                                        panel.connect(toggle_config.clone(), cx);
                                    }
                                })
                                .ok();
                        }),
                )
                .item(
                    ContextMenuEntry::new(open_label)
                        .icon(IconName::ArrowUpRight)
                        .icon_position(IconPosition::Start)
                        .disabled(is_missing_driver)
                        .handler(move |window, cx| {
                            panel_for_open
                                .update(cx, |panel, cx| {
                                    panel.open_connection(&open_config, window, cx);
                                })
                                .ok();
                        }),
                )
                .separator()
                .item(
                    ContextMenuEntry::new(edit_label)
                        .icon(IconName::Pencil)
                        .icon_position(IconPosition::Start)
                        .handler(move |window, cx| {
                            panel_for_edit
                                .update(cx, |panel, cx| {
                                    panel.edit_connection(edit_config.clone(), window, cx);
                                })
                                .ok();
                        }),
                )
                .item(
                    ContextMenuEntry::new(delete_label)
                        .icon(IconName::Trash)
                        .icon_position(IconPosition::Start)
                        .handler(move |_window, cx| {
                            panel_for_delete
                                .update(cx, |panel, cx| {
                                    panel.delete_connection(delete_config.clone(), cx);
                                })
                                .ok();
                        }),
                )
        });

        window.focus(&context_menu.focus_handle(cx), cx);
        let subscription = cx.subscribe(&context_menu, |this, _, _: &DismissEvent, cx| {
            this.context_menu.take();
            cx.notify();
        });
        self.context_menu = Some((context_menu, position, subscription));
        cx.notify();
    }

    fn render_header(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .w_full()
            .h(Tab::container_height(cx))
            .px_2()
            .gap_1p5()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().tab_bar_background)
            .child(Icon::new(IconName::DatabaseZap).size(IconSize::XSmall))
            .child(
                Label::new(tr(cx, "database.panel.title", "Databases"))
                    .size(LabelSize::Small)
                    .weight(FontWeight::MEDIUM),
            )
            .child(div().flex_1())
            .child(
                IconButton::new("add-database-connection", IconName::Plus)
                    .icon_size(IconSize::XSmall)
                    .tooltip(Tooltip::text(tr(
                        cx,
                        "database.panel.add",
                        "Add connection",
                    )))
                    .on_click(cx.listener(|_, _, window, cx| {
                        window.dispatch_action(Box::new(crate::AddDatabaseConnection), cx);
                    })),
            )
            .into_any_element()
    }

    fn render_empty_state(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_2()
            .p_4()
            .child(
                Icon::new(IconName::DatabaseZap)
                    .size(IconSize::XLarge)
                    .color(Color::Muted),
            )
            .child(
                Label::new(tr(
                    cx,
                    "database.panel.empty",
                    "No connections yet. Add one or open a database file.",
                ))
                .size(LabelSize::Small)
                .color(Color::Muted),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new(
                            "database-panel-add-connection",
                            tr(cx, "database.panel.add", "Add connection"),
                        )
                        .style(ButtonStyle::Filled)
                        .on_click(cx.listener(|_, _, window, cx| {
                            window.dispatch_action(Box::new(crate::AddDatabaseConnection), cx);
                        })),
                    )
                    .child(
                        Button::new(
                            "database-panel-open-file",
                            tr(cx, "database.panel.open_file", "Open database file"),
                        )
                        .on_click(cx.listener(|_, _, window, cx| {
                            window.dispatch_action(Box::new(crate::OpenDatabase), cx);
                        })),
                    ),
            )
            .into_any_element()
    }

    fn render_row(&self, config: &ConnectionConfig, cx: &mut Context<Self>) -> AnyElement {
        let (status_text, status_color) = self.status_text(config, cx);
        let connected = matches!(
            self.manager.status(&config.id),
            Some(ConnectionStatus::Connected | ConnectionStatus::Querying)
        );
        let is_missing_driver = self.manager.capabilities_for(&config.driver_id).is_none();
        let id = config.id.0.clone();
        let selected = self.selected.as_ref() == Some(&config.id);

        let connect_button = if connected {
            IconButton::new(
                ElementId::Name(format!("disconnect-{id}").into()),
                IconName::Stop,
            )
            .icon_size(IconSize::XSmall)
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
            .icon_size(IconSize::XSmall)
            .tooltip(Tooltip::text(tr(cx, "database.panel.connect", "Connect")))
            .disabled(is_missing_driver)
            .on_click(cx.listener({
                let query_config = config.clone();
                move |this, _, _, cx| this.connect(query_config.clone(), cx)
            }))
        };

        let hover_actions = h_flex()
            .gap_0p5()
            .child(
                IconButton::new(
                    ElementId::Name(format!("open-{id}").into()),
                    IconName::ArrowUpRight,
                )
                .icon_size(IconSize::XSmall)
                .tooltip(Tooltip::text(tr(
                    cx,
                    "database.panel.open",
                    "Open database",
                )))
                .disabled(is_missing_driver)
                .on_click(cx.listener({
                    let open_config = config.clone();
                    move |this, _, window, cx| this.open_connection(&open_config, window, cx)
                })),
            )
            .child(
                IconButton::new(
                    ElementId::Name(format!("edit-{id}").into()),
                    IconName::Pencil,
                )
                .icon_size(IconSize::XSmall)
                .tooltip(Tooltip::text(tr(
                    cx,
                    "database.panel.edit",
                    "Edit connection",
                )))
                .on_click(cx.listener({
                    let edit_config = config.clone();
                    move |this, _, window, cx| this.edit_connection(edit_config.clone(), window, cx)
                })),
            )
            .visible_on_hover("list_item");

        ListItem::new(ElementId::Name(format!("connection-{id}").into()))
            .spacing(ListItemSpacing::Sparse)
            .selectable(true)
            .toggle_state(selected)
            .start_slot(
                Icon::new(IconName::DatabaseZap)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(Label::new(config.display_name.clone()).truncate())
                    .child(
                        Label::new(status_text)
                            .size(LabelSize::Small)
                            .color(status_color)
                            .truncate(),
                    ),
            )
            .end_slot(
                h_flex()
                    .gap_0p5()
                    .child(hover_actions)
                    .child(connect_button),
            )
            .on_click(cx.listener({
                let config = config.clone();
                move |this, event: &ClickEvent, window, cx| {
                    if event.click_count() > 1 {
                        this.open_connection(&config, window, cx);
                    } else {
                        this.select(config.id.clone(), window, cx);
                    }
                }
            }))
            .on_secondary_mouse_down(cx.listener({
                let config = config.clone();
                move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.deploy_context_menu(event.position, config.clone(), window, cx);
                }
            }))
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
        let mut column = v_flex()
            .key_context("DatabasePanel")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::open_selected))
            .size_full()
            .bg(cx.theme().colors().panel_background)
            .child(self.render_header(cx));

        if configs.is_empty() {
            column = column.child(self.render_empty_state(cx));
        } else {
            let mut rows = v_flex()
                .id("database-panel-connections")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .py_1();
            for config in &configs {
                rows = rows.child(self.render_row(config, cx));
            }
            column = column.child(rows);
        }

        column.children(self.context_menu.as_ref().map(|(menu, position, _)| {
            deferred(
                anchored()
                    .position(*position)
                    .anchor(Anchor::TopLeft)
                    .child(menu.clone()),
            )
            .with_priority(1)
        }))
    }
}

impl Panel for ConnectionPanel {
    fn persistent_name() -> &'static str {
        "DatabasePanel"
    }

    fn panel_key() -> &'static str {
        "DatabasePanel"
    }

    fn position(&self, _window: &Window, cx: &App) -> DockPosition {
        match DatabaseSettings::get_global(cx).dock {
            DockSide::Left => DockPosition::Left,
            DockSide::Right => DockPosition::Right,
        }
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right)
    }

    fn set_position(
        &mut self,
        position: DockPosition,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let fs = <dyn Fs>::global(cx);
        update_settings_file(fs, cx, move |settings, _cx| {
            let dock = match position {
                DockPosition::Left | DockPosition::Bottom => DockSide::Left,
                DockPosition::Right => DockSide::Right,
            };
            settings.database.get_or_insert_default().dock = Some(dock);
        });
    }

    fn default_size(&self, _window: &Window, cx: &App) -> Pixels {
        DatabaseSettings::get_global(cx).default_width
    }

    fn min_size(&self, _window: &Window, _cx: &App) -> Option<Pixels> {
        Some(px(220.))
    }

    fn icon(&self, _window: &Window, cx: &App) -> Option<ui::IconName> {
        DatabaseSettings::get_global(cx)
            .button
            .then_some(ui::IconName::DatabaseZap)
    }

    fn icon_tooltip(&self, _window: &Window, cx: &App) -> Option<SharedString> {
        Some(tr(cx, "menu.view.database_panel", "Database Panel").into())
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(crate::ToggleDatabasePanel)
    }

    fn activation_priority(&self) -> u32 {
        5
    }

    fn hide_button_setting(&self, _: &App) -> Option<workspace::HideStatusItem> {
        Some(workspace::HideStatusItem::new(|settings| {
            settings.database.get_or_insert_default().button = Some(false);
        }))
    }
}
