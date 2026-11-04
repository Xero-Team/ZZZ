//! Text and JSON export for result sets and described schemas.
//!
//! Export is pure: it reads a `ResultSet` (with staged edits applied) or a list
//! of `TableSchema` values and returns a string. Nothing here touches a driver,
//! so the grid and the schema panel can offer the same formats without
//! duplicating formatting rules.

use serde_json::{Map, Value as Json, json};

use crate::model::{ResultSet, TableSchema};

/// Clipboard export formats offered by the result grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Markdown,
    Html,
    /// An array of objects keyed by column name, so another tool can consume the
    /// result without re-parsing a table.
    Json,
}

/// Formats available for schema export. Markdown and HTML are display formats,
/// which a schema has no natural table shape for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaExportFormat {
    Csv,
    Json,
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
        ExportFormat::Json => export_json_rows(result_set, order),
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

fn export_json_rows(result_set: &ResultSet, order: &[usize]) -> String {
    let rows: Vec<Json> = order
        .iter()
        .filter_map(|row_index| {
            let row = result_set.rows.get(*row_index)?;
            let mut object = Map::with_capacity(result_set.columns.len());
            for (column_index, column) in result_set.columns.iter().enumerate() {
                let value = row.get(column_index).unwrap_or(&crate::model::Value::Null);
                object.insert(column.name.clone(), value.to_json());
            }
            Some(Json::Object(object))
        })
        .collect();
    // Pretty-printing keeps a single row readable; a machine consumer does not
    // care about the extra whitespace.
    serde_json::to_string_pretty(&Json::Array(rows)).unwrap_or_else(|_| "[]".to_owned())
}

/// Export described schemas as one flat row per column (CSV) or one object per
/// table (JSON). Both carry the primary key and foreign keys so the export is a
/// usable description of the database, not just a column dump.
pub fn export_schema(schemas: &[TableSchema], format: SchemaExportFormat) -> String {
    match format {
        SchemaExportFormat::Csv => export_schema_csv(schemas),
        SchemaExportFormat::Json => export_schema_json(schemas),
    }
}

fn export_schema_csv(schemas: &[TableSchema]) -> String {
    let mut output = String::from("table,column,type,nullable,primary_key,foreign_key_reference\n");
    for schema in schemas {
        let table = schema.reference.qualified_name();
        for column in &schema.columns {
            let reference = schema
                .foreign_keys
                .iter()
                .find(|foreign_key| foreign_key.columns.contains(&column.name))
                .map_or_else(String::new, |foreign_key| {
                    format!(
                        "{}({})",
                        foreign_key.referenced_table,
                        foreign_key.referenced_columns.join(", ")
                    )
                });
            let cells = [
                csv_escape(&table),
                csv_escape(&column.name),
                csv_escape(&format!("{:?}", column.declared_type)),
                csv_escape(if column.nullable { "yes" } else { "no" }),
                csv_escape(if column.primary_key { "yes" } else { "no" }),
                csv_escape(&reference),
            ];
            output.push_str(&cells.join(","));
            output.push('\n');
        }
    }
    output
}

