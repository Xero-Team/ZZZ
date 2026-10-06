use anyhow::Context as _;
use gpui::{App, AppContext as _, Context, Entity, Task, WeakEntity, Window};
use project::{Project, ProjectPath};
use workspace::{ItemId, Workspace, WorkspaceId, delete_unloaded_items, item::SerializableItem};

use crate::{
    persistence::VideoViewerDb,
    view::{VideoView, VideoViewEvent},
};

impl SerializableItem for VideoView {
    fn serialized_item_kind() -> &'static str {
        "VideoView"
    }

    fn cleanup(
        workspace_id: WorkspaceId,
        alive_items: Vec<ItemId>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<()>> {
        let db = VideoViewerDb::global(cx);
        delete_unloaded_items(alive_items, workspace_id, "video_viewers", &db, cx)
    }

    fn deserialize(
        project: Entity<Project>,
        _workspace: WeakEntity<Workspace>,
        workspace_id: WorkspaceId,
        item_id: ItemId,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Entity<Self>>> {
        let db = VideoViewerDb::global(cx);
        window.spawn(cx, async move |cx| {
            let record = db
                .get_video(item_id, workspace_id)?
                .context("no video path found for item")?;

            let worktree_task = project.update(cx, |project, cx| {
                project.find_or_create_worktree(record.path.clone(), false, cx)
            });
            let (worktree, legacy_relative_path) =
                worktree_task.await.context("video path not found")?;
            let worktree_id = worktree.read_with(cx, |worktree, _cx| worktree.id());
            let relative_path = project::restore_persisted_rel_path(
                legacy_relative_path,
                record.provider_path.as_deref(),
            )?;

            let project_path = ProjectPath {
                worktree_id,
                path: relative_path,
            };

            let open_task = cx
                .update(|_window, cx| {
                    <crate::VideoItem as project::ProjectItem>::try_open(
                        &project,
                        &project_path,
                        cx,
                    )
                })?
                .context("not a video path")?;
            let video_item = open_task.await?;

            cx.update(|window, cx| cx.new(|cx| VideoView::new(video_item, project, window, cx)))
        })
    }

    fn serialize(
        &mut self,
        workspace: &mut Workspace,
        item_id: ItemId,
        _closing: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Task<anyhow::Result<()>>> {
        let workspace_id = workspace.database_id()?;
        let project_path = self.project_path(cx);
        let path = self.project.read(cx).absolutize(&project_path, cx)?;
        let provider_path =
            project::serialize_persisted_provider_path(self.video_item.read(cx).vfs_path.as_ref());
        let db = VideoViewerDb::global(cx);

        Some(cx.background_spawn(async move {
            db.save_video_path(item_id, workspace_id, path, provider_path)
                .await
        }))
    }

    fn should_serialize(&self, event: &Self::Event) -> bool {
        matches!(event, VideoViewEvent::TitleChanged)
    }
}
