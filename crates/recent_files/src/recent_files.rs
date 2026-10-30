//! A picker for recently opened files.
//!
//! `Workspace::active_item_path_changed` records absolute paths into the
//! `recent_files` table in `WorkspaceDb`. This crate reads them back and
//! presents them in a picker. Files that no longer exist are filtered out.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use file_icons::FileIcons;
use gpui::{
    App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, SharedString, Styled, Task, WeakEntity, Window, actions, rems,
};
use picker::{Picker, PickerDelegate};
use ui::{Color, Icon, ListItem, ListItemSpacing, prelude::*};
use util::{ResultExt as _, paths::PathExt as _};
use workspace::{ModalView, OpenOptions, Workspace, WorkspaceDb};

const PANEL_WIDTH_REMS: f32 = 34.;

actions!(
    recent_files,
    [
        /// Toggles the recent files picker.
        Toggle,
    ]
);

pub fn init(cx: &mut App) {
    cx.observe_new(RecentFiles::register).detach();
}

pub struct RecentFiles {
    picker: Entity<Picker<RecentFilesDelegate>>,
}

impl ModalView for RecentFiles {}

impl EventEmitter<DismissEvent> for RecentFiles {}

impl Focusable for RecentFiles {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.picker.focus_handle(cx)
    }
}

impl RecentFiles {
    fn register(
        workspace: &mut Workspace,
        _window: Option<&mut Window>,
        _cx: &mut Context<Workspace>,
    ) {
        workspace.register_action(|workspace, _: &Toggle, window, cx| {
            let Some(recent_files) = workspace.active_modal::<Self>(cx) else {
                Self::open(workspace, window, cx);
                return;
            };

            recent_files.update(cx, |recent_files, cx| {
                recent_files.picker.update(cx, |picker, cx| {
                    picker.cycle_selection(window, cx);
                });
            });
        });
    }

    fn open(workspace: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
        let workspace_handle = workspace.weak_handle();
        workspace.toggle_modal(window, cx, move |window, cx| {
            Self::new(workspace_handle, window, cx)
        });
    }

    fn new(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let delegate = RecentFilesDelegate::new(workspace);
        let picker = cx.new(|cx| Picker::uniform_list(delegate, window, cx));

        cx.spawn(async move |this, cx| {
            if let Some(paths) = cx
                .update(|cx| WorkspaceDb::global(cx).recent_files(100))
                .log_err()
            {
                // Stat up to 100 paths. Doing it here keeps the file system off
                // the UI thread that opens the picker.
                let paths = cx
                    .background_spawn(async move {
                        paths
                            .into_iter()
                            .map(PathBuf::from)
                            .filter(|path| path.exists())
                            .collect::<Vec<_>>()
                    })
                    .await;
                this.update(cx, |this, cx| {
                    this.picker.update(cx, |picker, cx| {
                        picker.delegate.set_paths(paths, cx);
                    });
                })
                .log_err();
            }
        })
        .detach();

        Self { picker }
    }
}

impl Render for RecentFiles {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context("RecentFiles")
            .w(rems(PANEL_WIDTH_REMS))
            .child(self.picker.clone())
    }
}

/// A recently opened file, with the rendered and searchable forms of its path
/// precomputed: both are rebuilt on every keystroke otherwise.
struct RecentFile {
    path: PathBuf,
    /// `homify(path)`, used as the muted second line of the list item.
    display_path: String,
    /// Lowercase `display_path`, used to match the query.
    search_path: String,
}

impl RecentFile {
    fn new(path: PathBuf) -> Self {
        let display_path = homify(&path);
        let search_path = display_path.to_lowercase();
        Self {
            path,
            display_path,
            search_path,
        }
    }
}

struct RecentFilesDelegate {
    workspace: WeakEntity<Workspace>,
    files: Vec<RecentFile>,
    matches: Vec<usize>,
    selected_index: usize,
}

impl RecentFilesDelegate {
    fn new(workspace: WeakEntity<Workspace>) -> Self {
        Self {
            workspace,
            files: Vec::new(),
            matches: Vec::new(),
            selected_index: 0,
        }
    }

    fn set_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Picker<Self>>) {
        self.files = paths.into_iter().map(RecentFile::new).collect();
        self.matches = (0..self.files.len()).collect();
        self.selected_index = 0;
        cx.notify();
    }

    fn icon_for_file(&self, path: &Path, cx: &App) -> Option<Icon> {
        let file_name = path.file_name()?;
        let icon = FileIcons::get_icon(file_name.as_ref(), cx)?;
        Some(Icon::from_path(icon).color(Color::Muted))
    }
}

impl PickerDelegate for RecentFilesDelegate {
    type ListItem = ListItem;

    fn name() -> &'static str {
        "RecentFilesDelegate"
    }

    fn placeholder_text(&self, _window: &mut Window, cx: &mut App) -> Arc<str> {
        i18n::tr(cx, "recent_files.placeholder", "Open recent files...").into()
    }

    fn no_matches_text(&self, _window: &mut Window, cx: &mut App) -> Option<SharedString> {
        Some(i18n::tr(cx, "recent_files.no_matches", "No recent files").into())
    }

    fn match_count(&self) -> usize {
        self.matches.len()
    }

    fn selected_index(&self) -> usize {
        self.selected_index
    }

    fn set_selected_index(
        &mut self,
        ix: usize,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) {
        self.selected_index = ix;
    }

    fn update_matches(
        &mut self,
        query: String,
        _window: &mut Window,
        _cx: &mut Context<Picker<Self>>,
    ) -> Task<()> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            self.matches = (0..self.files.len()).collect();
        } else {
            self.matches = self
                .files
                .iter()
                .enumerate()
                .filter(|(_, file)| file.search_path.contains(&query))
                .map(|(index, _)| index)
                .collect();
        }
        self.selected_index = 0;
        Task::ready(())
    }

    fn confirm(&mut self, _secondary: bool, window: &mut Window, cx: &mut Context<Picker<Self>>) {
        let Some(path) = self
            .matches
            .get(self.selected_index)
            .and_then(|match_index| self.files.get(*match_index))
            .map(|file| file.path.clone())
        else {
            return;
        };

        self.workspace
            .update(cx, |workspace, cx| {
                workspace
                    .open_abs_path(path, OpenOptions::default(), window, cx)
                    .detach_and_log_err(cx);
            })
            .log_err();
        self.dismissed(window, cx);
    }

    fn dismissed(&mut self, _window: &mut Window, cx: &mut Context<Picker<Self>>) {
        cx.emit(DismissEvent);
    }

    fn render_match(
        &self,
        ix: usize,
        selected: bool,
        _window: &mut Window,
        cx: &mut Context<Picker<Self>>,
    ) -> Option<Self::ListItem> {
        let file = self.files.get(*self.matches.get(ix)?)?;
        let file_icon = self.icon_for_file(&file.path, cx);
        let file_name = file.path.file_name().map_or_else(
            || file.display_path.clone(),
            |name| name.to_string_lossy().into_owned(),
        );

        Some(
            ListItem::new(ix)
                .spacing(ListItemSpacing::Sparse)
                .start_slot::<Icon>(file_icon)
                .inset(true)
                .toggle_state(selected)
                .child(
                    h_flex().gap_2().child(Label::new(file_name)).child(
                        Label::new(file.display_path.clone())
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
                ),
        )
    }
}

fn homify(path: &Path) -> String {
    path.compact().to_string_lossy().into_owned()
}
