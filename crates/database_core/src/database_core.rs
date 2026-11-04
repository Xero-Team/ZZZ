//! Protocol and domain model for the relational database explorer.
//!
//! This crate owns no GPUI or workspace types. It stays UI-agnostic so the model,
//! SQL analysis, result-set state machine, and driver protocol can be unit tested
//! without a rendering context. `database_ui` is the only crate that wires these
//! into the editor.

pub mod error;
pub mod model;
pub mod sql;

pub use error::{DatabaseError, DatabaseErrorKind};
pub use model::{
    Capabilities, Catalog, Column, ColumnType, ConnectionConfig, ConnectionId, ConnectionStatus,
    DataSource, ForeignKey, Index, ObjectKind, PrimaryKey, ResultSet, ResultSetState, Row, Schema,
    Table, TableRef, TableSchema, Value,
};
pub use sql::{SqlStatement, StatementKind, split_statements};
