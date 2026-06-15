use anyhow::Context as _;
use gpui::{App, AppContext as _, Context, Entity, Task, WeakEntity, Window};
use project::{Project, ProjectPath};
use workspace::{ItemId, Workspace, WorkspaceId, delete_unloaded_items, item::SerializableItem};

use crate::{
    persistence::PdfViewerDb,
    view::{PdfView, PdfViewEvent},
};

impl SerializableItem for PdfView {
    fn serialized_item_kind() -> &'static str {
        "PdfView"
    }

    fn cleanup(
        workspace_id: WorkspaceId,
        alive_items: Vec<ItemId>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<()>> {
        let db = PdfViewerDb::global(cx);
        delete_unloaded_items(alive_items, workspace_id, "pdf_viewers", &db, cx)
    }

    fn deserialize(
        project: Entity<Project>,
        _workspace: WeakEntity<Workspace>,
        workspace_id: WorkspaceId,
        item_id: ItemId,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<Entity<Self>>> {
        let db = PdfViewerDb::global(cx);
        window.spawn(cx, async move |cx| {
            let record = db
                .get_pdf(item_id, workspace_id)?
                .context("no PDF path found for item")?;
            let zoom_mode = record.zoom_mode();
            let (page, zoom) = (record.page.max(0) as usize, record.zoom);

            let worktree_task = project.update(cx, |project, cx| {
                project.find_or_create_worktree(record.path.clone(), false, cx)
            });
            let (worktree, relative_path) = worktree_task.await.context("PDF path not found")?;
            let worktree_id = worktree.read_with(cx, |worktree, _cx| worktree.id());

            let project_path = ProjectPath {
                worktree_id,
                path: relative_path,
            };

            let open_task = cx
                .update(|_window, cx| {
                    <crate::PdfItem as project::ProjectItem>::try_open(&project, &project_path, cx)
                })?
                .context("not a PDF path")?;
            let pdf_item = open_task.await?;

            cx.update(|window, cx| {
                cx.new(|cx| {
                    let mut view = PdfView::new(pdf_item, project, window, cx);
                    view.restore_state(page, zoom_mode, zoom);
                    view
                })
            })
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
        let page = self.current_page() as i64;
        let zoom = self.zoom();
        let zoom_mode = self.zoom_mode_key().to_owned();
        let db = PdfViewerDb::global(cx);

        Some(cx.background_spawn(async move {
            db.save_pdf(item_id, workspace_id, path, page, zoom_mode, zoom)
                .await
        }))
    }

    fn should_serialize(&self, event: &Self::Event) -> bool {
        matches!(event, PdfViewEvent::TitleChanged)
    }
}
