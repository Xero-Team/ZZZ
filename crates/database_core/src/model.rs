//! Domain model for relational data sources and result sets.
//!
//! A `ResultSet` is a distinct domain object from `csv_preview::TableLikeContent`:
//! it owns a state machine, a typed value system, a windowed row cache, edit
//! tracking, and a streaming cursor. None of those exist on a static text table.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A typed cell value. Values keep their runtime type so sorting, editing, and
/// export do not degrade everything to strings (D-07).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
    /// ISO-8601 calendar date (`YYYY-MM-DD`).
    Date(String),
    /// ISO-8601 timestamp.
    DateTime(String),
    /// ISO-8601 time-of-day.
    Time(String),
    Bool(bool),
    /// Arbitrary-precision decimal kept as text to avoid binary float drift.
    Numeric(String),
    /// A JSON document kept as its canonical text form.
    Json(String),
}

impl Value {
    /// Whether the value is SQL NULL.
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Render for display. BLOBs are never inlined as binary (D-07): they show a
    /// length and a short hex digest.
    pub fn display(&self) -> String {
        match self {
            Value::Null => "NULL".to_owned(),
            Value::Integer(number) => number.to_string(),
            Value::Real(number) => format_number(*number),
            Value::Text(text) => text.clone(),
            Value::Blob(bytes) => blob_digest(bytes),
            Value::Date(value) | Value::DateTime(value) | Value::Time(value) => value.clone(),
            Value::Bool(value) => value.to_string(),
            Value::Numeric(value) | Value::Json(value) => value.clone(),
        }
    }

    /// The canonical text used when copying a cell or exporting to CSV.
    pub fn to_export_text(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Blob(bytes) => blob_digest(bytes),
            other => other.display(),
        }
    }

    /// Render as JSON for export. BLOBs stay a digest string (D-07): binary data
    /// is never inlined. Numeric text is emitted as a string to preserve
    /// precision that a JSON number would round.
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::Value as Json;
        match self {
            Value::Null => Json::Null,
            Value::Integer(number) => Json::from(*number),
            Value::Real(number) => serde_json::Number::from_f64(*number)
                .map_or_else(|| Json::String(self.display()), Json::Number),
            Value::Text(text) => Json::String(text.clone()),
            Value::Blob(bytes) => Json::String(blob_digest(bytes)),
            Value::Date(value) | Value::DateTime(value) | Value::Time(value) => {
                Json::String(value.clone())
            }
            Value::Bool(value) => Json::Bool(*value),
            Value::Numeric(value) => Json::String(value.clone()),
            Value::Json(text) => {
                serde_json::from_str(text).unwrap_or_else(|_| Json::String(text.clone()))
            }
        }
    }

    /// Ordering key used by client-side sorting. NULLs sort first, then numbers,
    /// booleans, and finally text (by Unicode scalar order).
    pub fn sort_key(&self) -> (u8, String) {
        match self {
            Value::Null => (0, String::new()),
            Value::Integer(number) => (
                1,
                format!("{:020}", (*number as i128) + i128::from(i64::MAX) + 1),
            ),
            Value::Real(number) => (1, format!("{:030.15}", number)),
            Value::Bool(value) => (2, if *value { "1" } else { "0" }.to_owned()),
            Value::Numeric(value) => (1, value.clone()),
            other => (3, other.display()),
        }
    }
}

fn format_number(number: f64) -> String {
    if number.is_nan() {
        "NaN".to_owned()
    } else if number.is_infinite() {
        if number.is_sign_negative() {
            "-Inf"
        } else {
            "Inf"
        }
        .to_owned()
    } else {
        let text = number.to_string();
        // SQLite reports integral REALs as `3.0`; keep the decimal point so the
        // value is not silently re-typed as an integer on edit round-trip.
        if !text.contains('.') && !text.contains('e') && !text.contains('E') {
            format!("{text}.0")
        } else {
            text
        }
    }
}

fn blob_digest(bytes: &[u8]) -> String {
    const PREVIEW_BYTES: usize = 8;
    let preview: String = bytes
        .iter()
        .take(PREVIEW_BYTES)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let suffix = if bytes.len() > PREVIEW_BYTES {
        "…".to_owned()
    } else {
        String::new()
    };
    format!("[blob {}B: {preview}{suffix}]", bytes.len())
}

