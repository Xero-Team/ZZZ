//! CSV import for table data.
//!
//! Parsing is deliberately independent of any driver: it turns text into typed
//! values for a known `TableSchema`, and the session writes them with
//! parameterized `INSERT` statements inside a transaction. Nothing here builds
//! SQL from cell text.

use std::fmt;

use crate::model::{EditValueError, TableSchema, Value, parse_edited_value};
use crate::sql::quote_identifier;

/// A parsed CSV document: one header row plus field rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvTable {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// Why an import could not be prepared. These are preparse/typing failures, so
/// the UI can point at the offending row instead of showing a driver error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    /// The document has no header row.
    Empty,
    /// The text ends inside a quoted field.
    UnterminatedQuote,
    FieldCount {
        row: usize,
        expected: usize,
        found: usize,
    },
    /// A header names a column the table does not have.
    UnknownColumn(String),
    /// A column with no CSV header is not nullable, so inserted rows would fail.
    MissingColumn(String),
    /// A cell cannot be converted to its column's type.
    Value {
        row: usize,
        column: String,
        error: EditValueError,
    },
    /// Binary columns have no textual import form (D-07).
    BinaryColumn(String),
}

impl fmt::Display for ImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportError::Empty => formatter.write_str("the CSV file has no header row"),
            ImportError::UnterminatedQuote => {
                formatter.write_str("a quoted CSV field is not terminated")
            }
            ImportError::FieldCount {
                row,
                expected,
                found,
            } => write!(
                formatter,
                "row {row} has {found} fields but the header has {expected}"
            ),
            ImportError::UnknownColumn(column) => {
                write!(formatter, "the table has no column named {column}")
            }
            ImportError::MissingColumn(column) => {
                write!(formatter, "the CSV file has no value for {column}")
            }
            ImportError::Value { row, column, error } => {
                write!(formatter, "row {row} column {column}: {error:?}")
            }
            ImportError::BinaryColumn(column) => {
                write!(formatter, "binary column {column} cannot be imported")
            }
        }
    }
}

impl std::error::Error for ImportError {}

/// Parse RFC 4180 CSV. Accepts `\r\n` and `\n` record separators, doubled quotes
/// inside quoted fields, and a UTF-8 BOM. A trailing newline does not produce an
/// empty row.
pub fn parse_csv(text: &str) -> Result<CsvTable, ImportError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut record_has_content = false;
    let mut characters = text.chars().peekable();

    while let Some(character) = characters.next() {
        if in_quotes {
            match character {
                '"' => {
                    if characters.peek() == Some(&'"') {
                        characters.next();
                        field.push('"');
                    } else {
                        in_quotes = false;
                    }
                }
                other => field.push(other),
            }
            continue;
        }

        match character {
            '"' if field.is_empty() => in_quotes = true,
            ',' => {
                record.push(std::mem::take(&mut field));
                record_has_content = true;
            }
            '\r' if characters.peek() == Some(&'\n') => {}
            '\n' => {
                if record_has_content || !field.is_empty() {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    record_has_content = false;
                }
            }
            other => {
                field.push(other);
                record_has_content = true;
            }
        }
    }

    if in_quotes {
        return Err(ImportError::UnterminatedQuote);
    }
    if record_has_content || !field.is_empty() {
        record.push(field);
        records.push(record);
    }

    let mut records = records.into_iter();
    let headers = records.next().ok_or(ImportError::Empty)?;
    if headers.len() == 1 && headers[0].is_empty() {
        return Err(ImportError::Empty);
    }
    let rows: Vec<Vec<String>> = records.collect();
    for (index, row) in rows.iter().enumerate() {
        if row.len() != headers.len() {
            return Err(ImportError::FieldCount {
                // Header is row 1, so data row 0 is row 2 in the file.
                row: index + 2,
                expected: headers.len(),
                found: row.len(),
            });
        }
    }
    Ok(CsvTable { headers, rows })
}

/// The rows an import would insert, in table column order.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedImport {
    /// Column names in the order the `INSERT` statements bind them.
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

/// Map CSV cells onto the table's columns and convert each cell to a typed
/// value. Every table column must be present in the header unless it is
/// nullable; a header for a column the table does not have is an error rather
/// than silently ignored data.
pub fn prepare_import(schema: &TableSchema, csv: &CsvTable) -> Result<PreparedImport, ImportError> {
    for header in &csv.headers {
        if !schema.columns.iter().any(|column| &column.name == header) {
            return Err(ImportError::UnknownColumn(header.clone()));
        }
    }
    for column in &schema.columns {
        if !csv.headers.iter().any(|header| header == &column.name) {
            if column.nullable {
                continue;
            }
            return Err(ImportError::MissingColumn(column.name.clone()));
        }
    }

    // Bind in table column order so the generated INSERT lists every column the
    // file provides, independent of the header order.
    let mut bindings: Vec<(usize, &str)> = Vec::new();
    for (column_index, column) in schema.columns.iter().enumerate() {
        if let Some(position) = csv.headers.iter().position(|header| header == &column.name) {
            bindings.push((column_index, csv.headers[position].as_str()));
        }
    }
    let columns: Vec<String> = bindings
        .iter()
        .map(|(_, name)| (*name).to_owned())
        .collect();

    let mut rows = Vec::with_capacity(csv.rows.len());
    for (index, row) in csv.rows.iter().enumerate() {
        let mut values = Vec::with_capacity(bindings.len());
        for (column_index, name) in &bindings {
            let column = &schema.columns[*column_index];
            if matches!(column.declared_type, crate::model::ColumnType::Blob) {
                return Err(ImportError::BinaryColumn((*name).to_owned()));
            }
            let cell = csv
                .headers
                .iter()
                .position(|header| header == name)
                .and_then(|position| row.get(position))
                .map_or("", String::as_str);
            let value = parse_edited_value(&column.declared_type, cell).map_err(|error| {
                ImportError::Value {
                    row: index + 2,
                    column: (*name).to_owned(),
                    error,
                }
            })?;
            values.push(value);
        }
        rows.push(values);
    }
    Ok(PreparedImport { columns, rows })
}

