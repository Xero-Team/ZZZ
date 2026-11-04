//! The database browser `Item`: schema tree, SQL editor, and result sets.

use std::ops::Range;
use std::sync::Arc;

use database_core::{
    Capabilities, Catalog, ColumnType, ConnectionConfig, ConnectionId, ConnectionManager,
    EditValueError, ExportFormat, QuerySession, ResultSet, StatementKind, TableRef, TableSchema,
    classify, export_result_set, parse_edited_value, statement_at,
};
use editor::{Editor, EditorElement, EditorStyle, MultiBufferOffset};
use gpui::*;
use i18n::tr;
use project::Project;
use ui::prelude::*;
use ui::{
    Button, ButtonSize, ButtonStyle, ColumnWidthConfig, CopyableTableCell, IconButton, Table,
    TableAccessibility, TableCellTextAlignment, TableSortDirection, Tooltip, table_sort_button,
    with_copy_on_right_click,
};
use workspace::Item;
use workspace::item::SaveOptions;

use crate::connection_dialog::describe_error;
use crate::credentials::KeychainCredentials;
use crate::result_grid::{CellClass, CellPresentation, ResultGridState, SortDirection};
use crate::{CancelCellEdit, CancelQuery, RunSelection, RunStatement, StageCellEdit};

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditOperation {
    Apply,
    Commit,
    Rollback,
}