impl fmt::Display for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.display())
    }
}

/// The declared type of a column, kept separately from each cell's runtime type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum ColumnType {
    Integer,
    Real,
    Text,
    Blob,
    Date,
    DateTime,
    Time,
    Bool,
    Numeric {
        precision: Option<u32>,
        scale: Option<u32>,
    },
    Json,
    /// A driver-specific type that has no common representation yet.
    Other {
        declared: String,
    },
}

impl ColumnType {
    /// Map a SQLite declared type to the closest common type (SQLite type
    /// affinity rules, plus conventional DATE/JSON/BOOL/NUMERIC names).
    pub fn from_declared(declared: &str) -> Self {
        let normalized = declared.trim().to_ascii_uppercase();
        if normalized.contains("INT") {
            ColumnType::Integer
        } else if normalized.contains("CHAR")
            || normalized.contains("CLOB")
            || normalized.contains("TEXT")
        {
            ColumnType::Text
        } else if normalized.contains("BLOB") || normalized.is_empty() {
            ColumnType::Blob
        } else if normalized.contains("REAL")
            || normalized.contains("FLOA")
            || normalized.contains("DOUB")
        {
            ColumnType::Real
        } else if normalized == "DATE" {
            ColumnType::Date
        } else if normalized.contains("DATETIME") || normalized.contains("TIMESTAMP") {
            ColumnType::DateTime
        } else if normalized == "TIME" {
            ColumnType::Time
        } else if normalized == "BOOL" || normalized == "BOOLEAN" {
            ColumnType::Bool
        } else if normalized.contains("DECIMAL") || normalized.contains("NUMERIC") {
            ColumnType::Numeric {
                precision: None,
                scale: None,
            }
        } else if normalized == "JSON" {
            ColumnType::Json
        } else {
            ColumnType::Other {
                declared: declared.to_owned(),
            }
        }
    }
}

/// Metadata for one result-set or table column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Column {
    pub name: String,
    pub declared_type: ColumnType,
    pub nullable: bool,
    pub primary_key: bool,
}

impl Column {
    pub fn new(name: impl Into<String>, declared_type: ColumnType) -> Self {
        Self {
            name: name.into(),
            declared_type,
            nullable: true,
            primary_key: false,
        }
    }
}

/// One result row, positionally aligned with `ResultSet::columns`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Row(pub Vec<Value>);

impl Row {
    pub fn get(&self, index: usize) -> Option<&Value> {
        self.0.get(index)
    }
}

/// The lifecycle of a result set. Cancellation is a successful terminal state,
/// not a failure (D-08).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultSetState {
    Running,
    Ready,
    Cancelled,
    Failed,
    Closed,
}

/// A pending cell edit, keyed by row and column. It is staged until an explicit
/// commit (D-05); rollback drops all entries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingEdit {
    pub row_index: usize,
    pub column_index: usize,
    pub value: Value,
}

/// Why a row stream stopped before reaching its end. The UI turns this into a
/// localized message; the model keeps only the numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Truncation {
    /// The row budget that stopped the pull.
    pub row_limit: usize,
}

/// Why text typed into a cell editor cannot become a typed value. The UI maps
/// these to localized messages; the model never embeds user-facing prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditValueError {
    /// Binary data has no textual editing form in the grid (D-07).
    BinaryNotEditable,
    InvalidInteger,
    InvalidReal,
    InvalidBool,
}

