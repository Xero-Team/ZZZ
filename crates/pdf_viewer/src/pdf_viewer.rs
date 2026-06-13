mod actions;
mod document;
mod item;
mod persistence;
mod search;
mod serializable;
mod ui;
mod view;
mod worker;

use std::sync::Arc;

use anyhow::Context as _;
use gpui::{App, AppContext, Entity, Task, actions};
use project::{Project, ProjectEntryId, ProjectPath, WorktreeId};
use util::rel_path::RelPath;

pub use ui::PdfToolbarControls;
pub use view::PdfView;

pub(crate) const MIN_ZOOM: f32 = 0.1;
pub(crate) const MAX_ZOOM: f32 = 10.0;
pub(crate) const ZOOM_STEP: f32 = 1.2;
/// Base resolution at 100% zoom. PDF user-space is 72 units per inch, so a DPI
/// above that renders crisper than 1:1 device pixels on the page points.
pub(crate) const BASE_DPI: f32 = 144.0;
pub(crate) const PAGE_HORIZONTAL_MARGIN: f32 = 24.0;
pub(crate) const PAGE_VERTICAL_MARGIN: f32 = 24.0;
pub(crate) const PAGE_GAP: f32 = 20.0;
/// Layout size for a page whose reported dimensions are missing or degenerate.
/// US Letter at 72 dpi; only used so such pages keep a slot in the continuous
/// layout instead of shifting every later page's index.
pub(crate) const FALLBACK_PAGE_WIDTH: f32 = 612.0;
pub(crate) const FALLBACK_PAGE_HEIGHT: f32 = 792.0;
/// Memory ceiling for cached page bitmaps. Renders are evicted farthest-first
/// once the cached bytes exceed this, and idle prefetch stops filling the cache
/// at this bound, so a large document never renders its entire page set into
/// memory at once. At ~7.7 MB per Letter page at 100% zoom this keeps roughly
/// 65 pages resident.
pub(crate) const CACHE_MEMORY_BUDGET: u64 = 512 * 1024 * 1024;

actions!(
    pdf_viewer,
    [
        /// Go to the previous page.
        PreviousPage,
        /// Go to the next page.
        NextPage,
        /// Zoom in.
        ZoomIn,
        /// Zoom out.
        ZoomOut,
        /// Reset zoom to 100%.
        ResetZoom,
        /// Fit the page width to the view.
        FitWidth,
        /// Fit the whole page in the view.
        FitPage,
        /// Toggle the navigation sidebar.
        ToggleSidebar,
        /// Focus the search field.
        ToggleSearch,
        /// Jump to the next search match.
        NextMatch,
        /// Jump to the previous search match.
        PreviousMatch,
    ]
);

/// The project-level handle for an open PDF file. Mirrors `image_viewer`'s
/// `ImageItem` role: it identifies the file within a worktree and is what the
/// workspace dispatches on when a `.pdf` path is opened.
pub struct PdfItem {
    path: Arc<RelPath>,
    worktree_id: WorktreeId,
    entry_id: Option<ProjectEntryId>,
}

impl PdfItem {
    fn project_path(&self) -> ProjectPath {
        ProjectPath {
            worktree_id: self.worktree_id,
            path: self.path.clone(),
        }
    }
}

impl project::ProjectItem for PdfItem {
    fn try_open(
        project: &Entity<Project>,
        path: &ProjectPath,
        cx: &mut App,
    ) -> Option<Task<anyhow::Result<Entity<Self>>>> {
        let extension = path.path.extension()?;
        if !extension.eq_ignore_ascii_case("pdf") {
            return None;
        }

        let path = path.clone();
        let project = project.clone();
        Some(cx.spawn(async move |cx| {
            let entry_id = project.update(cx, |project, cx| {
                let worktree_id = path.worktree_id;
                let worktree = project
                    .worktree_for_id(worktree_id, cx)
                    .with_context(|| format!("worktree {worktree_id:?} not found"))?;
                anyhow::Ok(
                    worktree
                        .read(cx)
                        .entry_for_path(&path.path)
                        .map(|entry| entry.id),
                )
            })?;

            anyhow::Ok(cx.new(|_cx| PdfItem {
                path: path.path.clone(),
                worktree_id: path.worktree_id,
                entry_id,
            }))
        }))
    }

    fn entry_id(&self, _cx: &App) -> Option<ProjectEntryId> {
        self.entry_id
    }

    fn project_path(&self, _cx: &App) -> Option<ProjectPath> {
        Some(self.project_path())
    }

    fn is_dirty(&self) -> bool {
        false
    }
}

pub fn init(cx: &mut App) {
    workspace::register_project_item::<PdfView>(cx);
    workspace::register_serializable_item::<PdfView>(cx);
}
