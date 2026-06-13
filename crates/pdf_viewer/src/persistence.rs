use std::path::PathBuf;

use db::{
    query,
    sqlez::{domain::Domain, thread_safe_connection::ThreadSafeConnection},
    sqlez_macros::sql,
};
use workspace::{ItemId, WorkspaceDb, WorkspaceId};

use crate::view::ZoomMode;

/// A persisted PDF viewer entry: file path plus restorable reading state.
pub(crate) struct PdfRecord {
    pub path: PathBuf,
    pub page: i64,
    pub zoom_mode: String,
    pub zoom: f32,
}

impl PdfRecord {
    pub fn zoom_mode(&self) -> ZoomMode {
        match self.zoom_mode.as_str() {
            "custom" => ZoomMode::Custom,
            "fit_page" => ZoomMode::FitPage,
            _ => ZoomMode::FitWidth,
        }
    }
}

pub(crate) struct PdfViewerDb(ThreadSafeConnection);

impl Domain for PdfViewerDb {
    const NAME: &str = stringify!(PdfViewerDb);

    const MIGRATIONS: &[&str] = &[sql!(
        CREATE TABLE pdf_viewers (
            workspace_id INTEGER,
            item_id INTEGER UNIQUE,

            pdf_path BLOB,
            page INTEGER,
            zoom_mode TEXT,
            zoom REAL,

            PRIMARY KEY(workspace_id, item_id),
            FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id)
            ON DELETE CASCADE
        ) STRICT;
    )];
}

db::static_connection!(PdfViewerDb, [WorkspaceDb]);

impl PdfViewerDb {
    query! {
        pub async fn save_pdf(
            item_id: ItemId,
            workspace_id: WorkspaceId,
            pdf_path: PathBuf,
            page: i64,
            zoom_mode: String,
            zoom: f32
        ) -> Result<()> {
            INSERT OR REPLACE INTO pdf_viewers(item_id, workspace_id, pdf_path, page, zoom_mode, zoom)
            VALUES (?, ?, ?, ?, ?, ?)
        }
    }

    pub fn get_pdf(
        &self,
        item_id: ItemId,
        workspace_id: WorkspaceId,
    ) -> anyhow::Result<Option<crate::persistence::PdfRecord>> {
        let row: Option<(PathBuf, i64, String, f32)> =
            self.select_pdf_row(item_id, workspace_id)?;
        Ok(row.map(
            |(path, page, zoom_mode, zoom)| crate::persistence::PdfRecord {
                path,
                page,
                zoom_mode,
                zoom,
            },
        ))
    }

    query! {
        fn select_pdf_row(item_id: ItemId, workspace_id: WorkspaceId) -> Result<Option<(PathBuf, i64, String, f32)>> {
            SELECT pdf_path, page, zoom_mode, zoom
            FROM pdf_viewers
            WHERE item_id = ? AND workspace_id = ?
        }
    }
}