/// Convert the text of the cell editor into a typed value for `column_type`.
///
/// The literal `NULL` (any case) always means SQL NULL. Empty text means NULL
/// for non-text types and the empty string for text-like types, matching what a
/// user expects in each case. Binary columns cannot be edited here.
pub fn parse_edited_value(column_type: &ColumnType, text: &str) -> Result<Value, EditValueError> {
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("NULL") {
        return Ok(Value::Null);
    }
    match column_type {
        ColumnType::Blob => Err(EditValueError::BinaryNotEditable),
        ColumnType::Integer => {
            if trimmed.is_empty() {
                return Ok(Value::Null);
            }
            trimmed
                .parse::<i64>()
                .map(Value::Integer)
                .map_err(|_| EditValueError::InvalidInteger)
        }
        ColumnType::Real => {
            if trimmed.is_empty() {
                return Ok(Value::Null);
            }
            trimmed
                .parse::<f64>()
                .map(Value::Real)
                .map_err(|_| EditValueError::InvalidReal)
        }
        ColumnType::Numeric { .. } => {
            if trimmed.is_empty() {
                return Ok(Value::Null);
            }
            // Keep the literal text: precision beyond f64 must survive the
            // round trip to the driver.
            if trimmed.parse::<f64>().is_err() {
                return Err(EditValueError::InvalidReal);
            }
            Ok(Value::Numeric(trimmed.to_owned()))
        }
        ColumnType::Bool => {
            if trimmed.is_empty() {
                return Ok(Value::Null);
            }
            match trimmed.to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" => Ok(Value::Bool(true)),
                "false" | "0" | "no" => Ok(Value::Bool(false)),
                _ => Err(EditValueError::InvalidBool),
            }
        }
        ColumnType::Date => Ok(if trimmed.is_empty() {
            Value::Null
        } else {
            Value::Date(trimmed.to_owned())
        }),
        ColumnType::DateTime => Ok(if trimmed.is_empty() {
            Value::Null
        } else {
            Value::DateTime(trimmed.to_owned())
        }),
        ColumnType::Time => Ok(if trimmed.is_empty() {
            Value::Null
        } else {
            Value::Time(trimmed.to_owned())
        }),
        ColumnType::Text | ColumnType::Json | ColumnType::Other { .. } => {
            Ok(Value::Text(text.to_owned()))
        }
    }
}

/// An independent, stateful result set (D-06). Rows are cached by window; the
/// bounded stream fills that cache without materializing the whole query result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultSet {
    pub columns: Vec<Column>,
    pub state: ResultSetState,
    /// Whether this result set may be edited. Derived from driver capabilities
    /// and the shape of the query (a primary key must be present).
    pub editable: bool,
    /// Names of the columns that form the primary key for edit targeting.
    pub primary_key_columns: Vec<String>,
    /// Windowed row cache. A window is a contiguous slice of the streamed rows.
    pub rows: Vec<Row>,
    /// Total rows reported by the driver, when known.
    pub total_rows: Option<usize>,
    /// Staged edits, not yet written to the database.
    pub pending_edits: BTreeMap<(usize, usize), Value>,
    /// Whether more rows can be pulled from the stream.
    pub exhausted: bool,
    /// Set when the row budget stopped the pull before the stream ended. The UI
    /// localizes this, so the model stores the limit rather than a message.
    pub truncation: Option<Truncation>,
    /// The SQL that produced this result, for history and re-run.
    pub sql: String,
}

impl ResultSet {
    /// Build an empty running result set from column metadata.
    pub fn new(columns: Vec<Column>, sql: impl Into<String>) -> Self {
        let primary_key_columns = columns
            .iter()
            .filter(|column| column.primary_key)
            .map(|column| column.name.clone())
            .collect();
        Self {
            columns,
            state: ResultSetState::Running,
            editable: false,
            primary_key_columns,
            rows: Vec::new(),
            total_rows: None,
            pending_edits: BTreeMap::new(),
            exhausted: false,
            truncation: None,
            sql: sql.into(),
        }
    }

    /// Append a row to the window cache, respecting `max_rows`. Returns false
    /// when the budget is reached so the caller can stop pulling.
    pub fn push_row(&mut self, row: Row, max_rows: usize) -> bool {
        if self.rows.len() >= max_rows {
            self.exhausted = true;
            self.truncation = Some(Truncation {
                row_limit: max_rows,
            });
            return false;
        }
        self.rows.push(row);
        true
    }

    /// Stage a cell edit. Returns false when the result set is not editable.
    pub fn stage_edit(&mut self, row_index: usize, column_index: usize, value: Value) -> bool {
        if !self.editable || self.rows.get(row_index).is_none() {
            return false;
        }
        self.pending_edits.insert((row_index, column_index), value);
        true
    }

