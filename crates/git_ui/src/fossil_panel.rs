//! A read-only panel for Fossil checkouts.
//!
//! Fossil repositories are opened by the same `git` crate abstraction as Git
//! ones, but the backend has no index, no stashes, no remotes and no commit
//! graph (see `RepositoryBackend::Fossil`). Rather than rendering the Git
//! panel's affordances and failing at click time, this panel shows the one thing
//! a Fossil checkout supports here: the list of changed files and their diffs.

use std::ops::Range;
use std::sync::Arc;

use git::status::StageStatus;
use gpui::{
    Action, App, AsyncWindowContext, Context, Entity, EventEmitter, FocusHandle, Focusable, Pixels,
    ScrollStrategy, SharedString, Subscription, UniformListScrollHandle, WeakEntity, Window,
    actions, rems, uniform_list,
};
use i18n::tr;
use project::Fs;
use project::Project;
use project::git_store::{GitStoreEvent, Repository};
use settings::Settings as _;
use ui::prelude::*;
use workspace::{
    Workspace,
    dock::{DockPosition, Panel, PanelEvent},
};

use crate::changes_list::{self, Section, StatusEntry};
use crate::fossil_panel_settings::FossilPanelSettings;
use crate::project_diff::ProjectDiff;

const FOSSIL_PANEL_KEY: &str = "FossilPanel";

actions!(
    fossil_panel,
    [
        /// Toggles focus on the Fossil panel.
        ToggleFocus,
    ]
);

pub fn register(workspace: &mut Workspace) {
    workspace.register_action(|workspace, _: &ToggleFocus, window, cx| {
        workspace.toggle_panel_focus::<FossilPanel>(window, cx);
    });
}

/// One row of the Fossil panel's list.
///
/// Unlike the Git panel there is no tree and no staging column, so the rows are
/// just the section headings and the files under them.
#[derive(Debug, PartialEq, Eq, Clone)]
enum FossilListEntry {
    Header(Section),
    File(StatusEntry),
}

pub struct FossilPanel {
    project: Entity<Project>,
    workspace: WeakEntity<Workspace>,
    active_repository: Option<Entity<Repository>>,
    entries: Vec<FossilListEntry>,
    selected_entry: Option<usize>,
    focus_handle: FocusHandle,
    scroll_handle: UniformListScrollHandle,
    fs: Arc<dyn Fs>,
    _subscriptions: Vec<Subscription>,
}

impl FossilPanel {
    pub async fn load(
        workspace: WeakEntity<Workspace>,
        mut cx: AsyncWindowContext,
    ) -> anyhow::Result<Entity<Self>> {
        workspace.update_in(&mut cx, |workspace, window, cx| {
            cx.new(|cx| FossilPanel::new(workspace, window, cx))
        })
    }

    fn new(workspace: &mut Workspace, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let project = workspace.project().clone();
        let fs = workspace.app_state().fs.clone();
        let git_store = project.read(cx).git_store().clone();
        let subscriptions = vec![cx.subscribe(&git_store, |this, _, _: &GitStoreEvent, cx| {
            this.rebuild(cx);
        })];

        let mut panel = Self {
            project,
            workspace: workspace.weak_handle(),
            active_repository: None,
            entries: Vec::new(),
            selected_entry: None,
            focus_handle: cx.focus_handle(),
            scroll_handle: UniformListScrollHandle::default(),
            fs,
            _subscriptions: subscriptions,
        };
        panel.rebuild(cx);
        panel
    }

