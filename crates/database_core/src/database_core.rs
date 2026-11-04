//! Protocol and domain model for the relational database explorer.
//!
//! This crate owns no GPUI or workspace types. It stays UI-agnostic so the model,
//! SQL analysis, result-set state machine, and driver protocol can be unit tested
//! without a rendering context. `database_ui` is the only crate that wires these
//! into the editor.

pub mod connection;
pub mod driver;
pub mod error;
pub mod extension_driver;
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
pub use error::{DatabaseError, DatabaseErrorKind};
pub use extension_driver::{ExtensionDriver, ExtensionDriverConfig};
pub use model::{
    Capabilities, Catalog, Column, ColumnType, ConnectionConfig, ConnectionId, ConnectionStatus,
    DataSource, EditValueError, ForeignKey, Index, ObjectKind, PendingEdit, PrimaryKey, ResultSet,
    ResultSetState, Row, Schema, Table, TableRef, TableSchema, Truncation, Value,
    parse_edited_value,
};
pub use session::{
    DEFAULT_WINDOW_LIMIT, ExportFormat, QuerySession, build_update_statements, column_label,
    export_csv, export_result_set,
};
pub use sql::{
    SqlStatement, StatementKind, classify, editable_target, split_statements, statement_at,
};
pub use sqlite::{SQLITE_DRIVER_ID, SqliteConnection, SqliteDriver};
