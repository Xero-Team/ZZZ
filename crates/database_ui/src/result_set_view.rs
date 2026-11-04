//! The database browser `Item`: schema tree, SQL editor, and result sets.

use std::ops::Range;
use std::sync::Arc;

use database_core::{
    Capabilities, Catalog, ConnectionConfig, ConnectionId, ConnectionManager, ExportFormat,
    QuerySession, ResultSet, TableRef, TableSchema, export_result_set, statement_at,
};
use editor::{Editor, EditorElement, EditorStyle, MultiBufferOffset};
use gpui::*;
use i18n::tr;
use ui::prelude::*;
use ui::{
    Button, ButtonSize, ButtonStyle, ColumnWidthConfig, CopyableTableCell, IconButton, Table,
    TableAccessibility, TableCellTextAlignment, TableSortDirection, Tooltip, table_sort_button,
    with_copy_on_right_click,
};
use workspace::Item;

use crate::connection_dialog::describe_error;
use crate::credentials::KeychainCredentials;
use crate::result_grid::{CellClass, CellPresentation, ResultGridState, SortDirection};
use crate::{CancelQuery, RunSelection, RunStatement};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunMode {
    Statement,
    Selection,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RunError {
    NoSelection,
    EmptySql,
}

/// Resolve the SQL a run action should execute. Pure so the selection and
/// current-statement rules are unit tested without an editor.
pub(crate) fn sql_to_run(
    mode: RunMode,
    text: &str,
    selection: &str,
    cursor_byte: usize,
) -> Result<String, RunError> {
    let selection = selection.trim();
    match mode {
        RunMode::Selection => {
            if selection.is_empty() {
                Err(RunError::NoSelection)
            } else {
                Ok(selection.to_owned())
            }
        }
        RunMode::Statement => {
            if !selection.is_empty() {
                return Ok(selection.to_owned());
            }
            statement_at(text, cursor_byte)
                .map(|statement| statement.text)
                .ok_or(RunError::EmptySql)
        }
    }
}

struct QueryTab {
    sql: String,
    /// Shared so the render path does not clone every row on each frame.
    result: Option<Arc<ResultSet>>,
    message: Option<QueryMessage>,
}

enum QueryMessage {
    Info(String),
    Error(String),
}

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
    session: Option<QuerySession>,
    query_running: bool,
    tabs: Vec<QueryTab>,
    active_tab: usize,
    editor: Entity<Editor>,
    grid: ResultGridState,
    _load_task: Option<Task<()>>,
    _query_task: Option<Task<()>>,
}

