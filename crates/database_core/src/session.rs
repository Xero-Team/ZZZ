//! Query execution, cancellation, transactions, and staged edits.
//!
//! `QuerySession` is the owner of an in-flight query, an open transaction, and
//! the staged edits for one result set (invariant 6). It converts the driver's
//! bounded `RowStream` into a windowed `ResultSet` and generates parameterized
//! DML for edits so SQL injection is impossible (D-05).

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::connection::ConnectionManager;
use crate::driver::{DriverConnection, DriverResult, ExecutedQuery, QueryHandle};
use crate::error::DatabaseError;
use crate::model::{
    Capabilities, Column, ConnectionId, ResultSet, ResultSetState, TableRef, Value,
};
use crate::sql::editable_target;

/// The maximum number of rows materialized in a result window. Beyond this the
/// stream is cancelled and a truncation diagnostic is attached (Phase 4 / D-06).
pub const DEFAULT_WINDOW_LIMIT: usize = 10_000;

/// One query session. There is one session per result-set view (or per
/// notebook cell); a cell cancelling its query does not affect other cells.
pub struct QuerySession {
    connection_id: ConnectionId,
    connection: Arc<dyn DriverConnection>,
    manager: Arc<ConnectionManager>,
    capabilities: Capabilities,
    current_query: Option<QueryHandle>,
    transaction: Option<crate::driver::TransactionId>,
    result_set: Option<ResultSet>,
    table: Option<TableRef>,
    /// Values already written to the open transaction by `apply_pending_edits`.
    /// Kept separate from `pending_edits` so a second Apply only sends the delta,
    /// and so Rollback can tell which cached cells to distrust.
    applied_edits: BTreeMap<(usize, usize), Value>,
    /// Number of UPDATE statements written to the open transaction so far.
    applied_statements: usize,
    /// Rows the driver reported as changed by those statements.
    applied_rows: usize,
}

impl QuerySession {
    pub fn new(
        connection_id: ConnectionId,
        connection: Arc<dyn DriverConnection>,
        manager: Arc<ConnectionManager>,
        capabilities: Capabilities,
    ) -> Self {
        Self {
            connection_id,
            connection,
            manager,
            capabilities,
            current_query: None,
            transaction: None,
            result_set: None,
            table: None,
            applied_edits: BTreeMap::new(),
            applied_statements: 0,
            applied_rows: 0,
        }
    }

    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    pub fn result_set(&self) -> Option<&ResultSet> {
        self.result_set.as_ref()
    }

    pub fn result_set_mut(&mut self) -> Option<&mut ResultSet> {
        self.result_set.as_mut()
    }