fn edit_value_error_message(cx: &App, error: EditValueError) -> String {
    match error {
        EditValueError::BinaryNotEditable => tr(
            cx,
            "database.edit.binary_read_only",
            "Binary values cannot be edited here.",
        ),
        EditValueError::InvalidInteger => {
            tr(cx, "database.edit.invalid_integer", "Enter a whole number.")
        }
        EditValueError::InvalidReal => tr(cx, "database.edit.invalid_number", "Enter a number."),
        EditValueError::InvalidBool => {
            tr(cx, "database.edit.invalid_boolean", "Enter true or false.")
        }
    }
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
    /// Single-line editor for the selected cell. Kept alive so its focus handle
    /// and text survive re-renders.
    value_editor: Entity<Editor>,
    selected_cell: Option<(usize, usize)>,
    edit_running: bool,
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

        let value_editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text(
                &tr(cx, "database.edit.value_placeholder", "NULL"),
                window,
                cx,
            );
            editor
        });

        let view_handle = cx.entity().downgrade();
        editor.update(cx, |editor, _cx| {
            let run_handle = view_handle.clone();
            editor
                .register_action(move |_: &RunStatement, window, cx| {
                    run_handle
                        .update(cx, |view, cx| view.run(RunMode::Statement, window, cx))
                        .ok();
                })
                .detach();
            let selection_handle = view_handle.clone();
            editor
                .register_action(move |_: &RunSelection, window, cx| {
                    selection_handle
                        .update(cx, |view, cx| view.run(RunMode::Selection, window, cx))
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
        value_editor.update(cx, |editor, _cx| {
            let stage_handle = view_handle.clone();
            editor
                .register_action(move |_: &StageCellEdit, window, cx| {
                    stage_handle
                        .update(cx, |view, cx| view.stage_selected_cell(window, cx))
                        .ok();
                })
                .detach();
            let cancel_handle = view_handle.clone();
            editor
                .register_action(move |_: &CancelCellEdit, window, cx| {
                    cancel_handle
                        .update(cx, |view, cx| view.cancel_cell_edit(window, cx))
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
            value_editor,
            selected_cell: None,
            edit_running: false,
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

    fn run(&mut self, mode: RunMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.query_running || self.edit_running {
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

        // A statement that touches every row or changes the schema is confirmed
        // first (D-05). The run is deferred so the prompt can be cancelled.
        let kind = classify(&sql);
        if kind.requires_confirmation() {
            let message = match kind {
                StatementKind::Ddl => tr(
                    cx,
                    "database.query.confirm_ddl",
                    "This schema change cannot be undone outside a transaction. Run it?",
                ),
                _ => tr(
                    cx,
                    "database.query.confirm_without_where",
                    "This statement has no WHERE clause and affects every row. Run it?",
                ),
            };
            let run_label = tr(cx, "database.query.run", "Run");
            let cancel_label = tr(cx, "database.query.cancel_prompt", "Cancel");
            let answer = window.prompt(
                PromptLevel::Warning,
                &message,
                None,
                &[run_label.as_str(), cancel_label.as_str()],
                cx,
            );
            cx.spawn(async move |view, cx| {
                if matches!(answer.await, Ok(0)) {
                    view.update(cx, |view, cx| view.start_query(sql, cx)).ok();
                }
            })
            .detach();
            return;
        }

        self.start_query(sql, cx);
    }

    fn start_query(&mut self, sql: String, cx: &mut Context<Self>) {
        if self.query_running || self.edit_running {
            return;
        }
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
        // Running a new query replaces the result set that owns the staged and
        // applied edits, so those edits would become invisible. Make the user
        // settle them first rather than silently dropping or orphaning them.
        if session.has_pending_edits() || session.applied_edit_count() > 0 {
            self.session = Some(session);
            self.set_message(
                QueryMessage::Info(tr(
                    cx,
                    "database.edit.settle_before_query",
                    "Commit or roll back the pending edits before running another query.",
                )),
                cx,
            );
            return;
        }

        let tab_index = self.active_tab;
        self.query_running = true;
        self.selected_cell = None;
        if let Some(tab) = self.tabs.get_mut(tab_index) {
            tab.sql = sql.clone();
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
            if result.is_ok() && matches!(classify(&sql), StatementKind::Select) {
                // Editability depends on the real primary key, so ask the driver
                // instead of guessing from the column names.
                if let Err(error) = session.bind_table_after_query(&sql).await {
                    log::warn!("failed to resolve the editable table for a query: {error}");
                }
            }
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

    fn active_result(&self) -> Option<Arc<ResultSet>> {
        self.tabs
            .get(self.active_tab)
            .and_then(|tab| tab.result.clone())
    }

    /// Copy the session's live result set into the active tab. The session is
    /// the owner of edits; the tab only holds the snapshot the grid renders.
    fn refresh_active_result(&mut self, cx: &mut Context<Self>) {
        let result = self
            .session
            .as_ref()
            .and_then(|session| session.result_set().cloned())
            .map(Arc::new);
        self.grid.sync(result.as_deref(), &mut *cx);
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.result = result;
        }
        cx.notify();
    }

    fn select_cell(
        &mut self,
        row_index: usize,
        column_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(result) = self.active_result() else {
            return;
        };
        let Some(column) = result.columns.get(column_index) else {
            return;
        };
        if !result.editable {
            let message = if self.capabilities.row_editing {
                tr(
                    cx,
                    "database.edit.needs_primary_key",
                    "Only a single-table SELECT that returns the primary key can be edited.",
                )
            } else {
                tr(
                    cx,
                    "database.edit.driver_read_only",
                    "This connection does not support row editing.",
                )
            };
            self.set_message(QueryMessage::Info(message), cx);
            return;
        }
        if matches!(column.declared_type, ColumnType::Blob) {
            self.set_message(
                QueryMessage::Info(tr(
                    cx,
                    "database.edit.binary_read_only",
                    "Binary values cannot be edited here.",
                )),
                cx,
            );
            return;
        }

        self.selected_cell = Some((row_index, column_index));
        let text = match result.value_at(row_index, column_index) {
            Some(value) if !value.is_null() => value.to_export_text(),
            _ => String::new(),
        };
        self.value_editor
            .update(cx, |editor, cx| editor.set_text(text, window, cx));
        self.value_editor
            .update(cx, |editor, cx| editor.focus_handle(cx))
            .focus(window, cx);
        cx.notify();
    }

    fn cancel_cell_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_cell = None;
        self.editor
            .update(cx, |editor, cx| editor.focus_handle(cx))
            .focus(window, cx);
        cx.notify();
    }

    fn stage_selected_cell(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((row_index, column_index)) = self.selected_cell else {
            return;
        };
        let Some(result) = self.active_result() else {
            return;
        };
        let Some(column) = result.columns.get(column_index) else {
            return;
        };
        let text = self.value_editor.read(cx).text(cx);
        let value = match parse_edited_value(&column.declared_type, &text) {
            Ok(value) => value,
            Err(error) => {
                self.set_message(QueryMessage::Error(edit_value_error_message(cx, error)), cx);
                return;
            }
        };
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if !session.stage_edit(row_index, column_index, value) {
            self.set_message(
                QueryMessage::Error(tr(
                    cx,
                    "database.edit.not_editable",
                    "This cell cannot be edited.",
                )),
                cx,
            );
            return;
        }
        self.refresh_active_result(cx);
        self.set_message(
            QueryMessage::Info(tr(
                cx,
                "database.edit.staged",
                "Edit staged. Apply it to the transaction when you are ready.",
            )),
            cx,
        );
        self.value_editor
            .update(cx, |editor, cx| editor.focus_handle(cx))
            .focus(window, cx);
    }

    fn discard_pending_edits(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if !session.discard_pending_edits() {
            self.set_message(
                QueryMessage::Info(tr(
                    cx,
                    "database.edit.discard_requires_rollback",
                    "Some edits are already applied. Roll back the transaction to discard them.",
                )),
                cx,
            );
            return;
        }
        self.refresh_active_result(cx);
        self.set_message(
            QueryMessage::Info(tr(cx, "database.edit.discarded", "Staged edits discarded.")),
            cx,
        );
    }

    fn apply_edits(&mut self, cx: &mut Context<Self>) {
        self.run_edit_operation(EditOperation::Apply, cx);
    }

    fn commit_edits(&mut self, cx: &mut Context<Self>) {
        self.run_edit_operation(EditOperation::Commit, cx);
    }

    fn rollback_edits(&mut self, cx: &mut Context<Self>) {
        self.run_edit_operation(EditOperation::Rollback, cx);
    }

    fn run_edit_operation(&mut self, operation: EditOperation, cx: &mut Context<Self>) {
        if self.edit_running || self.query_running {
            return;
        }
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
        self.edit_running = true;
        cx.notify();
        let task = cx.spawn(async move |view, cx| {
            let outcome = match operation {
                EditOperation::Apply => session.apply_pending_edits().await,
                EditOperation::Commit => session.commit_edits().await,
                EditOperation::Rollback => session.rollback_edits().await.map(|()| 0usize),
            };
            view.update(cx, |view, cx| {
                view.session = Some(session);
                view.edit_running = false;
                view.refresh_active_result(cx);
                match outcome {
                    Ok(applied) => {
                        let message = match operation {
                            EditOperation::Apply => tr(
                                cx,
                                "database.edit.applied",
                                "Applied {} edits inside the transaction.",
                            )
                            .replacen("{}", &applied.to_string(), 1),
                            EditOperation::Commit => tr(
                                cx,
                                "database.edit.committed",
                                "Committed {} edits.",
                            )
                            .replacen("{}", &applied.to_string(), 1),
                            EditOperation::Rollback => tr(
                                cx,
                                "database.edit.rolled_back",
                                "Rolled back. The database is unchanged.",
                            ),
                        };
                        view.set_message(QueryMessage::Info(message), cx);
                    }
                    Err(error) => {
                        let message =
                            tr(cx, "database.edit.failed", "The edit operation failed: {}")
                                .replacen("{}", &describe_error(cx, &error), 1);
                        view.set_message(QueryMessage::Error(message), cx);
                    }
                }
            })
            .ok();
        });
        self._query_task = Some(task);
    }

    fn render_edit_bar(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let result = self.active_result()?;
        let session = self.session.as_ref()?;
        let pending = session
            .result_set()
            .map_or(0, |set| set.pending_edit_count());
        let applied = session.applied_edit_count();
        let applied_statements = session.applied_statement_count();
        let affected_rows = session.applied_row_count();
        let open = session.has_open_transaction();
        let editable = result.editable;
        if result.columns.is_empty() {
            // A DML result has no cells to edit; only a transaction bar would
            // make sense, and that is handled below.
            if pending == 0 && applied == 0 && !open {
                return None;
            }
        }

        let muted = cx.theme().colors().text_muted;
        let busy = self.edit_running || self.query_running;
        let mut bar = h_flex()
            .w_full()
            .flex_wrap()
            .gap_2()
            .items_center()
            .px_2()
            .py_1()
            .text_sm()
            .border_b_1()
            .border_color(cx.theme().colors().border_variant);

        if !editable && pending == 0 && applied == 0 && !open {
            // Say why instead of silently offering nothing to click.
            let reason = if self.capabilities.row_editing {
                tr(
                    cx,
                    "database.edit.needs_primary_key",
                    "Only a single-table SELECT that returns the primary key can be edited.",
                )
            } else {
                tr(
                    cx,
                    "database.edit.driver_read_only",
                    "This connection does not support row editing.",
                )
            };
            bar = bar.child(div().text_color(muted).child(reason));
        }

        if editable {
            match self.selected_cell {
                Some((row_index, column_index)) => {
                    let column_name = result
                        .columns
                        .get(column_index)
                        .map_or_else(String::new, |column| column.name.clone());
                    bar = bar
                        .child(
                            div().text_color(muted).child(
                                tr(cx, "database.edit.cell", "Row {}, column {}")
                                    .replacen("{}", &(row_index + 1).to_string(), 1)
                                    .replacen("{}", &column_name, 1),
                            ),
                        )
                        .child(
                            div()
                                .key_context("DatabaseValueEditor")
                                .w(px(280.))
                                .px_2()
                                .py_1()
                                .border_1()
                                .rounded_md()
                                .border_color(cx.theme().colors().border)
                                .child(EditorElement::new(
                                    &self.value_editor,
                                    EditorStyle::default(),
                                )),
                        )
                        .child(
                            Button::new(
                                "database-stage-edit",
                                tr(cx, "database.edit.stage", "Stage"),
                            )
                            .size(ButtonSize::Compact)
                            .style(ButtonStyle::Filled)
                            .disabled(busy)
                            .on_click(cx.listener(
                                |view, _, window, cx| {
                                    view.stage_selected_cell(window, cx);
                                },
                            )),
                        )
                        .child(
                            Button::new(
                                "database-cancel-edit",
                                tr(cx, "database.edit.cancel", "Cancel"),
                            )
                            .size(ButtonSize::Compact)
                            .disabled(busy)
                            .on_click(cx.listener(
                                |view, _, window, cx| {
                                    view.cancel_cell_edit(window, cx);
                                },
                            )),
                        );
                }
                None => {
                    bar = bar.child(div().text_color(muted).child(tr(
                        cx,
                        "database.edit.select_cell",
                        "Select a cell to edit it.",
                    )));
                }
            }
        }

        bar = bar.child(
            div().text_color(muted).child(
                tr(
                    cx,
                    "database.edit.counts",
                    "{} staged · {} statements applied · {} rows affected",
                )
                .replacen("{}", &pending.to_string(), 1)
                .replacen("{}", &applied_statements.to_string(), 1)
                .replacen("{}", &affected_rows.to_string(), 1),
            ),
        );

        if pending > 0 && applied == 0 {
            bar = bar.child(
                Button::new(
                    "database-discard-edits",
                    tr(cx, "database.edit.discard", "Discard Staged"),
                )
                .size(ButtonSize::Compact)
                .disabled(busy)
                .on_click(cx.listener(|view, _, _, cx| view.discard_pending_edits(cx))),
            );
        }
        if pending > 0 && editable {
            bar = bar.child(
                Button::new(
                    "database-apply-edits",
                    tr(cx, "database.edit.apply", "Apply"),
                )
                .size(ButtonSize::Compact)
                .disabled(busy)
                .on_click(cx.listener(|view, _, _, cx| view.apply_edits(cx))),
            );
        }
        if pending > 0 || applied > 0 || open {
            bar = bar
                .child(
                    Button::new(
                        "database-commit-edits",
                        tr(cx, "database.edit.commit", "Commit"),
                    )
                    .size(ButtonSize::Compact)
                    .style(ButtonStyle::Filled)
                    .disabled(busy)
                    .on_click(cx.listener(|view, _, _, cx| view.commit_edits(cx))),
                )
                .child(
                    Button::new(
                        "database-rollback-edits",
                        tr(cx, "database.edit.rollback", "Rollback"),
                    )
                    .size(ButtonSize::Compact)
                    .disabled(busy)
                    .on_click(cx.listener(|view, _, _, cx| view.rollback_edits(cx))),
                );
        }

        Some(bar.into_any_element())
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
        self.selected_cell = None;
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
        self.selected_cell = None;
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
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.run(RunMode::Statement, window, cx);
                        })),
                    )
                    .child(
                        Button::new(
                            "database-run-selection",
                            tr(cx, "database.query.run_selection", "Run Selection"),
                        )
                        .disabled(self.query_running)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.run(RunMode::Selection, window, cx);
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
        let cells_editable = result.editable;
        let table = table.uniform_list(
            "database-result-rows",
            row_count,
            cx.processor(move |view, range: Range<usize>, _window, cx| {
                range
                    .filter_map(|display_index| {
                        let row_index = *cells_order.get(display_index)?;
                        let row = cells_result.rows.get(row_index)?;
                        Some(view.render_result_row(
                            row,
                            row_index,
                            display_index,
                            cols,
                            cells_editable,
                            cx,
                        ))
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
        &self,
        row: &database_core::Row,
        row_index: usize,
        display_index: usize,
        columns: usize,
        editable: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let selected_cell = self.selected_cell;
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
                let cell = CopyableTableCell::new(
                    ElementId::Name(format!("database-cell-{display_index}-{column}").into()),
                    SharedString::from(presentation.text),
                )
                .copy_text(SharedString::from(presentation.copy_text))
                .text_alignment(alignment)
                .text_color(color)
                .copy_hint("database.grid.copy_cell_hint", "Right click to copy cell")
                .render(cx);
                let cell = cell.when(selected_cell == Some((row_index, column)), |cell| {
                    cell.bg(cx.theme().colors().element_selected)
                });
                let cell = if editable {
                    cell.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _, window, cx| {
                            view.select_cell(row_index, column, window, cx);
                        }),
                    )
                } else {
                    cell
                };
                cell.into_any_element()
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

    /// Uncommitted DB work is what "dirty" means here: staged edits, edits
    /// already written to a transaction, or a transaction the user opened.
    fn is_dirty(&self, _cx: &App) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.has_uncommitted_changes())
    }

    fn can_save(&self, cx: &App) -> bool {
        self.is_dirty(cx)
    }

    /// The platform "Save" action for this view commits the open transaction.
    fn save(
        &mut self,
        _options: SaveOptions,
        _project: Entity<Project>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        let Some(mut session) = self.session.take() else {
            return Task::ready(Ok(()));
        };
        self.edit_running = true;
        cx.notify();
        cx.spawn(async move |view, cx| {
            let outcome = session.commit_edits().await;
            view.update(cx, |view, cx| {
                view.session = Some(session);
                view.edit_running = false;
                view.refresh_active_result(cx);
                let message = match &outcome {
                    Ok(applied) => QueryMessage::Info(
                        tr(
                            cx,
                            "database.edit.committed_on_close",
                            "Committed {} edits.",
                        )
                        .replacen("{}", &applied.to_string(), 1),
                    ),
                    Err(error) => QueryMessage::Error(describe_error(cx, error)),
                };
                view.set_message(message, cx);
            })
            .ok();
            outcome
                .map(|_| ())
                .map_err(|error| anyhow::anyhow!(error.to_string()))
        })
    }

    /// Closing without saving must not leave the connection inside a
    /// transaction. The session is rolled back independently of this entity so
    /// the task survives the view being dropped.
    fn deactivated(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut session) = self.session.take() else {
            return;
        };
        // Run on the background executor so dropping the view does not cancel
        // the rollback. A connection must never outlive its transaction owner.
        cx.background_spawn(async move {
            if let Err(error) = session.rollback_edits().await {
                log::warn!("failed to roll back database edits while closing: {error}");
            }
        })
        .detach();
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
        let edit_bar = self.render_edit_bar(cx);
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
                            .children(edit_bar)
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
