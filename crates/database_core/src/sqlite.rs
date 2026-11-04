//! Built-in SQLite driver. Uses the workspace's bundled `libsqlite3-sys` FFI
//! directly (see D-02 revision: `sqlez` cannot satisfy cancellation, runtime
//! typed values, or streaming rows).
//!
//! The connection is serialized behind a `Mutex` because SQLite's per-connection
//! handle is not thread-safe under concurrent use. Cancellation does not take
//! that mutex: `sqlite3_interrupt` is documented as safe to call from a
//! different thread while a statement is running, so a control action can abort
//! a long query immediately (DB-EXP-001).

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::Path;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use async_trait::async_trait;
use libsqlite3_sys::*;
use parking_lot::Mutex;

use crate::driver::{
    DatabaseDriver, DriverConnection, DriverResult, ExecutedQuery, ExplainNode, ExplainPlan,
    Interrupt, QueryHandle, QueryMetadata, ROW_STREAM_CAPACITY, RowStream, TransactionId,
};
use crate::error::DatabaseError;
#[cfg(test)]
use crate::error::DatabaseErrorKind;
use crate::model::{
    Capabilities, Catalog, Column, ColumnType, ConnectionConfig, DataSource, ForeignKey, Index,
    ObjectKind, PrimaryKey, Row, Schema, Table, TableRef, TableSchema, Value,
};

/// The built-in driver's stable id. The UI must never branch on this string.
pub const SQLITE_DRIVER_ID: &str = "sqlite";

/// Wraps the raw connection so it can be shared (`Send + Sync`). All SQLite
/// calls go through the mutex; only `interrupt` uses the stored pointer.
struct SqliteHandle {
    connection: Mutex<*mut sqlite3>,
    /// The same pointer as an integer, used only by `sqlite3_interrupt`.
    interrupt_ptr: usize,
    transaction_counter: AtomicU64,
    closed: AtomicBool,
}

// SAFETY: every access to `connection` is serialized by the mutex. The
// `interrupt_ptr` is only passed to `sqlite3_interrupt`, which SQLite documents
// as safe to invoke from another thread on a running connection.
unsafe impl Send for SqliteHandle {}
unsafe impl Sync for SqliteHandle {}

impl Drop for SqliteHandle {
    fn drop(&mut self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        let connection = *self.connection.lock();
        if !connection.is_null() {
            unsafe {
                sqlite3_close(connection);
            }
        }
    }
}

struct ConnectionInterrupt {
    interrupt_ptr: usize,
}

impl Interrupt for ConnectionInterrupt {
    fn interrupt(&self) {
        if self.interrupt_ptr != 0 {
            // SAFETY: the pointer belongs to a live connection owned by the same
            // `SqliteHandle`; SQLite permits calling interrupt from any thread.
            unsafe {
                sqlite3_interrupt(self.interrupt_ptr as *mut sqlite3);
            }
        }
    }
}

/// The built-in driver. Construct once and register in the driver registry.
pub struct SqliteDriver;

#[async_trait]
impl DatabaseDriver for SqliteDriver {
    fn id(&self) -> &'static str {
        SQLITE_DRIVER_ID
    }

    fn display_name(&self) -> &'static str {
        "SQLite"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::sqlite()
    }

    async fn connect(&self, config: &ConnectionConfig) -> DriverResult<Arc<dyn DriverConnection>> {
        let handle = open_connection(config)?;
        Ok(Arc::new(SqliteConnection { handle }))
    }
}