    /// The effective value of a cell: the staged edit if any, else the cached row.
    pub fn value_at(&self, row_index: usize, column_index: usize) -> Option<&Value> {
        self.pending_edits
            .get(&(row_index, column_index))
            .or_else(|| self.rows.get(row_index)?.get(column_index))
    }

    pub fn pending_edit_count(&self) -> usize {
        self.pending_edits.len()
    }

    pub fn clear_pending_edits(&mut self) {
        self.pending_edits.clear();
    }

    /// Mark the set ready. `editable` is only granted when a primary key was
    /// discovered, because edits must target a specific row.
    pub fn mark_ready(&mut self, capabilities: &Capabilities) {
        self.state = ResultSetState::Ready;
        self.exhausted = true;
        self.editable = capabilities.row_editing && !self.primary_key_columns.is_empty();
    }

    pub fn mark_cancelled(&mut self) {
        self.state = ResultSetState::Cancelled;
    }

    pub fn mark_failed(&mut self) {
        self.state = ResultSetState::Failed;
    }

    pub fn mark_closed(&mut self) {
        self.state = ResultSetState::Closed;
        self.rows.clear();
        self.pending_edits.clear();
    }
}

/// Capabilities a driver declares. The UI derives action availability from this
/// struct only, never from `driver.id()` (invariant 1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub transactions: bool,
    pub row_editing: bool,
    pub explain: bool,
    pub er_diagram: bool,
    pub cancel: bool,
    pub streaming: bool,
    /// The driver authenticates connections, so the UI must collect a secret
    /// and store it in the keychain rather than in settings.
    #[serde(default)]
    pub requires_credentials: bool,
    pub multiple_catalogs: bool,
    pub multiple_schemas: bool,
    pub views: bool,
    pub foreign_keys: bool,
    pub indexes: bool,
}

impl Capabilities {
    /// The full capability set of the built-in SQLite driver.
    pub fn sqlite() -> Self {
        Self {
            transactions: true,
            row_editing: true,
            explain: true,
            er_diagram: true,
            cancel: true,
            streaming: true,
            requires_credentials: false,
            multiple_catalogs: false,
            multiple_schemas: false,
            views: true,
            foreign_keys: true,
            indexes: true,
        }
    }

    /// A conservative set for drivers that declare nothing.
    pub fn read_only() -> Self {
        Self {
            transactions: false,
            row_editing: false,
            explain: false,
            er_diagram: false,
            cancel: false,
            streaming: false,
            requires_credentials: false,
            multiple_catalogs: false,
            multiple_schemas: false,
            views: false,
            foreign_keys: false,
            indexes: false,
        }
    }
}

/// Whether a schema node is a table or a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Table,
    View,
}

/// A primary key: an ordered list of column names (composite keys supported).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimaryKey {
    pub name: Option<String>,
    pub columns: Vec<String>,
}

/// An index on a table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    /// The original SQL of a partial/expression index, when available.
    pub definition: Option<String>,
}

/// A foreign key: local columns reference columns on another table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignKey {
    pub name: Option<String>,
    pub columns: Vec<String>,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
    pub on_delete: Option<String>,
    pub on_update: Option<String>,
}

/// A reference to a table used to address it across catalog/schema boundaries,
/// without assuming every driver has all three levels.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TableRef {
    pub catalog: Option<String>,
    pub schema: Option<String>,
    pub name: String,
}

impl TableRef {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            catalog: None,
            schema: None,
            name: name.into(),
        }
    }

    /// `catalog.schema.name`, omitting the parts that are not set. Used for
    /// display and export; SQL identifiers go through the driver's quoting.
    pub fn qualified_name(&self) -> String {
        let mut parts: Vec<&str> = Vec::with_capacity(3);
        parts.extend(
            [&self.catalog, &self.schema]
                .into_iter()
                .flatten()
                .map(String::as_str),
        );
        parts.push(&self.name);
        parts.join(".")
    }
}

/// Summary of a table in the schema tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Table {
    pub reference: TableRef,
    pub kind: ObjectKind,
}

