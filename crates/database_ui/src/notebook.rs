//! SQL notebook: one `Item` with independent cells that share a connection.
//!
//! Each cell owns its own `QuerySession`, so a slow cell blocks neither the
//! editor nor the other cells, and cancelling a cell cancels only that cell's
//! query (through its `QueryCanceller`). Results stay inside the cell that
//! produced them. Execution goes through `QuerySession`; there is no Jupyter
//! kernel involved (Phase 10).

use std::sync::Arc;

use database_core::{
    Capabilities, ConnectionConfig, ConnectionId, ConnectionManager, QueryCanceller, QuerySession,
    ResultSet, ResultSetState, Value,
};
use editor::{Editor, EditorElement, EditorStyle};
use gpui::*;
use i18n::tr;
use project::Project;
use ui::prelude::*;
use ui::{Button, ButtonSize, IconName};
use workspace::Item;
use workspace::Workspace;
use workspace::item::SaveOptions;

use crate::connection_dialog::describe_error;
use crate::credentials::KeychainCredentials;
use crate::{CancelQuery, RunSelection, RunStatement};

/// Rows shown inside a cell. A notebook result is a preview, not the grid: the
/// full result belongs in the browser tab where it can be sorted and exported.
const NOTEBOOK_PREVIEW_ROWS: usize = 100;

/// Width of one preview column. Fixed so rows line up without measuring text.
const NOTEBOOK_COLUMN_WIDTH: f32 = 160.0;

/// A one-line note shown under a cell.
enum CellMessage {
    Info(String),
    Error(String),
}

impl CellMessage {
    fn render(&self, cx: &App) -> AnyElement {
        let (text, color) = match self {
            CellMessage::Info(text) => (text.clone(), cx.theme().colors().text_muted),
            CellMessage::Error(text) => (text.clone(), cx.theme().status().error),
        };
        div()
            .text_sm()
            .text_color(color)
            .child(text)
            .into_any_element()
    }
}

struct NotebookCell {
    id: u64,
    editor: Entity<Editor>,
    /// Created on first run so opening a notebook does not connect.
    session: Option<QuerySession>,
    /// Lets a Cancel button reach the in-flight query while the session itself
    /// lives inside the run task.
    canceller: QueryCanceller,
    result: Option<Arc<ResultSet>>,
    message: Option<CellMessage>,
    running: bool,
    /// Owning the task means dropping the cell cancels its query.
    _task: Option<Task<()>>,
}