fn open_connection(config: &ConnectionConfig) -> DriverResult<Arc<SqliteHandle>> {
    let uri = match &config.source {
        DataSource::File { path } => path_to_sqlite_uri(path)?,
        DataSource::InMemory { name } => {
            format!(
                "file:{}?mode=memory&cache=shared",
                name.replace(['?', '#', '&'], "_")
            )
        }
        DataSource::Server { .. } => {
            return Err(DatabaseError::driver(
                "SQLite driver does not accept server data sources",
            ));
        }
    };
    let c_uri = CString::new(uri).map_err(|error| DatabaseError::driver(error.to_string()))?;
    let mut raw: *mut sqlite3 = ptr::null_mut();
    // READWRITE without CREATE: opening a missing file must fail so a test
    // connection reports a classified error instead of silently creating an
    // empty database (which `sqlez::open_file` does).
    let flags = SQLITE_OPEN_READWRITE | SQLITE_OPEN_URI | SQLITE_OPEN_NOMUTEX;
    let code = unsafe { sqlite3_open_v2(c_uri.as_ptr(), &raw mut raw, flags, ptr::null()) };
    if code != SQLITE_OK {
        let message = raw_error_message(raw);
        if !raw.is_null() {
            unsafe {
                sqlite3_close(raw);
            }
        }
        return Err(classify_open_error(code, &message));
    }
    if raw.is_null() {
        return Err(DatabaseError::connection(
            "SQLite returned a null connection handle",
        ));
    }
    unsafe {
        sqlite3_extended_result_codes(raw, 1);
    }
    unsafe {
        sqlite3_busy_timeout(raw, 5000);
    }
    Ok(Arc::new(SqliteHandle {
        connection: Mutex::new(raw),
        interrupt_ptr: raw as usize,
        transaction_counter: AtomicU64::new(0),
        closed: AtomicBool::new(false),
    }))
}

fn path_to_sqlite_uri(path: &Path) -> DriverResult<String> {
    // Use a file: URI so '?' and '#' in a path are not interpreted as query
    // parameters. The path stays a PathBuf in config; conversion happens only
    // here, at the driver boundary, without assuming a local filesystem.
    let text = path
        .to_str()
        .ok_or_else(|| DatabaseError::connection("database path is not valid UTF-8"))?;
    Ok(format!("file:{}", text))
}

fn classify_open_error(code: c_int, message: &str) -> DatabaseError {
    match code {
        SQLITE_CANTOPEN | SQLITE_IOERR => DatabaseError::connection(message),
        SQLITE_AUTH => DatabaseError::authentication(message),
        SQLITE_PERM | SQLITE_READONLY => DatabaseError::permission(message),
        _ => DatabaseError::driver(format!("sqlite open failed ({code}): {message}")),
    }
}

/// A live SQLite connection.
pub struct SqliteConnection {
    handle: Arc<SqliteHandle>,
}

impl SqliteConnection {
    fn guard(&self) -> DriverResult<parking_lot::MutexGuard<'_, *mut sqlite3>> {
        if self.handle.closed.load(Ordering::SeqCst) {
            return Err(DatabaseError::connection("connection is closed"));
        }
        Ok(self.handle.connection.lock())
    }

    fn raw(guard: &parking_lot::MutexGuard<'_, *mut sqlite3>) -> *mut sqlite3 {
        **guard
    }

    /// Run a closure with the serialized connection, mapping SQLite failures to
    /// classified errors. The closure never outlives the lock.
    fn with_raw<T>(&self, f: impl FnOnce(*mut sqlite3) -> DriverResult<T>) -> DriverResult<T> {
        let guard = self.guard()?;
        let raw = Self::raw(&guard);
        f(raw)
    }

    fn last_error(raw: *mut sqlite3, code: c_int) -> DatabaseError {
        let message = raw_error_message(raw);
        classify_step_error(code, &message)
    }

    fn prepare(raw: *mut sqlite3, sql: &str) -> DriverResult<*mut sqlite3_stmt> {
        let c_sql = CString::new(sql).map_err(|error| DatabaseError::syntax(error.to_string()))?;
        let mut statement: *mut sqlite3_stmt = ptr::null_mut();
        let code = unsafe {
            sqlite3_prepare_v2(raw, c_sql.as_ptr(), -1, &raw mut statement, ptr::null_mut())
        };
        if code != SQLITE_OK {
            return Err(Self::last_error(raw, code));
        }
        Ok(statement)
    }
}

fn raw_error_message(raw: *mut sqlite3) -> String {
    if raw.is_null() {
        return "SQLite returned a null handle".to_owned();
    }
    let pointer = unsafe { sqlite3_errmsg(raw) };
    if pointer.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(pointer).to_string_lossy().into_owned() }
}

