//! The database browser `Item`.
//!
//! Phase 1 renders the real schema tree (catalogs, tables, columns, keys,
//! indexes, foreign keys) queried from the driver through `ConnectionManager`.
//! Query execution and the virtualized result grid are added in later phases.

use std::sync::Arc;

use database_core::{
    Capabilities, Catalog, ConnectionConfig, ConnectionId, ConnectionManager, TableRef, TableSchema,
};
use gpui::*;
use i18n::tr;
use ui::prelude::*;
use workspace::Item;

use crate::credentials::KeychainCredentials;

/// A browser tab for one database connection.
pub struct ResultSetView {
    focus_handle: FocusHandle,
    manager: Arc<ConnectionManager>,
    credentials: Arc<KeychainCredentials>,
    connection_id: ConnectionId,
    config: ConnectionConfig,
    capabilities: Capabilities,
    catalogs: Vec<Catalog>,
    expanded_table: Option<TableRef>,
    table_schema: Option<TableSchema>,
    status_text: SharedString,
    _load_task: Option<Task<()>>,
}

impl ResultSetView {
    pub fn new(
        manager: Arc<ConnectionManager>,
        credentials: Arc<KeychainCredentials>,
        config: ConnectionConfig,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let connection_id = config.id.clone();
        let capabilities = manager
            .capabilities_for(&config.driver_id)
            .unwrap_or_else(Capabilities::read_only);
        manager.upsert_config(config.clone());
        let focus_handle = cx.focus_handle();
        let mut view = Self {
            focus_handle,
            manager,
            credentials,
            connection_id,
            config,
            capabilities,
            catalogs: Vec::new(),
            expanded_table: None,
            table_schema: None,
            status_text: SharedString::from(tr(cx, "database.status.connecting", "Connecting…")),
            _load_task: None,
        };
        view.load_schema(cx);
        view
    }

    /// Query the schema tree over the live connection. The task is stored on the
    /// view so dropping the view cancels it (invariant 9).
    fn load_schema(&mut self, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
        let credentials = self.credentials.clone();
        let connection_id = self.connection_id.clone();
        let credential_key = self.config.credential_key();
        let task = cx.spawn(async move |view, cx| {
            let result = async {
                let secret = match credentials.read(&credential_key, cx).await {
                    Ok(secret) => secret,
                    Err(error) => {
                        log::warn!("failed to read database credentials: {error:#}");
                        None
                    }
                };
                let connection = manager.connect(&connection_id, secret.as_deref()).await?;
                connection.catalogs().await
            }
            .await;
            view.update(cx, |view, cx| match result {
                Ok(catalogs) => {
                    view.catalogs = catalogs;
                    view.status_text =
                        SharedString::from(tr(cx, "database.status.connected", "Connected"));
                    cx.notify();
                }
                Err(error) => {
                    view.status_text = SharedString::from(error.to_string());
                    cx.notify();
                }
            })
            .unwrap_or_else(|error| {
                log::error!("failed to update database schema view: {error:#}");
            });
        });
        self._load_task = Some(task);
    }

    fn toggle_table(&mut self, table: TableRef, cx: &mut Context<Self>) {
        if self.expanded_table.as_ref() == Some(&table) {
            self.expanded_table = None;
            self.table_schema = None;
            cx.notify();
            return;
        }
        self.expanded_table = Some(table.clone());
        let manager = self.manager.clone();
        let credentials = self.credentials.clone();
        let connection_id = self.connection_id.clone();
        let credential_key = self.config.credential_key();
        let task = cx.spawn(async move |view, cx| {
            let result = async {
                let secret = match credentials.read(&credential_key, cx).await {
                    Ok(secret) => secret,
                    Err(error) => {
                        log::warn!("failed to read database credentials: {error:#}");
                        None
                    }
                };
                let connection = manager.connect(&connection_id, secret.as_deref()).await?;
                connection.describe_table(&table).await
            }
            .await;
            view.update(cx, |view, cx| {
                view.table_schema = result.ok();
                cx.notify();
            })
            .unwrap_or_else(|error| {
                log::error!("failed to update database table details: {error:#}");
            });
        });
        self._load_task = Some(task);
    }

