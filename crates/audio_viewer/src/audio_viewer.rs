mod item;
mod persistence;
mod player;
mod serializable;
mod ui;
mod view;
mod waveform;

use anyhow::Context as _;
use gpui::{App, AppContext, Entity, Task, actions};
use project::{Project, ProjectEntryId, ProjectPath, WorktreeId};
use util::rel_path::RelPath;

use std::sync::Arc;

pub use ui::{AudioInfo, AudioToolbarControls};
pub use view::AudioView;

use crate::player::is_supported_audio_extension;

actions!(
    audio_viewer,
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
        /// Toggle mute.
        ToggleMute,
    ]
);

pub struct AudioItem {
    path: Arc<RelPath>,
    worktree_id: WorktreeId,
    entry_id: Option<ProjectEntryId>,
    resource_id: Option<vfs::ResourceId>,
    vfs_path: Option<vfs::VfsPath>,
}

impl AudioItem {
    fn project_path(&self) -> ProjectPath {
        ProjectPath {
            worktree_id: self.worktree_id,
            path: self.path.clone(),
        }
    }
}

impl project::ProjectItem for AudioItem {
    fn try_open(
        project: &Entity<Project>,
        path: &ProjectPath,
        cx: &mut App,
    ) -> Option<Task<anyhow::Result<Entity<Self>>>> {
        let extension = path.path.extension()?;
        if !is_supported_audio_extension(extension) {
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

            anyhow::Ok(cx.new(|_cx| AudioItem {
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
    workspace::register_project_item::<AudioView>(cx);
    workspace::register_serializable_item::<AudioView>(cx);
}