fn classify_step_error(code: c_int, message: &str) -> DatabaseError {
    match code {
        SQLITE_INTERRUPT => DatabaseError::cancelled(),
        SQLITE_ERROR => DatabaseError::syntax(message),
        SQLITE_CONSTRAINT => DatabaseError::constraint(message),
        SQLITE_AUTH => DatabaseError::authentication(message),
        SQLITE_PERM | SQLITE_READONLY => DatabaseError::permission(message),
        SQLITE_BUSY | SQLITE_LOCKED => DatabaseError::timeout(message),
        SQLITE_NOMEM | SQLITE_IOERR | SQLITE_CANTOPEN => DatabaseError::connection(message),
        _ => DatabaseError::other(format!("sqlite error ({code}): {message}")),
    }
}

fn column_metadata(statement: *mut sqlite3_stmt) -> Vec<Column> {
    let count = unsafe { sqlite3_column_count(statement) };
    let mut columns = Vec::with_capacity(count.max(0) as usize);
    for index in 0..count {
        let name_pointer = unsafe { sqlite3_column_name(statement, index) };
        let name = unsafe { CStr::from_ptr(name_pointer) }
            .to_string_lossy()
            .into_owned();
        let declared_pointer = unsafe { sqlite3_column_decltype(statement, index) };
        let declared = if declared_pointer.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr(declared_pointer) }
                .to_string_lossy()
                .into_owned()
        };
        columns.push(Column::new(name, ColumnType::from_declared(&declared)));
    }
    columns
}

fn read_row(statement: *mut sqlite3_stmt, column_count: usize) -> DriverResult<Row> {
    let mut values = Vec::with_capacity(column_count);
    for index in 0..column_count {
        let value = match unsafe { sqlite3_column_type(statement, index as c_int) } {
            SQLITE_INTEGER => {
                Value::Integer(unsafe { sqlite3_column_int64(statement, index as c_int) })
            }
            SQLITE_FLOAT => {
                Value::Real(unsafe { sqlite3_column_double(statement, index as c_int) })
            }
            SQLITE_TEXT => read_text(statement, index),
            SQLITE_BLOB => read_blob(statement, index),
            SQLITE_NULL => Value::Null,
            other => {
                return Err(DatabaseError::driver(format!(
                    "unknown SQLite column type {other}"
                )));
            }
        };
        values.push(value);
    }
    Ok(Row(values))
}

fn read_text(statement: *mut sqlite3_stmt, index: usize) -> Value {
    let pointer = unsafe { sqlite3_column_text(statement, index as c_int) };
    if pointer.is_null() {
        return Value::Null;
    }
    let length = unsafe { sqlite3_column_bytes(statement, index as c_int) } as usize;
    let slice = unsafe { std::slice::from_raw_parts(pointer, length) };
    Value::Text(String::from_utf8_lossy(slice).into_owned())
}

fn read_blob(statement: *mut sqlite3_stmt, index: usize) -> Value {
    let pointer = unsafe { sqlite3_column_blob(statement, index as c_int) };
    if pointer.is_null() {
        return Value::Blob(Vec::new());
    }
    let length = unsafe { sqlite3_column_bytes(statement, index as c_int) } as usize;
    let slice = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), length) };
    Value::Blob(slice.to_vec())
}

fn bind_value(statement: *mut sqlite3_stmt, index: c_int, value: &Value) -> DriverResult<()> {
    let code = match value {
        Value::Null => unsafe { sqlite3_bind_null(statement, index) },
        Value::Integer(number) => unsafe { sqlite3_bind_int64(statement, index, *number) },
        Value::Real(number) => unsafe { sqlite3_bind_double(statement, index, *number) },
        Value::Text(text)
        | Value::Date(text)
        | Value::DateTime(text)
        | Value::Time(text)
        | Value::Numeric(text)
        | Value::Json(text) => unsafe {
            sqlite3_bind_text(
                statement,
                index,
                text.as_ptr().cast::<c_char>(),
                text.len() as c_int,
                SQLITE_TRANSIENT(),
            )
        },
        Value::Blob(bytes) => unsafe {
            sqlite3_bind_blob(
                statement,
                index,
                bytes.as_ptr().cast::<c_void>(),
                bytes.len() as c_int,
                SQLITE_TRANSIENT(),
            )
        },
        Value::Bool(value) => unsafe { sqlite3_bind_int64(statement, index, i64::from(*value)) },
    };
    if code == SQLITE_OK {
        Ok(())
    } else {
        Err(DatabaseError::driver(format!("bind failed ({code})")))
    }
}