impl NotebookCell {
    fn new(id: u64, editor: Entity<Editor>) -> Self {
        Self {
            id,
            editor,
            session: None,
            canceller: QueryCanceller::new(),
            result: None,
            message: None,
            running: false,
            _task: None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RunCellMode {
    All,
    Selection,
}

/// One notebook tab for a database connection.
pub struct DatabaseNotebook {
    focus_handle: FocusHandle,
    manager: Arc<ConnectionManager>,
    credentials: Arc<KeychainCredentials>,
    connection_id: ConnectionId,
    config: ConnectionConfig,
    capabilities: Capabilities,
    cells: Vec<NotebookCell>,
    next_cell_id: u64,
}

impl DatabaseNotebook {
    pub fn new(
        manager: Arc<ConnectionManager>,
        credentials: Arc<KeychainCredentials>,
        config: ConnectionConfig,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let capabilities = manager
            .capabilities_for(&config.driver_id)
            .unwrap_or_else(Capabilities::read_only);
        manager.upsert_config(config.clone());
        let mut notebook = Self {
            focus_handle: cx.focus_handle(),
            manager,
            credentials,
            connection_id: config.id.clone(),
            config,
            capabilities,
            cells: Vec::new(),
            next_cell_id: 1,
        };
        notebook.add_cell(None, window, cx);
        notebook
    }

    /// Open a notebook for `config` in the active pane.
    pub fn deploy(
        workspace: &mut Workspace,
        config: ConnectionConfig,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let manager = crate::DatabaseState::manager(cx);
        let credentials = crate::DatabaseState::credentials(cx);
        let notebook = cx.new(|cx| Self::new(manager, credentials, config, window, cx));
        workspace.add_item_to_active_pane(Box::new(notebook), None, true, window, cx);
    }

    fn create_cell_editor(id: u64, window: &mut Window, cx: &mut Context<Self>) -> Entity<Editor> {
        let notebook = cx.entity().downgrade();
        let editor = cx.new(|cx| {
            let mut editor = Editor::multi_line(window, cx);
            editor.set_placeholder_text(
                &tr(cx, "database.notebook.cell_placeholder", "SELECT …"),
                window,
                cx,
            );
            editor
        });
        editor.update(cx, |editor, _cx| {
            let run_all = notebook.clone();
            editor
                .register_action(move |_: &RunStatement, window, cx| {
                    run_all
                        .update(cx, |notebook, cx| {
                            notebook.run_cell(id, RunCellMode::All, window, cx)
                        })
                        .ok();
                })
                .detach();
            let run_selection = notebook.clone();
            editor
                .register_action(move |_: &RunSelection, window, cx| {
                    run_selection
                        .update(cx, |notebook, cx| {
                            notebook.run_cell(id, RunCellMode::Selection, window, cx)
                        })
                        .ok();
                })
                .detach();
            let cancel = notebook.clone();
            editor
                .register_action(move |_: &CancelQuery, _, cx| {
                    cancel
                        .update(cx, |notebook, cx| notebook.cancel_cell(id, cx))
                        .ok();
                })
                .detach();
        });
        editor
    }

    fn add_cell(&mut self, after: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.next_cell_id;
        self.next_cell_id += 1;
        let editor = Self::create_cell_editor(id, window, cx);
        let index = after
            .map_or(self.cells.len(), |after| after + 1)
            .min(self.cells.len());
        self.cells
            .insert(index, NotebookCell::new(id, editor.clone()));
        editor
            .update(cx, |editor, cx| editor.focus_handle(cx))
            .focus(window, cx);
        cx.notify();
    }

    fn delete_cell(&mut self, index: usize, cx: &mut Context<Self>) {
        // The notebook always has somewhere to type.
        if self.cells.len() <= 1 || index >= self.cells.len() {
            return;
        }
        let cell = self.cells.remove(index);
        cell.canceller.cancel();
        cx.notify();
    }

    fn move_cell(&mut self, index: usize, up: bool, cx: &mut Context<Self>) {
        let Some(target) = moved_index(self.cells.len(), index, up) else {
            return;
        };
        self.cells.swap(index, target);
        cx.notify();
    }

    /// The text a run should execute: the selection when the user asked for
    /// one and there is one, otherwise the whole cell.
    fn cell_sql(&self, index: usize, mode: RunCellMode, cx: &mut Context<Self>) -> String {
        let Some(cell) = self.cells.get(index) else {
            return String::new();
        };
        if mode == RunCellMode::Selection {
            let (selection, _) = crate::selected_sql_text(&cell.editor, cx);
            if !selection.trim().is_empty() {
                return selection;
            }
        }
        cell.editor.read(cx).text(cx)
    }

    fn run_cell(
        &mut self,
        id: u64,
        mode: RunCellMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.cells.iter().position(|cell| cell.id == id) else {
            return;
        };
        if self.cells[index].running {
            return;
        }
        let sql = self.cell_sql(index, mode, cx);
        if sql.trim().is_empty() {
            self.cells[index].message = Some(CellMessage::Info(tr(
                cx,
                "database.notebook.empty_cell",
                "This cell has nothing to run.",
            )));
            cx.notify();
            return;
        }

        let cell = &mut self.cells[index];
        cell.running = true;
        cell.canceller.reset();
        cell.message = Some(CellMessage::Info(tr(
            cx,
            "database.notebook.running",
            "Running…",
        )));
        let session = cell.session.take();
        let canceller = cell.canceller.clone();
        let manager = self.manager.clone();
        let credentials = self.credentials.clone();
        let config = self.config.clone();
        let connection_id = self.connection_id.clone();
        let capabilities = self.capabilities.clone();
        // Keep focus in the cell the user just ran.
        cell.editor
            .update(cx, |editor, cx| editor.focus_handle(cx))
            .focus(window, cx);
        cx.notify();

        let task = cx.spawn(async move |view, cx| {
            let mut session = match session {
                Some(session) => session,
                None => match Self::connect_session(
                    &manager,
                    &credentials,
                    &config,
                    &connection_id,
                    capabilities,
                    cx,
                )
                .await
                {
                    Ok(session) => session,
                    Err(error) => {
                        view.update(cx, |view, cx| {
                            let message = tr(
                                cx,
                                "database.notebook.connect_failed",
                                "Could not connect: {}",
                            )
                            .replacen("{}", &format!("{error:#}"), 1);
                            view.finish_cell(id, None, None, Some(message), cx);
                        })
                        .ok();
                        return;
                    }
                },
            };
            let outcome = session
                .execute_cancellable(&sql, &[], Some(&canceller))
                .await;
            let cancelled = session.is_cancelled();
            let result = session.result_set().cloned().map(Arc::new);
            view.update(cx, |view, cx| {
                let error = outcome.err().map(|error| describe_error(cx, &error));
                view.finish_cell(id, Some(session), result, error, cx);
                if cancelled {
                    view.set_cell_message(
                        id,
                        CellMessage::Info(tr(cx, "database.notebook.cell_cancelled", "Cancelled.")),
                        cx,
                    );
                }
            })
            .ok();
        });
        self.cells[index]._task = Some(task);
    }

    async fn connect_session(
        manager: &Arc<ConnectionManager>,
        credentials: &Arc<KeychainCredentials>,
        config: &ConnectionConfig,
        connection_id: &ConnectionId,
        capabilities: Capabilities,
        cx: &AsyncApp,
    ) -> anyhow::Result<QuerySession> {
        let secret = match credentials.read(&config.credential_key(), cx).await {
            Ok(secret) => secret,
            Err(error) => {
                log::warn!("failed to read database credentials: {error:#}");
                None
            }
        };
        let connection = manager.connect(connection_id, secret.as_deref()).await?;
        Ok(QuerySession::new(
            connection_id.clone(),
            connection,
            manager.clone(),
            capabilities,
        ))
    }

    /// Store the finished run. `error` is already user-facing text; a
    /// cancelled run is not an error and leaves the message to the caller.
    fn finish_cell(
        &mut self,
        id: u64,
        session: Option<QuerySession>,
        result: Option<Arc<ResultSet>>,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(cell) = self.cells.iter_mut().find(|cell| cell.id == id) else {
            return;
        };
        cell.running = false;
        cell.session = session;
        cell.result = result;
        cell.message = error.map_or_else(
            || {
                cell.result
                    .as_ref()
                    .map(|result| CellMessage::Info(result_summary(result, cx)))
            },
            |error| Some(CellMessage::Error(error)),
        );
        cx.notify();
    }

    fn set_cell_message(&mut self, id: u64, message: CellMessage, cx: &mut Context<Self>) {
        if let Some(cell) = self.cells.iter_mut().find(|cell| cell.id == id) {
            cell.message = Some(message);
            cx.notify();
        }
    }

    fn cancel_cell(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(cell) = self.cells.iter_mut().find(|cell| cell.id == id) else {
            return;
        };
        if !cell.running {
            return;
        }
        cell.canceller.cancel();
        cell.message = Some(CellMessage::Info(tr(
            cx,
            "database.notebook.cancelling",
            "Cancelling…",
        )));
        cx.notify();
    }

    fn render_cell(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let cell = &self.cells[index];
        let id = cell.id;
        let running = cell.running;
        let result = cell.result.clone();
        let message = cell.message.as_ref().map(|message| message.render(cx));
        let editor = cell.editor.clone();
        let border = cx.theme().colors().border;
        let muted = cx.theme().colors().text_muted;
        let can_move_up = index > 0;
        let can_move_down = index + 1 < self.cells.len();
        let can_delete = self.cells.len() > 1;

        let mut header = h_flex()
            .w_full()
            .gap_1()
            .items_center()
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("[{}]", index + 1)),
            )
            .child(
                Button::new(
                    ElementId::Name(format!("database-notebook-run-{id}").into()),
                    tr(cx, "database.notebook.run", "Run"),
                )
                .size(ButtonSize::Compact)
                .disabled(running)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.run_cell(id, RunCellMode::All, window, cx);
                })),
            );
        if running {
            header = header.child(
                Button::new(
                    ElementId::Name(format!("database-notebook-cancel-{id}").into()),
                    tr(cx, "database.notebook.cancel", "Cancel"),
                )
                .size(ButtonSize::Compact)
                .on_click(cx.listener(move |this, _, _, cx| this.cancel_cell(id, cx))),
            );
        }
        header = header
            .child(
                Button::new(
                    ElementId::Name(format!("database-notebook-add-{id}").into()),
                    tr(cx, "database.notebook.add_below", "Add Cell Below"),
                )
                .size(ButtonSize::Compact)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.add_cell(Some(index), window, cx);
                })),
            )
            .child(
                Button::new(
                    ElementId::Name(format!("database-notebook-up-{id}").into()),
                    tr(cx, "database.notebook.move_up", "Up"),
                )
                .size(ButtonSize::Compact)
                .disabled(!can_move_up)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.move_cell(index, true, cx);
                })),
            )
            .child(
                Button::new(
                    ElementId::Name(format!("database-notebook-down-{id}").into()),
                    tr(cx, "database.notebook.move_down", "Down"),
                )
                .size(ButtonSize::Compact)
                .disabled(!can_move_down)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.move_cell(index, false, cx);
                })),
            )
            .child(
                Button::new(
                    ElementId::Name(format!("database-notebook-delete-{id}").into()),
                    tr(cx, "database.notebook.delete", "Delete"),
                )
                .size(ButtonSize::Compact)
                .disabled(!can_delete)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.delete_cell(index, cx);
                })),
            );

        let mut body = v_flex().gap_2().p_2().w_full().child(
            div()
                .w_full()
                .min_h(px(72.))
                .border_1()
                .rounded_md()
                .border_color(border)
                .child(EditorElement::new(&editor, EditorStyle::default())),
        );
        if let Some(message) = message {
            body = body.child(message);
        }
        if let Some(result) = result {
            body = body.child(render_cell_result(&result, cx));
        }

        v_flex()
            .id(ElementId::Name(
                format!("database-notebook-cell-{id}").into(),
            ))
            .w_full()
            .gap_1()
            .pb_2()
            .border_b_1()
            .border_color(border)
            .child(header)
            .child(body)
            .into_any_element()
    }

    fn render_cells(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let count = self.cells.len();
        let mut column = v_flex().w_full();
        for index in 0..count {
            column = column.child(self.render_cell(index, cx));
        }
        column
            .child(
                Button::new(
                    "database-notebook-add-cell",
                    tr(cx, "database.notebook.add_cell", "Add Cell"),
                )
                .size(ButtonSize::Compact)
                .on_click(cx.listener(|this, _, window, cx| this.add_cell(None, window, cx))),
            )
            .into_any_element()
    }
}

