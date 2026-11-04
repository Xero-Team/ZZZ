//! Protocol and domain model for the relational database explorer.
//!
//! This crate owns no GPUI or workspace types. It stays UI-agnostic so the model,
//! SQL analysis, result-set state machine, and driver protocol can be unit tested
//! without a rendering context. `database_ui` is the only crate that wires these
//! into the editor.

pub mod connection;
pub mod driver;
pub mod er_diagram;
pub mod error;
pub mod export;
pub mod extension_driver;
pub mod import;
pub mod model;
pub mod session;
pub mod sql;
pub mod sqlite;

pub use connection::ConnectionManager;
pub use driver::{
    DatabaseDriver, DriverConnection, DriverResult, ExecutedQuery, ExplainNode, ExplainPlan,
    Interrupt, QueryHandle, QueryMetadata, ROW_STREAM_CAPACITY, RowStream, TransactionId,
    result_set_for,
};
pub use er_diagram::{diagrammable, er_diagram};
pub use error::{DatabaseError, DatabaseErrorKind};
pub use export::{ExportFormat, SchemaExportFormat, export_csv, export_result_set, export_schema};
pub use extension_driver::{ExtensionDriver, ExtensionDriverConfig};
pub use import::{
    CsvTable, ImportError, PreparedImport, insert_statement, parse_csv, prepare_import,
};
pub use model::{
    Capabilities, Catalog, Column, ColumnType, ConnectionConfig, ConnectionId, ConnectionStatus,
    DataSource, EditValueError, ForeignKey, Index, ObjectKind, PendingEdit, PrimaryKey, ResultSet,
    ResultSetState, Row, Schema, Table, TableRef, TableSchema, Truncation, Value,
    parse_edited_value,
};
pub use session::{
    DEFAULT_WINDOW_LIMIT, ImportFailure, ImportOutcome, QuerySession, build_update_statements,
    column_label,
};
pub use sql::{
    SqlStatement, StatementKind, classify, editable_target, quote_identifier, quote_table,
    split_statements, statement_at,
};
pub use sqlite::{SQLITE_DRIVER_ID, SqliteConnection, SqliteDriver};