/// Full structure of a table (columns, primary key, indexes, foreign keys).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableSchema {
    pub reference: TableRef,
    pub kind: ObjectKind,
    pub columns: Vec<Column>,
    pub primary_key: Option<PrimaryKey>,
    pub indexes: Vec<Index>,
    pub foreign_keys: Vec<ForeignKey>,
    pub definition: Option<String>,
}

/// A schema containing its tables and views.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schema {
    pub name: String,
    pub tables: Vec<Table>,
}

/// A catalog containing its schemas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    pub name: String,
    pub schemas: Vec<Schema>,
}

/// A stable, non-secret identifier for a saved connection.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ConnectionId(pub String);

/// The non-sensitive connection configuration persisted in settings.json.
/// Passwords and tokens never live here (D-09).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub id: ConnectionId,
    pub display_name: String,
    pub driver_id: String,
    /// How the target is reached. `File` keeps a `PathBuf` so remote paths are
    /// not assumed to be local (invariant 7); resolution is the host's job.
    pub source: DataSource,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub database: Option<String>,
    /// Extra driver-specific, non-secret options.
    pub options: BTreeMap<String, String>,
}

impl ConnectionConfig {
    /// The key under which credentials are stored in the keychain. It contains
    /// no secret and is stable across renames of the display name.
    pub fn credential_key(&self) -> String {
        format!("zzz-database:{}", self.id.0)
    }
}

/// The target of a connection. Kept as data, not a resolved path, so the host
/// can decide how to reach a remote project file (D-10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum DataSource {
    File { path: PathBuf },
    Server { host: String, port: u16 },
    InMemory { name: String },
}