impl ResultSetView {
    pub fn new(
        manager: Arc<ConnectionManager>,
        credentials: Arc<KeychainCredentials>,
        config: ConnectionConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let connection_id = config.id.clone();
        let capabilities = manager
            .capabilities_for(&config.driver_id)
            .unwrap_or_else(Capabilities::read_only);
        manager.upsert_config(config.clone());
        let focus_handle = cx.focus_handle();
        let editor = cx.new(|cx| {
            let mut editor = Editor::multi_line(window, cx);
            editor.set_placeholder_text(
                &tr(cx, "database.query.placeholder", "SELECT * FROM …"),
                window,
                cx,
            );
            editor
        });

        let view_handle = cx.entity().downgrade();
        editor.update(cx, |editor, _cx| {
            let run_handle = view_handle.clone();
            editor
                .register_action(move |_: &RunStatement, _, cx| {
                    run_handle
                        .update(cx, |view, cx| view.run(RunMode::Statement, cx))
                        .ok();
                })
                .detach();
            let selection_handle = view_handle.clone();
            editor
                .register_action(move |_: &RunSelection, _, cx| {
                    selection_handle
                        .update(cx, |view, cx| view.run(RunMode::Selection, cx))
                        .ok();
                })
                .detach();
            let cancel_handle = view_handle.clone();
            editor
                .register_action(move |_: &CancelQuery, _, cx| {
                    cancel_handle
                        .update(cx, |view, cx| view.cancel_query(cx))
                        .ok();
                })
                .detach();
        });

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
            session: None,
            query_running: false,
            tabs: vec![QueryTab {
                sql: String::new(),
                result: None,
                message: None,
            }],
            active_tab: 0,
            editor,
            grid: ResultGridState::new(cx),
            _load_task: None,
            _query_task: None,
        };
        view.load_schema(cx);
        view
    }

    /// Query the schema tree over the live connection and keep the connection as
    /// a session so the editor can run statements. The task is stored on the
    /// view so dropping the view cancels it (invariant 9).
    fn load_schema(&mut self, cx: &mut Context<Self>) {
        let manager = self.manager.clone();
        let credentials = self.credentials.clone();
        let connection_id = self.connection_id.clone();
        let credential_key = self.config.credential_key();
        let capabilities = self.capabilities.clone();
        let task = cx.spawn(async move |view, cx| {
            let result: database_core::DriverResult<(
                Vec<Catalog>,
                Arc<dyn database_core::DriverConnection>,
            )> = async {
                let secret = match credentials.read(&credential_key, cx).await {
                    Ok(secret) => secret,
                    Err(error) => {
                        log::warn!("failed to read database credentials: {error:#}");
                        None
                    }
                };
                let connection = manager.connect(&connection_id, secret.as_deref()).await?;
                let catalogs = connection.catalogs().await?;
                Ok((catalogs, connection))
            }
            .await;
            view.update(cx, |view, cx| match result {
                Ok((catalogs, connection)) => {
                    view.catalogs = catalogs;
                    view.session = Some(QuerySession::new(
                        connection_id,
                        connection,
                        manager,
                        capabilities,
                    ));
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

    fn selected_sql(&self, cx: &mut Context<Self>) -> (String, usize) {
        let display_snapshot = self
            .editor
            .update(cx, |editor, cx| editor.display_snapshot(cx));
        let editor = self.editor.read(cx);
        let buffer = editor.buffer().read(cx).read(cx);
        let selections = editor
            .selections
            .all::<MultiBufferOffset>(&display_snapshot);
        let mut parts = Vec::new();
        let mut cursor = 0usize;
        for selection in &selections {
            cursor = selection.head().0;
            if selection.start != selection.end {
                parts.push(
                    buffer
                        .text_for_range(selection.start..selection.end)
                        .collect::<String>(),
                );
            }
        }
        (parts.join("\n"), cursor)
    }

    fn run(&mut self, mode: RunMode, cx: &mut Context<Self>) {
        if self.query_running {
            return;
        }
        let (selection, cursor) = self.selected_sql(cx);
        let text = self.editor.read(cx).text(cx);
        let sql = match sql_to_run(mode, &text, &selection, cursor) {
            Ok(sql) => sql,
            Err(RunError::NoSelection) => {
                self.set_message(
                    QueryMessage::Info(tr(
                        cx,
                        "database.query.select_first",
                        "Select the SQL to run.",
                    )),
                    cx,
                );
                return;
            }
            Err(RunError::EmptySql) => {
                self.set_message(
                    QueryMessage::Info(tr(cx, "database.query.empty", "Nothing to run.")),
                    cx,
                );
                return;
            }
        };
        let Some(mut session) = self.session.take() else {
            self.set_message(
                QueryMessage::Error(tr(
                    cx,
                    "database.query.not_connected",
                    "Not connected to the database.",
                )),
                cx,
            );
            return;
        };

        let tab_index = self.active_tab;
        self.query_running = true;
        if let Some(tab) = self.tabs.get_mut(tab_index) {
            tab.sql = text;
            tab.message = Some(QueryMessage::Info(tr(
                cx,
                "database.query.running",
                "Running…",
            )));
        }
        cx.notify();
        let task = cx.spawn(async move |view, cx| {
            let result = session.execute(&sql, &[]).await;
            let cancelled = session.is_cancelled();
            let result_set = session.result_set().cloned().map(Arc::new);
            view.update(cx, |view, cx| {
                view.session = Some(session);
                view.query_running = false;
                view.grid.sync(result_set.as_deref(), &mut *cx);
                let row_count = result_set.as_ref().map_or(0, |set| set.rows.len());
                let truncation = result_set.as_ref().and_then(|set| set.truncation);
                if let Some(tab) = view.tabs.get_mut(tab_index) {
                    tab.result = result_set;
                    tab.message = Some(match result {
                        Ok(()) if cancelled => QueryMessage::Info(tr(
                            cx,
                            "database.query.cancelled",
                            "Query cancelled.",
                        )),
                        Ok(()) => match truncation {
                            Some(truncation) => QueryMessage::Info(
                                tr(
                                    cx,
                                    "database.query.truncated",
                                    "Showing the first {} rows; the result is larger.",
                                )
                                .replacen(
                                    "{}",
                                    &truncation.row_limit.to_string(),
                                    1,
                                ),
                            ),
                            None => QueryMessage::Info(
                                tr(cx, "database.query.rows", "{} rows").replacen(
                                    "{}",
                                    &row_count.to_string(),
                                    1,
                                ),
                            ),
                        },
                        Err(error) => QueryMessage::Error(describe_error(cx, &error)),
                    });
                }
                cx.notify();
            })
            .ok();
        });
        self._query_task = Some(task);
    }

    fn cancel_query(&mut self, cx: &mut Context<Self>) {
        self.manager.cancel_queries(&self.connection_id);
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.message = Some(QueryMessage::Info(tr(
                cx,
                "database.query.cancelling",
                "Cancelling…",
            )));
        }
        cx.notify();
    }

    fn set_message(&mut self, message: QueryMessage, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.message = Some(message);
        }
        cx.notify();
    }

    fn switch_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index == self.active_tab || index >= self.tabs.len() {
            return;
        }
        let current_text = self.editor.read(cx).text(cx);
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.sql = current_text;
        }
        self.active_tab = index;
        let text = self.tabs[index].sql.clone();
        let result = self.tabs[index].result.clone();
        self.editor.update(cx, |editor, cx| {
            editor.set_text(text, window, cx);
        });
        self.grid.sync(result.as_deref(), &mut *cx);
        cx.notify();
    }

    fn add_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current_text = self.editor.read(cx).text(cx);
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.sql = current_text;
        }
        self.tabs.push(QueryTab {
            sql: String::new(),
            result: None,
            message: None,
        });
        self.active_tab = self.tabs.len() - 1;
        self.editor.update(cx, |editor, cx| {
            editor.set_text(String::new(), window, cx);
        });
        self.grid.sync(None, &mut *cx);
        cx.notify();
    }

    fn render_toolbar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let mut tabs = h_flex().gap_1();
        for index in 0..self.tabs.len() {
            let active = index == self.active_tab;
            let mut button = Button::new(
                ElementId::Name(format!("database-query-tab-{index}").into()),
                tr(cx, "database.query.tab", "Query {}").replacen(
                    "{}",
                    &(index + 1).to_string(),
                    1,
                ),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.switch_tab(index, window, cx);
            }));
            if active {
                button = button.style(ButtonStyle::Filled);
            }
            tabs = tabs.child(button);
        }
        tabs = tabs.child(
            IconButton::new("database-query-new-tab", IconName::Plus)
                .tooltip(Tooltip::text(tr(cx, "database.query.new_tab", "New query")))
                .on_click(cx.listener(|this, _, window, cx| this.add_tab(window, cx))),
        );

        h_flex()
            .w_full()
            .gap_2()
            .justify_between()
            .child(tabs)
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new(
                            "database-run-statement",
                            tr(cx, "database.query.run", "Run"),
                        )
                        .style(ButtonStyle::Filled)
                        .disabled(self.query_running)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.run(RunMode::Statement, cx);
                        })),
                    )
                    .child(
                        Button::new(
                            "database-run-selection",
                            tr(cx, "database.query.run_selection", "Run Selection"),
                        )
                        .disabled(self.query_running)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.run(RunMode::Selection, cx);
                        })),
                    )
                    .child(
                        Button::new(
                            "database-cancel-query",
                            tr(cx, "database.query.cancel", "Cancel Query"),
                        )
                        .disabled(!self.query_running)
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_query(cx))),
                    ),
            )
            .into_any_element()
    }

    fn render_message(&self, cx: &App) -> Option<AnyElement> {
        let tab = self.tabs.get(self.active_tab)?;
        let message = tab.message.as_ref()?;
        let (text, color) = match message {
            QueryMessage::Info(text) => (text.clone(), cx.theme().colors().text_muted),
            QueryMessage::Error(text) => (text.clone(), cx.theme().status().error),
        };
        Some(
            div()
                .text_sm()
                .text_color(color)
                .child(text)
                .into_any_element(),
        )
    }

    fn render_result(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(result) = self
            .tabs
            .get(self.active_tab)
            .and_then(|tab| tab.result.clone())
        else {
            return v_flex().into_any_element();
        };
        if result.columns.is_empty() {
            return v_flex().into_any_element();
        }

        let muted = cx.theme().colors().text_muted;
        let cols = result.columns.len();
        // If the grid has not adopted this result yet, show physical order rather
        // than silently hiding rows.
        let order = if self.grid.order().len() == result.rows.len() {
            Arc::new(self.grid.order().to_vec())
        } else {
            Arc::new((0..result.rows.len()).collect::<Vec<_>>())
        };
        let row_count = order.len();
        let sort = self.grid.sort();

        let mut headers = Vec::with_capacity(cols);
        for (index, column) in result.columns.iter().enumerate() {
            let active = sort
                .filter(|(sorted, _)| *sorted == index)
                .map(|(_, direction)| match direction {
                    SortDirection::Ascending => TableSortDirection::Ascending,
                    SortDirection::Descending => TableSortDirection::Descending,
                });
            let header_text = div()
                .id(ElementId::Name(
                    format!("database-column-header-{index}").into(),
                ))
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(column.name.clone());
            headers.push(
                h_flex()
                    .justify_between()
                    .items_center()
                    .w_full()
                    .font_weight(FontWeight::BOLD)
                    .child(with_copy_on_right_click(
                        header_text,
                        SharedString::from(column.name.clone()),
                        "database.grid.copy_column_hint",
                        "Right click to copy column name",
                    ))
                    .child(table_sort_button(
                        ElementId::Name(format!("database-sort-{index}").into()),
                        active,
                        cx.listener(move |view, _, _, cx| view.toggle_sort(index, cx)),
                        cx,
                    ))
                    .into_any_element(),
            );
        }

        let accessibility_result = result.clone();
        let accessibility_order = order.clone();
        let accessibility = TableAccessibility::new(
            "database-result-table",
            tr(cx, "database.grid.label", "Query result"),
            result
                .columns
                .iter()
                .map(|column| SharedString::from(column.name.clone())),
            move |row, column, _cx| {
                accessibility_order
                    .get(row)
                    .and_then(|index| accessibility_result.rows.get(*index))
                    .and_then(|row| row.get(column))
                    .map_or_else(
                        || SharedString::from("NULL"),
                        |value| SharedString::from(value.display()),
                    )
            },
        );

        let table = Table::new(cols)
            .interactable(&self.grid.interaction)
            .striped()
            .disable_base_style()
            .width_config(ColumnWidthConfig::Resizable(
                self.grid.column_widths.clone(),
            ))
            .header(headers);
        let table = match table.with_accessibility(accessibility) {
            Ok(table) => table,
            Err(error) => {
                // The headers come from the same column list, so a mismatch is a
                // programming error rather than something the user can fix.
                log::error!("database result table accessibility mismatch: {error}");
                return div()
                    .child(tr(
                        cx,
                        "database.grid.unavailable",
                        "The result table could not be displayed.",
                    ))
                    .into_any_element();
            }
        };

        let cells_result = result.clone();
        let cells_order = order.clone();
        let table = table.uniform_list(
            "database-result-rows",
            row_count,
            cx.processor(move |_view, range: Range<usize>, _window, cx| {
                range
                    .filter_map(|display_index| {
                        let row_index = *cells_order.get(display_index)?;
                        let row = cells_result.rows.get(row_index)?;
                        Some(Self::render_result_row(row, display_index, cols, cx))
                    })
                    .collect()
            }),
        );

        let mut summary = h_flex().gap_3().items_center().child(
            div().text_xs().text_color(muted).child(
                tr(cx, "database.query.result_shape", "{} rows × {} columns")
                    .replacen("{}", &row_count.to_string(), 1)
                    .replacen("{}", &cols.to_string(), 1),
            ),
        );
        if let Some(truncation) = result.truncation {
            summary = summary.child(
                div().text_xs().text_color(muted).child(
                    tr(
                        cx,
                        "database.query.truncated",
                        "Showing the first {} rows; the result is larger.",
                    )
                    .replacen("{}", &truncation.row_limit.to_string(), 1),
                ),
            );
        }
        if let (Some((column, _)), Some(_)) = (sort, result.truncation) {
            let column_name = result
                .columns
                .get(column)
                .map_or_else(String::new, |column| column.name.clone());
            summary = summary.child(
                div().text_xs().text_color(muted).child(
                    tr(
                        cx,
                        "database.grid.sorted_note",
                        "Sorted by {} within the {} loaded rows.",
                    )
                    .replacen("{}", &column_name, 1)
                    .replacen("{}", &row_count.to_string(), 1),
                ),
            );
        }

        let mut copy_buttons = h_flex().gap_1();
        for (format, key, fallback) in [
            (ExportFormat::Csv, "database.grid.copy_csv", "Copy CSV"),
            (
                ExportFormat::Markdown,
                "database.grid.copy_markdown",
                "Copy Markdown",
            ),
            (ExportFormat::Html, "database.grid.copy_html", "Copy HTML"),
        ] {
            let export_result = result.clone();
            let export_order = order.clone();
            copy_buttons = copy_buttons.child(
                Button::new(
                    ElementId::Name(format!("database-copy-{key}").into()),
                    tr(cx, key, fallback),
                )
                .size(ButtonSize::Compact)
                .on_click(cx.listener(move |_, _, _, cx| {
                    let text = export_result_set(&export_result, format, export_order.as_slice());
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                })),
            );
        }

        v_flex()
            .id("database-query-result")
            .size_full()
            .gap_1()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(summary)
                    .child(copy_buttons),
            )
            .child(div().flex_1().min_h_0().child(table))
            .into_any_element()
    }

    fn toggle_sort(&mut self, column: usize, cx: &mut Context<Self>) {
        let result = self
            .tabs
            .get(self.active_tab)
            .and_then(|tab| tab.result.clone());
        self.grid.toggle_sort(column, result.as_deref());
        cx.notify();
    }

    fn render_schema_tree(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let hover = cx.theme().colors().element_hover;
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
                            .hover(|style| style.bg(hover))
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

    fn render_result_row(
        row: &database_core::Row,
        display_index: usize,
        columns: usize,
        cx: &App,
    ) -> Vec<AnyElement> {
        (0..columns)
            .map(|column| {
                let presentation = CellPresentation::for_value(row.get(column));
                let (color, alignment) = match presentation.class {
                    CellClass::Number => (Color::Default, TableCellTextAlignment::Right),
                    CellClass::Null | CellClass::Binary => {
                        (Color::Muted, TableCellTextAlignment::Left)
                    }
                    CellClass::Text => (Color::Default, TableCellTextAlignment::Left),
                };
                CopyableTableCell::new(
                    ElementId::Name(format!("database-cell-{display_index}-{column}").into()),
                    SharedString::from(presentation.text),
                )
                .copy_text(SharedString::from(presentation.copy_text))
                .text_alignment(alignment)
                .text_color(color)
                .copy_hint("database.grid.copy_cell_hint", "Right click to copy cell")
                .render(cx)
                .into_any_element()
            })
            .collect()
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
        let toolbar = self.render_toolbar(cx);
        let message = self.render_message(cx);
        let result = self.render_result(cx);

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
                    .child(
                        v_flex()
                            .id("database-schema-tree")
                            .w(px(280.))
                            .h_full()
                            .overflow_y_scroll()
                            .child(schema_tree)
                            .child(table_details),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .h_full()
                            .p_2()
                            .gap_2()
                            .child(toolbar)
                            .child(
                                div()
                                    .key_context("DatabaseEditor")
                                    .h(px(180.))
                                    .w_full()
                                    .border_1()
                                    .rounded_md()
                                    .border_color(cx.theme().colors().border)
                                    .child(EditorElement::new(
                                        &self.editor,
                                        EditorStyle::default(),
                                    )),
                            )
                            .children(message)
                            .child(div().flex_1().child(result)),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{RunError, RunMode, sql_to_run};

    #[test]
    fn run_statement_prefers_the_selection() {
        assert_eq!(
            sql_to_run(RunMode::Statement, "SELECT 1; SELECT 2;", "SELECT 2", 0),
            Ok("SELECT 2".to_owned())
        );
    }

    #[test]
    fn run_statement_falls_back_to_the_statement_under_the_cursor() {
        let sql = "SELECT 1;\nSELECT 2;";
        let cursor = sql.rfind("SELECT 2").expect("second statement");
        assert_eq!(
            sql_to_run(RunMode::Statement, sql, "", cursor + 2),
            Ok("SELECT 2".to_owned())
        );
    }

    #[test]
    fn run_selection_requires_a_selection() {
        assert_eq!(
            sql_to_run(RunMode::Selection, "SELECT 1", "   ", 0),
            Err(RunError::NoSelection)
        );
        assert_eq!(
            sql_to_run(RunMode::Selection, "SELECT 1", "SELECT 1", 0),
            Ok("SELECT 1".to_owned())
        );
    }

    #[test]
    fn empty_text_is_not_runnable() {
        assert_eq!(
            sql_to_run(RunMode::Statement, "  -- nothing\n", "", 0),
            Err(RunError::EmptySql)
        );
    }
}
