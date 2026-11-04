//! Local execution history for the database browser.
//!
//! Only what the history panel needs is stored: the SQL text, which connection
//! ran it, when, how many rows came back and how long it took. Result contents
//! and credentials are never written here (D-11).

use crate::{
    sqlez::{domain::Domain, thread_safe_connection::ThreadSafeConnection},
    sqlez_macros::sql,
};

/// Entries kept per connection. The panel searches locally, and an unbounded
/// history would grow without ever being read.
const MAX_ENTRIES_PER_CONNECTION: i64 = 500;

/// One executed statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryHistoryEntry {
    pub id: i64,
    pub connection_id: String,
    pub statement: String,
    /// Unix seconds.
    pub executed_at: i64,
    pub row_count: Option<i64>,
    pub duration_ms: Option<i64>,
    pub success: bool,
}

pub struct QueryHistoryDb(ThreadSafeConnection);

impl Domain for QueryHistoryDb {
    const NAME: &str = stringify!(QueryHistoryDb);

    const MIGRATIONS: &[&str] = &[sql!(
        CREATE TABLE query_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            connection_id TEXT NOT NULL,
            statement TEXT NOT NULL,
            executed_at INTEGER NOT NULL,
            row_count INTEGER,
            duration_ms INTEGER,
            success INTEGER NOT NULL
        ) STRICT;
        CREATE INDEX query_history_by_connection
            ON query_history(connection_id, executed_at DESC);
    )];
}

crate::static_connection!(QueryHistoryDb, []);

impl QueryHistoryDb {
    crate::query! {
        pub async fn insert_entry(
            connection_id: String,
            statement: String,
            executed_at: i64,
            row_count: Option<i64>,
            duration_ms: Option<i64>,
            success: i64
        ) -> Result<()> {
            INSERT INTO query_history(
                connection_id, statement, executed_at, row_count, duration_ms, success
            ) VALUES (?, ?, ?, ?, ?, ?)
        }
    }

    /// Keep only the newest [`MAX_ENTRIES_PER_CONNECTION`] entries of one
    /// connection. Called after every insert so the table cannot grow forever.
    pub async fn prune(&self, connection_id: String) -> anyhow::Result<()> {
        use anyhow::Context as _;

        // Both placeholders bind the same value: the subquery keeps the newest
        // rows for this connection, and the outer delete drops the rest of it.
        let statement = sql!(
            DELETE FROM query_history
            WHERE connection_id = ?
              AND id NOT IN (
                SELECT id FROM query_history
                WHERE connection_id = ?
                ORDER BY id DESC
                LIMIT 500
              )
        );
        self.write(move |connection| {
            connection.exec_bound::<(String, String)>(statement)?((
                connection_id.clone(),
                connection_id,
            ))
            .context("failed to prune query history")
        })
        .await
    }

    pub async fn record(
        &self,
        connection_id: String,
        statement: String,
        executed_at: i64,
        row_count: Option<i64>,
        duration_ms: Option<i64>,
        success: bool,
    ) -> anyhow::Result<()> {
        let success = i64::from(success);
        self.insert_entry(
            connection_id.clone(),
            statement,
            executed_at,
            row_count,
            duration_ms,
            success,
        )
        .await?;
        self.prune(connection_id).await
    }

    /// Newest entries first. `needle` matches the SQL text literally (not as a
    /// LIKE pattern), so a `%` typed by the user stays a `%`.
    pub async fn entries(
        &self,
        connection_id: String,
        needle: Option<String>,
        limit: i64,
    ) -> anyhow::Result<Vec<QueryHistoryEntry>> {
        let needle = needle.filter(|needle| !needle.is_empty());
        let limit = limit.clamp(1, MAX_ENTRIES_PER_CONNECTION);
        let rows = match needle {
            Some(needle) => self.entries_matching(connection_id, needle, limit).await?,
            None => self.recent_entries(connection_id, limit).await?,
        };
        Ok(rows
            .into_iter()
            .map(
                |(id, connection_id, statement, executed_at, row_count, duration_ms, success)| {
                    QueryHistoryEntry {
                        id,
                        connection_id,
                        statement,
                        executed_at,
                        row_count,
                        duration_ms,
                        success: success != 0,
                    }
                },
            )
            .collect())
    }