fn export_schema_json(schemas: &[TableSchema]) -> String {
    let tables: Vec<Json> = schemas
        .iter()
        .map(|schema| {
            let columns: Vec<Json> = schema
                .columns
                .iter()
                .map(|column| {
                    json!({
                        "name": column.name,
                        "type": format!("{:?}", column.declared_type),
                        "nullable": column.nullable,
                        "primary_key": column.primary_key,
                    })
                })
                .collect();
            let foreign_keys: Vec<Json> = schema
                .foreign_keys
                .iter()
                .map(|foreign_key| {
                    json!({
                        "columns": foreign_key.columns,
                        "referenced_table": foreign_key.referenced_table,
                        "referenced_columns": foreign_key.referenced_columns,
                    })
                })
                .collect();
            json!({
                "table": schema.reference.qualified_name(),
                "kind": match schema.kind {
                    crate::model::ObjectKind::Table => "table",
                    crate::model::ObjectKind::View => "view",
                },
                "columns": columns,
                "primary_key": schema
                    .primary_key
                    .as_ref()
                    .map(|primary_key| primary_key.columns.clone())
                    .unwrap_or_default(),
                "foreign_keys": foreign_keys,
            })
        })
        .collect();
    serde_json::to_string_pretty(&Json::Array(tables)).unwrap_or_else(|_| "[]".to_owned())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Column, ColumnType, ForeignKey, ObjectKind, PrimaryKey, Row, Value};

    fn result_set() -> ResultSet {
        let mut set = ResultSet::new(
            vec![
                Column::new("id", ColumnType::Integer),
                Column::new("name", ColumnType::Text),
                Column::new("score", ColumnType::Real),
                Column::new("photo", ColumnType::Blob),
            ],
            "SELECT * FROM people",
        );
        set.push_row(
            Row(vec![
                Value::Integer(1),
                Value::Text("A|B\nC".into()),
                Value::Real(1.5),
                Value::Blob(vec![0xde, 0xad]),
            ]),
            100,
        );
        set.push_row(
            Row(vec![
                Value::Integer(2),
                Value::Null,
                Value::Real(-0.25),
                Value::Null,
            ]),
            100,
        );
        set.mark_ready(&crate::model::Capabilities::sqlite());
        set
    }

    fn users_schema() -> TableSchema {
        let mut id = Column::new("id", ColumnType::Integer);
        id.primary_key = true;
        id.nullable = false;
        TableSchema {
            reference: crate::model::TableRef::new("users"),
            kind: ObjectKind::Table,
            columns: vec![id, Column::new("name", ColumnType::Text)],
            primary_key: Some(PrimaryKey {
                name: None,
                columns: vec!["id".to_owned()],
            }),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            definition: None,
        }
    }

    #[test]
    fn json_export_keeps_types_and_nulls() {
        let set = result_set();
        let json = export_result_set(&set, ExportFormat::Json, &[0, 1]);
        let parsed: Json = serde_json::from_str(&json).expect("valid json");
        let rows = parsed.as_array().expect("array");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["id"], json!(1));
        assert_eq!(rows[0]["name"], json!("A|B\nC"));
        assert_eq!(rows[0]["score"], json!(1.5));
        assert_eq!(rows[0]["photo"], json!("[blob 2B: dead]"));
        assert!(rows[1]["name"].is_null());
    }

    #[test]
    fn json_export_follows_the_display_order() {
        let set = result_set();
        let json = export_result_set(&set, ExportFormat::Json, &[1, 0]);
        let parsed: Json = serde_json::from_str(&json).expect("valid json");
        let rows = parsed.as_array().expect("array");
        assert_eq!(rows[0]["id"], json!(2));
        assert_eq!(rows[1]["id"], json!(1));
    }

    #[test]
    fn schema_csv_lists_every_column_with_its_key_role() {
        let csv = export_schema(&[users_schema()], SchemaExportFormat::Csv);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(
            lines[0],
            "table,column,type,nullable,primary_key,foreign_key_reference"
        );
        assert_eq!(lines[1], "users,id,Integer,no,yes,");
        assert_eq!(lines[2], "users,name,Text,yes,no,");
    }

    #[test]
    fn schema_json_describes_keys_and_foreign_keys() {
        let mut orders = users_schema();
        orders.reference = crate::model::TableRef::new("orders");
        orders
            .columns
            .push(Column::new("user_id", ColumnType::Integer));
        orders.foreign_keys.push(ForeignKey {
            name: None,
            columns: vec!["user_id".to_owned()],
            referenced_table: "users".to_owned(),
            referenced_columns: vec!["id".to_owned()],
            on_delete: None,
            on_update: None,
        });
        let json = export_schema(&[users_schema(), orders], SchemaExportFormat::Json);
        let parsed: Json = serde_json::from_str(&json).expect("valid json");
        let tables = parsed.as_array().expect("array");
        assert_eq!(tables[1]["table"], json!("orders"));
        assert_eq!(tables[1]["primary_key"], json!(["id"]));
        assert_eq!(
            tables[1]["foreign_keys"][0]["referenced_table"],
            json!("users")
        );
        assert_eq!(tables[1]["columns"][2]["name"], json!("user_id"));
    }
}
