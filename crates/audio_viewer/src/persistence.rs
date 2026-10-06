use std::path::PathBuf;

use db::{
    query,
    sqlez::{domain::Domain, thread_safe_connection::ThreadSafeConnection},
    sqlez_macros::sql,
};
use workspace::{ItemId, WorkspaceDb, WorkspaceId};

pub(crate) struct AudioRecord {
    pub path: PathBuf,
    pub provider_path: Option<Vec<u8>>,
}

pub(crate) struct AudioViewerDb(ThreadSafeConnection);

impl Domain for AudioViewerDb {
    const NAME: &str = stringify!(AudioViewerDb);

    const MIGRATIONS: &[&str] = &[
        sql!(
            CREATE TABLE audio_viewers (
                workspace_id INTEGER,
                item_id INTEGER UNIQUE,

                audio_path BLOB,

                PRIMARY KEY(workspace_id, item_id),
                FOREIGN KEY(workspace_id) REFERENCES workspaces(workspace_id)
                ON DELETE CASCADE
            ) STRICT;
        ),
        sql!(ALTER TABLE audio_viewers ADD COLUMN provider_path BLOB;),
    ];
}

db::static_connection!(AudioViewerDb, [WorkspaceDb]);

impl AudioViewerDb {
    query! {
        pub async fn save_audio_path(
            item_id: ItemId,
            workspace_id: WorkspaceId,
            audio_path: PathBuf,
            provider_path: Option<Vec<u8>>
        ) -> Result<()> {
            INSERT OR REPLACE INTO audio_viewers(item_id, workspace_id, audio_path, provider_path)
            VALUES (?, ?, ?, ?)
        }
    }

    pub fn get_audio(
        &self,
        item_id: ItemId,
        workspace_id: WorkspaceId,
    ) -> anyhow::Result<Option<AudioRecord>> {
        let record = self.get_audio_row(item_id, workspace_id)?;
        Ok(record.map(|(path, provider_path)| AudioRecord {
            path,
            provider_path,
        }))
    }

    query! {
        fn get_audio_row(item_id: ItemId, workspace_id: WorkspaceId) -> Result<Option<(PathBuf, Option<Vec<u8>>)>> {
            SELECT audio_path, provider_path FROM audio_viewers WHERE item_id = ? AND workspace_id = ?
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    async fn test_audio_provider_path_round_trip(cx: &mut gpui::TestAppContext) {
        let workspace_db = cx.update(|cx| WorkspaceDb::global(cx));
        let workspace_id = workspace_db
            .next_id()
            .await
            .expect("workspace ID should be allocated");
        let audio_db = cx.update(|cx| AudioViewerDb::global(cx));
        let provider_path = vec![0, 1, 2, 0xff];

        audio_db
            .save_audio_path(
                1,
                workspace_id,
                PathBuf::from("legacy-audio.mp3"),
                Some(provider_path.clone()),
            )
            .await
            .expect("audio record should save");

        let record = audio_db
            .get_audio(1, workspace_id)
            .expect("audio record should load")
            .expect("audio record should exist");
        assert_eq!(record.path, PathBuf::from("legacy-audio.mp3"));
        assert_eq!(record.provider_path, Some(provider_path));

        audio_db
            .save_audio_path(1, workspace_id, PathBuf::from("legacy-only.mp3"), None)
            .await
            .expect("legacy audio record should save");
        let legacy_record = audio_db
            .get_audio(1, workspace_id)
            .expect("legacy audio record should load")
            .expect("legacy audio record should exist");
        assert_eq!(legacy_record.path, PathBuf::from("legacy-only.mp3"));
        assert_eq!(legacy_record.provider_path, None);
    }
}