/// Where a cell moves to, or `None` when it is already at the edge.
fn moved_index(len: usize, index: usize, up: bool) -> Option<usize> {
    if up {
        index.checked_sub(1)
    } else {
        Some(index + 1).filter(|target| *target < len)
    }
}

/// One line describing what a finished run produced.
fn result_summary(result: &ResultSet, cx: &App) -> String {
    if result.columns.is_empty() {
        return match result.total_rows {
            Some(rows) => tr(cx, "database.notebook.rows_affected", "{} rows affected").replacen(
                "{}",
                &rows.to_string(),
                1,
            ),
            None => tr(
                cx,
                "database.notebook.statement_done",
                "Statement executed.",
            ),
        };
    }
    let summary = tr(cx, "database.notebook.result_rows", "{} rows").replacen(
        "{}",
        &result.rows.len().to_string(),
        1,
    );
    match result.state {
        ResultSetState::Cancelled => tr(cx, "database.notebook.cell_cancelled", "Cancelled."),
        ResultSetState::Failed => tr(cx, "database.notebook.cell_failed", "The query failed."),
        ResultSetState::Running | ResultSetState::Ready | ResultSetState::Closed => summary,
    }
}

fn render_cell_result(result: &ResultSet, cx: &App) -> AnyElement {
    if result.columns.is_empty() {
        return v_flex().into_any_element();
    }
    let muted = cx.theme().colors().text_muted;
    let border = cx.theme().colors().border;

    let mut header = h_flex().gap_2();
    for column in &result.columns {
        header = header.child(
            div()
                .w(px(NOTEBOOK_COLUMN_WIDTH))
                .flex_shrink_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .child(column.name.clone()),
        );
    }

    let mut rows = v_flex().gap_0p5();
    let shown = result.rows.len().min(NOTEBOOK_PREVIEW_ROWS);
    for row in result.rows.iter().take(shown) {
        let mut line = h_flex().gap_2();
        for column_index in 0..result.columns.len() {
            let value = row.get(column_index).unwrap_or(&Value::Null);
            let text = if value.is_null() {
                "NULL".to_owned()
            } else {
                value.to_export_text()
            };
            line = line.child(
                div()
                    .w(px(NOTEBOOK_COLUMN_WIDTH))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_xs()
                    .text_color(if value.is_null() {
                        muted
                    } else {
                        cx.theme().colors().text
                    })
                    .child(text),
            );
        }
        rows = rows.child(line);
    }

    let mut body = v_flex()
        .id("database-notebook-result")
        .w_full()
        .gap_1()
        .p_2()
        .border_1()
        .rounded_md()
        .border_color(border)
        .child(header)
        .child(rows);
    if shown < result.rows.len() {
        body = body.child(
            div().text_xs().text_color(muted).child(
                tr(
                    cx,
                    "database.notebook.preview_truncated",
                    "Showing the first {} of {} rows.",
                )
                .replacen("{}", &shown.to_string(), 1)
                .replacen("{}", &result.rows.len().to_string(), 1),
            ),
        );
    }
    body.into_any_element()
}

