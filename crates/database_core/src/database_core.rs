//! Protocol and domain model for the relational database explorer.
//!
//! This crate owns no GPUI or workspace types. It stays UI-agnostic so the model,
//! SQL analysis, result-set state machine, and driver protocol can be unit tested
//! without a rendering context. `database_ui` is the only crate that wires these
//! into the editor.

pub mod driver;
pub mod error;
pub mod model;
pub mod sql;
pub mod sqlite;

pub use driver::{
    DatabaseDriver, DriverConnection, DriverResult, ExecutedQuery, ExplainNode, ExplainPlan,
    Interrupt, QueryHandle, QueryMetadata, ROW_STREAM_CAPACITY, RowStream, TransactionId,
    result_set_for,
};
pub use error::{DatabaseError, DatabaseErrorKind};
pub use model::{
    Capabilities, Catalog, Column, ColumnType, ConnectionConfig, ConnectionId, ConnectionStatus,
    DataSource, ForeignKey, Index, ObjectKind, PendingEdit, PrimaryKey, ResultSet, ResultSetState,
    Row, Schema, Table, TableRef, TableSchema, Value,
};
pub use sql::{SqlStatement, StatementKind, classify, split_statements};
pub use sqlite::{SQLITE_DRIVER_ID, SqliteConnection, SqliteDriver};
