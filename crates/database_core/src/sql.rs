//! SQL text analysis: statement splitting, classification, and the editable
//! result shape.
//!
//! Kept driver-agnostic and pure so the "run current statement", "run
//! selection", destructive-statement confirmation, and edit targeting paths are
//! independently testable. This is a lexer over SQL text, not a full SQL parser.

use crate::model::TableRef;

/// The category of a SQL statement, used to decide whether a confirmation is
/// required before execution (D-05) and whether a result set is expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementKind {
    Select,
    Insert,
    Update {
        has_where: bool,
    },
    Delete {
        has_where: bool,
    },
    /// DROP / TRUNCATE / ALTER / CREATE and other schema changes.
    Ddl,
    /// EXPLAIN (including EXPLAIN QUERY PLAN).
    Explain,
    Transaction,
    Pragma,
    Other,
    Empty,
}

impl StatementKind {
    /// Whether executing this statement requires a second confirmation because
    /// its effect is broad or irreversible (D-05).
    pub fn requires_confirmation(&self) -> bool {
        match self {
            StatementKind::Update { has_where: false }
            | StatementKind::Delete { has_where: false }
            | StatementKind::Ddl => true,
            _ => false,
        }
    }

    /// Whether the statement normally produces a result set.
    pub fn returns_rows(&self) -> bool {
        matches!(
            self,
            StatementKind::Select | StatementKind::Explain | StatementKind::Pragma
        )
    }
}

/// A statement extracted from a larger SQL document, with its byte range so the
/// editor can highlight or replace exactly what ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlStatement {
    pub text: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub kind: StatementKind,
}

impl SqlStatement {
    pub fn from_selection(text: impl Into<String>) -> Self {
        let text = text.into();
        let kind = classify(&text);
        Self {
            text,
            start_byte: 0,
            end_byte: 0,
            kind,
        }
    }
}

/// Split a SQL document into statements at top-level semicolons. Semicolons in
/// string literals, quoted identifiers, line comments, and block comments are
/// ignored. Each returned statement keeps the byte offset of its first token.
pub fn split_statements(sql: &str) -> Vec<SqlStatement> {
    let bytes = sql.as_bytes();
    let mut statements = Vec::new();
    let mut current_start: Option<usize> = None;
    let mut last_non_trivia = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'\'' | b'"' | b'`' => {
                if current_start.is_none() {
                    current_start = Some(index);
                }
                index = skip_quoted(bytes, index, byte);
                last_non_trivia = index.saturating_sub(1);
            }
            b'-' if index + 1 < bytes.len() && bytes[index + 1] == b'-' => {
                index = skip_line_comment(bytes, index);
            }
            b'/' if index + 1 < bytes.len() && bytes[index + 1] == b'*' => {
                index = skip_block_comment(bytes, index);
            }
            b';' => {
                if let Some(start) = current_start.take() {
                    let end = index;
                    let text = &sql[start..end];
                    if !text.trim().is_empty() {
                        statements.push(SqlStatement {
                            text: text.trim().to_owned(),
                            start_byte: start + leading_whitespace_len(text),
                            end_byte: start + trailing_trimmed_len(text),
                            kind: classify(text),
                        });
                    }
                }
                index += 1;
            }
            _ => {
                if current_start.is_none() && !byte.is_ascii_whitespace() {
                    current_start = Some(index);
                }
                if !byte.is_ascii_whitespace() {
                    last_non_trivia = index;
                }
                index += 1;
            }
        }
    }

    if let Some(start) = current_start {
        let text = &sql[start..=last_non_trivia];
        if !text.trim().is_empty() {
            statements.push(SqlStatement {
                text: text.trim().to_owned(),
                start_byte: start + leading_whitespace_len(&sql[start..=last_non_trivia]),
                end_byte: start + trailing_trimmed_len(&sql[start..=last_non_trivia]),
                kind: classify(text),
            });
        }
    }
    statements
}

/// Pick the statement under the cursor. When the cursor sits after a statement
/// terminator (or in the whitespace between statements), the preceding statement
/// wins so "run" still means the statement the user just typed.
pub fn statement_at(sql: &str, cursor_byte: usize) -> Option<SqlStatement> {
    let statements = split_statements(sql);
    let cursor = cursor_byte.min(sql.len());
    if let Some(statement) = statements
        .iter()
        .find(|statement| statement.start_byte <= cursor && cursor <= statement.end_byte)
    {
        return Some(statement.clone());
    }
    statements
        .into_iter()
        .rfind(|statement| statement.end_byte <= cursor)
}

fn leading_whitespace_len(text: &str) -> usize {
    text.len() - text.trim_start().len()
}

fn trailing_trimmed_len(text: &str) -> usize {
    text.trim_end().len()
}

