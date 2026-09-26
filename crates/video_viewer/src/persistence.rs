use std::path::PathBuf;

use db::{
    query,
    sqlez::{domain::Domain, thread_safe_connection::ThreadSafeConnection},
    sqlez_macros::sql,
};
use workspace::{ItemId, WorkspaceDb, WorkspaceId};

pub(crate) struct VideoViewerDb(ThreadSafeConnection);

impl Domain for VideoViewerDb {
    const NAME: &str = stringify!(VideoViewerDb);

    const MIGRATIONS: &[&str] = &[sql!(
        CREATE TABLE video_viewers (
            workspace_id INTEGER,
            item_id INTEGER UNIQUE,

            video_path BLOB,

            PRIMARY KEY(workspace_id, item_id),
            FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id)
            ON DELETE CASCADE
        ) STRICT;
    )];
}

db::static_connection!(VideoViewerDb, [WorkspaceDb]);

impl VideoViewerDb {
    query! {
        pub async fn save_video_path(
            item_id: ItemId,
            workspace_id: WorkspaceId,
            video_path: PathBuf
        ) -> Result<()> {
            INSERT OR REPLACE INTO video_viewers(item_id, workspace_id, video_path)
            VALUES (?, ?, ?)
        }
    }

    query! {
        pub fn get_video_path(item_id: ItemId, workspace_id: WorkspaceId) -> Result<Option<PathBuf>> {
            SELECT video_path
            FROM video_viewers
            WHERE item_id = ? AND workspace_id = ?
        }
    }
}
