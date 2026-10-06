use std::path::PathBuf;

use db::{
    query,
    sqlez::{domain::Domain, thread_safe_connection::ThreadSafeConnection},
    sqlez_macros::sql,
};
use workspace::{ItemId, WorkspaceDb, WorkspaceId};

pub(crate) struct VideoRecord {
    pub path: PathBuf,
    pub provider_path: Option<Vec<u8>>,
}

pub(crate) struct VideoViewerDb(ThreadSafeConnection);

impl Domain for VideoViewerDb {
    const NAME: &str = stringify!(VideoViewerDb);

    const MIGRATIONS: &[&str] = &[
        sql!(
            CREATE TABLE video_viewers (
                workspace_id INTEGER,
                item_id INTEGER UNIQUE,

                video_path BLOB,

                PRIMARY KEY(workspace_id, item_id),
                FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id)
                ON DELETE CASCADE
            ) STRICT;
        ),
        sql!(ALTER TABLE video_viewers ADD COLUMN provider_path BLOB;),
    ];
}

db::static_connection!(VideoViewerDb, [WorkspaceDb]);

impl VideoViewerDb {
    query! {
        pub async fn save_video_path(
            item_id: ItemId,
            workspace_id: WorkspaceId,
            video_path: PathBuf,
            provider_path: Option<Vec<u8>>
        ) -> Result<()> {
            INSERT OR REPLACE INTO video_viewers(item_id, workspace_id, video_path, provider_path)
            VALUES (?, ?, ?, ?)
        }
    }

    pub fn get_video(
        &self,
        item_id: ItemId,
        workspace_id: WorkspaceId,
    ) -> anyhow::Result<Option<VideoRecord>> {
        let record = self.get_video_row(item_id, workspace_id)?;
        Ok(record.map(|(path, provider_path)| VideoRecord {
            path,
            provider_path,
        }))
    }

    query! {
        fn get_video_row(item_id: ItemId, workspace_id: WorkspaceId) -> Result<Option<(PathBuf, Option<Vec<u8>>)>> {
            SELECT video_path, provider_path FROM video_viewers WHERE item_id = ? AND workspace_id = ?
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    async fn test_video_provider_path_round_trip(cx: &mut gpui::TestAppContext) {
        let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
        let workspace_id = workspace_db
            .next_id()
            .await
            .expect("workspace ID should be allocated");
        let video_db = cx.update(|cx| VideoViewerDb::global(cx));
        let provider_path = vec![0, 1, 2, 0xff];

        video_db
            .save_video_path(
                1,
                workspace_id,
                PathBuf::from("legacy-video.mp4"),
                Some(provider_path.clone()),
            )
            .await
            .expect("video record should save");

        let record = video_db
            .get_video(1, workspace_id)
            .expect("video record should load")
            .expect("video record should exist");
        assert_eq!(record.path, PathBuf::from("legacy-video.mp4"));
        assert_eq!(record.provider_path, Some(provider_path));

        video_db
            .save_video_path(1, workspace_id, PathBuf::from("legacy-only.mp4"), None)
            .await
            .expect("legacy video record should save");
        let legacy_record = video_db
            .get_video(1, workspace_id)
            .expect("legacy video record should load")
            .expect("legacy video record should exist");
        assert_eq!(legacy_record.path, PathBuf::from("legacy-only.mp4"));
        assert_eq!(legacy_record.provider_path, None);
    }
}
