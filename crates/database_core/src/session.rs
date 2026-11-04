//! Query execution, cancellation, transactions, and staged edits.
//!
//! `QuerySession` is the owner of an in-flight query, an open transaction, and
//! the staged edits for one result set (invariant 6). It converts the driver's
//! bounded `RowStream` into a windowed `ResultSet` and generates parameterized
//! DML for edits so SQL injection is impossible (D-05).

use std::sync::Arc;

use crate::connection::ConnectionManager;
use crate::driver::{DriverConnection, DriverResult, ExecutedQuery, QueryHandle};
use crate::error::DatabaseError;
use crate::model::{
    Capabilities, Column, ConnectionId, ResultSet, ResultSetState, TableRef, Value,
};

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
        let executed: ExecutedQuery = self.connection.execute(sql, params).await?;
        let query_id = executed.handle.id();
        self.manager
            .track_query(&self.connection_id, executed.handle.clone());
        self.current_query = Some(executed.handle.clone());

        let mut result_set = crate::driver::result_set_for(&executed.metadata, sql);
        result_set.total_rows = executed.metadata.affected_rows;
        let mut rows = 0usize;
        while let Some(item) = executed.rows.next().await {
            let row = match item {
                Ok(row) => row,
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
                executed.handle.cancel();
                break;
            }
            rows += 1;
        }
        if executed.handle.is_cancelled() {
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
        let Some(transaction) = self.transaction.take() else {
            return Ok(());
        };
        self.connection.commit(transaction).await
    }

    pub async fn rollback(&mut self) -> DriverResult<()> {
        let Some(transaction) = self.transaction.take() else {
            return Ok(());
        };
        self.connection.rollback(transaction).await
    }

    pub fn has_open_transaction(&self) -> bool {
        self.transaction.is_some()
    }

    pub fn has_pending_edits(&self) -> bool {
        self.result_set
            .as_ref()
            .is_some_and(|set| set.pending_edit_count() > 0)
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

    /// Commit staged edits inside a transaction: one parameterized UPDATE per
    /// changed row, targeting the row by its primary key columns. No user text
    /// is ever concatenated into SQL.
    pub async fn commit_edits(&mut self) -> DriverResult<usize> {
        if !self.has_pending_edits() {
            return Ok(0);
        }
        let Some(table) = self.table.clone() else {
            return Err(DatabaseError::driver(
                "cannot commit edits without a target table",
            ));
        };
        if !self.capabilities.transactions {
            return Err(DatabaseError::driver("transactions are not supported"));
        }
        let transaction = if let Some(transaction) = self.transaction {
            transaction
        } else {
            let transaction = self.connection.begin().await?;
            self.transaction = Some(transaction);
            transaction
        };

        let result_set = self
            .result_set
            .as_ref()
            .ok_or_else(|| DatabaseError::driver("no result set to commit"))?;
        let statements = build_update_statements(&table, result_set)?;
        let mut applied = 0usize;
        for (sql, params) in statements {
            let executed = self.connection.execute(&sql, &params).await?;
            // Drain DML (no rows expected) so the streaming task is owned and
            // finishes before the next statement. A constraint failure surfaces
            // here as the stream's final item and aborts the commit.
            while let Some(item) = executed.rows.next().await {
                item?;
            }
            applied += 1;
        }
        self.connection.commit(transaction).await?;
        self.transaction = None;
        if let Some(result_set) = &mut self.result_set {
            // Apply staged values to the cached rows so the view reflects the
            // committed state, then clear the staging area.
            for ((row_index, column_index), value) in std::mem::take(&mut result_set.pending_edits)
            {
                if let Some(row) = result_set.rows.get_mut(row_index)
                    && let Some(cell) = row.0.get_mut(column_index)
                {
                    *cell = value;
                }
            }
        }
        Ok(applied)
    }

    /// Discard staged edits and roll back the transaction (D-05).
    pub async fn rollback_edits(&mut self) -> DriverResult<()> {
        if let Some(result_set) = &mut self.result_set {
            result_set.clear_pending_edits();
        }
        self.rollback().await
    }
}

/// Build parameterized UPDATE statements for staged edits. Each changed row
/// yields one `UPDATE "table" SET "col" = ? ... WHERE "pk" = ? ...`. Column and
/// table names are quoted; values are always bound parameters.
pub fn build_update_statements(
    table: &TableRef,
    result_set: &ResultSet,
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

    let quoted_table = quote_identifier(&table.name);
    let mut by_row: std::collections::BTreeMap<usize, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (row_index, column_index) in result_set.pending_edits.keys() {
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
            let value = result_set
                .pending_edits
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

/// Export the effective (staged-edit-aware) rows as CSV text (Phase 4 / D-06).
/// Values are quoted per RFC 4180; BLOBs use their digest, NULL is empty.
pub fn export_csv(result_set: &ResultSet) -> String {
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
    for row_index in 0..result_set.rows.len() {
        let cells: Vec<String> = (0..result_set.columns.len())
            .map(|column_index| {
                result_set
                    .value_at(row_index, column_index)
                    .map(|value| csv_escape(&value.to_export_text()))
                    .unwrap_or_default()
            })
            .collect();
        output.push_str(&cells.join(","));
        output.push('\n');
    }
    output
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
    use crate::model::{ColumnType, Row};

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
    fn column_label_is_informational() {
        let label = column_label(&Column::new("id", ColumnType::Integer));
        assert!(label.starts_with("id ("));
    }
}