    /// Execute a statement and fill a bounded window of rows. The query handle
    /// is tracked by the manager so disconnect can cancel it, and is finished
    /// here once the stream closes.
    pub async fn execute(&mut self, sql: &str, params: &[Value]) -> DriverResult<()> {
        self.cancel_current();
        // A result set owns its staged edits. A new query replaces the rows, so
        // the old edit bookkeeping cannot describe the new result.
        self.table = None;
        self.applied_edits.clear();
        self.applied_statements = 0;
        self.applied_rows = 0;
        let executed: ExecutedQuery = match self.connection.execute(sql, params).await {
            Ok(executed) => executed,
            Err(error) if error.is_cancelled() => {
                // A driver that could not return a handle yet still reports a
                // user cancellation as a successful terminal state (D-08).
                let mut result_set = ResultSet::new(Vec::new(), sql.to_owned());
                result_set.mark_cancelled();
                self.result_set = Some(result_set);
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        let query_id = executed.handle.id();
        self.manager
            .track_query(&self.connection_id, executed.handle.clone());
        self.current_query = Some(executed.handle.clone());

        let mut result_set = crate::driver::result_set_for(&executed.metadata, sql);
        result_set.total_rows = executed.metadata.affected_rows;
        let mut rows = 0usize;
        let mut budget_reached = false;
        while let Some(item) = executed.rows.next().await {
            let row = match item {
                Ok(row) => row,
                Err(error) if error.is_cancelled() => {
                    result_set.mark_cancelled();
                    result_set.total_rows = Some(rows);
                    self.manager.finish_query(&self.connection_id, query_id);
                    self.current_query = None;
                    self.result_set = Some(result_set);
                    return Ok(());
                }
                Err(error) => {
                    // A step error is terminal. Keep the rows already read so the
                    // view can show them, but report the failure to the caller.
                    result_set.mark_failed();
                    result_set.total_rows = Some(rows);
                    self.manager.finish_query(&self.connection_id, query_id);
                    self.current_query = None;
                    self.result_set = Some(result_set);
                    return Err(error);
                }
            };
            if !result_set.push_row(row, DEFAULT_WINDOW_LIMIT) {
                // Window budget reached: cancel the still-running stream so the
                // driver stops stepping and no orphan task remains.
                budget_reached = true;
                executed.handle.cancel();
                break;
            }
            rows += 1;
        }
        if budget_reached {
            // We stopped the stream ourselves. The rows we kept are valid, so
            // this is a truncation; reporting `Cancelled` would make a large
            // result look like a user action (D-08 covers user cancellation).
            result_set.mark_ready(&self.capabilities);
        } else if executed.handle.is_cancelled() {
            result_set.mark_cancelled();
        } else {
            result_set.mark_ready(&self.capabilities);
        }
        result_set.total_rows = Some(rows);
        self.manager.finish_query(&self.connection_id, query_id);
        self.current_query = None;
        self.result_set = Some(result_set);
        Ok(())
    }

    /// Cancel the in-flight query. Cancellation is a successful terminal state,
    /// never an error (D-08).
    pub fn cancel_current(&mut self) {
        if let Some(handle) = self.current_query.take() {
            handle.cancel();
            self.manager.finish_query(&self.connection_id, handle.id());
        }
        if let Some(result_set) = &mut self.result_set {
            result_set.mark_cancelled();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.result_set
            .as_ref()
            .is_some_and(|set| set.state == ResultSetState::Cancelled)
    }

    pub async fn begin_transaction_async(&mut self) -> DriverResult<()> {
        if !self.capabilities.transactions {
            return Err(DatabaseError::driver("transactions are not supported"));
        }
        if self.transaction.is_some() {
            return Ok(());
        }
        let transaction = self.connection.begin().await?;
        self.transaction = Some(transaction);
        Ok(())
    }

    pub async fn commit(&mut self) -> DriverResult<()> {
        let Some(transaction) = self.transaction else {
            return Ok(());
        };
        self.connection.commit(transaction).await?;
        self.transaction = None;
        // Only fold values that actually reached the database; a staged edit
        // that was never applied must stay staged so the UI keeps showing it as
        // pending rather than claiming it was saved.
        let fully_applied = self.result_set.as_ref().is_some_and(|result_set| {
            result_set
                .pending_edits
                .iter()
                .all(|(key, value)| self.applied_edits.get(key) == Some(value))
        });
        if fully_applied {
            if let Some(result_set) = &mut self.result_set {
                for ((row_index, column_index), value) in
                    std::mem::take(&mut result_set.pending_edits)
                {
                    if let Some(cell) = result_set
                        .rows
                        .get_mut(row_index)
                        .and_then(|row| row.0.get_mut(column_index))
                    {
                        *cell = value;
                    }
                }
            }
        }
        self.applied_edits.clear();
        self.applied_statements = 0;
        self.applied_rows = 0;
        Ok(())
    }

    pub async fn rollback(&mut self) -> DriverResult<()> {
        let Some(transaction) = self.transaction else {
            return Ok(());
        };
        self.connection.rollback(transaction).await?;
        self.transaction = None;
        // The transaction is gone, so anything written by Apply is gone too.
        // Keep the cached rows untouched: they still hold the pre-edit values.
        self.applied_edits.clear();
        self.applied_statements = 0;
        self.applied_rows = 0;
        Ok(())
    }

    pub fn has_open_transaction(&self) -> bool {
        self.transaction.is_some()
    }

    /// Whether closing the view would lose work: staged edits, edits written to
    /// an uncommitted transaction, or an open transaction started by hand.
    pub fn has_uncommitted_changes(&self) -> bool {
        self.has_pending_edits() || self.has_open_transaction()
    }

    pub fn has_pending_edits(&self) -> bool {
        self.result_set
            .as_ref()
            .is_some_and(|set| set.pending_edit_count() > 0)
    }

    /// Number of edits already written to the open transaction.
    pub fn applied_edit_count(&self) -> usize {
        self.applied_edits.len()
    }

    /// Number of UPDATE statements sent to the open transaction.
    pub fn applied_statement_count(&self) -> usize {
        self.applied_statements
    }

    /// Rows the driver reported as changed by those statements.
    pub fn applied_row_count(&self) -> usize {
        self.applied_rows
    }

    /// Drop staged edits that were never applied. Returns false when an Apply
    /// already happened: clearing the display would then contradict the values
    /// in the open transaction, so the caller must Rollback instead.
    pub fn discard_pending_edits(&mut self) -> bool {
        if !self.applied_edits.is_empty() {
            return false;
        }
        if let Some(result_set) = &mut self.result_set {
            result_set.clear_pending_edits();
        }
        true
    }

    /// Bind the result set to a single table so edits can target rows by primary
    /// key. A join or computed result stays read-only.
    pub fn set_table(&mut self, table: TableRef, primary_key_columns: Vec<String>) {
        self.table = Some(table);
        if let Some(result_set) = &mut self.result_set {
            result_set.primary_key_columns = primary_key_columns;
            result_set.editable =
                self.capabilities.row_editing && !result_set.primary_key_columns.is_empty();
        }
    }

    /// Stage a cell edit. Fails when the result set is not editable.
    pub fn stage_edit(&mut self, row_index: usize, column_index: usize, value: Value) -> bool {
        self.result_set
            .as_mut()
            .is_some_and(|set| set.stage_edit(row_index, column_index, value))
    }

    /// Write staged edits to the open transaction without committing. Each
    /// changed row becomes one parameterized UPDATE targeting the row by its
    /// primary key. The batch runs inside `SAVEPOINT zzz_edit_batch`, so a
    /// failing statement in the middle reverts only this batch and leaves any
    /// outer transaction (and previously applied statements) intact.
    ///
    /// Returns the number of statements written by this call.
    pub async fn apply_pending_edits(&mut self) -> DriverResult<usize> {
        if !self.has_pending_edits() {
            return Ok(0);
        }
        if !self.capabilities.transactions {
            return Err(DatabaseError::driver("transactions are not supported"));
        }
        let Some(table) = self.table.clone() else {
            return Err(DatabaseError::driver(
                "cannot apply edits without a target table",
            ));
        };

        let delta = {
            let result_set = self
                .result_set
                .as_ref()
                .ok_or_else(|| DatabaseError::driver("no result set to edit"))?;
            let delta: BTreeMap<(usize, usize), Value> = result_set
                .pending_edits
                .iter()
                .filter(|(key, value)| self.applied_edits.get(*key) != Some(*value))
                .map(|(key, value)| (*key, value.clone()))
                .collect();
            if delta.is_empty() {
                return Ok(0);
            }
            delta
        };

        let statements = {
            let result_set = self
                .result_set
                .as_ref()
                .ok_or_else(|| DatabaseError::driver("no result set to edit"))?;
            build_update_statements_for_edits(&table, result_set, &delta)?
        };

        if self.transaction.is_none() {
            self.transaction = Some(self.connection.begin().await?);
        }
        self.execute_simple("SAVEPOINT zzz_edit_batch", &[]).await?;

        let mut applied = 0usize;
        let mut affected_rows = 0usize;
        for (sql, params) in &statements {
            match self.execute_simple(sql, params).await {
                Ok(rows) => {
                    applied += 1;
                    affected_rows += rows;
                }
                Err(error) => {
                    // Revert only this batch. Swallowing a cleanup failure would
                    // hide a transaction that is no longer in a known state.
                    self.execute_simple("ROLLBACK TO SAVEPOINT zzz_edit_batch", &[])
                        .await?;
                    self.execute_simple("RELEASE SAVEPOINT zzz_edit_batch", &[])
                        .await?;
                    return Err(error);
                }
            }
        }
        self.execute_simple("RELEASE SAVEPOINT zzz_edit_batch", &[])
            .await?;

        for (key, value) in delta {
            self.applied_edits.insert(key, value);
        }
        self.applied_statements += applied;
        self.applied_rows += affected_rows;
        Ok(applied)
    }

    /// Apply remaining staged edits and commit the transaction. This is the
    /// Commit action: it never leaves an applied-but-uncommitted batch behind.
    pub async fn commit_edits(&mut self) -> DriverResult<usize> {
        if !self.has_pending_edits() && !self.has_open_transaction() {
            return Ok(0);
        }
        let applied = self.apply_pending_edits().await?;
        self.commit().await?;
        Ok(applied)
    }

    /// Apply any remaining staged edits and roll the transaction back, then
    /// discard the staged edits. The cached rows are untouched, so the grid
    /// instantly shows the pre-edit values again.
    pub async fn rollback_edits(&mut self) -> DriverResult<()> {
        if let Some(result_set) = &mut self.result_set {
            result_set.clear_pending_edits();
        }
        self.rollback().await
    }

    /// Bind the result set to a single table so edits can target rows by primary
    /// key. Only `SELECT ... FROM <table>` shapes are eligible; joins, unions,
    /// grouping, and subqueries stay read-only. Returns whether editing is on.
    pub async fn bind_table_after_query(&mut self, sql: &str) -> DriverResult<bool> {
        self.table = None;
        if let Some(result_set) = &mut self.result_set {
            result_set.primary_key_columns.clear();
            result_set.editable = false;
        }
        if !self.capabilities.row_editing {
            return Ok(false);
        }
        let Some(table) = editable_target(sql) else {
            return Ok(false);
        };
        let schema = self.connection.describe_table(&table).await?;
        let Some(primary_key) = schema.primary_key else {
            return Ok(false);
        };
        let Some(result_set) = &self.result_set else {
            return Ok(false);
        };
        let all_present = primary_key
            .columns
            .iter()
            .all(|name| result_set.columns.iter().any(|column| &column.name == name));
        if !all_present {
            return Ok(false);
        }
        self.set_table(table, primary_key.columns);
        Ok(true)
    }

    /// Execute a statement for its side effect, draining any row stream so the
    /// driver releases its connection and the query handle is always finished,
    /// even when the statement fails.
    async fn execute_simple(&mut self, sql: &str, params: &[Value]) -> DriverResult<usize> {
        let executed = self.connection.execute(sql, params).await?;
        let affected_rows = executed.metadata.affected_rows.unwrap_or(0);
        let query_id = executed.handle.id();
        self.manager
            .track_query(&self.connection_id, executed.handle.clone());
        let rows = executed.rows;
        let mut failure: Option<DatabaseError> = None;
        while let Some(item) = rows.next().await {
            if let Err(error) = item {
                failure = Some(error);
                break;
            }
        }
        self.manager.finish_query(&self.connection_id, query_id);
        match failure {
            Some(error) => Err(error),
            None => Ok(affected_rows),
        }
    }
}

/// Build parameterized UPDATE statements for staged edits. Each changed row
/// yields one `UPDATE "table" SET "col" = ? ... WHERE "pk" = ? ...`. Column and
/// table names are quoted; values are always bound parameters.
pub fn build_update_statements(
    table: &TableRef,
    result_set: &ResultSet,
) -> DriverResult<Vec<(String, Vec<Value>)>> {
    build_update_statements_for_edits(table, result_set, &result_set.pending_edits)
}

/// Same as [`build_update_statements`], but for an explicit edit map. Apply
/// passes only the edits that have not reached the transaction yet.
pub fn build_update_statements_for_edits(
    table: &TableRef,
    result_set: &ResultSet,
    edits: &BTreeMap<(usize, usize), Value>,
) -> DriverResult<Vec<(String, Vec<Value>)>> {
    if result_set.primary_key_columns.is_empty() {
        return Err(DatabaseError::driver(
            "cannot edit a result set without a primary key",
        ));
    }
    let pk_indices: Vec<usize> = result_set
        .primary_key_columns
        .iter()
        .filter_map(|name| {
            result_set
                .columns
                .iter()
                .position(|column| &column.name == name)
        })
        .collect();
    if pk_indices.len() != result_set.primary_key_columns.len() {
        return Err(DatabaseError::driver(
            "primary key columns are missing from the result set",
        ));
    }

    let quoted_table = quote_table(table);
    let mut by_row: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (row_index, column_index) in edits.keys() {
        by_row.entry(*row_index).or_default().push(*column_index);
    }

    let mut statements = Vec::new();
    for (row_index, mut column_indices) in by_row {
        column_indices.sort_unstable();
        let row = result_set
            .rows
            .get(row_index)
            .ok_or_else(|| DatabaseError::driver("edit targets a missing row"))?;
        let mut assignments = Vec::with_capacity(column_indices.len());
        let mut params = Vec::with_capacity(column_indices.len() + pk_indices.len());
        for column_index in column_indices {
            let column = result_set
                .columns
                .get(column_index)
                .ok_or_else(|| DatabaseError::driver("edit targets a missing column"))?;
            let value = edits
                .get(&(row_index, column_index))
                .ok_or_else(|| DatabaseError::driver("edit value is missing"))?;
            assignments.push(format!("{} = ?", quote_identifier(&column.name)));
            params.push(value.clone());
        }
        let mut predicates = Vec::with_capacity(pk_indices.len());
        for pk_index in &pk_indices {
            let column = result_set
                .columns
                .get(*pk_index)
                .ok_or_else(|| DatabaseError::driver("primary key column is missing"))?;
            let value = row
                .get(*pk_index)
                .ok_or_else(|| DatabaseError::driver("primary key value is missing"))?;
            predicates.push(format!("{} = ?", quote_identifier(&column.name)));
            params.push(value.clone());
        }
        statements.push((
            format!(
                "UPDATE {quoted_table} SET {} WHERE {}",
                assignments.join(", "),
                predicates.join(" AND ")
            ),
            params,
        ));
    }
    Ok(statements)
}

fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn quote_table(table: &TableRef) -> String {
    let mut quoted = String::new();
    for part in [&table.catalog, &table.schema]
        .into_iter()
        .flatten()
        .chain(std::iter::once(&table.name))
    {
        if !quoted.is_empty() {
            quoted.push('.');
        }
        quoted.push_str(&quote_identifier(part));
    }
    quoted
}

/// Clipboard export formats offered by the result grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Markdown,
    Html,
}

/// Export the effective (staged-edit-aware) rows as CSV text. Values are quoted
/// per RFC 4180; BLOBs use their digest, NULL is empty.
pub fn export_csv(result_set: &ResultSet) -> String {
    export_result_set(
        result_set,
        ExportFormat::Csv,
        &(0..result_set.rows.len()).collect::<Vec<_>>(),
    )
}

/// Export the rows in `order` (indices into `result_set.rows`). The order is the
/// display order, so what is copied matches what the user sees after sorting.
pub fn export_result_set(result_set: &ResultSet, format: ExportFormat, order: &[usize]) -> String {
    match format {
        ExportFormat::Csv => export_csv_rows(result_set, order),
        ExportFormat::Markdown => export_markdown_rows(result_set, order),
        ExportFormat::Html => export_html_rows(result_set, order),
    }
}

fn export_csv_rows(result_set: &ResultSet, order: &[usize]) -> String {
    let mut output = String::new();
    output.push_str(
        &result_set
            .columns
            .iter()
            .map(|column| csv_escape(&column.name))
            .collect::<Vec<_>>()
            .join(","),
    );
    output.push('\n');
    for row_index in order {
        let cells: Vec<String> = (0..result_set.columns.len())
            .map(|column_index| {
                result_set
                    .value_at(*row_index, column_index)
                    .map(|value| csv_escape(&value.to_export_text()))
                    .unwrap_or_default()
            })
            .collect();
        output.push_str(&cells.join(","));
        output.push('\n');
    }
    output
}

fn export_markdown_rows(result_set: &ResultSet, order: &[usize]) -> String {
    let mut output = String::new();
    output.push('|');
    for column in &result_set.columns {
        output.push(' ');
        output.push_str(&markdown_escape(&column.name));
        output.push_str(" |");
    }
    output.push('\n');
    output.push('|');
    for _ in &result_set.columns {
        output.push_str(" --- |");
    }
    output.push('\n');
    for row_index in order {
        output.push('|');
        for column_index in 0..result_set.columns.len() {
            output.push(' ');
            if let Some(value) = result_set.value_at(*row_index, column_index) {
                output.push_str(&markdown_escape(&value.to_export_text()));
            }
            output.push_str(" |");
        }
        output.push('\n');
    }
    output
}

fn export_html_rows(result_set: &ResultSet, order: &[usize]) -> String {
    let mut output = String::from("<table>\n<thead><tr>");
    for column in &result_set.columns {
        output.push_str("<th>");
        output.push_str(&html_escape(&column.name));
        output.push_str("</th>");
    }
    output.push_str("</tr></thead>\n<tbody>\n");
    for row_index in order {
        output.push_str("<tr>");
        for column_index in 0..result_set.columns.len() {
            output.push_str("<td>");
            if let Some(value) = result_set.value_at(*row_index, column_index) {
                output.push_str(&html_escape(&value.to_export_text()));
            }
            output.push_str("</td>");
        }
        output.push_str("</tr>\n");
    }
    output.push_str("</tbody>\n</table>\n");
    output
}

fn markdown_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace("\r\n", "<br>")
        .replace(['\n', '\r'], "<br>")
}

fn html_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// A small helper used by the UI to describe a column without a driver call.
pub fn column_label(column: &Column) -> String {
    format!("{} ({:?})", column.name, column.declared_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DatabaseErrorKind;
    use crate::model::{ColumnType, ConnectionConfig, DataSource, Row};
    use crate::sqlite::{SQLITE_DRIVER_ID, SqliteDriver};

    async fn new_session() -> QuerySession {
        static NEXT_DATABASE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let name = format!(
            "session-test-{}",
            NEXT_DATABASE.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        );
        let display_name = name.clone();
        let source = DataSource::InMemory { name };
        new_session_with_source(display_name.clone(), display_name, source).await
    }

    async fn new_session_with_source(
        id: String,
        display_name: String,
        source: DataSource,
    ) -> QuerySession {
        let manager = Arc::new(ConnectionManager::new());
        manager.register_driver(Arc::new(SqliteDriver));
        let id = ConnectionId(id);
        manager.upsert_config(ConnectionConfig {
            id: id.clone(),
            display_name,
            driver_id: SQLITE_DRIVER_ID.to_owned(),
            source,
            host: None,
            port: None,
            user: None,
            database: None,
            options: Default::default(),
        });
        let connection = manager.connect(&id, None).await.expect("connect");
        QuerySession::new(id, connection, manager, Capabilities::sqlite())
    }

    fn temporary_database_path(label: &str) -> std::path::PathBuf {
        static NEXT_FILE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "zzz-database-test-{label}-{}-{}.sqlite",
            std::process::id(),
            NEXT_FILE.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        std::fs::File::create(&path).expect("create database file");
        path
    }

    async fn file_session(path: &std::path::Path, label: &str) -> QuerySession {
        new_session_with_source(
            label.to_owned(),
            label.to_owned(),
            DataSource::File {
                path: path.to_path_buf(),
            },
        )
        .await
    }

    #[test]
    fn execute_fills_a_ready_result_set() {
        futures::executor::block_on(async {
            let mut session = new_session().await;
            session
                .execute("SELECT 1 AS value", &[])
                .await
                .expect("execute");
            let result = session.result_set().expect("result");
            assert_eq!(result.state, ResultSetState::Ready);
            assert_eq!(result.rows.len(), 1);
            assert_eq!(result.rows[0].get(0), Some(&Value::Integer(1)));
        });
    }

    #[test]
    fn execute_surfaces_classified_errors() {
        futures::executor::block_on(async {
            let mut session = new_session().await;
            session
                .execute("CREATE TABLE t (id INTEGER PRIMARY KEY)", &[])
                .await
                .expect("create");
            let error = session
                .execute("INSERT INTO t VALUES (1), (1)", &[])
                .await
                .expect_err("constraint violation");
            assert_eq!(error.kind, DatabaseErrorKind::Constraint);
            assert_eq!(
                session.result_set().map(|result| result.state),
                Some(ResultSetState::Failed)
            );
        });
    }

    #[test]
    fn window_budget_truncates_without_reporting_a_user_cancel() {
        futures::executor::block_on(async {
            let mut session = new_session().await;
            session
                .execute(
                    "WITH RECURSIVE counter(value) AS (
                        VALUES(1) UNION ALL SELECT value + 1 FROM counter WHERE value < 1000000
                    ) SELECT value FROM counter",
                    &[],
                )
                .await
                .expect("execute");
            let result = session.result_set().expect("result");
            assert_eq!(result.state, ResultSetState::Ready);
            assert_eq!(result.rows.len(), DEFAULT_WINDOW_LIMIT);
            assert_eq!(
                result.truncation,
                Some(crate::model::Truncation {
                    row_limit: DEFAULT_WINDOW_LIMIT
                })
            );
            assert!(!session.is_cancelled());
        });
    }

    fn editable_set() -> ResultSet {
        let mut primary_key_column = Column::new("id", ColumnType::Integer);
        primary_key_column.primary_key = true;
        let mut set = ResultSet::new(
            vec![primary_key_column, Column::new("name", ColumnType::Text)],
            "SELECT id, name FROM users",
        );
        set.push_row(
            Row(vec![Value::Integer(1), Value::Text("Alice".into())]),
            100,
        );
        set.push_row(Row(vec![Value::Integer(2), Value::Text("Bob".into())]), 100);
        set.mark_ready(&Capabilities::sqlite());
        set
    }

    #[test]
    fn dml_is_parameterized_and_targets_primary_key() {
        let mut set = editable_set();
        assert!(set.stage_edit(0, 1, Value::Text("Alicia'.drop".into())));
        let statements = build_update_statements(&TableRef::new("users"), &set).expect("build");
        assert_eq!(statements.len(), 1);
        let (sql, params) = &statements[0];
        assert_eq!(sql, "UPDATE \"users\" SET \"name\" = ? WHERE \"id\" = ?");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], Value::Text("Alicia'.drop".into()));
        assert_eq!(params[1], Value::Integer(1));
        assert!(!sql.contains("Alicia"), "user value must not appear in SQL");
    }

    #[test]
    fn dml_requires_primary_key() {
        let mut set = ResultSet::new(
            vec![Column::new("name", ColumnType::Text)],
            "SELECT name FROM users",
        );
        set.push_row(Row(vec![Value::Text("Alice".into())]), 100);
        set.mark_ready(&Capabilities::sqlite());
        assert!(build_update_statements(&TableRef::new("users"), &set).is_err());
    }

    async fn create_users_table(session: &mut QuerySession) {
        session
            .execute(
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE)",
                &[],
            )
            .await
            .expect("create table");
        session
            .execute("INSERT INTO users VALUES (1, 'Alice'), (2, 'Bob')", &[])
            .await
            .expect("insert rows");
    }

    async fn stored_name(session: &mut QuerySession, id: i64) -> Value {
        session
            .execute("SELECT name FROM users WHERE id = ?", &[Value::Integer(id)])
            .await
            .expect("select name");
        session
            .result_set()
            .and_then(|result| result.rows.first())
            .and_then(|row| row.get(0))
            .cloned()
            .expect("stored name")
    }

    #[test]
    fn apply_writes_inside_the_transaction_and_commit_persists_it() {
        futures::executor::block_on(async {
            let path = temporary_database_path("apply-commit");
            let mut writer = file_session(&path, "writer").await;
            create_users_table(&mut writer).await;
            let mut observer = file_session(&path, "observer").await;

            writer
                .execute("SELECT id, name FROM users", &[])
                .await
                .expect("select");
            assert!(
                writer
                    .bind_table_after_query("SELECT id, name FROM users")
                    .await
                    .expect("bind")
            );
            assert!(writer.stage_edit(0, 1, Value::Text("Alicia".into())));

            assert_eq!(writer.apply_pending_edits().await.expect("apply"), 1);
            assert!(writer.has_open_transaction());
            assert_eq!(writer.applied_edit_count(), 1);
            assert_eq!(writer.applied_statement_count(), 1);
            // Applying again without new edits sends nothing.
            assert_eq!(writer.apply_pending_edits().await.expect("re-apply"), 0);

            // The second connection sees the pre-transaction value.
            assert_eq!(
                stored_name(&mut observer, 1).await,
                Value::Text("Alice".into())
            );

            writer.commit_edits().await.expect("commit");
            assert!(!writer.has_open_transaction());
            assert_eq!(
                stored_name(&mut observer, 1).await,
                Value::Text("Alicia".into())
            );
            std::fs::remove_file(&path).expect("remove database");
        });
    }

    #[test]
    fn rollback_discards_applied_edits_and_restores_the_cached_value() {
        futures::executor::block_on(async {
            let path = temporary_database_path("rollback");
            let mut writer = file_session(&path, "writer").await;
            create_users_table(&mut writer).await;
            let mut observer = file_session(&path, "observer").await;

            writer
                .execute("SELECT id, name FROM users", &[])
                .await
                .expect("select");
            writer
                .bind_table_after_query("SELECT id, name FROM users")
                .await
                .expect("bind");
            assert!(writer.stage_edit(1, 1, Value::Text("Robert".into())));
            assert_eq!(
                writer.result_set().map(|set| set.pending_edit_count()),
                Some(1)
            );
            writer.apply_pending_edits().await.expect("apply");
            writer.rollback_edits().await.expect("rollback");

            assert!(!writer.has_open_transaction());
            assert!(!writer.has_pending_edits());
            assert_eq!(
                writer.result_set().and_then(|set| set.value_at(1, 1)),
                Some(&Value::Text("Bob".into()))
            );
            assert_eq!(
                stored_name(&mut observer, 2).await,
                Value::Text("Bob".into())
            );
            std::fs::remove_file(&path).expect("remove database");
        });
    }

    #[test]
    fn failing_batch_reverts_to_the_savepoint_and_keeps_the_transaction() {
        futures::executor::block_on(async {
            let path = temporary_database_path("savepoint");
            let mut writer = file_session(&path, "writer").await;
            create_users_table(&mut writer).await;
            let mut observer = file_session(&path, "observer").await;

            writer
                .execute("SELECT id, name FROM users", &[])
                .await
                .expect("select");
            writer
                .bind_table_after_query("SELECT id, name FROM users")
                .await
                .expect("bind");
            // The first update is valid, the second violates the UNIQUE index.
            // Both must be discarded: no half-applied batch.
            assert!(writer.stage_edit(0, 1, Value::Text("Carol".into())));
            assert!(writer.stage_edit(1, 1, Value::Text("Carol".into())));
            let error = writer
                .apply_pending_edits()
                .await
                .expect_err("unique violation");
            assert_eq!(error.kind, DatabaseErrorKind::Constraint);
            assert_eq!(writer.applied_edit_count(), 0);
            assert_eq!(writer.applied_statement_count(), 0);
            // The batch failed, so the edits are still staged and the transaction
            // is still open for a corrected retry.
            assert_eq!(
                writer.result_set().map(|set| set.pending_edit_count()),
                Some(2)
            );
            assert!(writer.has_open_transaction());
            assert_eq!(
                stored_name(&mut observer, 1).await,
                Value::Text("Alice".into())
            );
            assert_eq!(
                stored_name(&mut observer, 2).await,
                Value::Text("Bob".into())
            );

            writer.rollback_edits().await.expect("rollback");
            assert!(!writer.has_pending_edits());
            std::fs::remove_file(&path).expect("remove database");
        });
    }

    #[test]
    fn binding_requires_a_simple_select_with_its_primary_key() {
        futures::executor::block_on(async {
            let mut session = new_session().await;
            session
                .execute(
                    "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)",
                    &[],
                )
                .await
                .expect("create users");
            session
                .execute("CREATE TABLE numbers (value INTEGER, label TEXT)", &[])
                .await
                .expect("create numbers");

            session
                .execute("SELECT id, name FROM users", &[])
                .await
                .expect("select");
            assert!(
                session
                    .bind_table_after_query("SELECT id, name FROM users")
                    .await
                    .expect("bind")
            );
            assert_eq!(
                session
                    .result_set()
                    .map(|set| set.primary_key_columns.clone()),
                Some(vec!["id".to_owned()])
            );

            session
                .execute("SELECT name FROM users", &[])
                .await
                .expect("select");
            assert!(
                !session
                    .bind_table_after_query("SELECT name FROM users")
                    .await
                    .expect("bind")
            );
            assert_eq!(session.result_set().map(|set| set.editable), Some(false));

            session
                .execute("SELECT u.id, u.name FROM users AS u WHERE u.id > 0", &[])
                .await
                .expect("select");
            assert!(
                session
                    .bind_table_after_query("SELECT u.id, u.name FROM users AS u WHERE u.id > 0")
                    .await
                    .expect("bind")
            );

            session
                .execute(
                    "SELECT users.id FROM users JOIN users AS other ON other.id = users.id",
                    &[],
                )
                .await
                .expect("select");
            assert!(
                !session
                    .bind_table_after_query(
                        "SELECT users.id FROM users JOIN users AS other ON other.id = users.id"
                    )
                    .await
                    .expect("bind")
            );

            session
                .execute("SELECT value FROM numbers", &[])
                .await
                .expect("select");
            assert!(
                !session
                    .bind_table_after_query("SELECT value FROM numbers")
                    .await
                    .expect("bind")
            );
        });
    }

    #[test]
    fn clearing_staged_edits_is_refused_after_apply() {
        futures::executor::block_on(async {
            let mut session = new_session().await;
            create_users_table(&mut session).await;
            session
                .execute("SELECT id, name FROM users", &[])
                .await
                .expect("select");
            session
                .bind_table_after_query("SELECT id, name FROM users")
                .await
                .expect("bind");
            assert!(session.stage_edit(0, 1, Value::Text("Alicia".into())));
            assert!(session.discard_pending_edits());
            assert!(!session.has_pending_edits());

            assert!(session.stage_edit(0, 1, Value::Text("Alicia".into())));
            session.apply_pending_edits().await.expect("apply");
            // Clearing now would hide a value the transaction already holds.
            assert!(!session.discard_pending_edits());
            assert!(session.has_pending_edits());
            session.rollback_edits().await.expect("rollback");
        });
    }

    #[test]
    fn csv_export_uses_effective_values_and_escapes() {
        let mut set = editable_set();
        set.stage_edit(1, 1, Value::Text("Bob, Jr\n\"B\"".into()));
        let csv = export_csv(&set);
        assert!(csv.starts_with("id,name\n"));
        assert!(csv.contains("1,Alice\n"));
        assert!(csv.contains("\"Bob, Jr\n\"\"B\"\"\""));
    }

    #[test]
    fn markdown_export_escapes_pipes_and_newlines() {
        let mut set = editable_set();
        set.stage_edit(0, 1, Value::Text("A|B\nC".into()));
        let markdown = export_result_set(&set, ExportFormat::Markdown, &[0, 1]);
        assert!(markdown.starts_with("| id | name |\n| --- | --- |\n"));
        assert!(markdown.contains("A\\|B<br>C"));
    }

    #[test]
    fn html_export_escapes_markup() {
        let mut set = editable_set();
        set.stage_edit(0, 1, Value::Text("<b>&\"'</b>".into()));
        let html = export_result_set(&set, ExportFormat::Html, &[0, 1]);
        assert!(html.contains("<td>&lt;b&gt;&amp;&quot;&#39;&lt;/b&gt;</td>"));
    }

    #[test]
    fn export_follows_the_display_order() {
        let set = editable_set();
        let csv = export_result_set(&set, ExportFormat::Csv, &[1, 0]);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[1], "2,Bob");
        assert_eq!(lines[2], "1,Alice");
    }

    #[test]
    fn column_label_is_informational() {
        let label = column_label(&Column::new("id", ColumnType::Integer));
        assert!(label.starts_with("id ("));
    }
}