    /// Picks the repository this panel owns.
    ///
    /// A Fossil checkout is the only non-Git backend in the store, which is what
    /// separates it from every Git repository in the same project.
    fn fossil_repository(&self, cx: &App) -> Option<Entity<Repository>> {
        let git_store = self.project.read(cx).git_store();
        let git_store = git_store.read(cx);

        if let Some(active) = git_store.active_repository()
            && is_fossil_checkout(&active, cx)
        {
            return Some(active);
        }

        git_store
            .repositories()
            .iter()
            .filter(|(_, repository)| is_fossil_checkout(repository, cx))
            .min_by_key(|(id, _)| id.0)
            .map(|(_, repository)| repository.clone())
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.active_repository = self.fossil_repository(cx);
        self.entries = self
            .active_repository
            .as_ref()
            .map(|repository| entries_for_repository(&repository.read(cx)))
            .unwrap_or_default();

        if self
            .selected_entry
            .is_some_and(|index| index >= self.entries.len())
        {
            self.selected_entry = None;
        }

        cx.notify();
    }

    fn open_selected(&mut self, _: &menu::Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self
            .selected_entry
            .and_then(|index| self.entries.get(index))
            .and_then(|entry| match entry {
                FossilListEntry::File(entry) => Some(entry.clone()),
                FossilListEntry::Header(_) => None,
            })
        else {
            return;
        };
        self.open_diff(&entry, window, cx);
    }

    fn open_diff(&mut self, entry: &StatusEntry, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        workspace.update(cx, |workspace, cx| {
            ProjectDiff::deploy_at(workspace, Some(entry.clone()), window, cx);
        });
    }

    fn select_next(&mut self, _: &menu::SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        self.select_step(true, window, cx);
    }

    fn select_previous(
        &mut self,
        _: &menu::SelectPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_step(false, window, cx);
    }

    fn select_step(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let is_selectable = |index: usize| -> bool {
            matches!(self.entries.get(index), Some(FossilListEntry::File(_)))
        };

        let next = match self.selected_entry {
            Some(current) => {
                let mut index = current;
                loop {
                    if forward {
                        index += 1;
                        if index >= self.entries.len() {
                            return;
                        }
                    } else {
                        if index == 0 {
                            return;
                        }
                        index -= 1;
                    }
                    if is_selectable(index) {
                        break index;
                    }
                }
            }
            None => {
                let indices: Box<dyn Iterator<Item = usize>> = if forward {
                    Box::new(0..self.entries.len())
                } else {
                    Box::new((0..self.entries.len()).rev())
                };
                match indices.into_iter().find(|index| is_selectable(*index)) {
                    Some(index) => index,
                    None => return,
                }
            }
        };

        self.selected_entry = Some(next);
        self.scroll_handle.scroll_to_item(next, ScrollStrategy::Top);
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (name, branch) = match self.active_repository.as_ref() {
            Some(repository) => {
                let repository = repository.read(cx);
                let name = repository.display_name();
                let branch = repository
                    .branch
                    .as_ref()
                    .map(|branch| SharedString::from(branch.name().to_string()));
                (name, branch)
            }
            None => (SharedString::default(), None),
        };

        h_flex()
            .h(rems(1.75))
            .px_3()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(Label::new(name).size(LabelSize::Small))
            .children(branch.map(|branch| {
                Label::new(branch)
                    .size(LabelSize::Small)
                    .color(Color::Muted)
            }))
    }

