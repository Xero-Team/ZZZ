mod item;
mod persistence;
mod serializable;
mod ui;
mod view;

use anyhow::Context as _;
use gpui::{App, AppContext, Entity, Task, actions};
use project::{Project, ProjectEntryId, ProjectPath, WorktreeId};
use util::rel_path::RelPath;
use video::is_supported_video_extension;

use std::sync::Arc;

pub use ui::{VideoInfo, VideoToolbarControls};
pub use view::VideoView;

actions!(
    video_viewer,
    [
        /// Toggle play or pause.
        TogglePlay,
        /// Stop playback and return to the start.
        Stop,
        /// Seek forward by five seconds.
        SeekForward,
        /// Seek backward by five seconds.
        SeekBackward,
        /// Seek to the start of the file.
        SeekToStart,
        /// Seek to the end of the file.
        SeekToEnd,
        /// Advance to the next frame.
        StepForward,
        /// Go back to the previous frame.
        StepBackward,
        /// Toggle mute.
        ToggleMute,
        /// Increase the volume.
        IncreaseVolume,
        /// Decrease the volume.
        DecreaseVolume,
        /// Increase the playback speed.
        IncreaseSpeed,
        /// Decrease the playback speed.
        DecreaseSpeed,
        /// Reset the playback speed to normal.
        ResetSpeed,
        /// Toggle looping playback.
        ToggleLoop,
    ]
);

pub struct VideoItem {
    path: Arc<RelPath>,
    worktree_id: WorktreeId,
    entry_id: Option<ProjectEntryId>,
    resource_id: Option<vfs::ResourceId>,
    vfs_path: Option<vfs::VfsPath>,
}

impl VideoItem {
    fn project_path(&self) -> ProjectPath {
        ProjectPath {
            worktree_id: self.worktree_id,
            path: self.path.clone(),
        }
    }
}

impl project::ProjectItem for VideoItem {
    fn try_open(
        project: &Entity<Project>,
        path: &ProjectPath,
        cx: &mut App,
    ) -> Option<Task<anyhow::Result<Entity<Self>>>> {
        let extension = path.path.extension()?;
        if !is_supported_video_extension(extension) {
            return None;
        }

        let path = path.clone();
        let project = project.clone();
        Some(cx.spawn(async move |cx| {
            let identity = project.update(cx, |project, cx| {
                project
                    .entry_identity_for_project_path(&path, cx)
                    .with_context(|| format!("worktree {:?} not found", path.worktree_id))
            })?;

            anyhow::Ok(cx.new(|_cx| VideoItem {
                path: path.path.clone(),
                worktree_id: path.worktree_id,
                entry_id: identity.entry_id,
                resource_id: identity.resource_id,
                vfs_path: identity.vfs_path,
            }))
        }))
    }

    fn entry_id(&self, _cx: &App) -> Option<ProjectEntryId> {
        self.entry_id
    }

    fn resource_id(&self, _cx: &App) -> Option<vfs::ResourceId> {
        self.resource_id
    }

    fn vfs_path(&self, _cx: &App) -> Option<vfs::VfsPath> {
        self.vfs_path.clone()
    }

    fn project_path(&self, _cx: &App) -> Option<ProjectPath> {
        Some(self.project_path())
    }

    fn is_dirty(&self) -> bool {
        false
    }
}

pub fn init(cx: &mut App) {
    workspace::register_project_item::<VideoView>(cx);
    workspace::register_serializable_item::<VideoView>(cx);
}