/// Runtime connection state. Never persisted (invariant 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Querying,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_type_affinity() {
        assert_eq!(ColumnType::from_declared("INTEGER"), ColumnType::Integer);
        assert_eq!(ColumnType::from_declared("VARCHAR(255)"), ColumnType::Text);
        assert_eq!(
            ColumnType::from_declared("DECIMAL(10,2)"),
            ColumnType::Numeric {
                precision: None,
                scale: None
            }
        );
        assert_eq!(ColumnType::from_declared("TIMESTAMP"), ColumnType::DateTime);
        assert_eq!(ColumnType::from_declared(""), ColumnType::Blob);
        assert!(matches!(
            ColumnType::from_declared("GEOMETRY"),
            ColumnType::Other { .. }
        ));
    }

    #[test]
    fn blob_is_never_inlined() {
        let value = Value::Blob(vec![0xde, 0xad, 0xbe, 0xef, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        let display = value.display();
        assert!(display.starts_with("[blob 13B: deadbeef"));
        assert!(display.contains('…'));
        assert_eq!(value.to_export_text(), display);
    }

    #[test]
    fn null_exports_as_empty_string() {
        assert_eq!(Value::Null.display(), "NULL");
        assert_eq!(Value::Null.to_export_text(), "");
    }

    #[test]
    fn real_keeps_decimal_point() {
        assert_eq!(Value::Real(3.0).display(), "3.0");
    }

    #[test]
    fn result_set_bounds_rows() {
        let mut set = ResultSet::new(vec![Column::new("id", ColumnType::Integer)], "SELECT 1");
        assert!(set.push_row(Row(vec![Value::Integer(1)]), 1));
        assert!(!set.push_row(Row(vec![Value::Integer(2)]), 1));
        assert_eq!(set.truncation, Some(Truncation { row_limit: 1 }));
        assert_eq!(set.rows.len(), 1);
    }

    #[test]
    fn edited_text_is_parsed_by_column_type() {
        assert_eq!(
            parse_edited_value(&ColumnType::Integer, "42"),
            Ok(Value::Integer(42))
        );
        assert_eq!(
            parse_edited_value(&ColumnType::Integer, "  -7 "),
            Ok(Value::Integer(-7))
        );
        assert_eq!(
            parse_edited_value(&ColumnType::Real, "1.5"),
            Ok(Value::Real(1.5))
        );
        assert_eq!(
            parse_edited_value(&ColumnType::Bool, "YES"),
            Ok(Value::Bool(true))
        );
        assert_eq!(
            parse_edited_value(&ColumnType::Bool, "0"),
            Ok(Value::Bool(false))
        );
        assert_eq!(
            parse_edited_value(&ColumnType::Text, " a b "),
            Ok(Value::Text(" a b ".into()))
        );
        assert_eq!(
            parse_edited_value(&ColumnType::Json, "{\"a\": 1}"),
            Ok(Value::Text("{\"a\": 1}".into()))
        );
        assert_eq!(
            parse_edited_value(&ColumnType::Date, "2026-01-02"),
            Ok(Value::Date("2026-01-02".into()))
        );
        assert_eq!(
            parse_edited_value(
                &ColumnType::Numeric {
                    precision: None,
                    scale: None
                },
                "12345678901234567890.123456789"
            ),
            Ok(Value::Numeric("12345678901234567890.123456789".into()))
        );
    }

    #[test]
    fn edited_null_and_empty_values_are_explicit() {
        assert_eq!(
            parse_edited_value(&ColumnType::Integer, "null"),
            Ok(Value::Null)
        );
        assert_eq!(
            parse_edited_value(&ColumnType::Integer, ""),
            Ok(Value::Null)
        );
        assert_eq!(
            parse_edited_value(&ColumnType::Text, ""),
            Ok(Value::Text(String::new()))
        );
        assert_eq!(parse_edited_value(&ColumnType::Date, ""), Ok(Value::Null));
    }

    #[test]
    fn edited_values_reject_unparseable_input_and_binary_columns() {
        assert_eq!(
            parse_edited_value(&ColumnType::Integer, "12x"),
            Err(EditValueError::InvalidInteger)
        );
        assert_eq!(
            parse_edited_value(&ColumnType::Real, "infinity!"),
            Err(EditValueError::InvalidReal)
        );
        assert_eq!(
            parse_edited_value(&ColumnType::Bool, "maybe"),
            Err(EditValueError::InvalidBool)
        );
        assert_eq!(
            parse_edited_value(&ColumnType::Blob, "00ff"),
            Err(EditValueError::BinaryNotEditable)
        );
    }

    #[test]
    fn edits_require_primary_key_and_capability() {
        let mut column = Column::new("id", ColumnType::Integer);
        column.primary_key = true;
        let mut set = ResultSet::new(vec![column], "SELECT id FROM t");
        set.push_row(Row(vec![Value::Integer(1)]), 100);
        set.mark_ready(&Capabilities::sqlite());
        assert!(set.editable);
        assert!(set.stage_edit(0, 0, Value::Integer(9)));
        assert_eq!(set.value_at(0, 0), Some(&Value::Integer(9)));
        set.clear_pending_edits();
        assert_eq!(set.value_at(0, 0), Some(&Value::Integer(1)));

        let mut without_key = ResultSet::new(
            vec![Column::new("id", ColumnType::Integer)],
            "SELECT id FROM t",
        );
        without_key.mark_ready(&Capabilities::sqlite());
        assert!(!without_key.editable);
        assert!(!without_key.stage_edit(0, 0, Value::Integer(9)));
    }

    #[test]
    fn cancel_is_not_a_failure() {
        let mut set = ResultSet::new(vec![], "SELECT 1");
        set.mark_cancelled();
        assert_eq!(set.state, ResultSetState::Cancelled);
    }

    #[test]
    fn sort_key_orders_nulls_and_numbers() {
        let mut values = [
            Value::Integer(10),
            Value::Null,
            Value::Integer(2),
            Value::Text("a".into()),
        ];
        values.sort_by_key(Value::sort_key);
        assert_eq!(values[0], Value::Null);
        assert_eq!(values[1], Value::Integer(2));
        assert_eq!(values[2], Value::Integer(10));
    }

    #[test]
    fn config_has_no_secret_fields() -> anyhow::Result<()> {
        let serialized = serde_json::to_string(&ConnectionConfig {
            id: ConnectionId("local".into()),
            display_name: "Local".into(),
            driver_id: "sqlite".into(),
            source: DataSource::InMemory {
                name: "test".into(),
            },
            host: None,
            port: None,
            user: None,
            database: None,
            options: BTreeMap::new(),
        })?;
        assert!(!serialized.to_ascii_lowercase().contains("password"));
        assert!(!serialized.to_ascii_lowercase().contains("token"));
        Ok(())
    }
}