    fn render_body(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.entries.is_empty() {
            let message = if self.active_repository.is_some() {
                tr(cx, "git_ui.fossil_panel.no_changes", "No changes")
            } else {
                tr(
                    cx,
                    "git_ui.fossil_panel.no_repository",
                    "Open a Fossil checkout to see its changes",
                )
            };

            return v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .child(
                    Label::new(message)
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element();
        }

        let path_style = self.project.read(cx).path_style(cx);
        let selected_entry = self.selected_entry;

        uniform_list(
            "fossil-entries",
            self.entries.len(),
            cx.processor(move |this, range: Range<usize>, window, cx| {
                let mut items = Vec::with_capacity(range.len());

                for index in range {
                    let Some(entry) = this.entries.get(index) else {
                        continue;
                    };

                    match entry {
                        FossilListEntry::Header(section) => {
                            items.push(
                                h_flex()
                                    .id(index)
                                    .h(rems(1.75))
                                    .px_3()
                                    .child(
                                        Label::new(changes_list::section_title(*section, cx))
                                            .size(LabelSize::Small)
                                            .color(Color::Muted),
                                    )
                                    .into_any_element(),
                            );
                        }
                        FossilListEntry::File(entry) => {
                            let entry = entry.clone();
                            let clicked = entry.clone();
                            items.push(
                                h_flex()
                                    .id(index)
                                    .h(rems(1.75))
                                    .px_3()
                                    .gap_1p5()
                                    .w_full()
                                    .cursor_pointer()
                                    .border_1()
                                    .when(
                                        selected_entry == Some(index)
                                            && this.focus_handle.is_focused(window),
                                        |this| {
                                            this.border_color(
                                                cx.theme().colors().panel_focused_border,
                                            )
                                        },
                                    )
                                    .when(selected_entry == Some(index), |this| {
                                        this.bg(cx.theme().colors().element_selected)
                                    })
                                    .hover(|this| this.bg(cx.theme().colors().ghost_element_hover))
                                    .child(crate::git_status_icon(entry.status))
                                    .child(
                                        h_flex()
                                            .min_w_0()
                                            .flex_1()
                                            .gap_1()
                                            .child(Label::new(entry.display_name(path_style)).when(
                                                entry.status.is_deleted(),
                                                Label::strikethrough,
                                            ))
                                            .children(entry.parent_dir(path_style).map(|parent| {
                                                Label::new(SharedString::from(parent))
                                                    .size(LabelSize::Small)
                                                    .color(Color::Muted)
                                            })),
                                    )
                                    .when_some(entry.diff_stat, |this, stat| {
                                        this.child(ui::DiffStat::new(
                                            format!("fossil-diff-stat-{index}"),
                                            stat.added as usize,
                                            stat.deleted as usize,
                                        ))
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.selected_entry = Some(index);
                                        this.open_diff(&clicked, window, cx);
                                    }))
                                    .into_any_element(),
                            );
                        }
                    }
                }

                items
            }),
        )
        .track_scroll(&self.scroll_handle)
        .flex_1()
        .size_full()
        .into_any_element()
    }
}

impl Render for FossilPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("fossil_panel")
            .key_context("FossilPanel")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::open_selected))
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().colors().panel_background)
            .child(self.render_header(cx))
            .child(self.render_body(cx))
    }
}

impl Focusable for FossilPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<PanelEvent> for FossilPanel {}

impl Panel for FossilPanel {
    fn persistent_name() -> &'static str {
        FOSSIL_PANEL_KEY
    }

    fn panel_key() -> &'static str {
        FOSSIL_PANEL_KEY
    }

    fn position(&self, _window: &Window, cx: &App) -> DockPosition {
        FossilPanelSettings::get_global(cx).dock
    }

    fn position_is_valid(&self, position: DockPosition) -> bool {
        matches!(position, DockPosition::Left | DockPosition::Right)
    }

    fn set_position(
        &mut self,
        position: DockPosition,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        settings::update_settings_file(self.fs.clone(), cx, move |settings, _| {
            settings.fossil_panel.get_or_insert_default().dock = Some(position.into())
        });
    }

    fn default_size(&self, _window: &Window, cx: &App) -> Pixels {
        FossilPanelSettings::get_global(cx).default_width
    }

    fn icon(&self, _window: &Window, cx: &App) -> Option<IconName> {
        FossilPanelSettings::get_global(cx)
            .button
            .then_some(IconName::FileDiff)
    }

    fn starts_open(&self, _window: &Window, cx: &App) -> bool {
        FossilPanelSettings::get_global(cx).starts_open
    }

    fn icon_tooltip(&self, _window: &Window, cx: &App) -> Option<SharedString> {
        Some(tr(cx, "git_ui.fossil_panel.title", "Fossil Panel").into())
    }

    fn toggle_action(&self) -> Box<dyn Action> {
        Box::new(ToggleFocus)
    }

    fn activation_priority(&self) -> u32 {
        11
    }

    fn hide_button_setting(&self, _: &App) -> Option<workspace::HideStatusItem> {
        Some(workspace::HideStatusItem::new(|settings| {
            settings.fossil_panel.get_or_insert_default().button = Some(false);
        }))
    }
}

