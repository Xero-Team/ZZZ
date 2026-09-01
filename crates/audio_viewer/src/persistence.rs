use std::path::PathBuf;

use db::{
    query,
    sqlez::{domain::Domain, thread_safe_connection::ThreadSafeConnection},
    sqlez_macros::sql,
};
use workspace::{ItemId, WorkspaceDb, WorkspaceId};

pub(crate) struct AudioViewerDb(ThreadSafeConnection);

impl Domain for AudioViewerDb {
    const NAME: &str = stringify!(AudioViewerDb);

    const MIGRATIONS: &[&str] = &[sql!(
        CREATE TABLE audio_viewers (
            workspace_id INTEGER,
            item_id INTEGER UNIQUE,

            audio_path BLOB,

            PRIMARY KEY(workspace_id, item_id),
            FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id)
            ON DELETE CASCADE
        ) STRICT;
    )];
}

db::static_connection!(AudioViewerDb, [WorkspaceDb]);

impl AudioViewerDb {
    query! {
        pub async fn save_audio_path(
            item_id: ItemId,
            workspace_id: WorkspaceId,
            audio_path: PathBuf
        ) -> Result<()> {
            INSERT OR REPLACE INTO audio_viewers(item_id, workspace_id, audio_path)
            VALUES (?, ?, ?)
        }
    }

    query! {
        pub fn get_audio_path(item_id: ItemId, workspace_id: WorkspaceId) -> Result<Option<PathBuf>> {
            SELECT audio_path
            FROM audio_viewers
            WHERE item_id = ? AND workspace_id = ?
        }
    }
}