fn skip_quoted(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == quote {
            // A doubled quote is an escaped quote inside the literal.
            if index + 1 < bytes.len() && bytes[index + 1] == quote {
                index += 2;
                continue;
            }
            return index + 1;
        }
        index += 1;
    }
    bytes.len()
}

fn skip_line_comment(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 2;
    while index < bytes.len() && bytes[index] != b'\n' {
        index += 1;
    }
    index
}

fn skip_block_comment(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 2;
    while index + 1 < bytes.len() {
        if bytes[index] == b'*' && bytes[index + 1] == b'/' {
            return index + 2;
        }
        index += 1;
    }
    bytes.len()
}

/// Classify a statement by its leading keyword. The one piece of lexical
/// context needed beyond the first word is whether UPDATE/DELETE carry a WHERE
/// clause at the top level; a naive substring search would be fooled by a
/// subquery or a string containing "where".
pub fn classify(sql: &str) -> StatementKind {
    let tokens = significant_tokens(sql);
    if tokens.is_empty() {
        return StatementKind::Empty;
    }
    let first = tokens[0].text.to_ascii_uppercase();
    let has_where = tokens
        .iter()
        .any(|token| token.depth == 0 && token.text.eq_ignore_ascii_case("WHERE"));
    match first.as_str() {
        "SELECT" | "WITH" | "VALUES" | "TABLE" => StatementKind::Select,
        "INSERT" | "REPLACE" => StatementKind::Insert,
        "UPDATE" => StatementKind::Update { has_where },
        "DELETE" => StatementKind::Delete { has_where },
        "DROP" | "TRUNCATE" | "ALTER" | "CREATE" | "RENAME" => StatementKind::Ddl,
        "EXPLAIN" => StatementKind::Explain,
        "BEGIN" | "COMMIT" | "ROLLBACK" | "SAVEPOINT" | "RELEASE" => StatementKind::Transaction,
        "PRAGMA" => StatementKind::Pragma,
        _ => StatementKind::Other,
    }
}

/// The single table an `SELECT ... FROM <table>` reads from, when the shape is
/// simple enough that each result row maps to exactly one table row.
///
/// Conservative on purpose: joins, comma-separated table lists, unions, grouping,
/// distinct, and subqueries all return `None`. A wrong answer here would let the
/// UI offer editing for rows it cannot target safely. The caller still has to
/// check that the primary key is present in the result columns.
pub fn editable_target(sql: &str) -> Option<TableRef> {
    let tokens = significant_tokens(sql);
    if !tokens
        .first()
        .is_some_and(|token| token.text.eq_ignore_ascii_case("SELECT"))
    {
        return None;
    }
    const UNSAFE_KEYWORDS: [&str; 8] = [
        "JOIN",
        "UNION",
        "INTERSECT",
        "EXCEPT",
        "GROUP",
        "HAVING",
        "DISTINCT",
        "WINDOW",
    ];
    if tokens.iter().any(|token| {
        token.depth == 0
            && UNSAFE_KEYWORDS
                .iter()
                .any(|kw| token.text.eq_ignore_ascii_case(kw))
    }) {
        return None;
    }
    let from_indices: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| token.depth == 0 && token.text.eq_ignore_ascii_case("FROM"))
        .map(|(index, _)| index)
        .collect();
    // More than one top-level FROM means a subquery or another clause we do not
    // model; zero means there is no target table at all.
    if from_indices.len() != 1 {
        return None;
    }
    let from_index = from_indices[0];
    let from_token = &tokens[from_index];
    let table_token = tokens.get(from_index + 1)?;
    // A subquery starts with `(`, so the token after FROM is nested and the gap
    // is not plain whitespace.
    if table_token.depth != 0 || !sql[from_token.end..table_token.start].trim().is_empty() {
        return None;
    }

    let mut parts = vec![table_token.text.clone()];
    let mut cursor = table_token.end;
    let mut index = from_index + 2;
    while let Some(next) = tokens.get(index) {
        let gap = &sql[cursor..next.start];
        if gap.trim() == "." {
            if next.depth != 0 {
                return None;
            }
            parts.push(next.text.clone());
            cursor = next.end;
            index += 1;
        } else {
            break;
        }
    }
    if parts.len() > 3 {
        return None;
    }

    if let Some(next) = tokens.get(index) {
        let gap = &sql[cursor..next.start];
        // A comma here is a table list (`FROM a, b`), not a single target.
        if next.depth != 0 || gap.contains(',') {
            return None;
        }
        if next.text.eq_ignore_ascii_case("AS") {
            let alias = tokens.get(index + 1)?;
            if alias.depth != 0 {
                return None;
            }
            index += 2;
        } else if !is_clause_keyword(&next.text) {
            // A bare alias (`FROM users u`).
            index += 1;
        }
        if let Some(trailing) = tokens.get(index)
            && !is_clause_keyword(&trailing.text)
        {
            return None;
        }
    }

    let name = parts.pop()?;
    let schema = parts.pop();
    let catalog = parts.pop();
    let mut table = TableRef::new(name);
    table.schema = schema;
    table.catalog = catalog;
    Some(table)
}