#[async_trait]
impl DriverConnection for SqliteConnection {
    async fn test(&self) -> DriverResult<()> {
        self.with_raw(|raw| {
            let statement = Self::prepare(raw, "SELECT 1")?;
            let code = unsafe { sqlite3_step(statement) };
            unsafe {
                sqlite3_finalize(statement);
            }
            if code == SQLITE_ROW || code == SQLITE_DONE {
                Ok(())
            } else {
                Err(Self::last_error(raw, code))
            }
        })
    }

    async fn catalogs(&self) -> DriverResult<Vec<Catalog>> {
        let tables = self.query_schema_objects()?;
        Ok(vec![Catalog {
            name: "main".to_owned(),
            schemas: vec![Schema {
                name: "main".to_owned(),
                tables,
            }],
        }])
    }

    async fn describe_table(&self, table: &TableRef) -> DriverResult<TableSchema> {
        self.describe(table)
    }

    async fn execute(&self, sql: &str, params: &[Value]) -> DriverResult<ExecutedQuery> {
        let (sender, stream) = RowStream::new(ROW_STREAM_CAPACITY);
        let cancelled = Arc::new(AtomicBool::new(false));
        let interrupt = Arc::new(ConnectionInterrupt {
            interrupt_ptr: self.handle.interrupt_ptr,
        });
        let handle = QueryHandle::new(cancelled.clone(), Some(interrupt));

        let (metadata, statement) = self.with_raw(|raw| {
            let statement = Self::prepare(raw, sql)?;
            for (position, value) in params.iter().enumerate() {
                bind_value(statement, position as c_int + 1, value)?;
            }
            let columns = column_metadata(statement);
            Ok((
                QueryMetadata {
                    columns,
                    affected_rows: None,
                },
                statement,
            ))
        })?;

        // Streaming runs on a blocking thread because `sqlite3_step` is
        // synchronous CPU work. The thread takes the connection lock for the
        // whole stream, so no other query interleaves on this connection.
        // Raw pointers are passed as usize because bare SQLite handles are not
        // Send; the guard keeps the handle valid for the thread's lifetime.
        let column_count = metadata.columns.len();
        let flag = cancelled;
        let connection_lock = self.handle.clone();
        let statement_address = statement as usize;
        std::thread::spawn(move || {
            let guard = connection_lock.connection.lock();
            let raw = *guard;
            let statement = statement_address as *mut sqlite3_stmt;
            stream_rows(raw, statement, column_count, &sender, &flag);
            // Finalize on the same thread that stepped the statement.
            unsafe {
                sqlite3_finalize(statement);
            }
            drop(guard);
        });

        Ok(ExecutedQuery {
            handle,
            metadata,
            rows: stream,
        })
    }

    async fn begin(&self) -> DriverResult<TransactionId> {
        self.exec_simple("BEGIN")?;
        let id = self
            .handle
            .transaction_counter
            .fetch_add(1, Ordering::SeqCst);
        Ok(TransactionId(id))
    }

    async fn commit(&self, _transaction: TransactionId) -> DriverResult<()> {
        self.exec_simple("COMMIT")
    }

    async fn rollback(&self, _transaction: TransactionId) -> DriverResult<()> {
        self.exec_simple("ROLLBACK")
    }