fn is_fossil_checkout(repository: &Entity<Repository>, cx: &App) -> bool {
    repository.read(cx).backend().is_fossil()
}

/// Builds the sectioned list of changed files.
///
/// Fossil has no merge conflicts to surface in this panel and no index, so every
/// file lands in either `Tracked` or `New`, unstaged.
fn entries_for_repository(repository: &Repository) -> Vec<FossilListEntry> {
    section_entries(repository.cached_status().map(|status| StatusEntry {
        repo_path: status.repo_path,
        status: status.status,
        staging: StageStatus::Unstaged,
        diff_stat: status.diff_stat,
    }))
}

/// Groups `statuses` into the `Tracked` and `New` sections, each sorted by path.
///
/// Split from [`entries_for_repository`] so the grouping is testable without a
/// repository entity.
fn section_entries(statuses: impl IntoIterator<Item = StatusEntry>) -> Vec<FossilListEntry> {
    let mut tracked = Vec::new();
    let mut new = Vec::new();

    for entry in statuses {
        if entry.status.is_created() {
            new.push(entry);
        } else {
            tracked.push(entry);
        }
    }

    let mut entries = Vec::with_capacity(tracked.len() + new.len() + 2);
    for (section, mut files) in [(Section::Tracked, tracked), (Section::New, new)] {
        if files.is_empty() {
            continue;
        }
        files.sort_by(|a, b| a.repo_path.cmp(&b.repo_path));
        entries.push(FossilListEntry::Header(section));
        entries.extend(files.into_iter().map(FossilListEntry::File));
    }
    entries
}

#[cfg(test)]
mod tests {
    use git::repository::RepoPath;
    use git::status::{FileStatus, StatusCode};

    use super::*;

    fn status_entry(path: &str, status: FileStatus) -> StatusEntry {
        StatusEntry {
            repo_path: RepoPath::new(path).expect("valid repo path"),
            status,
            staging: StageStatus::Unstaged,
            diff_stat: None,
        }
    }

    fn repo_path(entry: &FossilListEntry) -> &RepoPath {
        match entry {
            FossilListEntry::File(entry) => &entry.repo_path,
            FossilListEntry::Header(_) => panic!("expected a file entry"),
        }
    }

    #[test]
    fn groups_modified_files_under_tracked_and_new_files_under_new() {
        let entries = section_entries([
            status_entry("src/lib.rs", FileStatus::worktree(StatusCode::Modified)),
            status_entry("README.md", FileStatus::Untracked),
            status_entry("src/main.rs", FileStatus::worktree(StatusCode::Modified)),
        ]);

        assert_eq!(
            entries,
            vec![
                FossilListEntry::Header(Section::Tracked),
                FossilListEntry::File(status_entry(
                    "src/lib.rs",
                    FileStatus::worktree(StatusCode::Modified)
                )),
                FossilListEntry::File(status_entry(
                    "src/main.rs",
                    FileStatus::worktree(StatusCode::Modified)
                )),
                FossilListEntry::Header(Section::New),
                FossilListEntry::File(status_entry("README.md", FileStatus::Untracked)),
            ]
        );
    }

    #[test]
    fn omits_empty_sections() {
        let entries = section_entries([status_entry(
            "src/lib.rs",
            FileStatus::worktree(StatusCode::Modified),
        )]);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0], FossilListEntry::Header(Section::Tracked));
        assert_eq!(
            repo_path(&entries[1]).as_std_path(),
            std::path::Path::new("src/lib.rs")
        );
    }
}