    crate::query! {
        async fn recent_entries(
            connection_id: String,
            limit: i64
        ) -> Result<Vec<(i64, String, String, i64, Option<i64>, Option<i64>, i64)>> {
            SELECT id, connection_id, statement, executed_at, row_count, duration_ms, success
            FROM query_history
            WHERE connection_id = ?
            ORDER BY id DESC
            LIMIT ?
        }
    }

    crate::query! {
        async fn entries_matching(
            connection_id: String,
            needle: String,
            limit: i64
        ) -> Result<Vec<(i64, String, String, i64, Option<i64>, Option<i64>, i64)>> {
            SELECT id, connection_id, statement, executed_at, row_count, duration_ms, success
            FROM query_history
            WHERE connection_id = ? AND instr(statement, ?) > 0
            ORDER BY id DESC
            LIMIT ?
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    async fn history_records_sql_metadata_and_survives_reload(cx: &mut gpui::TestAppContext) {
        cx.executor().allow_parking();
        let history = QueryHistoryDb::open_test_db("test_query_history_records_metadata").await;
        history
            .record(
                "conn-a".to_owned(),
                "SELECT 1".to_owned(),
                1_700_000_000,
                Some(1),
                Some(4),
                true,
            )
            .await
            .expect("record");

        let entries = history
            .entries("conn-a".to_owned(), None, 100)
            .await
            .expect("entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].statement, "SELECT 1");
        assert_eq!(entries[0].row_count, Some(1));
        assert_eq!(entries[0].duration_ms, Some(4));
        assert!(entries[0].success);

        // A second connection's history is separate.
        assert!(
            history
                .entries("conn-b".to_owned(), None, 100)
                .await
                .expect("entries")
                .is_empty()
        );
    }

    #[gpui::test]
    async fn search_matches_literally_and_returns_newest_first(cx: &mut gpui::TestAppContext) {
        cx.executor().allow_parking();
        let history = QueryHistoryDb::open_test_db("test_query_history_search").await;
        for (index, statement) in ["SELECT 1", "UPDATE t SET a = 2", "SELECT 100%"]
            .iter()
            .enumerate()
        {
            history
                .record(
                    "conn".to_owned(),
                    (*statement).to_owned(),
                    1_700_000_000 + index as i64,
                    None,
                    None,
                    index != 1,
                )
                .await
                .expect("record");
        }

        let all = history
            .entries("conn".to_owned(), None, 100)
            .await
            .expect("entries");
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].statement, "SELECT 100%");
        assert!(!all[1].success);

        let select = history
            .entries("conn".to_owned(), Some("SELECT".to_owned()), 100)
            .await
            .expect("search");
        assert_eq!(select.len(), 2);

        // `%` is a literal here, not a wildcard.
        let percent = history
            .entries("conn".to_owned(), Some("%".to_owned()), 100)
            .await
            .expect("search");
        assert_eq!(percent.len(), 1);
        assert_eq!(percent[0].statement, "SELECT 100%");
    }

    #[gpui::test]
    async fn history_is_pruned_per_connection(cx: &mut gpui::TestAppContext) {
        cx.executor().allow_parking();
        let history = QueryHistoryDb::open_test_db("test_query_history_prune").await;
        for index in 0..(MAX_ENTRIES_PER_CONNECTION + 10) {
            history
                .record(
                    "conn".to_owned(),
                    format!("SELECT {index}"),
                    1_700_000_000 + index,
                    None,
                    None,
                    true,
                )
                .await
                .expect("record");
        }
        let entries = history
            .entries("conn".to_owned(), None, MAX_ENTRIES_PER_CONNECTION)
            .await
            .expect("entries");
        assert_eq!(entries.len() as i64, MAX_ENTRIES_PER_CONNECTION);
        assert_eq!(
            entries[0].statement,
            format!("SELECT {}", MAX_ENTRIES_PER_CONNECTION + 9)
        );
    }
}