    fn render_schema_tree(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let mut column = v_flex().gap_1().p_2();
        for catalog in &self.catalogs {
            let catalog_label =
                tr(cx, "database.tree.catalog", "Catalog: {}").replacen("{}", &catalog.name, 1);
            column = column.child(div().font_weight(FontWeight::BOLD).child(catalog_label));
            for schema in &catalog.schemas {
                column = column.child(div().pl_2().child(schema.name.clone()));
                for table in &schema.tables {
                    let table_ref = table.reference.clone();
                    let marker = match table.kind {
                        database_core::ObjectKind::Table => "▸",
                        database_core::ObjectKind::View => "◇",
                    };
                    let label = format!("{marker} {}", table.reference.name);
                    column = column.child(
                        div()
                            .id(ElementId::Name(
                                format!("db-table-{}", table.reference.name).into(),
                            ))
                            .pl_4()
                            .hover(|style| style.bg(theme.colors().element_hover))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.toggle_table(table_ref.clone(), cx);
                            }))
                            .child(label),
                    );
                }
            }
        }
        column.into_any_element()
    }

    fn render_table_details(&self, cx: &App) -> AnyElement {
        let Some(schema) = &self.table_schema else {
            return v_flex().into_any_element();
        };
        let mut column = v_flex().gap_1().p_2();
        column = column.child(div().font_weight(FontWeight::BOLD).child(tr(
            cx,
            "database.tree.columns",
            "Columns",
        )));
        for column_meta in &schema.columns {
            let suffix = if column_meta.primary_key { " (PK)" } else { "" };
            column = column.child(div().pl_2().child(format!(
                "{} : {:?}{suffix}",
                column_meta.name, column_meta.declared_type
            )));
        }
        if let Some(primary_key) = &schema.primary_key {
            column = column.child(
                div()
                    .pl_2()
                    .child(format!("PK: {}", primary_key.columns.join(", "))),
            );
        }
        for index in &schema.indexes {
            let unique = if index.unique { " UNIQUE" } else { "" };
            column = column.child(div().pl_2().child(format!(
                "INDEX {} ({}){unique}",
                index.name,
                index.columns.join(", ")
            )));
        }
        for foreign_key in &schema.foreign_keys {
            column = column.child(div().pl_2().child(format!(
                "FK ({}) → {}({})",
                foreign_key.columns.join(", "),
                foreign_key.referenced_table,
                foreign_key.referenced_columns.join(", ")
            )));
        }
        column.into_any_element()
    }
}

impl Focusable for ResultSetView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<()> for ResultSetView {}

impl Item for ResultSetView {
    type Event = ();

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::DatabaseZap))
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        tr(cx, "database.tab.title", "Database: {}")
            .replacen("{}", &self.config.display_name, 1)
            .into()
    }
}

impl Render for ResultSetView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().colors().editor_background;
        let muted = cx.theme().colors().text_muted;
        let schema_tree = self.render_schema_tree(cx);
        let table_details = self.render_table_details(cx);
        v_flex()
            .size_full()
            .bg(background)
            .track_focus(&self.focus_handle(cx))
            .child(
                h_flex()
                    .p_2()
                    .gap_2()
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .child(self.config.display_name.clone()),
                    )
                    .child(div().text_color(muted).child(self.status_text.clone()))
                    .child(
                        div().text_color(muted).child(
                            tr(cx, "database.status.capabilities", "Editing: {}").replacen(
                                "{}",
                                if self.capabilities.row_editing {
                                    tr(cx, "database.capability.yes", "yes")
                                } else {
                                    tr(cx, "database.capability.no", "no")
                                }
                                .as_str(),
                                1,
                            ),
                        ),
                    ),
            )
            .child(
                h_flex()
                    .size_full()
                    .child(v_flex().w(px(320.)).child(schema_tree))
                    .child(v_flex().flex_1().child(table_details)),
            )
    }
}