fn is_clause_keyword(word: &str) -> bool {
    [
        "WHERE",
        "ORDER",
        "LIMIT",
        "OFFSET",
        "RETURNING",
        "FETCH",
        "FOR",
    ]
    .iter()
    .any(|keyword| word.eq_ignore_ascii_case(keyword))
}

/// Quote one SQL identifier so a table or column name can be used in generated
/// SQL. Embedded double quotes are doubled, which is the SQL standard escape and
/// what SQLite, PostgreSQL and MySQL accept; user values never take this path.
pub fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Quote a possibly schema- or catalog-qualified table reference.
pub fn quote_table(table: &TableRef) -> String {
    let mut quoted = String::new();
    for part in [&table.catalog, &table.schema]
        .into_iter()
        .flatten()
        .chain(std::iter::once(&table.name))
    {
        if !quoted.is_empty() {
            quoted.push('.');
        }
        quoted.push_str(&quote_identifier(part));
    }
    quoted
}

/// A word token with its parenthesis nesting depth at the point it appears.
struct SqlToken {
    text: String,
    depth: i32,
    start: usize,
    end: usize,
}

/// Extract word tokens while skipping comments and quoted literals, so keyword
/// detection is not confused by `'drop'` or `-- select` in the SQL text. Parenthesis
/// depth is tracked so a WHERE inside a subquery does not count as a top-level
/// WHERE of an UPDATE/DELETE.
fn significant_tokens(sql: &str) -> Vec<SqlToken> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    let mut depth = 0i32;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'\'' | b'"' | b'`' => {
                index = skip_quoted(bytes, index, byte);
            }
            b'-' if index + 1 < bytes.len() && bytes[index + 1] == b'-' => {
                index = skip_line_comment(bytes, index);
            }
            b'/' if index + 1 < bytes.len() && bytes[index + 1] == b'*' => {
                index = skip_block_comment(bytes, index);
            }
            b'(' => {
                depth += 1;
                index += 1;
            }
            b')' => {
                depth -= 1;
                index += 1;
            }
            byte if byte.is_ascii_alphabetic() => {
                let start = index;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                tokens.push(SqlToken {
                    text: sql[start..index].to_owned(),
                    depth,
                    start,
                    end: index,
                });
            }
            _ => index += 1,
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_top_level_semicolons() {
        let statements = split_statements("SELECT 1; SELECT 2; SELECT 3");
        assert_eq!(statements.len(), 3);
        assert_eq!(statements[0].text, "SELECT 1");
        assert_eq!(statements[2].text, "SELECT 3");
    }

    #[test]
    fn statement_at_prefers_the_statement_under_the_cursor() {
        let sql = "SELECT 1;\nSELECT 2;";
        let second = sql.rfind("SELECT 2").expect("second statement");
        assert_eq!(
            statement_at(sql, second + 3).map(|statement| statement.text),
            Some("SELECT 2".to_owned())
        );
    }

    #[test]
    fn statement_at_falls_back_to_the_previous_statement() {
        let sql = "SELECT 1;\n\nSELECT 2";
        let between = sql.find("\n\n").expect("gap") + 1;
        assert_eq!(
            statement_at(sql, between).map(|statement| statement.text),
            Some("SELECT 1".to_owned())
        );
        assert_eq!(
            statement_at("-- only a comment", 5).map(|statement| statement.text),
            None
        );
    }

    #[test]
    fn ignores_semicolons_in_strings_and_comments() {
        let sql = "SELECT 'a;b' AS x; -- note; still comment\nSELECT 2 /* block; */ ;";
        let statements = split_statements(sql);
        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0].text, "SELECT 'a;b' AS x");
        // The leading comment is trivia, not part of the statement text; the
        // important property is that neither comment semicolon split a statement.
        assert_eq!(statements[1].text, "SELECT 2 /* block; */");
    }

    #[test]
    fn trailing_comment_stays_inside_its_statement() {
        let statements = split_statements("SELECT 1 -- drop; nope\n; SELECT 2");
        assert_eq!(statements.len(), 2);
        assert_eq!(classify(&statements[0].text), StatementKind::Select);
        assert_eq!(statements[1].text, "SELECT 2");
    }

    #[test]
    fn handles_doubled_quote_escape() {
        let statements = split_statements("SELECT 'it''s; fine'; SELECT 2");
        assert_eq!(statements.len(), 2);
        assert_eq!(statements[0].text, "SELECT 'it''s; fine'");
    }

    #[test]
    fn byte_offsets_point_at_statement() {
        let sql = "  SELECT 1;  ";
        let statements = split_statements(sql);
        assert_eq!(statements.len(), 1);
        assert_eq!(
            &sql[statements[0].start_byte..statements[0].end_byte],
            "SELECT 1"
        );
    }

    #[test]
    fn classifies_destructive_statements() {
        assert!(classify("DELETE FROM users").requires_confirmation());
        assert!(classify("UPDATE users SET name = 'x'").requires_confirmation());
        assert!(!classify("DELETE FROM users WHERE id = 1").requires_confirmation());
        assert!(!classify("UPDATE users SET name = 'x' WHERE id = 1").requires_confirmation());
        assert!(classify("DROP TABLE users").requires_confirmation());
        assert!(classify("ALTER TABLE users ADD COLUMN age INT").requires_confirmation());
        assert!(classify("CREATE TABLE t (id INT)").requires_confirmation());
    }

    #[test]
    fn classification_ignores_keywords_inside_literals() {
        assert_eq!(classify("SELECT 'drop table x'"), StatementKind::Select);
        assert_eq!(classify("-- drop table x\nSELECT 1"), StatementKind::Select);
    }

    #[test]
    fn explain_and_transaction_kinds() {
        assert_eq!(
            classify("EXPLAIN QUERY PLAN SELECT 1"),
            StatementKind::Explain
        );
        assert!(classify("EXPLAIN QUERY PLAN SELECT 1").returns_rows());
        assert_eq!(classify("BEGIN TRANSACTION"), StatementKind::Transaction);
        assert_eq!(classify("PRAGMA table_info('t')"), StatementKind::Pragma);
    }

    #[test]
    fn where_inside_subquery_does_not_count_as_top_level() {
        // A top-level WHERE still protects the statement even when a subquery
        // also contains WHERE; only the depth-0 WHERE counts.
        let with_top_level_where =
            "DELETE FROM users WHERE id IN (SELECT id FROM banned WHERE active = 1)";
        assert!(!classify(with_top_level_where).requires_confirmation());
        assert!(!classify("DELETE FROM users WHERE id = 1").requires_confirmation());
        // No top-level WHERE at all: the DELETE affects every row.
        assert!(classify("DELETE FROM users").requires_confirmation());
    }

    #[test]
    fn empty_input() {
        assert!(split_statements("   \n -- only a comment\n").is_empty());
        assert_eq!(classify("   "), StatementKind::Empty);
    }

    #[test]
    fn editable_target_accepts_a_simple_select() {
        assert_eq!(
            editable_target("SELECT id, name FROM users"),
            Some(TableRef::new("users"))
        );
        assert_eq!(
            editable_target("select * from main.users where id = 1 order by name"),
            Some(TableRef {
                catalog: None,
                schema: Some("main".to_owned()),
                name: "users".to_owned(),
            })
        );
        assert_eq!(
            editable_target("SELECT u.id FROM users AS u"),
            Some(TableRef::new("users"))
        );
        assert_eq!(
            editable_target("SELECT u.id FROM users u LIMIT 10 OFFSET 5"),
            Some(TableRef::new("users"))
        );
        assert_eq!(
            editable_target("SELECT id FROM main.users"),
            Some(TableRef {
                catalog: None,
                schema: Some("main".to_owned()),
                name: "users".to_owned(),
            })
        );
    }

    #[test]
    fn editable_target_rejects_shapes_that_do_not_map_one_row_to_one_table_row() {
        for sql in [
            "SELECT a.id, b.id FROM a JOIN b ON a.id = b.id",
            "SELECT id FROM a, b",
            "SELECT id FROM a NATURAL JOIN b",
            "SELECT id FROM a UNION SELECT id FROM b",
            "SELECT id FROM a INTERSECT SELECT id FROM b",
            "SELECT id FROM a GROUP BY id",
            "SELECT DISTINCT id FROM a",
            "SELECT id FROM (SELECT id FROM a) AS nested",
            "WITH source AS (SELECT id FROM a) SELECT id FROM source",
            "UPDATE a SET id = 1",
            "SELECT 1",
            "SELECT id FROM a, b WHERE a.id = b.id",
        ] {
            assert_eq!(editable_target(sql), None, "{sql}");
        }
    }

    #[test]
    fn editable_target_ignores_keywords_in_literals_and_comments() {
        assert_eq!(
            editable_target("SELECT id FROM users WHERE name = 'join group by'"),
            Some(TableRef::new("users"))
        );
        assert_eq!(
            editable_target("SELECT id -- from other\nFROM users"),
            Some(TableRef::new("users"))
        );
    }
}
