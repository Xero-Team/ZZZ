//! Mermaid `erDiagram` generation from described schemas.
//!
//! Kept in `database_core` and free of GPUI: the diagram source is a plain
//! string, so it can be tested without a window and reused by any surface that
//! shows a schema. The generated source is what `mermaid_render` rasterizes.

use std::collections::{BTreeSet, HashMap};

use crate::model::{ColumnType, ObjectKind, TableRef, TableSchema};

/// Build an `erDiagram` for every described table.
///
/// Entities become `<type> <name> [PK|FK]` blocks and each foreign key becomes a
/// `||--o{` relationship (one parent row, zero or more child rows). Relationships
/// are only emitted when both endpoints are in `schemas`, so the diagram never
/// contains a dangling box for a table the caller did not describe.
pub fn er_diagram(schemas: &[TableSchema]) -> String {
    let mut output = String::from("erDiagram\n");
    let mut entity_names = HashMap::with_capacity(schemas.len());
    // Drivers report a foreign key's target as a bare table name, so keep a
    // second index that ignores the schema qualifier.
    let mut bare_names: HashMap<String, String> = HashMap::with_capacity(schemas.len());
    let mut used = BTreeSet::new();
    for schema in schemas {
        let name = unique_entity_name(&schema.reference, &mut used);
        entity_names.insert(schema.reference.qualified_name(), name.clone());
        bare_names
            .entry(schema.reference.name.clone())
            .or_insert_with(|| name.clone());

        let foreign_key_columns: BTreeSet<&str> = schema
            .foreign_keys
            .iter()
            .flat_map(|foreign_key| foreign_key.columns.iter().map(String::as_str))
            .collect();

        output.push_str("    ");
        output.push_str(&name);
        output.push_str(" {\n");
        for column in &schema.columns {
            // Mermaid allows one key marker per attribute, so a column that is
            // both part of the primary key and a foreign key is marked PK.
            let marker = if column.primary_key {
                " PK"
            } else if foreign_key_columns.contains(column.name.as_str()) {
                " FK"
            } else {
                ""
            };
            output.push_str("        ");
            output.push_str(&attribute_token(&mermaid_type(&column.declared_type)));
            output.push(' ');
            output.push_str(&attribute_token(&column.name));
            output.push_str(marker);
            output.push('\n');
        }
        output.push_str("    }\n");
    }

    for schema in schemas {
        let Some(child) = entity_names.get(&schema.reference.qualified_name()) else {
            continue;
        };
        for foreign_key in &schema.foreign_keys {
            let parent = entity_names
                .get(&foreign_key.referenced_table)
                .or_else(|| bare_names.get(&foreign_key.referenced_table));
            let Some(parent) = parent else {
                continue;
            };
            output.push_str("    ");
            output.push_str(parent);
            output.push_str(" ||--o{ ");
            output.push_str(child);
            output.push_str(" : \"");
            output.push_str(&label_text(&foreign_key.columns.join(", ")));
            output.push_str("\"\n");
        }
    }

    output
}

fn mermaid_type(column_type: &ColumnType) -> String {
    match column_type {
        ColumnType::Integer => "INTEGER".to_owned(),
        ColumnType::Real => "REAL".to_owned(),
        ColumnType::Text => "TEXT".to_owned(),
        ColumnType::Blob => "BLOB".to_owned(),
        ColumnType::Date => "DATE".to_owned(),
        ColumnType::DateTime => "DATETIME".to_owned(),
        ColumnType::Time => "TIME".to_owned(),
        ColumnType::Bool => "BOOLEAN".to_owned(),
        ColumnType::Numeric { .. } => "NUMERIC".to_owned(),
        ColumnType::Json => "JSON".to_owned(),
        ColumnType::Other { declared } => attribute_token(declared),
    }
}

/// Reduce a name to a single Mermaid token. Unicode letters are kept, which is
/// what makes CJK table and column names work; anything Mermaid treats as
/// structure (spaces, dots, quotes, braces) becomes `_`.
fn attribute_token(name: &str) -> String {
    let mut token = String::with_capacity(name.len());
    for character in name.chars() {
        if character.is_alphanumeric() || character == '_' || character == '-' {
            token.push(character);
        } else {
            token.push('_');
        }
    }
    if token.is_empty() {
        token.push('_');
    }
    token
}

/// Entity names are sanitized like attributes, and a collisions gets a numeric
/// suffix. Two tables whose names sanitize identically would otherwise be drawn
/// as one entity with merged columns.
fn unique_entity_name(reference: &TableRef, used: &mut BTreeSet<String>) -> String {
    let base = attribute_token(&reference.qualified_name());
    let mut candidate = base.clone();
    let mut suffix = 2;
    while !used.insert(candidate.clone()) {
        candidate = format!("{base}_{suffix}");
        suffix += 1;
    }
    candidate
}