    async fn explain(&self, sql: &str) -> DriverResult<ExplainPlan> {
        let explain_sql = format!("EXPLAIN QUERY PLAN {sql}");
        let rows = self.collect_rows(&explain_sql, &[], 4)?;
        let mut nodes = Vec::with_capacity(rows.len());
        for row in rows {
            let id = as_i64(row.get(0));
            let parent = as_i64(row.get(1));
            let detail = row.get(3).map(Value::display).unwrap_or_default();
            nodes.push(ExplainNode { id, parent, detail });
        }
        Ok(ExplainPlan { nodes })
    }

    async fn close(&self) -> DriverResult<()> {
        // Dropping the last Arc closes the connection via SqliteHandle::Drop.
        // Mark closed first so subsequent guard() calls fail with a clear error.
        if self.handle.closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let raw = *self.handle.connection.lock();
        if !raw.is_null() {
            unsafe {
                sqlite3_close(raw);
            }
        }
        Ok(())
    }
}

fn stream_rows(
    raw: *mut sqlite3,
    statement: *mut sqlite3_stmt,
    column_count: usize,
    sender: &async_channel::Sender<Row>,
    flag: &AtomicBool,
) {
    loop {
        if flag.load(Ordering::SeqCst) {
            break;
        }
        let code = unsafe { sqlite3_step(statement) };
        match code {
            SQLITE_ROW => {
                let Ok(row) = read_row(statement, column_count) else {
                    break;
                };
                if sender.send_blocking(row).is_err() {
                    // Receiver dropped: the query was cancelled or the view
                    // closed. Interrupt so a blocked step returns promptly.
                    unsafe {
                        sqlite3_interrupt(raw);
                    }
                    break;
                }
            }
            SQLITE_DONE => break,
            SQLITE_INTERRUPT => break,
            _ => break,
        }
    }
}

fn as_i64(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Integer(number)) => *number,
        _ => 0,
    }
}

impl SqliteConnection {
    fn query_schema_objects(&self) -> DriverResult<Vec<Table>> {
        let rows = self.collect_rows(
            "SELECT name, type FROM sqlite_master WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name",
            &[],
            2,
        )?;
        let mut tables = Vec::with_capacity(rows.len());
        for row in rows {
            let name = row.get(0).map(Value::display).unwrap_or_default();
            let kind = match row.get(1).map(Value::display).as_deref() {
                Some("view") => ObjectKind::View,
                _ => ObjectKind::Table,
            };
            tables.push(Table {
                reference: TableRef::new(name),
                kind,
            });
        }
        Ok(tables)
    }