/// Build the parameterized `INSERT` for one row. Column names are quoted; values
/// are always bound, so CSV text can never become SQL.
pub fn insert_statement(table: &str, columns: &[String]) -> String {
    let quoted_columns: Vec<String> = columns
        .iter()
        .map(|column| quote_identifier(column))
        .collect();
    format!(
        "INSERT INTO {table} ({}) VALUES ({})",
        quoted_columns.join(", "),
        vec!["?"; columns.len()].join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Column, ColumnType, ObjectKind, TableRef};

    fn schema() -> TableSchema {
        let mut id = Column::new("id", ColumnType::Integer);
        id.nullable = false;
        let mut name = Column::new("name", ColumnType::Text);
        name.nullable = false;
        let mut score = Column::new("score", ColumnType::Real);
        score.nullable = false;
        TableSchema {
            reference: TableRef::new("people"),
            kind: ObjectKind::Table,
            columns: vec![id, name, score],
            primary_key: None,
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            definition: None,
        }
    }

    #[test]
    fn parses_quoted_fields_embedded_newlines_and_bom() {
        let csv = parse_csv(
            "\u{feff}id,name\r\n1,\"A, B\"\r\n2,\"line\nbreak\"\r\n3,\"say \"\"hi\"\"\"\r\n",
        )
        .expect("parse");
        assert_eq!(csv.headers, vec!["id", "name"]);
        assert_eq!(csv.rows[0], vec!["1", "A, B"]);
        assert_eq!(csv.rows[1], vec!["2", "line\nbreak"]);
        assert_eq!(csv.rows[2], vec!["3", "say \"hi\""]);
    }

    #[test]
    fn parses_empty_trailing_fields_and_rejects_ragged_rows() {
        let csv = parse_csv("id,name\n1,\n").expect("parse");
        assert_eq!(csv.rows[0], vec!["1", ""]);

        let error = parse_csv("id,name\n1\n").expect_err("ragged row");
        assert_eq!(
            error,
            ImportError::FieldCount {
                row: 2,
                expected: 2,
                found: 1
            }
        );
    }

    #[test]
    fn rejects_an_unterminated_quote_and_an_empty_document() {
        assert_eq!(
            parse_csv("id,name\n1,\"open\n").expect_err("unterminated"),
            ImportError::UnterminatedQuote
        );
        assert_eq!(parse_csv("").expect_err("empty"), ImportError::Empty);
        assert_eq!(parse_csv("\n\n").expect_err("empty"), ImportError::Empty);
    }

    #[test]
    fn prepare_import_orders_values_by_table_columns() {
        let csv = parse_csv("score,name,id\n1.5,Alice,1\n").expect("parse");
        let prepared = prepare_import(&schema(), &csv).expect("prepare");
        assert_eq!(prepared.columns, vec!["id", "name", "score"]);
        assert_eq!(
            prepared.rows[0],
            vec![
                Value::Integer(1),
                Value::Text("Alice".into()),
                Value::Real(1.5)
            ]
        );
    }

    #[test]
    fn prepare_import_rejects_unknown_missing_and_unparseable_cells() {
        let csv = parse_csv("id,unknown\n1,x\n").expect("parse");
        assert_eq!(
            prepare_import(&schema(), &csv).expect_err("unknown column"),
            ImportError::UnknownColumn("unknown".to_owned())
        );

        // `name` and `score` are required because the test schema declares them
        // non-nullable; a file without them would insert NULLs, so refuse it.
        let csv = parse_csv("id\n1\n").expect("parse");
        assert_eq!(
            prepare_import(&schema(), &csv).expect_err("missing column"),
            ImportError::MissingColumn("name".to_owned())
        );

        let csv = parse_csv("id,name,score\nx,Alice,1\n").expect("parse");
        let error = prepare_import(&schema(), &csv).expect_err("bad integer");
        assert!(matches!(error, ImportError::Value { row: 2, .. }));
    }

    #[test]
    fn nullable_columns_may_be_absent_from_the_file() {
        let mut schema = schema();
        schema.columns[2].nullable = true;
        let csv = parse_csv("id,name\n1,Alice\n").expect("parse");
        let prepared = prepare_import(&schema, &csv).expect("prepare");
        assert_eq!(prepared.columns, vec!["id", "name"]);
    }

    #[test]
    fn insert_statement_quotes_columns_and_binds_values() {
        assert_eq!(
            insert_statement("\"people\"", &["id".to_owned(), "a\"b".to_owned()]),
            "INSERT INTO \"people\" (\"id\", \"a\"\"b\") VALUES (?, ?)"
        );
    }
}