/// Relationship labels are free text inside double quotes, so only the quote
/// character and line breaks need removing.
fn label_text(label: &str) -> String {
    label
        .replace('"', "'")
        .replace(['\n', '\r'], " ")
        .trim()
        .to_owned()
}

/// Whether the driver can describe a table well enough to diagram it. Views are
/// included because they still have columns and can be referenced.
pub fn diagrammable(schema: &TableSchema) -> bool {
    matches!(schema.kind, ObjectKind::Table | ObjectKind::View) && !schema.columns.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Column, ForeignKey, Index, PrimaryKey};

    fn column(name: &str, declared_type: ColumnType) -> Column {
        Column::new(name, declared_type)
    }

    fn table(name: &str, columns: Vec<Column>) -> TableSchema {
        TableSchema {
            reference: TableRef::new(name),
            kind: ObjectKind::Table,
            columns,
            primary_key: None,
            indexes: Vec::<Index>::new(),
            foreign_keys: Vec::new(),
            definition: None,
        }
    }

    #[test]
    fn emits_tables_columns_and_keys() {
        let mut id = column("id", ColumnType::Integer);
        id.primary_key = true;
        let mut customer_id = column("customer_id", ColumnType::Integer);
        customer_id.nullable = false;
        let mut orders = table(
            "orders",
            vec![id, customer_id, column("total", ColumnType::Real)],
        );
        orders.primary_key = Some(PrimaryKey {
            name: None,
            columns: vec!["id".to_owned()],
        });
        orders.foreign_keys.push(ForeignKey {
            name: None,
            columns: vec!["customer_id".to_owned()],
            referenced_table: "customers".to_owned(),
            referenced_columns: vec!["id".to_owned()],
            on_delete: None,
            on_update: None,
        });
        let mut customers_id = column("id", ColumnType::Integer);
        customers_id.primary_key = true;
        let customers = table("customers", vec![customers_id]);

        let source = er_diagram(&[customers, orders]);
        assert!(source.starts_with("erDiagram\n"));
        assert!(source.contains("    customers {\n        INTEGER id PK\n    }\n"));
        assert!(source.contains(
            "    orders {\n        INTEGER id PK\n        INTEGER customer_id FK\n        REAL total\n    }\n"
        ));
        assert!(source.contains("    customers ||--o{ orders : \"customer_id\"\n"));
    }

    #[test]
    fn cjk_names_stay_readable() {
        let mut id = column("编号", ColumnType::Integer);
        id.primary_key = true;
        let mut schema = table("顧客", vec![id, column("名前", ColumnType::Text)]);
        schema.foreign_keys.push(ForeignKey {
            name: None,
            columns: vec!["名前".to_owned()],
            referenced_table: "顧客".to_owned(),
            referenced_columns: vec!["编号".to_owned()],
            on_delete: None,
            on_update: None,
        });

        let source = er_diagram(&[schema]);
        assert!(
            source.contains("    顧客 {\n        INTEGER 编号 PK\n        TEXT 名前 FK\n    }\n")
        );
        assert!(source.contains("    顧客 ||--o{ 顧客 : \"名前\"\n"));
    }

    #[test]
    fn punctuation_becomes_a_token_and_collisions_get_a_suffix() {
        let first = table("order items", vec![column("unit price", ColumnType::Real)]);
        let second = table("order_items", vec![column("id", ColumnType::Integer)]);
        let source = er_diagram(&[first, second]);

        assert!(source.contains("    order_items {\n        REAL unit_price\n    }\n"));
        assert!(source.contains("    order_items_2 {\n        INTEGER id\n    }\n"));
    }

    #[test]
    fn relationships_to_undescribed_tables_are_omitted() {
        let mut schema = table("orders", vec![column("id", ColumnType::Integer)]);
        schema.foreign_keys.push(ForeignKey {
            name: None,
            columns: vec!["id".to_owned()],
            referenced_table: "missing".to_owned(),
            referenced_columns: vec!["id".to_owned()],
            on_delete: None,
            on_update: None,
        });
        let source = er_diagram(&[schema]);
        assert!(!source.contains("--"));
    }

    #[test]
    fn schema_qualified_tables_keep_the_schema_in_the_entity_name() {
        let mut schema = table("users", vec![column("id", ColumnType::Integer)]);
        schema.reference.schema = Some("main".to_owned());
        let source = er_diagram(&[schema]);
        assert!(source.contains("    main_users {\n"));
    }
}