    fn describe(&self, table: &TableRef) -> DriverResult<TableSchema> {
        let name = &table.name;
        // Identifiers are quoted by doubling embedded quotes; user SQL is never
        // concatenated into a value slot, so this is injection-safe.
        let quoted = quote_identifier(name);
        let column_rows = self.collect_rows(&format!("PRAGMA table_info({quoted})"), &[], 6)?;
        let mut columns = Vec::with_capacity(column_rows.len());
        let mut primary_key_columns = Vec::new();
        for row in &column_rows {
            let column_name = row.get(1).map(Value::display).unwrap_or_default();
            let declared = row.get(2).map(Value::display).unwrap_or_default();
            let not_null = as_i64(row.get(3)) != 0;
            let primary_key_position = as_i64(row.get(5));
            let mut column = Column::new(column_name.clone(), ColumnType::from_declared(&declared));
            column.nullable = !not_null;
            column.primary_key = primary_key_position > 0;
            columns.push(column);
            if primary_key_position > 0 {
                primary_key_columns.push((primary_key_position, column_name));
            }
        }
        primary_key_columns.sort_by_key(|(position, _)| *position);
        let primary_key = if primary_key_columns.is_empty() {
            None
        } else {
            Some(PrimaryKey {
                name: None,
                columns: primary_key_columns
                    .into_iter()
                    .map(|(_, name)| name)
                    .collect(),
            })
        };

        let foreign_key_rows =
            self.collect_rows(&format!("PRAGMA foreign_key_list({quoted})"), &[], 8)?;
        let mut foreign_keys = Vec::new();
        // Group rows by (id, referenced table) so composite keys stay together.
        let mut grouped: std::collections::BTreeMap<(i64, String), Vec<&Row>> =
            std::collections::BTreeMap::new();
        for row in &foreign_key_rows {
            let id = as_i64(row.get(0));
            let referenced_table = row.get(2).map(Value::display).unwrap_or_default();
            grouped.entry((id, referenced_table)).or_default().push(row);
        }
        for ((_, referenced_table), rows) in grouped {
            let mut columns = Vec::new();
            let mut referenced_columns = Vec::new();
            let mut on_delete = None;
            let mut on_update = None;
            for row in rows {
                columns.push(row.get(3).map(Value::display).unwrap_or_default());
                let referenced_column = row.get(4).map(Value::display).unwrap_or_default();
                referenced_columns.push(if referenced_column.is_empty() {
                    // SQLite leaves the referenced column empty when it is the
                    // primary key of the referenced table.
                    String::new()
                } else {
                    referenced_column
                });
                on_delete = Some(row.get(5).map(Value::display).unwrap_or_default());
                on_update = Some(row.get(6).map(Value::display).unwrap_or_default());
            }
            foreign_keys.push(ForeignKey {
                name: None,
                columns,
                referenced_table,
                referenced_columns,
                on_delete,
                on_update,
            });
        }

        let index_list_rows = self.collect_rows(&format!("PRAGMA index_list({quoted})"), &[], 5)?;
        let mut indexes = Vec::new();
        for row in &index_list_rows {
            let index_name = row.get(1).map(Value::display).unwrap_or_default();
            let unique = as_i64(row.get(2)) != 0;
            if index_name.starts_with("sqlite_autoindex_") {
                continue;
            }
            let quoted_index = quote_identifier(&index_name);
            let info_rows =
                self.collect_rows(&format!("PRAGMA index_info({quoted_index})"), &[], 3)?;
            let mut index_columns = Vec::new();
            for info in &info_rows {
                index_columns.push(info.get(2).map(Value::display).unwrap_or_default());
            }
            indexes.push(Index {
                name: index_name,
                columns: index_columns,
                unique,
                definition: None,
            });
        }

        let definition = self
            .collect_rows(
                "SELECT sql FROM sqlite_master WHERE name = ?1",
                &[Value::Text(name.clone())],
                1,
            )?
            .into_iter()
            .next()
            .and_then(|row| row.get(0).map(Value::display));

        Ok(TableSchema {
            reference: table.clone(),
            kind: ObjectKind::Table,
            columns,
            primary_key,
            indexes,
            foreign_keys,
            definition,
        })
    }

    fn exec_simple(&self, sql: &str) -> DriverResult<()> {
        self.with_raw(|raw| {
            let statement = Self::prepare(raw, sql)?;
            let code = unsafe { sqlite3_step(statement) };
            unsafe {
                sqlite3_finalize(statement);
            }
            if code == SQLITE_DONE || code == SQLITE_ROW {
                Ok(())
            } else {
                Err(Self::last_error(raw, code))
            }
        })
    }

    /// Collect a bounded set of rows for schema/EXPLAIN queries. These are
    /// always small and need no streaming.
    fn collect_rows(
        &self,
        sql: &str,
        params: &[Value],
        column_count: usize,
    ) -> DriverResult<Vec<Row>> {
        self.with_raw(|raw| {
            let statement = Self::prepare(raw, sql)?;
            for (position, value) in params.iter().enumerate() {
                bind_value(statement, position as c_int + 1, value)?;
            }
            let mut rows = Vec::new();
            loop {
                let code = unsafe { sqlite3_step(statement) };
                match code {
                    SQLITE_ROW => rows.push(read_row(statement, column_count)?),
                    SQLITE_DONE => break,
                    _ => {
                        let error = Self::last_error(raw, code);
                        unsafe {
                            sqlite3_finalize(statement);
                        }
                        return Err(error);
                    }
                }
            }
            unsafe {
                sqlite3_finalize(statement);
            }
            Ok(rows)
        })
    }
}

fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_config() -> ConnectionConfig {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        ConnectionConfig {
            id: crate::model::ConnectionId(format!(
                "test_{}",
                NEXT_ID.fetch_add(1, Ordering::SeqCst)
            )),
            display_name: "Test".into(),
            driver_id: SQLITE_DRIVER_ID.into(),
            source: DataSource::InMemory {
                name: format!(
                    "db_test_{}_{}",
                    std::process::id(),
                    NEXT_ID.fetch_add(1, Ordering::SeqCst)
                ),
            },
            host: None,
            port: None,
            user: None,
            database: None,
            options: Default::default(),
        }
    }

    #[test]
    fn open_missing_file_is_a_classified_error() {
        let mut config = in_memory_config();
        config.source = DataSource::File {
            path: std::path::PathBuf::from("/definitely/missing/zzz_database_test.db"),
        };
        let result = open_connection(&config);
        assert!(result.is_err());
        assert_eq!(
            result.err().map(|error| error.kind),
            Some(DatabaseErrorKind::Connection)
        );
    }

    #[test]
    fn quote_identifier_escapes_quotes() {
        assert_eq!(quote_identifier("users"), "\"users\"");
        assert_eq!(quote_identifier("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn declared_type_mapping_is_used_for_columns() {
        // Build a tiny in-memory database directly through the driver and
        // verify schema introspection returns real column metadata.
        futures::executor::block_on(async {
            let config = in_memory_config();
            let handle = open_connection(&config).expect("open");
            let connection = SqliteConnection { handle };
            connection
                .exec_simple(
                    "CREATE TABLE customers (id INTEGER PRIMARY KEY, name TEXT NOT NULL, balance REAL)",
                )
                .expect("create");
            connection
                .exec_simple("CREATE INDEX idx_customers_name ON customers(name)")
                .expect("index");
            let catalogs = connection.catalogs().await.expect("catalogs");
            assert_eq!(catalogs.len(), 1);
            let table_names: Vec<&str> = catalogs[0].schemas[0]
                .tables
                .iter()
                .map(|table| table.reference.name.as_str())
                .collect();
            assert!(table_names.contains(&"customers"));

            let schema = connection
                .describe_table(&TableRef::new("customers"))
                .await
                .expect("describe");
            assert_eq!(schema.columns.len(), 3);
            assert_eq!(schema.columns[0].name, "id");
            assert!(schema.columns[0].primary_key);
            assert_eq!(schema.columns[1].declared_type, ColumnType::Text);
            assert!(!schema.columns[1].nullable);
            assert_eq!(schema.columns[2].declared_type, ColumnType::Real);
            assert_eq!(
                schema.primary_key.as_ref().map(|key| key.columns.clone()),
                Some(vec!["id".to_owned()])
            );
            assert_eq!(schema.indexes.len(), 1);
            assert_eq!(schema.indexes[0].columns, vec!["name".to_owned()]);
        });
    }

    #[test]
    fn execute_streams_typed_rows() {
        futures::executor::block_on(async {
            let config = in_memory_config();
            let handle = open_connection(&config).expect("open");
            let connection = SqliteConnection { handle };
            connection
                .exec_simple("CREATE TABLE t (id INTEGER, label TEXT, data BLOB)")
                .expect("create");
            connection
                .exec_simple("INSERT INTO t VALUES (1, 'alpha', x'DEADBEEF')")
                .expect("insert");
            let query = connection
                .execute("SELECT id, label, data, NULL FROM t ORDER BY id", &[])
                .await
                .expect("execute");
            assert_eq!(query.metadata.columns.len(), 4);
            let mut rows = Vec::new();
            while let Some(row) = query.rows.next().await {
                rows.push(row);
            }
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].get(0), Some(&Value::Integer(1)));
            assert_eq!(rows[0].get(1), Some(&Value::Text("alpha".into())));
            assert_eq!(
                rows[0].get(2),
                Some(&Value::Blob(vec![0xde, 0xad, 0xbe, 0xef]))
            );
            assert_eq!(rows[0].get(3), Some(&Value::Null));
        });
    }

    #[test]
    fn cancel_stops_a_long_running_query() {
        // DB-EXP-001: `sqlite3_interrupt` is called from the control path while
        // `sqlite3_step` runs on the streaming thread. A recursive CTE generates
        // a large amount of work; we wait for the first row to prove stepping
        // is in progress before interrupting.
        futures::executor::block_on(async {
            let config = in_memory_config();
            let handle = open_connection(&config).expect("open");
            let connection = SqliteConnection { handle };
            let sql = "WITH RECURSIVE counter(value) AS (
                VALUES(1) UNION ALL SELECT value + 1 FROM counter WHERE value < 100000000
            ) SELECT value FROM counter";
            let query = connection.execute(sql, &[]).await.expect("execute");
            assert!(
                query.rows.next().await.is_some(),
                "query starts producing rows"
            );
            query.handle.cancel();
            assert!(query.handle.is_cancelled());
            let started = std::time::Instant::now();
            let mut row_count = 0usize;
            while query.rows.next().await.is_some() {
                row_count += 1;
                if row_count > 100_000 {
                    break;
                }
            }
            assert!(
                started.elapsed() < std::time::Duration::from_secs(5),
                "interrupt should stop the query promptly"
            );
            // The stream closes after cancellation; cancellation is not an error.
            assert_eq!(query.rows.next().await, None);
        });
    }

    #[test]
    fn transactions_commit_and_rollback() {
        futures::executor::block_on(async {
            let config = in_memory_config();
            let handle = open_connection(&config).expect("open");
            let connection = SqliteConnection { handle };
            connection
                .exec_simple("CREATE TABLE t (value INTEGER)")
                .expect("create");

            let transaction = connection.begin().await.expect("begin");
            connection
                .exec_simple("INSERT INTO t VALUES (1)")
                .expect("insert");
            connection.rollback(transaction).await.expect("rollback");
            assert_eq!(count_rows(&connection, "SELECT value FROM t").await, 0);

            let transaction = connection.begin().await.expect("begin");
            connection
                .exec_simple("INSERT INTO t VALUES (2)")
                .expect("insert");
            connection.commit(transaction).await.expect("commit");
            assert_eq!(count_rows(&connection, "SELECT value FROM t").await, 1);
        });
    }

    #[test]
    fn explain_query_plan_returns_nodes() {
        futures::executor::block_on(async {
            let config = in_memory_config();
            let handle = open_connection(&config).expect("open");
            let connection = SqliteConnection { handle };
            connection
                .exec_simple("CREATE TABLE t (id INTEGER PRIMARY KEY, value TEXT)")
                .expect("create");
            let plan = connection
                .explain("SELECT * FROM t WHERE id = 1")
                .await
                .expect("explain");
            assert!(!plan.nodes.is_empty(), "EXPLAIN QUERY PLAN produces nodes");
            assert!(plan.nodes.iter().any(|node| !node.detail.is_empty()));
        });
    }

    async fn count_rows(connection: &SqliteConnection, sql: &str) -> usize {
        let query = connection.execute(sql, &[]).await.expect("execute");
        let mut count = 0usize;
        while query.rows.next().await.is_some() {
            count += 1;
        }
        count
    }

    #[test]
    fn parameter_binding_prevents_injection() {
        futures::executor::block_on(async {
            let config = in_memory_config();
            let handle = open_connection(&config).expect("open");
            let connection = SqliteConnection { handle };
            connection
                .exec_simple("CREATE TABLE t (value TEXT)")
                .expect("create");
            let malicious = "x'); DROP TABLE t; --";
            let query = connection
                .execute(
                    "INSERT INTO t(value) VALUES (?1)",
                    &[Value::Text(malicious.into())],
                )
                .await
                .expect("execute");
            while query.rows.next().await.is_some() {}
            let query = connection
                .execute("SELECT value FROM t", &[])
                .await
                .expect("select");
            let mut rows = Vec::new();
            while let Some(row) = query.rows.next().await {
                rows.push(row);
            }
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].get(0), Some(&Value::Text(malicious.into())));
            // The table still exists: the payload was bound as data, not SQL.
            assert!(connection.test().await.is_ok());
        });
    }
}
