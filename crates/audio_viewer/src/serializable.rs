use anyhow::Context as _;
use gpui::{App, AppContext as _, Context, Entity, Task, WeakEntity, Window};
use project::{Project, ProjectPath};
use workspace::{ItemId, Workspace, WorkspaceId, delete_unloaded_items, item::SerializableItem};

use crate::{
    persistence::AudioViewerDb,
    view::{AudioView, AudioViewEvent},
};

impl SerializableItem for AudioView {
    fn serialized_item_kind() -> &'static str {
        "AudioView"
    }

    fn cleanup(
        workspace_id: WorkspaceId,
        alive_items: Vec<ItemId>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<()>> {
        let db = AudioViewerDb::global(cx);
        delete_unloaded_items(alive_items, workspace_id, "audio_viewers", &db, cx)
    }

    fn deserialize(
        project: Entity<Project>,
        _workspace: WeakEntity<Workspace>,
        workspace_id: WorkspaceId,
        item_id: ItemId,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Entity<Self>>> {
        let db = AudioViewerDb::global(cx);
        window.spawn(cx, async move |cx| {
            let audio_path = db
                .get_audio_path(item_id, workspace_id)?
                .context("no audio path found for item")?;

            let worktree_task = project.update(cx, |project, cx| {
                project.find_or_create_worktree(audio_path.clone(), false, cx)
            });
            let (worktree, relative_path) = worktree_task.await.context("audio path not found")?;
            let worktree_id = worktree.read_with(cx, |worktree, _cx| worktree.id());

            let project_path = ProjectPath {
                worktree_id,
                path: relative_path,
            };

            let open_task = cx
                .update(|_window, cx| {
                    <crate::AudioItem as project::ProjectItem>::try_open(
                        &project,
                        &project_path,
                        cx,
                    )
                })?
                .context("not an audio path")?;
            let audio_item = open_task.await?;

            cx.update(|window, cx| cx.new(|cx| AudioView::new(audio_item, project, window, cx)))
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
        let path = self.abs_path(cx)?;
        let db = AudioViewerDb::global(cx);

        Some(
            cx.background_spawn(
                async move { db.save_audio_path(item_id, workspace_id, path).await },
            ),
        )
    }

    fn should_serialize(&self, event: &Self::Event) -> bool {
        matches!(event, AudioViewEvent::TitleChanged)
    }
}
