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
    pub provider_path: Option<Vec<u8>>,
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

    const MIGRATIONS: &[&str] = &[
        sql!(
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
        ),
        sql!(ALTER TABLE pdf_viewers ADD COLUMN provider_path BLOB;),
    ];
}

db::static_connection!(PdfViewerDb, [WorkspaceDb]);

impl PdfViewerDb {
    query! {
        pub async fn save_pdf(
            item_id: ItemId,
            workspace_id: WorkspaceId,
            pdf_path: PathBuf,
            provider_path: Option<Vec<u8>>,
            page: i64,
            zoom_mode: String,
            zoom: f32
        ) -> Result<()> {
            INSERT OR REPLACE INTO pdf_viewers(item_id, workspace_id, pdf_path, provider_path, page, zoom_mode, zoom)
            VALUES (?, ?, ?, ?, ?, ?, ?)
        }
    }

    pub fn get_pdf(
        &self,
        item_id: ItemId,
        workspace_id: WorkspaceId,
    ) -> anyhow::Result<Option<crate::persistence::PdfRecord>> {
        let row: Option<(PathBuf, Option<Vec<u8>>, i64, String, f32)> =
            self.select_pdf_row(item_id, workspace_id)?;
        Ok(row.map(
            |(path, provider_path, page, zoom_mode, zoom)| crate::persistence::PdfRecord {
                path,
                provider_path,
                page,
                zoom_mode,
                zoom,
            },
        ))
    }

    query! {
        fn select_pdf_row(item_id: ItemId, workspace_id: WorkspaceId) -> Result<Option<(PathBuf, Option<Vec<u8>>, i64, String, f32)>> {
            SELECT pdf_path, provider_path, page, zoom_mode, zoom
            FROM pdf_viewers
            WHERE item_id = ? AND workspace_id = ?
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    async fn test_pdf_provider_path_round_trip(cx: &mut gpui::TestAppContext) {
        let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
        let workspace_id = workspace_db
            .next_id()
            .await
            .expect("workspace ID should be allocated");
        let pdf_db = cx.update(|cx| PdfViewerDb::global(cx));
        let provider_path = vec![0, 1, 2, 0xff];

        pdf_db
            .save_pdf(
                1,
                workspace_id,
                PathBuf::from("legacy-document.pdf"),
                Some(provider_path.clone()),
                17,
                "fit_page".to_owned(),
                1.25,
            )
            .await
            .expect("PDF record should save");

        let record = pdf_db
            .get_pdf(1, workspace_id)
            .expect("PDF record should load")
            .expect("PDF record should exist");
        assert_eq!(record.path, PathBuf::from("legacy-document.pdf"));
        assert_eq!(record.provider_path, Some(provider_path));
        assert_eq!(record.page, 17);
        assert_eq!(record.zoom_mode, "fit_page");
        assert_eq!(record.zoom, 1.25);

        pdf_db
            .save_pdf(
                1,
                workspace_id,
                PathBuf::from("legacy-only.pdf"),
                None,
                2,
                "fit_width".to_owned(),
                0.75,
            )
            .await
            .expect("legacy PDF record should save");
        let legacy_record = pdf_db
            .get_pdf(1, workspace_id)
            .expect("legacy PDF record should load")
            .expect("legacy PDF record should exist");
        assert_eq!(legacy_record.path, PathBuf::from("legacy-only.pdf"));
        assert_eq!(legacy_record.provider_path, None);
        assert_eq!(legacy_record.page, 2);
        assert_eq!(legacy_record.zoom_mode, "fit_width");
        assert_eq!(legacy_record.zoom, 0.75);
    }
}