impl Focusable for DatabaseNotebook {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<()> for DatabaseNotebook {}

impl Item for DatabaseNotebook {
    type Event = ();

    fn tab_icon(&self, _window: &Window, _cx: &App) -> Option<Icon> {
        Some(Icon::new(IconName::DatabaseZap))
    }

    fn tab_content_text(&self, _detail: usize, cx: &App) -> SharedString {
        tr(cx, "database.notebook.tab_title", "SQL Notebook: {}")
            .replacen("{}", &self.config.display_name, 1)
            .into()
    }

    fn is_dirty(&self, _cx: &App) -> bool {
        false
    }

    fn save(
        &mut self,
        _options: SaveOptions,
        _project: Entity<Project>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }

    /// Closing the notebook stops in-flight cells. The canceller is enough
    /// here because the sessions are owned by the run tasks and drop with them;
    /// switching tabs does not call `on_removed`, so those queries keep running.
    fn on_removed(&self, _cx: &mut Context<Self>) {
        for cell in &self.cells {
            cell.canceller.cancel();
        }
    }
}

impl Render for DatabaseNotebook {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().colors().editor_background;
        let muted = cx.theme().colors().text_muted;
        let cells = self.render_cells(cx);

        v_flex()
            .size_full()
            .bg(background)
            .track_focus(&self.focus_handle(cx))
            .key_context("DatabaseNotebook")
            .child(
                h_flex()
                    .p_2()
                    .gap_2()
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .child(self.config.display_name.clone()),
                    )
                    .child(div().text_color(muted).child(tr(
                        cx,
                        "database.notebook.caption",
                        "One connection per notebook",
                    ))),
            )
            .child(
                v_flex()
                    .id("database-notebook-cells")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_2()
                    .child(cells),
            )
    }
}

