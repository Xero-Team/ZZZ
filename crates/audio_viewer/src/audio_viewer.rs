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

            anyhow::Ok(cx.new(|_cx| AudioItem {
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
    workspace::register_project_item::<AudioView>(cx);
    workspace::register_serializable_item::<AudioView>(cx);
}