#[cfg(test)]
mod tests {
    // Import names explicitly: `use super::*` would pull in GPUI's `test`
    // attribute macro through the parent's glob and shadow `#[test]`.
    use super::{NOTEBOOK_PREVIEW_ROWS, moved_index};

    fn swap_at(items: &mut [u64], index: usize, up: bool) -> Option<usize> {
        let target = moved_index(items.len(), index, up)?;
        items.swap(index, target);
        Some(target)
    }

    #[test]
    fn moving_a_cell_up_swaps_it_with_its_predecessor() {
        let mut items = [1, 2, 3];
        assert_eq!(swap_at(&mut items, 2, true), Some(1));
        assert_eq!(items, [1, 3, 2]);
        assert_eq!(swap_at(&mut items, 0, true), None);
    }

    #[test]
    fn moving_a_cell_down_stops_at_the_end() {
        let mut items = [1, 2, 3];
        assert_eq!(swap_at(&mut items, 0, false), Some(1));
        assert_eq!(items, [2, 1, 3]);
        assert_eq!(swap_at(&mut items, 2, false), None);
    }

    #[test]
    fn a_result_preview_is_bounded() {
        // The render path only takes the first N rows; assert the constant is
        // not accidentally unbounded.
        assert_eq!(NOTEBOOK_PREVIEW_ROWS, 100);
    }
}
